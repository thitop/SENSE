//! Multi-test calibration: per-test signals, the inverse-variance combiner and the block search.

use aim_trainer::sensitivity::blocks::{block_plan, warmup_plan, Amount, BlockScheduler};
use aim_trainer::sensitivity::combiner::{
    recommend, DirectionalSignal, EstimateSet, Signal, TestEstimate, TestKind, WeightSet,
};
use aim_trainer::sensitivity::joint_fit::fit_joint;
use aim_trainer::sensitivity::path_recorder::{ShotMovementPath, ShotTrajectoryPattern};
use aim_trainer::sensitivity::psa::{Crossover, PsaCalibrator};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::time::Instant;

fn est(kind: TestKind, cm: f64, rel: f64) -> TestEstimate {
    TestEstimate { kind, crossover: Crossover { cm_360: cm, rel_uncertainty: rel } }
}

fn combine(e: &[TestEstimate], w: &WeightSet) -> Option<aim_trainer::sensitivity::combiner::Combined> {
    EstimateSet::from_independent(e).combine(w)
}

fn conflicts(e: &[TestEstimate], w: &WeightSet) -> Vec<(TestKind, TestKind)> {
    EstimateSet::from_independent(e).conflicts(w)
}

// ---------- Signals ----------

#[test]
fn shot_patterns_map_to_signals() {
    assert_eq!(Signal::from(ShotTrajectoryPattern::Overshoot), Signal::TooFast);
    assert_eq!(Signal::from(ShotTrajectoryPattern::Undershoot), Signal::TooSlow);
    assert_eq!(Signal::from(ShotTrajectoryPattern::Direct), Signal::Direct);
}

#[test]
fn unclassified_shot_has_no_signal() {
    let now = Instant::now();
    let path = ShotMovementPath::from_samples(Vec::new(), now, now);
    assert_eq!(path.signal(), None);
}

// ---------- Combiner ----------

#[test]
fn one_test_combines_to_itself() {
    let c = combine(&[est(TestKind::Flick, 35.0, 0.2)], &WeightSet::neutral()).unwrap();
    assert!((c.cm_360 - 35.0).abs() < 1e-9);
    assert!((c.rel_uncertainty - 0.2).abs() < 1e-9);
}

#[test]
fn tighter_estimate_pulls_harder() {
    let w = WeightSet::neutral();
    let c = combine(&[est(TestKind::Flick, 30.0, 0.05), est(TestKind::Tracking, 45.0, 0.4)], &w).unwrap();
    assert!(c.cm_360 > 30.0 && c.cm_360 < 31.0, "{}", c.cm_360);
    // Two estimates together are tighter than the better one alone.
    assert!(c.rel_uncertainty < 0.05);
}

#[test]
fn equal_estimates_average_in_log_space() {
    let w = WeightSet::neutral();
    let c = combine(&[est(TestKind::Flick, 20.0, 0.2), est(TestKind::Tracking, 45.0, 0.2)], &w).unwrap();
    assert!((c.cm_360 - 30.0).abs() < 1e-9, "{}", c.cm_360); // sqrt(20 * 45)
}

#[test]
fn lower_weight_tests_count_less() {
    let w = WeightSet::neutral();
    assert!(w.get(TestKind::Placement) < w.get(TestKind::Recoil));
    assert!(w.get(TestKind::Recoil) < w.get(TestKind::Flick));
    let c = combine(&[est(TestKind::Flick, 30.0, 0.2), est(TestKind::Placement, 50.0, 0.2)], &w).unwrap();
    assert!(c.cm_360 < (30.0f64 * 50.0).sqrt(), "{}", c.cm_360);
}

#[test]
fn strafe_never_counts() {
    let w = WeightSet::neutral();
    assert_eq!(w.get(TestKind::Strafe), 0.0);
    assert!(combine(&[est(TestKind::Strafe, 30.0, 0.1)], &w).is_none());
    let c = combine(&[est(TestKind::Flick, 40.0, 0.3), est(TestKind::Strafe, 15.0, 0.01)], &w).unwrap();
    assert!((c.cm_360 - 40.0).abs() < 1e-9);
}

