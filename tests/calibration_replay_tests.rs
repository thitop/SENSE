//! The block scheduler records, for every round, the range it started from, its best guess
//! afterwards and how the next sensitivity was picked.

use aim_trainer::data::{CalibrationResult, RunDiagnostics};
use aim_trainer::game::calibration_run::CalibrationRun;
use aim_trainer::sensitivity::blocks::{BlockScheduler, NextPick, RoundGuess};
use aim_trainer::sensitivity::combiner::{Signal, TestKind};
use aim_trainer::sensitivity::paired::PairResult;
use aim_trainer::sensitivity::pooling::Pooled;
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
        for kind in [
            TestKind::Flick,
            TestKind::Micro,
            TestKind::Tracking,
            TestKind::Recoil,
            TestKind::Placement,
        ] {
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
        s.record(
            TestKind::Flick,
            if signal == Signal::TooFast {
                Signal::TooSlow
            } else {
                Signal::TooFast
            },
        );
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
    assert_eq!(
        t.next,
        NextPick::Midpoint {
            cm_360: s.current_cm_360
        }
    );
}

#[test]
fn later_rounds_follow_the_fit() {
    let mut saw_estimate = false;
    for seed in 0..30 {
        let mut rng = StdRng::seed_from_u64(seed);
        let s = run_blocks(&mut rng, 35.0);
        for (i, b) in s.history.iter().enumerate() {
            if let RoundGuess::Guess {
                cm_360,
                rel_uncertainty,
            } = b.trace.guess
            {
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
                assert_eq!(
                    cm,
                    s.history
                        .get(i + 1)
                        .map_or(s.current_cm_360, |n| n.tested_cm_360)
                );
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
                assert!(
                    guess_cm < lo || guess_cm > hi,
                    "{guess_cm} inside {lo}..{hi}"
                );
                assert!(
                    cm_360 == lo || cm_360 == hi,
                    "{cm_360} not an edge of {lo}..{hi}"
                );
            }
        }
    }
    assert!(clamped > 0, "no clamped pick in 300 simulated runs");
}

#[test]
fn one_sided_misses_fall_back_to_the_midpoint() {
    let s = run_all(Signal::TooFast, 4);
    for b in &s.history[2..] {
        assert!(
            matches!(
                b.trace.next,
                NextPick::FitFallbackMidpoint { .. } | NextPick::Last
            ),
            "{:?}",
            b.trace.next
        );
    }
    assert!(matches!(
        s.history[2].trace.next,
        NextPick::FitFallbackMidpoint { .. }
    ));
}

#[test]
fn last_round_says_last() {
    let s = run_all(Signal::TooFast, 7);
    assert!(s.is_completed);
    assert_eq!(s.history[6].trace.next, NextPick::Last);
}

#[test]
fn pick_range_is_where_the_next_round_was_picked() {
    for seed in 0..60 {
        let mut rng = StdRng::seed_from_u64(seed);
        let pref = [16.0, 20.0, 30.0, 50.0, 70.0, 79.0][seed as usize % 6];
        let s = run_blocks(&mut rng, pref);
        let n = s.history.len();
        for (i, b) in s.history.iter().enumerate() {
            let t = b.trace;
            let (lo, hi) = (t.pick_low_cm_360, t.pick_high_cm_360);
            if i + 1 == n {
                // The last round: the bounds it left behind.
                assert_eq!((lo, hi), (s.low_cm_360, s.high_cm_360), "seed {seed}");
                continue;
            }
            let after = s.history[i + 1].trace;
            let expected = if i == 0 {
                // Bisection: the middle of the bounds after the round.
                (after.low_cm_360, after.high_cm_360)
            } else {
                // The fit branch: the reach around those bounds.
                (
                    (after.low_cm_360 * (1.0 - FIT_REACH)).max(LOW),
                    (after.high_cm_360 * (1.0 + FIT_REACH)).min(HIGH),
                )
            };
            assert_eq!((lo, hi), expected, "seed {seed} round {i}");
            let next = s.history[i + 1].tested_cm_360;
            assert!(
                lo <= next && next <= hi,
                "seed {seed} round {i}: {next} outside {lo}..{hi}"
            );
        }
    }
}

