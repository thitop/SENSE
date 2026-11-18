//! The block scheduler records, for every round, the range it started from, its best guess
//! afterwards and how the next sensitivity was picked.

use aim_trainer::data::{CalibrationResult, RunDiagnostics};
use aim_trainer::game::calibration_run::CalibrationRun;
use aim_trainer::sensitivity::paired::PairResult;
use aim_trainer::sensitivity::pooling::Pooled;
use aim_trainer::sensitivity::blocks::{BlockScheduler, NextPick, RoundGuess};
use aim_trainer::sensitivity::combiner::{Signal, TestKind};
use rand::{rngs::StdRng, Rng, SeedableRng};

const LOW: f64 = 15.0;
const HIGH: f64 = 80.0;
/// `BlockScheduler::FIT_REACH` (private): the fit may go this far past the bounds.
const FIT_REACH: f64 = 0.20;

/// Same model as the simulated players in `multi_test_calibration_tests.rs`.
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

/// A simulated player who prefers `pref` at every test.
fn run_blocks(rng: &mut StdRng, pref: f64) -> BlockScheduler {
    let mut s = BlockScheduler::new(LOW, HIGH, 7);
    while !s.is_completed {
        let cm = s.current_cm_360;
        for kind in [TestKind::Flick, TestKind::Micro, TestKind::Tracking, TestKind::Recoil, TestKind::Placement] {
            let (n, clarity) = per_block(kind);
            for _ in 0..n {
                s.record(kind, simulated_signal(rng, pref, cm, clarity));
            }
        }
        s.end_block();
    }
    s
}

/// Every block says the same thing about every test.
fn run_all(signal: Signal, blocks: usize) -> BlockScheduler {
    let mut s = BlockScheduler::new(LOW, HIGH, 7);
    for _ in 0..blocks {
        for _ in 0..6 {
            s.record(TestKind::Flick, signal);
        }
        s.record(TestKind::Flick, if signal == Signal::TooFast { Signal::TooSlow } else { Signal::TooFast });
        s.end_block();
    }
    s
}

fn next_cm(next: NextPick) -> Option<f64> {
    match next {
        NextPick::Midpoint { cm_360 }
        | NextPick::FitEstimate { cm_360 }
        | NextPick::FitClamped { cm_360, .. }
        | NextPick::FitFallbackMidpoint { cm_360 } => Some(cm_360),
        NextPick::Last => None,
    }
}

#[test]
fn first_round_bisects_and_cannot_guess() {
    let s = run_all(Signal::TooFast, 1);
    let t = s.history[0].trace;
    assert_eq!(t.low_cm_360, LOW);
    assert_eq!(t.high_cm_360, HIGH);
    assert_eq!(t.guess, RoundGuess::OneSensitivity);
    assert_eq!(t.next, NextPick::Midpoint { cm_360: s.current_cm_360 });
}

#[test]
fn later_rounds_follow_the_fit() {
    let mut saw_estimate = false;
    for seed in 0..30 {
        let mut rng = StdRng::seed_from_u64(seed);
        let s = run_blocks(&mut rng, 35.0);
        for (i, b) in s.history.iter().enumerate() {
            if let RoundGuess::Guess { cm_360, rel_uncertainty } = b.trace.guess {
                assert!(cm_360.is_finite() && cm_360 > 0.0);
                assert!(rel_uncertainty.is_finite());
            }
            if i == 0 || i + 1 == s.history.len() {
                continue;
            }
            let next_tested = s.history[i + 1].tested_cm_360;
            match b.trace.next {
                NextPick::FitEstimate { cm_360 } => {
                    saw_estimate = true;
                    assert_eq!(cm_360, next_tested);
                }
                NextPick::FitClamped { cm_360, .. } | NextPick::FitFallbackMidpoint { cm_360 } => {
                    assert_eq!(cm_360, next_tested)
                }
                NextPick::Midpoint { .. } => panic!("round {i} should follow the fit"),
                NextPick::Last => panic!("round {i} of {} is not last", s.history.len()),
            }
        }
    }
    assert!(saw_estimate);
}