#[test]
fn no_estimates_no_result() {
    assert!(combine(&[], &WeightSet::neutral()).is_none());
}

#[test]
fn far_apart_tests_are_reported_as_conflicting() {
    let w = WeightSet::neutral();
    let tight = [est(TestKind::Flick, 28.0, 0.1), est(TestKind::Tracking, 41.0, 0.1)];
    assert_eq!(conflicts(&tight, &w), vec![(TestKind::Flick, TestKind::Tracking)]);
    // Same values with wide intervals: the gap may be noise.
    let loose = [est(TestKind::Flick, 28.0, 0.5), est(TestKind::Tracking, 41.0, 0.5)];
    assert!(conflicts(&loose, &w).is_empty());
    // A diagnostic-only test never conflicts.
    let strafe = [est(TestKind::Flick, 28.0, 0.1), est(TestKind::Strafe, 60.0, 0.1)];
    assert!(conflicts(&strafe, &w).is_empty());
}

#[test]
fn preset_leans_the_result_but_stays_near_the_neutral_value() {
    let estimates = [est(TestKind::Flick, 28.0, 0.15), est(TestKind::Tracking, 41.0, 0.15)];
    let tracking_heavy = WeightSet { tracking: 5.0, ..WeightSet::neutral() };
    let r = recommend(&EstimateSet::from_independent(&estimates), Some(&tracking_heavy)).unwrap();
    let preset = r.preset.unwrap();
    assert!(preset.cm_360 > r.neutral.cm_360, "preset should lean toward tracking");
    let sigma = r.neutral.rel_uncertainty / 1.96;
    let gap = (preset.cm_360 / r.neutral.cm_360).ln().abs();
    assert!(gap <= sigma + 1e-9, "gap {gap} > sigma {sigma}");
    assert_eq!(r.chosen().cm_360, preset.cm_360);
}

#[test]
fn preset_that_ignores_every_test_falls_back_to_neutral() {
    let estimates = [est(TestKind::Flick, 30.0, 0.2)];
    let nothing = WeightSet { placement: 0.0, flick: 0.0, tracking: 0.0, micro: 0.0, recoil: 0.0 };
    let r = recommend(&EstimateSet::from_independent(&estimates), Some(&nothing)).unwrap();
    assert!(r.preset.is_none());
    assert!((r.chosen().cm_360 - 30.0).abs() < 1e-9);
}

// ---------- Block plan ----------

#[test]
fn block_starts_with_uncounted_adaptation_shots() {
    let plan = block_plan(&mut StdRng::seed_from_u64(1));
    assert_eq!(plan[0].kind, TestKind::Flick);
    assert_eq!(plan[0].amount, Amount::Shots(2));
    assert!(!plan[0].counted);
    assert!(plan[1..].iter().all(|s| s.counted));
}

#[test]
fn block_has_every_scored_test_once_and_no_strafe() {
    let plan = block_plan(&mut StdRng::seed_from_u64(2));
    let amount = |k| plan[1..].iter().filter(|s| s.kind == k).map(|s| s.amount).collect::<Vec<_>>();
    assert_eq!(amount(TestKind::Flick), vec![Amount::Shots(18)]);
    assert_eq!(amount(TestKind::Micro), vec![Amount::Shots(8)]);
    assert_eq!(amount(TestKind::Tracking), vec![Amount::Seconds(12.0)]);
    assert_eq!(amount(TestKind::Recoil), vec![Amount::Sprays(1)]);
    assert_eq!(amount(TestKind::Placement), vec![Amount::Peeks(4)]);
    assert!(amount(TestKind::Strafe).is_empty());
}

#[test]
fn block_order_is_shuffled() {
    let order = |seed| block_plan(&mut StdRng::seed_from_u64(seed)).iter().map(|s| s.kind).collect::<Vec<_>>();
    let first = order(0);
    assert!((1..20).any(|s| order(s) != first));
}

#[test]
fn warmup_counts_nothing() {
    let plan = warmup_plan();
    assert!(!plan.is_empty());
    assert!(plan.iter().all(|s| !s.counted));
}

// ---------- Simulated players ----------

