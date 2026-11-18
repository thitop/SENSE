//! Combining recent calibration runs into one result.

use aim_trainer::data::CalibrationResult;
use aim_trainer::sensitivity::combiner::{recommend, TestKind};
use aim_trainer::sensitivity::joint_fit::fit_joint;
use aim_trainer::sensitivity::pooling::{
    is_answer, keep_recent, newest_agreeing, pool, pooled_answer, runs_agree, runs_to_pool, RunRecord, MAX_ANSWER_UNCERTAINTY, MAX_POOLED, SEARCH_CM,
    POOL_DAYS,
};
use chrono::{Duration, TimeZone, Utc};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap()
}

/// Expected counts from P(too fast | miss) = σ(−4·ln(cm / at)) for flick and micro.
fn run(days_ago: i64, dpi: f64, at: f64, cms: &[f64]) -> RunRecord {
    run_with(days_ago, dpi, at, cms, 6)
}

/// `run` with `misses` misses per test and sensitivity.
fn run_with(days_ago: i64, dpi: f64, at: f64, cms: &[f64], misses: usize) -> RunRecord {
    let rows = cms
        .iter()
        .flat_map(|&cm| {
            let p = 1.0 / (1.0 + (4.0 * (cm / at).ln()).exp());
            [TestKind::Flick, TestKind::Micro].map(|k| {
                let f = (misses as f64 * p).round() as usize;
                (k, cm, f, misses - f)
            })
        })
        .collect();
    RunRecord { at: now() - Duration::days(days_ago), dpi, rows, search: SEARCH_CM }
}

const CMS: [f64; 5] = [47.5, 40.0, 52.0, 50.0, 49.0];

#[test]
fn only_recent_runs_at_the_same_dpi_are_pooled() {
    let runs = vec![
        run(1, 800.0, 50.0, &CMS),
        run(POOL_DAYS + 1, 800.0, 50.0, &CMS),
        run(2, 1600.0, 50.0, &CMS),
        run(3, 800.0, 50.0, &CMS),
    ];
    let picked = runs_to_pool(&runs, now(), 800.0);
    assert_eq!(picked.len(), 2);
    assert!(picked.iter().all(|r| r.dpi == 800.0 && now() - r.at <= Duration::days(POOL_DAYS)));
}

#[test]
fn at_most_the_newest_runs_are_pooled() {
    let runs: Vec<RunRecord> = (0..8).map(|d| run(d, 800.0, 50.0, &CMS)).collect();
    let picked = runs_to_pool(&runs, now(), 800.0);
    assert_eq!(picked.len(), MAX_POOLED);
    assert!(picked.iter().all(|r| now() - r.at < Duration::days(MAX_POOLED as i64)), "newest first");
}

#[test]
fn one_run_pools_to_its_own_result() {
    let r = run(0, 800.0, 50.0, &CMS);
    let alone = recommend(&fit_joint(&r.rows).unwrap(), None).unwrap().neutral;
    let p = pool(&[&r]).unwrap();
    assert_eq!(p.runs, 1);
    assert!((p.cm_360 - alone.cm_360).abs() < 1e-9);
}

#[test]
fn two_runs_pool_into_a_narrower_result_between_them() {
    let (a, b) = (run(0, 800.0, 46.0, &CMS), run(1, 800.0, 54.0, &CMS));
    let p = pool(&[&a, &b]).unwrap();
    let alone = |r: &RunRecord| recommend(&fit_joint(&r.rows).unwrap(), None).unwrap().neutral;
    let (ra, rb) = (alone(&a), alone(&b));
    assert_eq!(p.runs, 2);
    assert!(ra.cm_360 < p.cm_360 && p.cm_360 < rb.cm_360, "{} {} {}", ra.cm_360, p.cm_360, rb.cm_360);
    assert!(p.rel_uncertainty < ra.rel_uncertainty.min(rb.rel_uncertainty));
    assert!(p.confidence > 0.0 && p.confidence <= 0.95);
    assert!(p.estimates.iter().any(|e| e.kind == TestKind::Micro));
}

#[test]
fn nothing_to_pool_is_none() {
    assert!(pool(&[]).is_none());
}

#[test]
fn old_runs_are_dropped_and_the_list_stays_short() {
    let mut runs: Vec<RunRecord> = (0..12).map(|d| run(d * 2, 800.0, 50.0, &CMS)).collect();
    keep_recent(&mut runs, now());
    assert!(runs.iter().all(|r| now() - r.at <= Duration::days(POOL_DAYS)));
    assert!(runs.len() <= 2 * MAX_POOLED);
}