#[test]
fn clamped_pick_is_recorded_as_clamped() {
    let mut clamped = 0;
    for seed in 0..300 {
        let mut rng = StdRng::seed_from_u64(seed);
        let pref = [16.0, 20.0, 30.0, 50.0, 70.0, 79.0][seed as usize % 6];
        let s = run_blocks(&mut rng, pref);
        for (i, b) in s.history.iter().enumerate() {
            if let Some(cm) = next_cm(b.trace.next) {
                assert_eq!(cm, s.history.get(i + 1).map_or(s.current_cm_360, |n| n.tested_cm_360));
            }
            if let NextPick::FitClamped { guess_cm, cm_360 } = b.trace.next {
                clamped += 1;
                // The pick sits at an edge of the reach and the guess is beyond it. The reach
                // is around the bounds after this round: the next round's starting range.
                let after = s.history[i + 1].trace;
                let (lo, hi) = (
                    (after.low_cm_360 * (1.0 - FIT_REACH)).max(LOW),
                    (after.high_cm_360 * (1.0 + FIT_REACH)).min(HIGH),
                );
                assert!(guess_cm < lo || guess_cm > hi, "{guess_cm} inside {lo}..{hi}");
                assert!(cm_360 == lo || cm_360 == hi, "{cm_360} not an edge of {lo}..{hi}");
            }
        }
    }
    assert!(clamped > 0, "no clamped pick in 300 simulated runs");
}

#[test]
fn one_sided_misses_fall_back_to_the_midpoint() {
    let s = run_all(Signal::TooFast, 4);
    for b in &s.history[2..] {
        assert!(matches!(b.trace.next, NextPick::FitFallbackMidpoint { .. } | NextPick::Last), "{:?}", b.trace.next);
    }
    assert!(matches!(s.history[2].trace.next, NextPick::FitFallbackMidpoint { .. }));
}

#[test]
fn last_round_says_last() {
    let s = run_all(Signal::TooFast, 7);
    assert!(s.is_completed);
    assert_eq!(s.history[6].trace.next, NextPick::Last);
}

#[test]
fn no_crossover_after_two_one_sided_rounds() {
    let s = run_all(Signal::TooFast, 2);
    assert_eq!(s.history[1].trace.guess, RoundGuess::NoCrossover);
}

#[test]
fn diagnostics_carry_each_rounds_trace() {
    let mut run = CalibrationRun::new(5);
    while !run.is_completed() {
        let seg = run.segment().unwrap();
        if seg.counted {
            let signal = if run.scheduler().current_cm_360 < 35.0 { Signal::TooFast } else { Signal::TooSlow };
            let band = (seg.kind == TestKind::Flick).then_some(aim_trainer::game::tasks::flick::FlickBand::Mid);
            for _ in 0..4 {
                run.record(seg.kind, band, signal);
            }
            if matches!(seg.kind, TestKind::Micro | TestKind::Tracking) {
                let (faster_cost, slower_cost) = if signal == Signal::TooFast { (2.0, 1.0) } else { (1.0, 2.0) };
                run.record_pair(seg.kind, PairResult { faster_cost, slower_cost, signal, band: None });
            }
        }
        run.finish_segment();
    }
    let d = run.diagnostics();
    assert_eq!(d.rounds.len(), 7);
    for (i, r) in d.rounds.iter().enumerate() {
        assert_eq!(r.trace, Some(run.scheduler().history[i].trace), "round {i}");
    }
    let expected = run.scheduler().recommendation(None).map(|r| r.neutral.rel_uncertainty);
    assert!(expected.is_some(), "the fit should produce an answer");
    assert_eq!(d.result_rel_uncertainty, expected);
}

#[test]
fn old_diagnostics_json_loads() {
    let json = r#"{"rounds":[{"tested_cm_360":40.0,"signals":{}}],"estimates":[],"conflicts":[],"fit_cm_360":null}"#;
    let d: RunDiagnostics = serde_json::from_str(json).unwrap();
    assert_eq!(d.rounds[0].trace, None);
    assert_eq!(d.result_rel_uncertainty, None);
}

#[test]
fn pooling_sets_the_pooled_uncertainty() {
    let mut r = CalibrationResult {
        cm_360: 40.0,
        confidence: 0.8,
        psa_cm: Some(40.0),
        stat_cm: None,
        rounds: vec![],
        samples: vec![],
        at: chrono::Utc::now(),
        diagnostics: Some(RunDiagnostics::default()),
        pooled_runs: 1,
        no_answer: false,
        runs_disagree: false,
        habits: None,
    };
    r.apply_pool(&Pooled {
        cm_360: 38.0,
        rel_uncertainty: 0.07,
        confidence: 0.9,
        estimates: vec![],
        conflicts: vec![],
        left_out: vec![],
        runs: 2,
    });
    assert_eq!(r.diagnostics.unwrap().result_rel_uncertainty, Some(0.07));
}
