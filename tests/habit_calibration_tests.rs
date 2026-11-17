use aim_trainer::data::calibration::*;
use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::analysis::*;
use aim_trainer::habits::record::*;
use aim_trainer::habits::trust::{habit_report, left_out_notes, still_warming_up};
use aim_trainer::habits::HabitReport;
use aim_trainer::sensitivity::combiner::{Signal, TestKind};
use aim_trainer::sensitivity::paired::Side;
use aim_trainer::sensitivity::pooling::Pooled;
use chrono::Utc;

fn ctx(cm: f32, block: u8) -> PlayContext {
    PlayContext { cm_360: cm, side: Some(Side::Faster), block: Some(block), warmup: false }
}

fn flick(band: FlickBand, landing: f32, strokes: u8, ctx: PlayContext) -> TargetRecord {
    TargetRecord {
        kind: AimKind::Flick,
        offset_deg: [10.0, 0.0],
        radius_deg: 1.0,
        first_landing: Some(landing),
        strokes,
        crossed_center: false,
        overshoot_deg: 0.0,
        time_to_hit_ms: None,
        misses: vec![],
        band: Some(band),
        ctx,
    }
}

fn micro(crossed: bool, ctx: PlayContext) -> TargetRecord {
    TargetRecord { kind: AimKind::Micro, crossed_center: crossed, band: None, ..flick(FlickBand::Mid, 1.0, 1, ctx) }
}

fn track(lag: f32, shake: f32, ctx: PlayContext) -> TrackingRecord {
    TrackingRecord {
        mean_along_deg: 0.0,
        lag_ms: lag,
        cross_rms_deg: shake,
        mean_speed_deg_s: 20.0,
        coverage: 0.8,
        radius_deg: 1.0,
        horizontal: true,
        ctx,
    }
}

const BLOCK_CMS: [f32; 7] = [20.0, 26.0, 33.0, 40.0, 47.0, 53.0, 60.0];

/// Every block at both sides (about ±12% of its cm), `per_side` records each.
fn for_each_group(mut f: impl FnMut(PlayContext)) {
    for (b, cm) in BLOCK_CMS.iter().enumerate() {
        for factor in [0.88f32, 1.12] {
            for _ in 0..3 {
                f(ctx(cm * factor, b as u8));
            }
        }
    }
}

fn result(cm: f64, no_answer: bool, diagnostics: Option<RunDiagnostics>) -> CalibrationResult {
    CalibrationResult {
        cm_360: cm,
        confidence: 0.8,
        psa_cm: Some(cm),
        stat_cm: None,
        rounds: vec![],
        samples: vec![],
        at: Utc::now(),
        diagnostics,
        pooled_runs: 1,
        no_answer,
        runs_disagree: false,
        habits: None,
    }
}

#[test]
fn habit_at_result_is_named() {
    let mut log = AimLog::default();
    for_each_group(|c| log.targets.push(flick(FlickBand::Far, 0.78, 1, c)));
    let f = calibration_habits(&log, 40.0);
    let found = f.iter().find(|f| f.habit == Habit::FlickShort(FlickBand::Far)).expect("named");
    assert!((found.value - 0.78).abs() < 1e-3);
}

#[test]
fn sensitivity_lean_is_not_a_habit() {
    let mut log = AimLog::default();
    for_each_group(|c| {
        let landing = 1.0 + 0.6 * (c.cm_360 / 35.0).ln();
        log.targets.push(flick(FlickBand::Far, landing, 1, c));
    });
    let f = calibration_habits(&log, 35.0);
    assert!(f.iter().all(|f| f.habit != Habit::FlickShort(FlickBand::Far)), "{f:?}");
    // The same play read at a result far below it would be short.
    let low = calibration_habits(&log, 20.0);
    assert!(low.iter().any(|f| f.habit == Habit::FlickShort(FlickBand::Far)));
}

#[test]
fn no_answer_names_nothing() {
    let mut log = AimLog::default();
    for_each_group(|c| log.targets.push(flick(FlickBand::Far, 0.78, 1, c)));
    let r = habit_report(&log, &result(40.0, true, None));
    assert!(r.no_answer);
    assert!(r.findings.is_empty() && r.left_out.is_empty());
    let ok = habit_report(&log, &result(40.0, false, None));
    assert!(!ok.no_answer && !ok.findings.is_empty());
}

#[test]
fn needs_three_blocks() {
    let mut log = AimLog::default();
    for i in 0..20 {
        log.targets.push(flick(FlickBand::Far, 0.5, 1, ctx(30.0 + (i % 2) as f32 * 5.0, (i % 2) as u8)));
    }
    assert!(calibration_habits(&log, 32.0).is_empty());
    log.targets.push(flick(FlickBand::Far, 0.5, 1, ctx(40.0, 2)));
    assert!(!calibration_habits(&log, 32.0).is_empty());
}