#[test]
fn no_crossover_after_two_one_sided_rounds() {
    let s = run_all(Signal::TooFast, 2);
    assert_eq!(s.history[1].trace.guess, RoundGuess::NoCrossover);
}

#[test]
fn diagnostics_carry_each_rounds_trace() {
    let run = threshold_run(5, 35.0);
    let d = run.diagnostics();
    assert_eq!(d.rounds.len(), 7);
    for (i, r) in d.rounds.iter().enumerate() {
        assert_eq!(r.trace, Some(run.scheduler().history[i].trace), "round {i}");
    }
    let expected = run
        .scheduler()
        .recommendation(None)
        .map(|r| r.neutral.rel_uncertainty);
    assert!(expected.is_some(), "the fit should produce an answer");
    assert_eq!(d.result_rel_uncertainty, expected);
}

/// A full run where every test says "too fast" below `pref` and "too slow" above it.
fn threshold_run(seed: u64, pref: f64) -> CalibrationRun {
    let mut run = CalibrationRun::new(seed);
    while !run.is_completed() {
        let seg = run.segment().unwrap();
        if seg.counted {
            let signal = if run.scheduler().current_cm_360 < pref {
                Signal::TooFast
            } else {
                Signal::TooSlow
            };
            let band = (seg.kind == TestKind::Flick)
                .then_some(aim_trainer::game::tasks::flick::FlickBand::Mid);
            for _ in 0..4 {
                run.record(seg.kind, band, signal);
            }
            if matches!(seg.kind, TestKind::Micro | TestKind::Tracking) {
                let (faster_cost, slower_cost) = if signal == Signal::TooFast {
                    (2.0, 1.0)
                } else {
                    (1.0, 2.0)
                };
                run.record_pair(
                    seg.kind,
                    PairResult {
                        faster_cost,
                        slower_cost,
                        signal,
                        band: None,
                    },
                );
            }
        }
        run.finish_segment();
    }
    run
}

/// A full run played by the simulated player of `run_blocks`, through `CalibrationRun`.
fn simulated_run(seed: u64, pref: f64) -> CalibrationRun {
    use aim_trainer::game::tasks::flick::FlickBand;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut run = CalibrationRun::new(seed);
    while !run.is_completed() {
        let seg = run.segment().unwrap();
        if seg.counted {
            let cm = run.scheduler().current_cm_360;
            let (n, clarity) = per_block(seg.kind);
            for i in 0..n {
                let signal = simulated_signal(&mut rng, pref, cm, clarity);
                if matches!(seg.kind, TestKind::Micro | TestKind::Tracking) {
                    let (faster_cost, slower_cost) = match signal {
                        Signal::TooFast => (2.0, 1.0),
                        Signal::TooSlow => (1.0, 2.0),
                        Signal::Direct => (1.0, 1.0),
                    };
                    run.record_pair(
                        seg.kind,
                        PairResult {
                            faster_cost,
                            slower_cost,
                            signal,
                            band: None,
                        },
                    );
                } else {
                    let band = (seg.kind == TestKind::Flick).then_some(FlickBand::ALL[i % 3]);
                    run.record(seg.kind, band, signal);
                }
            }
        }
        run.finish_segment();
    }
    run
}

/// The last `{:.1}` number in a line, e.g. the pick in "The best guess (12.0 cm) is … 14.0 cm."
fn last_number(line: &str) -> Option<String> {
    line.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|t| t.parse::<f64>().is_ok())
        .last()
        .map(|t| t.to_string())
}