#[test]
fn results_saved_before_pooling_count_as_one_run() {
    let old = r#"{"cm_360":30.0,"confidence":0.5,"psa_cm":30.0,"stat_cm":null,
        "rounds":[],"samples":[],"at":"2026-11-16T12:58:26Z"}"#;
    let r: CalibrationResult = serde_json::from_str(old).unwrap();
    assert_eq!(r.pooled_runs, 1);
}

#[test]
fn a_result_takes_the_pooled_numbers() {
    let (a, b) = (run(0, 800.0, 46.0, &CMS), run(1, 800.0, 54.0, &CMS));
    let p = pool(&[&a, &b]).unwrap();
    let old = r#"{"cm_360":46.0,"confidence":0.5,"psa_cm":46.0,"stat_cm":null,"rounds":[],"samples":[],
        "at":"2026-11-16T12:00:00Z","diagnostics":{"rounds":[],"estimates":[],"conflicts":[],"fit_cm_360":46.0}}"#;
    let mut r: CalibrationResult = serde_json::from_str(old).unwrap();
    r.apply_pool(&p);
    assert_eq!((r.cm_360, r.psa_cm, r.pooled_runs), (p.cm_360, Some(p.cm_360), 2));
    assert_eq!(r.confidence, p.confidence);
    let d = r.diagnostics.unwrap();
    assert_eq!(d.estimates.len(), p.estimates.len());
}

#[test]
fn runs_without_an_answer_are_not_pooled() {
    // Real play: two runs whose flicks said "too slow" everywhere broke every pool they joined.
    let mut lopsided = run(0, 800.0, 50.0, &CMS);
    for row in &mut lopsided.rows {
        *row = (row.0, row.1, 0, row.2 + row.3);
    }
    let runs = vec![lopsided, run(1, 800.0, 46.0, &CMS), run(2, 800.0, 54.0, &CMS)];
    let picked = runs_to_pool(&runs, now(), 800.0);
    assert_eq!(picked.len(), 2);
    assert!(pool(&picked).is_some());
}

/// A real run whose misses barely leaned either way: it came out at ±159%.
fn vague_run(days_ago: i64) -> RunRecord {
    use TestKind::{Flick, Micro, Tracking};
    let rows = vec![
        (Flick, 47.5, 4, 4), (Tracking, 47.5, 1, 1), (Micro, 47.5, 1, 2),
        (Flick, 31.25, 2, 5), (Tracking, 31.25, 0, 1), (Micro, 31.25, 1, 3),
        (Flick, 23.125, 4, 5), (Tracking, 23.125, 2, 0), (Micro, 23.125, 1, 2),
        (Flick, 37.5, 3, 5), (Tracking, 37.5, 1, 1), (Micro, 37.5, 2, 2),
        (Flick, 37.5, 3, 5), (Tracking, 37.5, 0, 2), (Micro, 37.5, 2, 2),
        (Flick, 23.15, 3, 4), (Tracking, 23.15, 2, 0), (Micro, 23.15, 1, 2),
        (Flick, 37.05, 6, 3), (Tracking, 37.05, 1, 0), (Micro, 37.05, 1, 2),
    ];
    RunRecord { at: now() - Duration::days(days_ago), dpi: 800.0, rows, search: SEARCH_CM }
}

#[test]
fn an_answer_wider_than_the_whole_search_is_no_answer() {
    // Real play: a run came out ±159% and dragged a pool of four runs down to 5% confidence.
    assert!(is_answer(0.30));
    assert!(!is_answer(1.59));
    let vague = vague_run(0);
    let alone = pool(&[&vague]).expect("the fit still gives a number");
    assert!(alone.rel_uncertainty > MAX_ANSWER_UNCERTAINTY, "{}", alone.rel_uncertainty);
    let runs = vec![vague, run(1, 800.0, 46.0, &CMS), run(2, 800.0, 54.0, &CMS)];
    assert_eq!(runs_to_pool(&runs, now(), 800.0).len(), 2);
}

