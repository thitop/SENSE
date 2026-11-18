use aim_trainer::data::calibration::*;
use aim_trainer::data::setup::{AimProblem, PlayerSetup};
use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::analysis::*;
use aim_trainer::habits::answers::*;
use aim_trainer::habits::record::*;
use aim_trainer::habits::trust::habit_report;
use aim_trainer::sensitivity::paired::Side;
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

fn track(shake: f32, ctx: PlayContext) -> TrackingRecord {
    TrackingRecord {
        mean_along_deg: 0.0,
        lag_ms: 90.0,
        cross_rms_deg: shake,
        mean_speed_deg_s: 20.0,
        coverage: 0.8,
        radius_deg: 1.0,
        horizontal: true,
        ctx,
    }
}

const BLOCK_CMS: [f32; 7] = [20.0, 26.0, 33.0, 40.0, 47.0, 53.0, 60.0];

fn for_each_group(mut f: impl FnMut(PlayContext)) {
    for (b, cm) in BLOCK_CMS.iter().enumerate() {
        for factor in [0.88f32, 1.12] {
            for _ in 0..3 {
                f(ctx(cm * factor, b as u8));
            }
        }
    }
}

fn result(setup: Option<PlayerSetup>) -> CalibrationResult {
    CalibrationResult {
        cm_360: 40.0,
        confidence: 0.8,
        psa_cm: Some(40.0),
        stat_cm: None,
        rounds: vec![],
        samples: vec![],
        at: Utc::now(),
        diagnostics: None,
        pooled_runs: 1,
        no_answer: false,
        runs_disagree: false,
        habits: None,
        plan: None,
        setup,
        own_cm_at_start: None,
    }
}

fn setup(problems: &[AimProblem]) -> PlayerSetup {
    let mut s = PlayerSetup::not_sure(40.0, Utc::now());
    s.problems = problems.to_vec();
    s
}

/// Shake, corrections, trailing, then Far past: Far past ranks fourth.
fn four_habit_log() -> AimLog {
    let mut log = AimLog::default();
    for_each_group(|c| {
        log.targets.push(flick(FlickBand::Short, 1.0, 6, c));
        log.targets.push(flick(FlickBand::Mid, 1.0, 6, c));
        log.targets.push(flick(FlickBand::Far, 1.30, 6, c));
        log.tracking.push(track(2.0, c));
    });
    log
}

#[test]
fn answer_promotes_a_fourth_ranked_habit() {
    let log = four_habit_log();
    let ranked = ranked_calibration_habits(&log, 40.0);
    assert_eq!(ranked[3].habit, Habit::FlickPast(FlickBand::Far), "{ranked:#?}");
    let r = habit_report(&log, &result(Some(setup(&[AimProblem::Overshoot]))));
    assert_eq!(r.findings.len(), 3);
    assert_eq!(r.findings[0].habit, Habit::FlickPast(FlickBand::Far));
    assert_eq!(
        r.answer_checks,
        vec![AnswerCheck { problem: AimProblem::Overshoot, verdict: AnswerVerdict::Agrees }]
    );
}

#[test]
fn opposite_habit_is_reported() {
    let mut log = AimLog::default();
    for_each_group(|c| log.targets.push(flick(FlickBand::Mid, 0.60, 1, c)));
    let r = habit_report(&log, &result(Some(setup(&[AimProblem::Overshoot]))));
    assert_eq!(r.answer_checks[0].verdict, AnswerVerdict::Opposite);
    assert_eq!(
        badge(Habit::FlickShort(FlickBand::Mid), &r.answer_checks),
        Some(AnswerVerdict::Opposite)
    );
}

#[test]
fn not_seen_only_when_nothing_matches() {
    let mut log = AimLog::default();
    for_each_group(|c| log.targets.push(flick(FlickBand::Mid, 0.60, 1, c)));
    let r =
        habit_report(&log, &result(Some(setup(&[AimProblem::ShakySmall, AimProblem::CantTurnFast]))));
    assert_eq!(r.answer_checks.len(), 1);
    assert_eq!(r.answer_checks[0].verdict, AnswerVerdict::NotSeen);
}

#[test]
fn no_setup_means_no_checks_and_old_order() {
    let log = four_habit_log();
    let r = habit_report(&log, &result(None));
    assert!(r.answer_checks.is_empty());
    assert_eq!(r.findings, calibration_habits(&log, 40.0));
}

#[test]
fn direction_sentences() {
    let ov = [AimProblem::Overshoot];
    let c = direction_checks(&ov, Some(40.0), 45.6);
    assert_eq!(
        direction_sentence(c[0].0, c[0].1),
        "You said you overshoot \u{2014} the test agrees: your result is 14% slower than what you play."
    );
    let c = direction_checks(&ov, Some(40.0), 41.0);
    assert_eq!(c[0].1, DirectionVerdict::Close);
    assert_eq!(
        direction_sentence(c[0].0, c[0].1),
        "You said you overshoot \u{2014} the test didn't: your result is close to what you play now."
    );
    let c = direction_checks(&ov, Some(40.0), 36.4);
    assert_eq!(
        direction_sentence(c[0].0, c[0].1),
        "You said you overshoot \u{2014} the test didn't: your result is 9% faster than what you play."
    );
    assert!(direction_checks(&ov, None, 36.4).is_empty());
}