#[test]
fn replay_of_a_real_run_tells_what_the_scheduler_did() {
    let mut clamped = 0;
    for seed in 0..40 {
        let pref = [16.0, 20.0, 30.0, 50.0, 70.0, 79.0][seed as usize % 6];
        let run = simulated_run(seed, pref);
        let result = run.build_result(vec![], None, chrono::Utc::now());
        let story = aim_trainer::ui::replay::replay(&result).expect("a traced run has a replay");
        let tested: Vec<f64> = run
            .scheduler()
            .history
            .iter()
            .map(|b| b.tested_cm_360)
            .collect();
        assert_eq!(story.rounds.len(), tested.len());
        for (i, r) in story.rounds.iter().enumerate() {
            let (lo, hi) = r.range.expect("every traced round has a range");
            assert!(
                lo - 1e-9 <= r.tested_cm_360 && r.tested_cm_360 <= hi + 1e-9,
                "seed {seed} round {i}: {} outside {lo}..{hi}",
                r.tested_cm_360
            );
            if r.next_line.contains("further than one round may move") {
                clamped += 1;
            }
            match tested.get(i + 1) {
                // A tied bisection round names no number.
                Some(_) if r.next_line == "About even → the range narrows around here" => {}
                Some(next) => assert_eq!(
                    last_number(&r.next_line),
                    Some(format!("{next:.1}")),
                    "seed {seed} round {i}: {:?}",
                    r.next_line
                ),
                None => assert_eq!(r.next_line, "That was the last round → result"),
            }
        }
    }
    assert!(clamped > 0, "no clamped pick: the clamp copy went untested");
}

#[test]
fn old_diagnostics_json_loads() {
    let json = r#"{"rounds":[{"tested_cm_360":40.0,"signals":{}}],"estimates":[],"conflicts":[],"fit_cm_360":null}"#;
    let d: RunDiagnostics = serde_json::from_str(json).unwrap();
    assert_eq!(d.rounds[0].trace, None);
    assert_eq!(d.result_rel_uncertainty, None);
}