#[test]
fn runs_near_each_other_agree_and_far_apart_ones_dont() {
    let (a, b) = (run(0, 800.0, 46.0, &CMS), run(1, 800.0, 54.0, &CMS));
    assert!(runs_agree(&[&a, &b]));
    // Real play: 31 and 52 cm/360 half an hour apart.
    let fast = [31.0, 26.0, 36.0, 33.0, 30.0];
    let (c, d) = (run_with(0, 800.0, 31.0, &fast, 12), run_with(1, 800.0, 52.0, &CMS, 12));
    assert!(!runs_agree(&[&c, &d]));
    assert!(runs_agree(&[&c]));
}

#[test]
fn the_newest_runs_that_agree_are_pooled() {
    let fast = run_with(0, 800.0, 31.0, &[31.0, 26.0, 36.0, 33.0, 30.0], 12);
    let (a, b) = (run_with(1, 800.0, 50.0, &CMS, 12), run_with(2, 800.0, 52.0, &CMS, 12));
    // The newest run disagrees with the rest: it stands alone.
    assert_eq!(newest_agreeing(&[&fast, &a, &b]).len(), 1);
    // An old run that disagrees is left out; the newest ones still pool.
    let picked = newest_agreeing(&[&a, &b, &fast]);
    assert_eq!(picked.len(), 2);
    assert!(picked.iter().all(|r| r.at != fast.at));
    assert!(newest_agreeing(&[]).is_empty());
}

const WIDE_CMS: [f64; 6] = [20.0, 28.0, 36.0, 44.0, 56.0, 75.0];

#[test]
fn pool_clamps_to_newest_run_range() {
    // `run(1, ..)` is newer than `run(2, ..)`: the newest run's pad is the player's current one.
    let mut newest = run(1, 800.0, 40.0, &WIDE_CMS);
    let mut older = run(2, 800.0, 40.0, &WIDE_CMS);
    newest.search = (15.0, 30.0);
    older.search = (15.0, 80.0);
    let p = pool(&[&newest, &older]).unwrap();
    assert_eq!(p.cm_360, 30.0);
    // The order they are passed in doesn't matter.
    assert_eq!(pool(&[&older, &newest]).unwrap().cm_360, 30.0);
}

#[test]
fn pool_may_exceed_an_older_runs_cap() {
    let mut newest = run(1, 800.0, 40.0, &WIDE_CMS);
    let mut older = run(2, 800.0, 40.0, &WIDE_CMS);
    newest.search = (15.0, 80.0);
    older.search = (15.0, 30.0);
    let p = pool(&[&newest, &older]).unwrap();
    assert!(p.cm_360 > 30.0, "{}", p.cm_360);
}

#[test]
fn pool_of_capped_runs_is_capped() {
    let mut a = run(1, 800.0, 40.0, &WIDE_CMS);
    let mut b = run(2, 800.0, 40.0, &WIDE_CMS);
    a.search = (15.0, 30.0);
    b.search = (15.0, 30.0);
    let p = pool(&[&a, &b]).unwrap();
    assert_eq!(p.cm_360, 30.0);
    assert!(p.confidence <= 0.3, "{}", p.confidence);
}

#[test]
fn a_pool_that_is_no_answer_is_not_used() {
    // Real play: 34.7 cm ±19% pooled with 23.1 ±56% and 16.6 ±49% passed the agreement test,
    // but only tracking crossed over in the pool: 7.8 cm ±193%, clamped to the 15 cm edge
    // and shown as the answer under a graph whose every round said ~35 cm.
    let runs: Vec<RunRecord> = serde_json::from_str(include_str!("fixtures/pool_without_an_answer.json")).unwrap();
    let latest = runs.iter().map(|r| r.at).max().unwrap();
    let picked = runs_to_pool(&runs, latest, 400.0);
    assert!(picked.iter().any(|r| r.at == latest), "the newest run has an answer of its own");
    let agreeing = newest_agreeing(&picked);
    assert!(agreeing.len() > 1);
    let pooled = pool(&agreeing).unwrap();
    assert!(!is_answer(pooled.rel_uncertainty), "the pool itself is wider than the search");
    assert_eq!(pooled_answer(&picked), None);
}

#[test]
fn a_pool_that_is_an_answer_is_used() {
    let (a, b) = (run(0, 800.0, 46.0, &CMS), run(1, 800.0, 54.0, &CMS));
    let picked = vec![&a, &b];
    assert_eq!(pooled_answer(&picked), pool(&newest_agreeing(&picked)));
    assert!(pooled_answer(&picked).is_some());
}

#[test]
fn one_run_is_not_a_pool() {
    let a = run(0, 800.0, 50.0, &CMS);
    assert_eq!(pooled_answer(&[&a]), None);
}