#[test]
fn warm_up_records_are_not_counted() {
    let mut log = AimLog::default();
    for_each_group(|c| {
        log.targets.push(flick(FlickBand::Far, 0.5, 1, PlayContext { warmup: true, block: None, ..c }))
    });
    assert!(calibration_habits(&log, 40.0).is_empty());
}

#[test]
fn at_most_three_ranked() {
    let mut log = AimLog::default();
    for_each_group(|c| {
        log.targets.push(flick(FlickBand::Short, 0.80, 1, c));
        log.targets.push(flick(FlickBand::Mid, 0.60, 1, c));
        log.targets.push(flick(FlickBand::Far, 0.40, 6, c));
        log.targets.push(micro(true, c));
        log.tracking.push(track(90.0, 2.0, c));
    });
    let f = calibration_habits(&log, 40.0);
    assert_eq!(f.len(), 3);
    let sev: Vec<f32> = f.iter().map(|f| f.habit.severity(f.value).unwrap()).collect();
    assert!(sev[0] >= sev[1] && sev[1] >= sev[2], "{sev:?}");
    assert!(f.iter().all(|f| f.value.is_finite()));
}

/// Four pairs per test per round; cost is a U in ln cm, optionally falling with the round.
fn rounds(cms: &[f64], fall_per_round: f64) -> Vec<RoundSignals> {
    cms.iter()
        .enumerate()
        .map(|(i, &cm)| {
            let u = 1.0 + 0.8 * (cm / 35.0).ln().powi(2);
            let cost = u * (1.0 - fall_per_round * i as f64);
            let mut pairs = Vec::new();
            for test in [TestKind::Flick.label(), TestKind::Micro.label(), TestKind::Tracking.label()] {
                for k in 0..4 {
                    let wobble = 1.0 + 0.01 * ((i + k) % 3) as f64;
                    pairs.push(PairRecord {
                        test: test.into(),
                        band: (test == TestKind::Flick.label()).then(|| "mid".to_string()),
                        faster_cost: cost * wobble,
                        slower_cost: cost * wobble * 1.1,
                        signal: Signal::TooFast,
                    });
                }
            }
            RoundSignals { tested_cm_360: cm, signals: Default::default(), pairs }
        })
        .collect()
}

const FAR_TO_NEAR: [f64; 8] = [62.0, 50.0, 41.0, 30.0, 26.0, 33.0, 36.0, 34.0];

#[test]
fn warm_up_not_confused_with_search() {
    assert!(!still_warming_up(&rounds(&FAR_TO_NEAR, 0.0)));
}

#[test]
fn warm_up_detected() {
    // 25% fall from the first round to the last.
    assert!(still_warming_up(&rounds(&FAR_TO_NEAR, 0.25 / 7.0)));
}

#[test]
fn few_rounds_never_warm_up() {
    assert!(!still_warming_up(&rounds(&FAR_TO_NEAR[..5], 0.1)));
    assert!(!still_warming_up(&[]));
}

fn diag(left_out: &[&str], estimate: f64) -> RunDiagnostics {
    RunDiagnostics {
        rounds: rounds(&[30.0, 40.0, 50.0], 0.0),
        estimates: vec![EstimateRecord { test: TestKind::Tracking.label().into(), cm_360: estimate, rel_uncertainty: 0.1 }],
        left_out: left_out.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn finding(habit: Habit) -> HabitFinding {
    HabitFinding { habit, value: 1.0, direction: None }
}

#[test]
fn left_out_note_needs_matching_lean() {
    let d = diag(&["Tracking"], 60.0); // above everything tested: leaned TooFast
    assert!(left_out_notes(&d, &[finding(Habit::TrackingTrails)]).is_empty());
    let notes = left_out_notes(&d, &[finding(Habit::FlickShort(FlickBand::Far)), finding(Habit::TrackingAhead)]);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].test, "Tracking");
    assert_eq!(notes[0].habit, Habit::TrackingAhead);
    // Below the tested range it leaned TooSlow.
    let d = diag(&["Tracking"], 20.0);
    assert_eq!(left_out_notes(&d, &[finding(Habit::TrackingTrails)]).len(), 1);
    // Not left out: no note.
    assert!(left_out_notes(&diag(&[], 60.0), &[finding(Habit::TrackingAhead)]).is_empty());
}