#[test]
fn trace_saved_without_pick_range_loads() {
    let json = r#"{"low_cm_360":15.0,"high_cm_360":80.0,"guess":"OneSensitivity","next":{"Midpoint":{"cm_360":47.5}}}"#;
    let t: aim_trainer::data::RoundTrace = serde_json::from_str(json).unwrap();
    assert_eq!((t.pick_low_cm_360, t.pick_high_cm_360), (0.0, 0.0));
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
        plan: None,
        setup: None,
        own_cm_at_start: None,
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
        CalibrationResult, CalibrationRound, EstimateRecord, NextPick, RoundGuess, RoundSignals,
        RoundTrace, RunDiagnostics,
    };
    use aim_trainer::sensitivity::blocks::SignalCounts;
    use aim_trainer::sensitivity::psa::PsaRoundConclusion as C;
    use aim_trainer::ui::replay::{replay, verdict, RoundView, Tone};
    use std::collections::BTreeMap;

    fn counts(f: usize, d: usize, s: usize) -> SignalCounts {
        SignalCounts {
            too_fast: f,
            direct: d,
            too_slow: s,
        }
    }

    fn trace(guess: RoundGuess, next: NextPick) -> RoundTrace {
        RoundTrace {
            low_cm_360: 15.0,
            high_cm_360: 80.0,
            pick_low_cm_360: 20.0,
            pick_high_cm_360: 60.0,
            guess,
            next,
        }
    }

    fn round(trace: Option<RoundTrace>) -> RoundSignals {
        let mut signals = BTreeMap::new();
        signals.insert("Tracking".to_string(), counts(5, 0, 1));
        signals.insert("Flick far".to_string(), counts(1, 0, 0));
        signals.insert("Flicking".to_string(), counts(1, 0, 4));
        signals.insert("Micro-adjustment".to_string(), counts(2, 3, 2));
        signals.insert("Counter-strafing".to_string(), counts(1, 0, 0));
        RoundSignals {
            tested_cm_360: 47.5,
            signals,
            pairs: vec![],
            trace,
        }
    }

    fn cal(rounds: Vec<RoundSignals>, conclusions: Vec<C>) -> CalibrationResult {
        CalibrationResult {
            cm_360: 34.2,
            confidence: 0.8,
            psa_cm: Some(34.2),
            stat_cm: None,
            rounds: conclusions
                .into_iter()
                .map(|conclusion| CalibrationRound {
                    tested_cm_360: 47.5,
                    conclusion,
                })
                .collect(),
            samples: vec![],
            at: chrono::Utc::now(),
            diagnostics: Some(RunDiagnostics {
                rounds,
                ..Default::default()
            }),
            pooled_runs: 1,
            no_answer: false,
            runs_disagree: false,
            habits: None,
            plan: None,
            setup: None,
            own_cm_at_start: None,
        }
    }

    /// The first of two rounds; the second is a legacy round.
    fn one(t: Option<RoundTrace>, c: C) -> RoundView {
        replay(&cal(vec![round(t), round(None)], vec![c, c]))
            .unwrap()
            .rounds
            .remove(0)
    }

    fn guess() -> RoundGuess {
        RoundGuess::Guess {
            cm_360: 35.0,
            rel_uncertainty: 0.2,
        }
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
        assert_eq!(
            line(m, C::BalancedNarrow),
            "About even → the range narrows around here"
        );
        assert_eq!(
            line(NextPick::FitEstimate { cm_360: 36.0 }, C::BalancedNarrow),
            "Next round tests 36.0 cm: the best guess so far."
        );
        assert_eq!(
            line(NextPick::FitClamped { guess_cm: 12.0, cm_360: 14.0 }, C::BalancedNarrow),
            "The best guess (12.0 cm) is further than one round may move, so the next round goes as far as it can: 14.0 cm."
        );
        let early = RoundGuess::Guess {
            cm_360: 12.0,
            rel_uncertainty: 4.0,
        };
        let t = trace(
            early,
            NextPick::FitClamped {
                guess_cm: 12.0,
                cm_360: 14.0,
            },
        );
        assert_eq!(
            one(Some(t), C::BalancedNarrow).next_line,
            "A rough first guess (12.0 cm) is further than one round may move, so the next round goes as far as it can: 14.0 cm."
        );
        assert_eq!(
            line(
                NextPick::FitFallbackMidpoint { cm_360: 40.0 },
                C::BalancedNarrow
            ),
            "Next round tests the middle of the range (40.0 cm)."
        );
        assert_eq!(
            line(NextPick::Last, C::BalancedNarrow),
            "That was the last round → result"
        );
    }

    #[test]
    fn guess_lines() {
        let g = |g| {
            one(Some(trace(g, NextPick::Last)), C::BalancedNarrow)
                .guess_line
                .unwrap()
        };
        assert_eq!(g(guess()), "So far SENSE thinks 35.0 cm ± 20%");
        assert_eq!(
            g(RoundGuess::OneSensitivity),
            "One sensitivity tested so far: SENSE needs two to start guessing"
        );
        assert_eq!(
            g(RoundGuess::NoCrossover),
            "Not enough to guess yet: every test leaned the same way"
        );
    }

    #[test]
    fn uncertain_guess_is_not_plotted() {
        let t = trace(
            RoundGuess::Guess {
                cm_360: 35.0,
                rel_uncertainty: 4.0,
            },
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
        let mut second = round(None);
        second.tested_cm_360 = 63.75;
        let legacy = |c| {
            replay(&cal(vec![round(None), second.clone()], vec![c, c]))
                .unwrap()
                .rounds
        };
        let r = legacy(C::TooFastNeedSlower).remove(0);
        assert_eq!(r.range, None);
        assert_eq!(r.guess_line, None);
        assert_eq!(r.next_line, "Overall too fast · next round tested 63.8 cm");
        assert_eq!(r.header, "Round 1 of 2");
        assert_eq!(
            legacy(C::TooSlowNeedFaster)[0].next_line,
            "Overall too slow · next round tested 63.8 cm"
        );
        assert_eq!(
            legacy(C::BalancedNarrow)[0].next_line,
            "About even · next round tested 63.8 cm"
        );
        assert_eq!(
            legacy(C::BalancedNarrow)[1].next_line,
            "That was the last round → result"
        );
    }

    #[test]
    fn range_is_the_one_the_round_was_picked_within() {
        let first = round(Some(trace(
            RoundGuess::OneSensitivity,
            NextPick::Midpoint { cm_360: 40.0 },
        )));
        let mut second = round(Some(trace(guess(), NextPick::Last)));
        second.tested_cm_360 = 40.0;
        // The bounds collapsed after round 1; the band is still where round 2 was picked from.
        let t = second.trace.as_mut().unwrap();
        (t.low_cm_360, t.high_cm_360) = (45.0, 45.0);
        let r = replay(&cal(vec![first, second], vec![C::BalancedNarrow; 2])).unwrap();
        assert_eq!(r.rounds[0].range, Some((15.0, 80.0)));
        assert_eq!(r.rounds[1].range, Some((20.0, 60.0)));
    }

    #[test]
    fn range_is_dropped_when_the_pick_range_was_not_saved() {
        let mut first = round(Some(trace(
            RoundGuess::OneSensitivity,
            NextPick::Midpoint { cm_360: 40.0 },
        )));
        let t = first.trace.as_mut().unwrap();
        (t.pick_low_cm_360, t.pick_high_cm_360) = (0.0, 0.0);
        let second = round(Some(trace(guess(), NextPick::Last)));
        let r = replay(&cal(vec![first, second], vec![C::BalancedNarrow; 2])).unwrap();
        assert_eq!(r.rounds[0].range, Some((15.0, 80.0)));
        assert_eq!(r.rounds[1].range, None);
    }

    #[test]
    fn conclusion_falls_back_to_row_counts() {
        // Rows: Flicking (1 fast, 4 slow), Micro (2, 2), Tracking (5, 1): 8 fast vs 7 slow.
        let c = cal(vec![round(None)], vec![]);
        assert_eq!(
            replay(&c).unwrap().rounds[0].conclusion,
            C::TooFastNeedSlower
        );
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
        let last = trace(
            RoundGuess::Guess {
                cm_360: 33.0,
                rel_uncertainty: 0.1,
            },
            NextPick::Last,
        );
        let mut c = cal(vec![round(Some(last))], vec![C::BalancedNarrow]);
        c.pooled_runs = 3;
        let lines = replay(&c).unwrap().result.lines;
        assert!(lines.contains(&(
            "Combined with 2 earlier runs → 34.2 cm".to_string(),
            Tone::Normal
        )));
        c.pooled_runs = 2;
        let lines = replay(&c).unwrap().result.lines;
        assert!(lines
            .iter()
            .any(|(l, _)| l == "Combined with 1 earlier run → 34.2 cm"));

        c.pooled_runs = 1;
        let d = c.diagnostics.as_mut().unwrap();
        d.left_out = vec!["Tracking".into()];
        d.estimates = vec![EstimateRecord {
            test: "Tracking".into(),
            cm_360: 20.0,
            rel_uncertainty: 0.1,
        }];
        let lines = replay(&c).unwrap().result.lines;
        assert!(lines
            .iter()
            .any(|(l, _)| l == "Leaving out Tracking moved it from 33.0 to 34.2 cm."));
        assert!(lines.contains(&(
            "Tracking: Left out: leaned one way at every sensitivity".to_string(),
            Tone::Warn
        )));
    }

    #[test]
    fn result_lines_give_each_test_in_cm() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.diagnostics.as_mut().unwrap().estimates = vec![
            EstimateRecord {
                test: "Flicking".into(),
                cm_360: 34.04,
                rel_uncertainty: 0.072,
            },
            EstimateRecord {
                test: "Micro-adjustment".into(),
                cm_360: 30.0,
                rel_uncertainty: 1.5,
            },
            EstimateRecord {
                test: "Flick far".into(),
                cm_360: 33.0,
                rel_uncertainty: 0.2,
            },
        ];
        let lines = replay(&c).unwrap().result.lines;
        assert_eq!(
            lines[0],
            (
                "Where your tests flip from \"too fast\" to \"too slow\". Each test on its own:"
                    .to_string(),
                Tone::Muted
            )
        );
        assert!(lines.contains(&("Flicking: 34.0 cm ± 7%".to_string(), Tone::Normal)));
        assert!(lines.contains(&("Micro-adjustment: no trend".to_string(), Tone::Normal)));
        assert!(!lines.iter().any(|(l, _)| l.starts_with("Flick far")));
    }

    #[test]
    fn no_answer_result_has_no_number() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.no_answer = true;
        let r = replay(&c).unwrap().result;
        assert_eq!(r.cm_360, None);
        assert_eq!(
            r.lines[0],
            (
                "No result: every test leaned the same way at every sensitivity tested".to_string(),
                Tone::Muted
            )
        );
    }

    #[test]
    fn no_answer_with_a_fit_too_uncertain_says_so() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.no_answer = true;
        c.diagnostics.as_mut().unwrap().result_rel_uncertainty = Some(1.7);
        let r = replay(&c).unwrap().result;
        assert_eq!(r.cm_360, None);
        assert_eq!(
            r.lines[0],
            (
                "No result: the tests flipped, but too unevenly to give a number".to_string(),
                Tone::Muted
            )
        );
    }

    #[test]
    fn no_answer_lists_only_left_out_tests() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.no_answer = true;
        let d = c.diagnostics.as_mut().unwrap();
        d.left_out = vec!["Tracking".into()];
        d.estimates = vec![
            EstimateRecord {
                test: "Flicking".into(),
                cm_360: 34.0,
                rel_uncertainty: 0.1,
            },
            EstimateRecord {
                test: "Tracking".into(),
                cm_360: 20.0,
                rel_uncertainty: 0.1,
            },
        ];
        let lines = replay(&c).unwrap().result.lines;
        assert_eq!(
            lines,
            [
                (
                    "No result: every test leaned the same way at every sensitivity tested"
                        .to_string(),
                    Tone::Muted
                ),
                (
                    "Tracking: Left out: leaned one way at every sensitivity".to_string(),
                    Tone::Warn
                ),
            ]
        );
    }

    #[test]
    fn leaving_out_line_needs_a_visible_move() {
        // 34.23 and the result 34.2 differ by less than 0.05: both read 34.2.
        let last = trace(
            RoundGuess::Guess {
                cm_360: 34.23,
                rel_uncertainty: 0.1,
            },
            NextPick::Last,
        );
        let mut c = cal(vec![round(Some(last))], vec![C::BalancedNarrow]);
        let d = c.diagnostics.as_mut().unwrap();
        d.left_out = vec!["Tracking".into()];
        d.estimates = vec![EstimateRecord {
            test: "Tracking".into(),
            cm_360: 20.0,
            rel_uncertainty: 0.1,
        }];
        let lines = replay(&c).unwrap().result.lines;
        assert!(
            lines
                .iter()
                .any(|(l, _)| l.starts_with("Tracking: Left out")),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|(l, _)| l.starts_with("Leaving out")),
            "{lines:?}"
        );
    }

    #[test]
    fn runs_disagree_adds_the_warning() {
        let mut c = cal(vec![round(None)], vec![C::BalancedNarrow]);
        c.runs_disagree = true;
        let lines = replay(&c).unwrap().result.lines;
        assert_eq!(
            lines.last(),
            Some(&(
                "Your recent runs disagree: they found quite different sensitivities, so this is the latest run \
                 alone. Your aim may still be settling in. Running it again right away won't fix that: warm up \
                 first, or calibrate again another day."
                    .to_string(),
                Tone::Warn
            ))
        );
        c.runs_disagree = false;
        assert!(!replay(&c)
            .unwrap()
            .result
            .lines
            .iter()
            .any(|(_, t)| *t == Tone::Warn));
    }
}