/// Same model as the flick-only simulation: faster than `pref` leans misses toward "too fast".
fn simulated_signal(rng: &mut StdRng, pref: f64, cm: f64, clarity: f64) -> Signal {
    let mismatch = (pref / cm).ln();
    let p_direct = 0.45 * (-(mismatch * mismatch) / (2.0 * 0.25f64.powi(2))).exp() + 0.15;
    let p_fast_given_miss = 1.0 / (1.0 + (-clarity * mismatch).exp());
    if rng.gen::<f64>() < p_direct {
        Signal::Direct
    } else if rng.gen::<f64>() < p_fast_given_miss {
        Signal::TooFast
    } else {
        Signal::TooSlow
    }
}

/// Preferred cm/360 per test and how many signals each test gives per block.
struct Player {
    prefs: [(TestKind, f64); 5],
}

impl Player {
    fn uniform(pref: f64) -> Self {
        Self::split(pref, pref)
    }
    /// Flick and micro like `fast`, tracking likes `slow`, the rest sit between.
    fn split(fast: f64, slow: f64) -> Self {
        let mid = (fast * slow).sqrt();
        Self {
            prefs: [
                (TestKind::Flick, fast),
                (TestKind::Micro, fast),
                (TestKind::Tracking, slow),
                (TestKind::Recoil, mid),
                (TestKind::Placement, mid),
            ],
        }
    }
}

/// Signals per block and how clear each test's signal is. Tracking windows are noisier
/// than shots, placement is mostly game sense.
fn per_block(kind: TestKind) -> (usize, f64) {
    match kind {
        TestKind::Flick => (9, 4.0),
        TestKind::Micro => (6, 4.0),
        TestKind::Tracking => (16, 1.5),
        TestKind::Recoil => (4, 4.0),
        TestKind::Placement => (4, 1.5),
        TestKind::Strafe => (0, 0.0),
    }
}