#[test]
fn report_uses_unpooled_left_out() {
    let mut log = AimLog::default();
    for_each_group(|c| log.tracking.push(track(-80.0, 0.1, c)));
    let mut r = result(40.0, false, Some(diag(&["Tracking"], 60.0)));
    let report = habit_report(&log, &r);
    assert_eq!(report.left_out.len(), 1, "{report:?}");
    r.habits = Some(report.clone());
    r.apply_pool(&Pooled {
        cm_360: 38.0,
        rel_uncertainty: 0.1,
        confidence: 0.9,
        estimates: vec![],
        conflicts: vec![],
        left_out: vec![],
        runs: 2,
    });
    assert!(r.diagnostics.as_ref().unwrap().left_out.is_empty());
    assert_eq!(r.habits, Some(report));
}

#[test]
fn old_profile_loads_without_habits() {
    let json = serde_json::to_string(&result(40.0, false, None)).unwrap();
    assert!(!json.contains("habits"));
    let back: CalibrationResult = serde_json::from_str(&json).unwrap();
    assert!(back.habits.is_none());
    let mut with = result(40.0, false, None);
    with.habits = Some(HabitReport { warming_up: true, ..Default::default() });
    let back: CalibrationResult = serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
    assert_eq!(back.habits, with.habits);
}

#[test]
fn real_labels_drive_warm_up_and_left_out() {
    // The labels diagnostics really carry: "Flicking", "Micro-adjustment", "Tracking".
    assert_eq!(Habit::FlickShort(FlickBand::Far).test_label(), TestKind::Flick.label());
    assert_eq!(Habit::MicroCreep.test_label(), TestKind::Micro.label());
    let r = rounds(&FAR_TO_NEAR, 0.25 / 7.0);
    assert!(r[0].pairs.iter().any(|p| p.test == "Flicking") && r[0].pairs.iter().any(|p| p.test == "Micro-adjustment"));
    // Only Flicking and Micro-adjustment fall: warm-up still fires without Tracking.
    let only_two: Vec<RoundSignals> = r
        .into_iter()
        .map(|mut x| {
            x.pairs.retain(|p| p.test != "Tracking");
            x
        })
        .collect();
    assert!(still_warming_up(&only_two));
    let d = RunDiagnostics {
        rounds: rounds(&[30.0, 40.0, 50.0], 0.0),
        estimates: vec![EstimateRecord { test: "Flicking".into(), cm_360: 60.0, rel_uncertainty: 0.1 }],
        left_out: vec!["Flicking".into()],
        ..Default::default()
    };
    let notes = left_out_notes(&d, &[finding(Habit::FlickPast(FlickBand::Far))]);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].test, "Flicking");
}

#[test]
fn a_measure_needs_its_own_sixteen_records() {
    let mut log = AimLog::default();
    let mut n = 0;
    for_each_group(|c| {
        log.targets.push(flick(FlickBand::Far, 0.78, 1, c)); // 42 far flicks
        n += 1;
        if n <= 12 {
            log.tracking.push(track(90.0, 0.1, c)); // only 12 tracking records
        }
    });
    let f = calibration_habits(&log, 40.0);
    assert!(f.iter().any(|f| f.habit == Habit::FlickShort(FlickBand::Far)));
    assert!(f.iter().all(|f| f.habit != Habit::TrackingTrails), "{f:?}");
}

#[test]
fn micro_overshoot_not_named_from_two_records() {
    let mut log = AimLog::default();
    for_each_group(|c| log.targets.push(flick(FlickBand::Far, 1.0, 1, c)));
    log.targets.push(micro(true, ctx(40.0, 3)));
    log.targets.push(micro(true, ctx(40.0, 3)));
    let f = calibration_habits(&log, 40.0);
    assert!(f.iter().all(|f| f.habit != Habit::MicroOvershoot), "{f:?}");
}

#[test]
fn miss_lean_needs_ten_misses() {
    let mut log = AimLog::default();
    let mut k = 0;
    for_each_group(|c| {
        let mut r = flick(FlickBand::Far, 1.0, 1, c);
        if k < 3 {
            r.misses = vec![[3.0, 0.0], [3.0, 0.0]]; // 6 misses, all leaning
        }
        k += 1;
        log.targets.push(r);
    });
    assert!(calibration_habits(&log, 40.0).iter().all(|f| f.habit != Habit::MissLeanFlick));
    let mut log2 = log.clone();
    for r in log2.targets.iter_mut().take(14) {
        r.misses = vec![[3.0, 0.0], [3.0, 0.0]]; // 28 misses over three blocks
    }
    assert!(calibration_habits(&log2, 40.0).iter().any(|f| f.habit == Habit::MissLeanFlick));
}