mod replay_playback {
    use aim_trainer::ui::replay::{ReplayState, STEP_SECS};
    use chrono::{TimeZone, Utc};

    #[test]
    fn tick_advances_every_step_secs_and_stops_on_result() {
        let mut s = ReplayState::default();
        s.play(0.0, 8);
        assert!(s.playing);
        s.tick(1.59, 8);
        assert_eq!(s.step, 0);
        s.tick(STEP_SECS, 8);
        assert_eq!(s.step, 1);
        for i in 2..=7 {
            s.tick(STEP_SECS * i as f64, 8);
            assert_eq!(s.step, i);
        }
        assert!(!s.playing);
        s.tick(STEP_SECS * 20.0, 8);
        assert_eq!(s.step, 7);
        assert!(!s.playing);
    }

    #[test]
    fn play_on_result_restarts() {
        let mut s = ReplayState::default();
        s.go_to(7, 8);
        s.play(3.0, 8);
        assert_eq!(s.step, 0);
        assert!(s.playing);
    }

    #[test]
    fn manual_step_stops_playback() {
        let mut s = ReplayState::default();
        s.play(0.0, 8);
        s.next(8);
        assert!(!s.playing);
        assert_eq!(s.step, 1);
        s.go_to(0, 8);
        s.prev(8);
        assert_eq!(s.step, 0);
        s.go_to(99, 8);
        assert_eq!(s.step, 7);
    }