fn run_blocks(rng: &mut StdRng, player: &Player) -> BlockScheduler {
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    while !s.is_completed {
        let cm = s.current_cm_360;
        for &(kind, pref) in &player.prefs {
            let (n, clarity) = per_block(kind);
            for _ in 0..n {
                s.record(kind, simulated_signal(rng, pref, cm, clarity));
            }
        }
        s.end_block();
    }
    s
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn rel_err(r: f64, pref: f64) -> f64 {
    (r - pref).abs() / pref * 100.0
}

fn flick_block(s: &mut BlockScheduler, signal: Signal) {
    for _ in 0..9 {
        s.record(TestKind::Flick, signal);
    }
    s.end_block();
}

#[test]
fn start_hint_is_ignored() {
    let hinted = BlockScheduler::with_start(15.0, 80.0, Some(28.0), 7);
    assert_eq!(hinted.current_cm_360, 47.5);
    let mut s = BlockScheduler::with_start(15.0, 80.0, Some(28.0), 7);
    flick_block(&mut s, Signal::TooFast);
    assert!((s.current_cm_360 - 63.75).abs() < 1e-9);
}

#[test]
fn blocks_land_near_the_preferred_sensitivity() {
    let mut rng = StdRng::seed_from_u64(7);
    let medians: Vec<(f64, f64)> = [20.0, 30.0, 40.0, 55.0, 70.0]
        .into_iter()
        .map(|pref| {
            let errs = (0..200)
                .map(|_| rel_err(run_blocks(&mut rng, &Player::uniform(pref)).recommended_cm_360.unwrap(), pref))
                .collect();
            (pref, median(errs))
        })
        .collect();
    println!("median error by preferred cm/360: {medians:.1?}");
    assert!(medians.iter().all(|&(_, m)| m <= 7.0), "median errors: {medians:.1?}");
}

#[test]
fn blocks_beat_flick_only_calibration() {
    let mut rng = StdRng::seed_from_u64(8);
    let (mut multi, mut flick) = (Vec::new(), Vec::new());
    for pref in [20.0, 30.0, 40.0, 55.0, 70.0] {
        for _ in 0..200 {
            multi.push(rel_err(run_blocks(&mut rng, &Player::uniform(pref)).recommended_cm_360.unwrap(), pref));
            let mut psa = PsaCalibrator::new(15.0, 80.0, 7);
            while !psa.is_completed {
                let cm = psa.current_test_cm_360;
                let shot = match simulated_signal(&mut rng, pref, cm, 4.0) {
                    Signal::TooFast => ShotTrajectoryPattern::Overshoot,
                    Signal::TooSlow => ShotTrajectoryPattern::Undershoot,
                    Signal::Direct => ShotTrajectoryPattern::Direct,
                };
                psa.record_shot(shot);
            }
            flick.push(rel_err(psa.recommended_cm_360.unwrap(), pref));
        }
    }
    let (m, f) = (median(multi), median(flick));
    println!("median error: blocks {m:.1}% vs flick only {f:.1}%");
    assert!(m < f, "blocks {m:.1}% vs flick only {f:.1}%");
}

/// Share of runs (as a fraction) where the player lands between `fast` and `slow`, and where
/// flick and tracking are reported as disagreeing.
fn split_runs(rng: &mut StdRng, fast: f64, slow: f64, runs: usize) -> (f64, f64) {
    let (mut between, mut flagged) = (0, 0);
    for _ in 0..runs {
        let s = run_blocks(rng, &Player::split(fast, slow));
        if (fast..=slow).contains(&s.recommended_cm_360.unwrap()) {
            between += 1;
        }
        if s.recommendation(None).is_some_and(|r| r.conflicts.contains(&(TestKind::Flick, TestKind::Tracking))) {
            flagged += 1;
        }
    }
    (between as f64 / runs as f64, flagged as f64 / runs as f64)
}

#[test]
fn split_player_lands_between_and_is_told_about_it() {
    let (between, flagged) = split_runs(&mut StdRng::seed_from_u64(9), 25.0, 50.0, 300);
    println!("25/50: between {between:.2}, flagged {flagged:.2}");
    assert!(between >= 0.9, "between {between:.2}");
    assert!(flagged >= 0.85, "flagged {flagged:.2}");
}

#[test]
fn players_whose_tests_agree_are_rarely_told_otherwise() {
    let mut rng = StdRng::seed_from_u64(13);
    let runs = 300;
    let flagged = (0..runs)
        .filter(|_| {
            let s = run_blocks(&mut rng, &Player::uniform(35.0));
            s.recommendation(None).is_some_and(|r| !r.conflicts.is_empty())
        })
        .count();
    println!("false conflicts: {flagged}/{runs}");
    assert!(flagged as f64 <= runs as f64 * 0.05, "{flagged}/{runs}");
}

/// Tracking leans "too slow" at every sensitivity in the range (as if its preference sat far
/// below it): a biased test, not a preference. It must not drag the result away from what
/// every other test agrees on.
fn biased_tracking(pref: f64) -> Player {
    Player {
        prefs: [
            (TestKind::Flick, pref),
            (TestKind::Micro, pref),
            (TestKind::Tracking, 6.0),
            (TestKind::Recoil, pref),
            (TestKind::Placement, pref),
        ],
    }
}

#[test]
fn a_test_biased_one_way_is_left_out() {
    let mut rng = StdRng::seed_from_u64(14);
    for pref in [30.0, 45.0] {
        let (mut kept, mut all, mut excluded) = (Vec::new(), Vec::new(), 0);
        for _ in 0..200 {
            let s = run_blocks(&mut rng, &biased_tracking(pref));
            kept.push(rel_err(s.recommended_cm_360.unwrap(), pref));
            all.push(rel_err(s.fit_cm_360.or(s.recommended_cm_360).unwrap(), pref));
            if s.excluded_tests() == vec![TestKind::Tracking] {
                excluded += 1;
            }
        }
        let (k, a) = (median(kept), median(all));
        println!("biased tracking, pref {pref}: result {k:.1}% vs every test {a:.1}%, tracking left out {excluded}/200");
        // Leaving it out never makes things worse by more than noise.
        assert!(k <= 10.0 && k <= a + 0.5, "pref {pref}: {k:.1}% vs {a:.1}%");
    }
}

#[test]
fn unbiased_tests_are_rarely_left_out() {
    let mut rng = StdRng::seed_from_u64(15);
    let left_out = (0..300)
        .filter(|_| !run_blocks(&mut rng, &Player::uniform(40.0)).excluded_tests().is_empty())
        .count();
    println!("unbiased runs with a test left out: {left_out}/300");
    assert!(left_out <= 30, "{left_out}/300");
}

#[test]
fn per_test_estimates_follow_each_test() {
    let mut rng = StdRng::seed_from_u64(10);
    let s = run_blocks(&mut rng, &Player::split(25.0, 50.0));
    let get = |k| s.estimates().into_iter().find(|e| e.kind == k).map(|e| e.crossover.cm_360);
    let (flick, tracking) = (get(TestKind::Flick).unwrap(), get(TestKind::Tracking).unwrap());
    assert!(flick < tracking, "flick {flick} tracking {tracking}");
}

#[test]
fn block_results_are_kept_per_test() {
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    s.record(TestKind::Flick, Signal::TooFast);
    s.record(TestKind::Flick, Signal::TooFast);
    s.record(TestKind::Tracking, Signal::TooSlow);
    s.record(TestKind::Micro, Signal::Direct);
    s.end_block();
    let b = &s.history[0];
    assert!((b.tested_cm_360 - 47.5).abs() < 1e-9);
    assert_eq!(b.counts(TestKind::Flick).too_fast, 2);
    assert_eq!(b.counts(TestKind::Tracking).too_slow, 1);
    assert_eq!(b.counts(TestKind::Micro).direct, 1);
    assert_eq!(s.block_number, 2);
}

#[test]
fn block_direction_follows_the_weighted_misses() {
    use aim_trainer::sensitivity::psa::PsaRoundConclusion::*;
    // One flick overshoot outweighs one placement undershoot (placement counts less).
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    s.record(TestKind::Flick, Signal::TooFast);
    s.record(TestKind::Placement, Signal::TooSlow);
    assert_eq!(s.end_block(), TooFastNeedSlower);
    assert!((s.low_cm_360 - 47.5).abs() < 1e-9);

    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    s.record(TestKind::Tracking, Signal::TooSlow);
    assert_eq!(s.end_block(), TooSlowNeedFaster);
    assert!((s.high_cm_360 - 47.5).abs() < 1e-9);

    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    s.record(TestKind::Flick, Signal::Direct);
    s.record(TestKind::Strafe, Signal::TooFast); // diagnostic only: no say in the direction
    assert_eq!(s.end_block(), BalancedNarrow);
}

#[test]
fn recommendation_stays_in_the_search_range_and_confidence_is_sane() {
    let mut rng = StdRng::seed_from_u64(11);
    for pref in [10.0, 120.0] {
        let s = run_blocks(&mut rng, &Player::uniform(pref));
        let r = s.recommended_cm_360.unwrap();
        assert!((15.0..=80.0).contains(&r), "{r}");
        assert!(s.confidence() <= 0.3, "edge answer should be low confidence: {}", s.confidence());
    }
    let s = run_blocks(&mut rng, &Player::uniform(35.0));
    assert!(s.confidence() >= 0.5, "{}", s.confidence());
}

// ---------- Joint fit ----------

#[test]
fn joint_fit_finds_each_tests_balance_point() {
    // Flick balances at sqrt(20*45)=30; tracking's misses are shifted slower.
    let rows = [
        (TestKind::Flick, 20.0, 8, 2),
        (TestKind::Flick, 45.0, 2, 8),
        (TestKind::Tracking, 20.0, 9, 1),
        (TestKind::Tracking, 45.0, 6, 4),
        (TestKind::Tracking, 70.0, 2, 8),
    ];
    let set = fit_joint(&rows).unwrap();
    let get = |k| set.estimates().into_iter().find(|e| e.kind == k).unwrap().crossover.cm_360;
    let (f, t) = (get(TestKind::Flick), get(TestKind::Tracking));
    assert!(f > 24.0 && f < 36.0, "flick {f}");
    assert!(t > f, "tracking {t} should be slower than flick {f}");
}

#[test]
fn joint_fit_keeps_a_test_with_one_sided_misses() {
    // Micro only ever missed "too fast": alone it has no fit, jointly it still gets an estimate.
    let rows = [
        (TestKind::Flick, 20.0, 8, 2),
        (TestKind::Flick, 45.0, 2, 8),
        (TestKind::Micro, 20.0, 3, 0),
        (TestKind::Micro, 45.0, 1, 0),
    ];
    let set = fit_joint(&rows).unwrap();
    assert!(set.estimates().iter().any(|e| e.kind == TestKind::Micro));
}

#[test]
fn joint_fit_needs_misses_both_ways() {
    assert!(fit_joint(&[(TestKind::Flick, 20.0, 5, 0), (TestKind::Flick, 40.0, 3, 0)]).is_none());
    assert!(fit_joint(&[(TestKind::Flick, 30.0, 5, 5)]).is_none());
    assert!(fit_joint(&[]).is_none());
}

// ---------- Tests whose misses lean at different rates ----------

/// Expected counts from P(too fast | miss) = σ(−slope · ln(cm / at)), 200 misses per point.
fn leaning_rows(kind: TestKind, at: f64, slope: f64, cms: &[f64]) -> Vec<(TestKind, f64, usize, usize)> {
    cms.iter()
        .map(|&cm| {
            let p = 1.0 / (1.0 + (slope * (cm / at).ln()).exp());
            let f = (200.0 * p).round() as usize;
            (kind, cm, f, 200 - f)
        })
        .collect()
}

#[test]
fn a_shallow_test_keeps_its_own_crossover_next_to_a_steep_one() {
    // Paired micro leans gently, flick steeply; tested points sit mostly on the slow side.
    let cms = [22.0, 26.0, 30.0, 34.0, 40.0, 48.0, 58.0];
    let mut rows = leaning_rows(TestKind::Flick, 30.0, 6.0, &cms);
    rows.extend(leaning_rows(TestKind::Micro, 30.0, 1.5, &cms));
    let set = fit_joint(&rows).unwrap();
    for e in set.estimates() {
        assert!((e.crossover.cm_360 / 30.0 - 1.0).abs() < 0.03, "{:?}: {:.1}", e.kind, e.crossover.cm_360);
    }
}

#[test]
fn a_test_without_a_trend_never_gets_an_absurd_estimate() {
    // Found by a random scan in review: micro pairs with no real lean next to clean flicks
    // gave micro ≈ 18 800 cm/360 (other draws gave 0.06 cm, and infinity, which serde saves
    // as null and the profile then fails to load).
    use TestKind::{Flick as F, Micro as M};
    let rows = [
        (F, 47.5, 2, 4), (M, 47.5, 3, 1), (F, 31.25, 3, 3), (M, 31.25, 4, 0), (F, 39.0, 3, 3), (M, 39.0, 2, 2),
        (F, 35.0, 3, 3), (M, 35.0, 3, 1), (F, 33.0, 2, 4), (M, 33.0, 0, 4), (F, 34.0, 2, 4), (M, 34.0, 3, 1),
        (F, 34.5, 2, 4), (M, 34.5, 3, 1),
    ];
    let set = fit_joint(&rows).unwrap();
    for e in set.estimates() {
        let (cm, u) = (e.crossover.cm_360, e.crossover.rel_uncertainty);
        assert!(cm.is_finite() && u.is_finite(), "{:?}: {cm} ±{u}", e.kind);
        assert!((5.0..=300.0).contains(&cm), "{:?}: {cm:.1}", e.kind);
    }
}

#[test]
fn one_pair_does_not_outvote_a_round_of_flicks() {
    // Real play: a single tracking pair carried as much say as nine flicks and pushed the
    // first, range-setting rounds the wrong way.
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    for sig in [Signal::TooFast, Signal::TooFast, Signal::TooFast, Signal::TooFast, Signal::TooSlow, Signal::TooSlow] {
        s.record(TestKind::Flick, sig);
    }
    s.record(TestKind::Tracking, Signal::TooSlow);
    assert_eq!(s.end_block(), aim_trainer::sensitivity::psa::PsaRoundConclusion::TooFastNeedSlower);
}
