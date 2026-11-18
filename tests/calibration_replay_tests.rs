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

// ---- Replay view model ----

mod replay_view {
    use aim_trainer::data::{
        CalibrationResult, CalibrationRound, EstimateRecord, NextPick, RoundGuess, RoundSignals, RoundTrace,
        RunDiagnostics,
    };
    use aim_trainer::sensitivity::blocks::SignalCounts;
    use aim_trainer::sensitivity::psa::PsaRoundConclusion as C;
    use aim_trainer::ui::replay::{replay, verdict, RoundView, Tone};
    use std::collections::BTreeMap;

    fn counts(f: usize, d: usize, s: usize) -> SignalCounts {
        SignalCounts { too_fast: f, direct: d, too_slow: s }
    }

    fn trace(guess: RoundGuess, next: NextPick) -> RoundTrace {
        RoundTrace { low_cm_360: 15.0, high_cm_360: 80.0, guess, next }
    }

    fn round(trace: Option<RoundTrace>) -> RoundSignals {
        let mut signals = BTreeMap::new();
        signals.insert("Tracking".to_string(), counts(5, 0, 1));
        signals.insert("Flick far".to_string(), counts(1, 0, 0));
        signals.insert("Flicking".to_string(), counts(1, 0, 4));
        signals.insert("Micro-adjustment".to_string(), counts(2, 3, 2));
        signals.insert("Counter-strafing".to_string(), counts(1, 0, 0));
        RoundSignals { tested_cm_360: 47.5, signals, pairs: vec![], trace }
    }

    fn cal(rounds: Vec<RoundSignals>, conclusions: Vec<C>) -> CalibrationResult {
        CalibrationResult {
            cm_360: 34.2,
            confidence: 0.8,
            psa_cm: Some(34.2),
            stat_cm: None,
            rounds: conclusions
                .into_iter()
                .map(|conclusion| CalibrationRound { tested_cm_360: 47.5, conclusion })
                .collect(),
            samples: vec![],
            at: chrono::Utc::now(),
            diagnostics: Some(RunDiagnostics { rounds, ..Default::default() }),
            pooled_runs: 1,
            no_answer: false,
            runs_disagree: false,
            habits: None,
        }
    }

    /// The first of two rounds; the second is a legacy round.
    fn one(t: Option<RoundTrace>, c: C) -> RoundView {
        replay(&cal(vec![round(t), round(None)], vec![c, c])).unwrap().rounds.remove(0)
    }

    fn guess() -> RoundGuess {
        RoundGuess::Guess { cm_360: 35.0, rel_uncertainty: 0.2 }
    }

    #[test]
    fn verdict_text_follows_pairs() {
        assert_eq!(verdict(counts(6, 0, 1)), "Better slower · too fast");
        assert_eq!(verdict(counts(1, 0, 6)), "Better faster · too slow");
        assert_eq!(verdict(counts(2, 3, 2)), "Even");
    }

    #[test]
    fn rows_skip_flick_bands_and_keep_order() {
        let r = one(None, C::BalancedNarrow);
        let names: Vec<_> = r.rows.iter().map(|r| r.test.as_str()).collect();
        assert_eq!(names, ["Flicking", "Micro-adjustment", "Tracking"]);
        assert_eq!(r.rows[0].verdict, "Better faster · too slow");
    }

    #[test]
    fn next_line_matches_each_pick() {
        let line = |next, c| one(Some(trace(guess(), next)), c).next_line;
        let m = NextPick::Midpoint { cm_360: 47.5 };
        assert_eq!(
            line(m, C::TooFastNeedSlower),
            "Overall too fast → the next round tries the middle of the slower half (47.5 cm)"
        );
        assert_eq!(
            line(m, C::TooSlowNeedFaster),
            "Overall too slow → the next round tries the middle of the faster half (47.5 cm)"
        );
        assert_eq!(line(m, C::BalancedNarrow), "About even → the range narrows around here");
        assert_eq!(
            line(NextPick::FitEstimate { cm_360: 36.0 }, C::BalancedNarrow),
            "Next round tests 36.0 cm: the best guess so far."
        );
        assert_eq!(
            line(NextPick::FitClamped { guess_cm: 12.0, cm_360: 14.0 }, C::BalancedNarrow),
            "The best guess (12.0 cm) is outside what's been tested, so the next round goes as far as it can: 14.0 cm."
        );
        assert_eq!(
            line(NextPick::FitFallbackMidpoint { cm_360: 40.0 }, C::BalancedNarrow),
            "Next round tests the middle of the range (40.0 cm)."
        );
        assert_eq!(line(NextPick::Last, C::BalancedNarrow), "That was the last round → result");
    }