    #[test]
    fn autoplay_waits_until_visible() {
        let mut s = ReplayState::default();
        s.arm_autoplay();
        assert!(!s.playing);
        s.on_visible(5.0, 8);
        assert!(s.playing);
        assert_eq!(s.step, 0);
        assert!(!s.autoplay_armed);
        s.next(8);
        s.on_visible(6.0, 8);
        assert!(!s.playing);
        assert_eq!(s.step, 1);
    }

    #[test]
    fn link_click_scrolls_and_plays() {
        let mut s = ReplayState::default();
        s.link_clicked(2.0, 8);
        assert!(s.scroll_pending);
        assert!(s.playing);
        assert_eq!(s.step, 0);
    }

    #[test]
    fn link_click_uses_up_the_autoplay() {
        let mut s = ReplayState::default();
        s.arm_autoplay();
        s.link_clicked(2.0, 8);
        s.next(8);
        assert!(!s.autoplay_armed);
        s.on_visible(3.0, 8);
        assert!(!s.playing);
        assert_eq!(s.step, 1);
    }

    #[test]
    fn leaving_the_page_unseen_disarms_autoplay() {
        let mut s = ReplayState::default();
        s.arm_autoplay();
        s.disarm();
        assert!(!s.autoplay_armed);
        s.on_visible(5.0, 8);
        assert!(!s.playing);
        assert_eq!(s.step, 0);
    }

    #[test]
    fn leaving_the_page_stops_playback_and_rewinds() {
        let mut s = ReplayState::default();
        s.play(0.0, 8);
        s.tick(STEP_SECS, 8);
        s.tick(STEP_SECS * 2.0, 8);
        assert_eq!((s.step, s.playing), (2, true));
        s.disarm();
        assert_eq!((s.step, s.playing), (0, false));
        s.go_to(5, 8);
        s.disarm();
        assert_eq!(s.step, 0);
    }

    #[test]
    fn new_result_resets_state() {
        let a = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let b = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
        let mut s = ReplayState::default();
        s.sync(a, 1);
        s.go_to(4, 8);
        s.sync(a, 1);
        assert_eq!(s.step, 4);
        s.arm_autoplay();
        s.play(0.0, 8);
        s.sync(b, 1);
        assert_eq!(s.step, 0);
        assert!(!s.playing);
        assert!(s.autoplay_armed);
        s.go_to(3, 8);
        s.sync(b, 3);
        assert_eq!(s.step, 0);
    }
}