    #[test]
    fn guess_lines() {
        let g = |g| one(Some(trace(g, NextPick::Last)), C::BalancedNarrow).guess_line.unwrap();
        assert_eq!(g(guess()), "So far SENSE thinks 35.0 cm ± 20%");
        assert_eq!(
            g(RoundGuess::OneSensitivity),
            "One sensitivity tested so far: SENSE needs two to start guessing"
        );
        assert_eq!(g(RoundGuess::NoCrossover), "Not enough to guess yet: every test leaned the same way");
    }

    #[test]
    fn uncertain_guess_is_not_plotted() {
        let t = trace(
            RoundGuess::Guess { cm_360: 35.0, rel_uncertainty: 4.0 },
            NextPick::FitEstimate { cm_360: 35.0 },
        );
        let r = one(Some(t), C::BalancedNarrow);
        assert_eq!(r.guess_point, None);
        assert_eq!(r.guess_line.as_deref(), Some("Too early to trust a guess"));
        assert!(r.next_line.ends_with("a rough first guess."));
        let ok = one(Some(trace(guess(), NextPick::Last)), C::BalancedNarrow);
        assert_eq!(ok.guess_point, Some((35.0, 0.2)));
    }

    #[test]
    fn legacy_round_uses_conclusion_wording() {
        let r = one(None, C::TooFastNeedSlower);
        assert_eq!(r.range, None);
        assert_eq!(r.guess_line, None);
        assert_eq!(r.next_line, "Overall too fast → next round went slower");
        assert_eq!(r.header, "Round 1 of 2");
        let c = cal(vec![round(None), round(None)], vec![C::TooFastNeedSlower, C::BalancedNarrow]);
        assert_eq!(replay(&c).unwrap().rounds[1].next_line, "That was the last round → result");
    }

    #[test]
    fn conclusion_falls_back_to_row_counts() {
        // Rows: Flicking (1 fast, 4 slow), Micro (2, 2), Tracking (5, 1): 8 fast vs 7 slow.
        let c = cal(vec![round(None)], vec![]);
        assert_eq!(replay(&c).unwrap().rounds[0].conclusion, C::TooFastNeedSlower);
    }

    #[test]
    fn no_diagnostics_means_no_replay() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.diagnostics = None;
        assert!(replay(&c).is_none());
        c.diagnostics = Some(RunDiagnostics::default());
        assert!(replay(&c).is_none());
    }

    #[test]
    fn result_lines_pooled_and_left_out() {
        let last = trace(RoundGuess::Guess { cm_360: 33.0, rel_uncertainty: 0.1 }, NextPick::Last);
        let mut c = cal(vec![round(Some(last))], vec![C::BalancedNarrow]);
        c.pooled_runs = 3;
        let lines = replay(&c).unwrap().result.lines;
        assert!(lines.contains(&("Combined with 2 earlier runs → 34.2 cm".to_string(), Tone::Normal)));
        c.pooled_runs = 2;
        let lines = replay(&c).unwrap().result.lines;
        assert!(lines.iter().any(|(l, _)| l == "Combined with 1 earlier run → 34.2 cm"));

        c.pooled_runs = 1;
        let d = c.diagnostics.as_mut().unwrap();
        d.left_out = vec!["Tracking".into()];
        d.estimates = vec![EstimateRecord { test: "Tracking".into(), cm_360: 20.0, rel_uncertainty: 0.1 }];
        let lines = replay(&c).unwrap().result.lines;
        assert!(lines.iter().any(|(l, _)| l == "Leaving out Tracking moved it from 33.0 to 34.2 cm."));
        assert!(lines.contains(&(
            "Tracking: Left out: leaned one way at every sensitivity".to_string(),
            Tone::Warn
        )));
    }

    #[test]
    fn no_answer_result_has_no_number() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.no_answer = true;
        let r = replay(&c).unwrap().result;
        assert_eq!(r.cm_360, None);
        assert_eq!(
            r.lines[0],
            ("No result: every test leaned the same way at every sensitivity tested".to_string(), Tone::Muted)
        );
    }
}
