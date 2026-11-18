//! The multi-test calibration run: warm-up, blocks of segments, buffered signals, results.

use aim_trainer::game::calibration::CalibrationProgress;
use aim_trainer::game::calibration_run::{CalibrationRun, SegmentEnd};
use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::sensitivity::blocks::Amount;
use aim_trainer::sensitivity::combiner::{Signal, TestKind};
use aim_trainer::sensitivity::psa::PsaRoundConclusion;
use glam::Vec3;

const PLAYER_CM: f64 = 41.6;

fn aim() -> Ray {
    Ray { origin: Vec3::new(0.0, 1.5, 0.0), direction: Vec3::NEG_Z }
}

/// Finish segments until the run leaves the warm-up.
fn skip_warmup(run: &mut CalibrationRun) {
    while run.in_warmup() {
        run.finish_segment();
    }
}

#[test]
fn starts_with_an_uncounted_warmup_at_the_players_sensitivity() {
    let run = CalibrationRun::new(1);
    assert!(run.in_warmup());
    assert_eq!(run.test_cm(PLAYER_CM), PLAYER_CM);
    let s = run.segment().unwrap();
    assert_eq!((s.kind, s.amount, s.counted), (TestKind::Flick, Amount::Shots(10), false));
}


#[test]
fn only_tests_that_exist_are_scheduled() {
    let mut run = CalibrationRun::new(2);
    let mut kinds = Vec::new();
    while !run.is_completed() {
        kinds.push(run.segment().unwrap().kind);
        run.finish_segment();
    }
    assert!(kinds.iter().all(|k| CalibrationRun::AVAILABLE.contains(k)), "{kinds:?}");
    for k in CalibrationRun::AVAILABLE {
        assert!(kinds.contains(&k), "{k:?} never scheduled");
    }
}

#[test]
fn warmup_ends_by_switching_to_the_first_test_sensitivity() {
    let mut run = CalibrationRun::new(3);
    let mut end = run.finish_segment();
    while run.in_warmup() {
        assert_eq!(end, SegmentEnd::Next);
        end = run.finish_segment();
    }
    assert_eq!(end, SegmentEnd::SensitivityChanged(None));
    assert_eq!(run.block_number(), 1);
    assert_eq!(run.test_cm(PLAYER_CM), 47.5);
    let s = run.segment().unwrap();
    assert_eq!((s.kind, s.amount, s.counted), (TestKind::Flick, Amount::Shots(2), false));
}

#[test]
fn warmup_and_adaptation_shots_record_nothing() {
    let mut run = CalibrationRun::new(4);
    run.record(TestKind::Flick, Some(FlickBand::Mid), Signal::TooFast);
    skip_warmup(&mut run);
    run.record(TestKind::Flick, Some(FlickBand::Mid), Signal::TooFast); // adaptation segment
    run.finish_segment();
    assert_eq!(run.scheduler().current_counts(TestKind::Flick).too_fast, 0);
}

/// Finish segments of the current block, recording `signal` for every counted one.
fn play_block(run: &mut CalibrationRun, signal: Signal) -> SegmentEnd {
    loop {
        let s = run.segment().unwrap();
        if s.counted {
            let band = (s.kind == TestKind::Flick).then_some(FlickBand::Far);
            run.record(s.kind, band, signal);
            run.record(s.kind, band, signal);
        }
        let end = run.finish_segment();
        if end != SegmentEnd::Next {
            return end;
        }
    }
}

#[test]
fn signals_count_when_their_segment_finishes() {
    let mut run = CalibrationRun::new(5);
    skip_warmup(&mut run);
    run.finish_segment(); // adaptation
    let s = run.segment().unwrap();
    run.record(s.kind, (s.kind == TestKind::Flick).then_some(FlickBand::Short), Signal::TooSlow);
    assert_eq!(run.scheduler().current_counts(s.kind).too_slow, 0, "buffered until the segment ends");
    run.finish_segment();
    assert_eq!(run.scheduler().current_counts(s.kind).too_slow, 1);
}

#[test]
fn signals_of_another_test_are_ignored() {
    let mut run = CalibrationRun::new(6);
    skip_warmup(&mut run);
    run.finish_segment(); // adaptation
    let s = run.segment().unwrap();
    let other = CalibrationRun::AVAILABLE.into_iter().find(|&k| k != s.kind).unwrap();
    run.record(other, None, Signal::TooSlow);
    run.finish_segment();
    assert_eq!(run.scheduler().current_counts(other).too_slow, 0);
}

#[test]
fn restarting_a_segment_drops_what_it_recorded() {
    let mut run = CalibrationRun::new(7);
    skip_warmup(&mut run);
    run.finish_segment(); // adaptation
    let s = run.segment().unwrap();
    run.record(s.kind, None, Signal::TooFast);
    run.restart_segment();
    assert_eq!(run.segment(), Some(s), "same segment again");
    run.finish_segment();
    assert_eq!(run.scheduler().current_counts(s.kind).too_fast, 0);
}

#[test]
fn a_block_ends_by_moving_the_sensitivity() {
    let mut run = CalibrationRun::new(8);
    skip_warmup(&mut run);
    let end = play_block(&mut run, Signal::TooFast);
    assert_eq!(end, SegmentEnd::SensitivityChanged(Some(PsaRoundConclusion::TooFastNeedSlower)));
    assert_eq!(run.block_number(), 2);
    assert!(run.test_cm(PLAYER_CM) > 47.5);
    let b = &run.scheduler().history[0];
    assert_eq!(b.counts(TestKind::Flick).too_fast, 2);
    assert_eq!(b.band_counts(FlickBand::Far).too_fast, 2);
    assert_eq!(b.counts(TestKind::Tracking).too_fast, 2);
}

#[test]
fn seven_blocks_complete_the_run_with_a_result() {
    let mut run = CalibrationRun::new(9);
    skip_warmup(&mut run);
    let mut ends = Vec::new();
    for i in 0..7 {
        ends.push(play_block(&mut run, if i % 2 == 0 { Signal::TooFast } else { Signal::TooSlow }));
    }
    assert_eq!(ends.last(), Some(&SegmentEnd::Completed));
    assert!(run.is_completed());
    assert_eq!(run.segment(), None);
    let r = run.build_result(vec![(40.0, 50.0)], Some(38.0), chrono::Utc::now());
    assert!((15.0..=80.0).contains(&r.cm_360));
    assert_eq!(r.rounds.len(), 7);
    assert_eq!(r.rounds[0].tested_cm_360, 47.5);
    assert_eq!(r.samples, vec![(40.0, 50.0)]);
    assert_eq!(r.stat_cm, Some(38.0));
    assert!((0.0..=1.0).contains(&r.confidence));
}

// ---------- Tasks for segments ----------

#[test]
fn each_segment_gets_a_matching_task() {
    let mut run = CalibrationRun::new(10);
    skip_warmup(&mut run);
    while !run.is_completed() && run.block_number() == 1 {
        let s = run.segment().unwrap();
        let mut task = run.make_task();
        task.start();
        task.update(0.001, aim());
        match s.kind {
            TestKind::Flick if s.counted => {
                assert_eq!(task.flick_spawn().and_then(|f| f.band).is_some(), true, "banded flicks")
            }
            TestKind::Flick => assert!(task.flick_spawn().is_some()),
            TestKind::Micro => assert!(task.micro_spawn().is_some()),
            TestKind::Tracking => {
                let mut windows = 0;
                while !task.is_complete() {
                    task.update(0.001, aim());
                    windows += task.take_tracking_windows().len();
                }
                // Summing f32 ticks can reach 12 s one tick early and lose the last window.
                assert!((23..=24).contains(&windows), "12 s of half-second windows: {windows}");
            }
            other => panic!("unexpected {other:?}"),
        }
        run.finish_segment();
    }
}

#[test]
fn same_seed_same_targets() {
    let first_micro = |seed| {
        let mut run = CalibrationRun::new(seed);
        skip_warmup(&mut run);
        while run.segment().unwrap().kind != TestKind::Micro {
            run.finish_segment();
        }
        let mut task = run.make_task();
        task.start();
        task.update(0.001, aim());
        task.micro_spawn().unwrap().offset_deg
    };
    assert_eq!(first_micro(11), first_micro(11));
    assert_ne!(first_micro(11), first_micro(12));
}

// ---------- HUD progress ----------

#[test]
fn progress_names_the_segment_and_the_warmup() {
    let mut run = CalibrationRun::new(13);
    let p = CalibrationProgress::from_run(&run, PLAYER_CM);
    assert!(p.warmup);
    assert_eq!(p.round, 0, "no round in progress yet");
    assert_eq!(p.testing_cm_360, PLAYER_CM);
    assert_eq!(p.segment_label, Some(TestKind::Flick.label()));
    assert_eq!((p.shot_in_round, p.shots_per_round), (1, run.segments_in_phase()));

    skip_warmup(&mut run);
    run.finish_segment();
    let p = CalibrationProgress::from_run(&run, PLAYER_CM);
    assert!(!p.warmup);
    assert_eq!((p.round, p.max_rounds), (1, 7));
    assert_eq!(p.testing_cm_360, 47.5);
    assert_eq!(p.shot_in_round, 2);
}

#[test]
fn psa_progress_has_no_segment() {
    let psa = aim_trainer::sensitivity::psa::PsaCalibrator::new(15.0, 80.0, 7);
    let p = CalibrationProgress::from_psa(&psa);
    assert_eq!(p.segment_label, None);
    assert!(!p.warmup);
}

// ---------- Diagnostics ----------

#[test]
fn the_result_keeps_what_each_test_said_per_round() {
    let mut run = CalibrationRun::new(21);
    skip_warmup(&mut run);
    for i in 0..7 {
        play_block(&mut run, if i % 2 == 0 { Signal::TooFast } else { Signal::TooSlow });
    }
    let r = run.build_result(Vec::new(), None, chrono::Utc::now());
    let d = r.diagnostics.expect("multi-test runs carry diagnostics");
    assert_eq!(d.rounds.len(), 7);
    let first = &d.rounds[0];
    assert_eq!(first.tested_cm_360, 47.5);
    assert_eq!(first.signals["Flicking"].too_fast, 2);
    assert_eq!(first.signals["Flick far"].too_fast, 2);
    assert_eq!(first.signals["Tracking"].too_fast, 2);
    assert!(!first.signals.contains_key("Flick short"), "bands with nothing recorded are left out");
}

#[test]
fn results_saved_before_diagnostics_still_load() {
    let old = r#"{"cm_360":30.0,"confidence":0.5,"psa_cm":30.0,"stat_cm":null,
        "rounds":[{"tested_cm_360":47.5,"conclusion":"TooFastNeedSlower"}],
        "samples":[],"at":"2026-11-16T12:58:26Z"}"#;
    let r: aim_trainer::data::CalibrationResult = serde_json::from_str(old).unwrap();
    assert!(r.diagnostics.is_none());
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains("diagnostics"), "nothing written when there is nothing to keep");
}

// ---------- Paired micro and tracking ----------

use aim_trainer::game::tasks::TaskManager;
use aim_trainer::sensitivity::paired::{PairResult, Side};

fn is_side_factor(f: f32) -> bool {
    f == Side::Faster.factor() || f == Side::Slower.factor()
}

#[test]
fn warmup_includes_micro() {
    let mut run = CalibrationRun::new(30);
    let mut kinds = Vec::new();
    while run.in_warmup() {
        let s = run.segment().unwrap();
        kinds.push((s.kind, s.amount, s.counted));
        run.finish_segment();
    }
    assert!(kinds.contains(&(TestKind::Micro, Amount::Shots(4), false)), "{kinds:?}");
}

#[test]
fn counted_micro_and_tracking_are_paired() {
    let mut run = CalibrationRun::new(31);
    skip_warmup(&mut run);
    while run.block_number() == 1 && !run.is_completed() {
        let s = run.segment().unwrap();
        let mut task = run.make_task();
        task.start();
        task.update(0.001, aim());
        match (s.kind, s.counted) {
            (TestKind::Micro, true) => {
                assert_eq!(s.amount, Amount::Shots(8));
                assert!(is_side_factor(task.sens_factor()));
            }
            (TestKind::Tracking, true) => {
                assert_eq!(s.amount, Amount::Seconds(12.0));
                assert!(is_side_factor(task.sens_factor()));
            }
            (TestKind::Flick, true) => assert!(is_side_factor(task.sens_factor())),
            _ => assert_eq!(task.sens_factor(), 1.0, "{s:?}"),
        }
        run.finish_segment();
    }
}

#[test]
fn warmup_micro_runs_at_factor_one() {
    let mut run = CalibrationRun::new(32);
    while run.segment().unwrap().kind != TestKind::Micro {
        run.finish_segment();
    }
    assert!(run.in_warmup());
    let mut task = run.make_task();
    task.start();
    task.update(0.001, aim());
    while !task.is_complete() {
        assert_eq!(task.sens_factor(), 1.0);
        let (pos, _) = *task.get_render_targets().last().unwrap();
        let ray = Ray { origin: aim().origin, direction: (pos - aim().origin).normalize() };
        assert!(task.on_click(ray));
        task.update(0.001, ray);
    }
    assert!(task.take_pair_results().is_empty());
}

fn too_fast_pair() -> PairResult {
    PairResult { faster_cost: 1.0, slower_cost: 0.5, signal: Signal::TooFast, band: None }
}

/// Advance to the first counted micro segment of block 1.
fn to_counted_micro(run: &mut CalibrationRun) {
    skip_warmup(run);
    while run.segment().map(|s| (s.kind, s.counted)) != Some((TestKind::Micro, true)) {
        run.finish_segment();
    }
}

#[test]
fn pairs_count_when_their_segment_finishes() {
    let mut run = CalibrationRun::new(33);
    to_counted_micro(&mut run);
    run.record_pair(TestKind::Micro, too_fast_pair());
    assert_eq!(run.scheduler().current_counts(TestKind::Micro).too_fast, 0, "buffered");
    run.finish_segment();
    // Counted in the block in progress, or in the finished block if micro came last.
    let counted = run.scheduler().current_counts(TestKind::Micro).too_fast
        + run.scheduler().history.first().map_or(0, |b| b.counts(TestKind::Micro).too_fast);
    assert_eq!(counted, 1);
    while run.block_number() == 1 {
        run.finish_segment();
    }
    let d = run.diagnostics();
    let pairs = &d.rounds[0].pairs;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].test, "Micro-adjustment");
    assert_eq!((pairs[0].faster_cost, pairs[0].slower_cost, pairs[0].signal), (1.0, 0.5, Signal::TooFast));
}

#[test]
fn pairs_of_another_test_are_ignored() {
    let mut run = CalibrationRun::new(34);
    to_counted_micro(&mut run);
    run.record_pair(TestKind::Tracking, too_fast_pair());
    run.finish_segment();
    assert_eq!(run.scheduler().current_counts(TestKind::Tracking).too_fast, 0);
}

#[test]
fn restarting_drops_recorded_pairs() {
    let mut run = CalibrationRun::new(35);
    to_counted_micro(&mut run);
    run.record_pair(TestKind::Micro, too_fast_pair());
    run.restart_segment();
    run.finish_segment();
    assert_eq!(run.scheduler().current_counts(TestKind::Micro).too_fast, 0);
    while run.block_number() == 1 {
        run.finish_segment();
    }
    assert!(run.diagnostics().rounds[0].pairs.is_empty());
}

#[test]
fn task_manager_factor_is_one_unless_running() {
    let mut run = CalibrationRun::new(36);
    to_counted_micro(&mut run);
    let mut tm = TaskManager::new();
    tm.set_custom_task(run.make_task(), aim_trainer::game::tasks::TaskType::Calibration);
    assert_eq!(tm.sens_factor(), 1.0, "not started");
    tm.start_task_now();
    tm.update(0.001, aim());
    assert!(is_side_factor(tm.sens_factor()));
    assert!(tm.take_pair_results().is_empty());
}

// ---------- Pooling ----------

#[test]
fn a_finished_run_records_what_each_test_said_per_round() {
    let mut run = CalibrationRun::new(40);
    skip_warmup(&mut run);
    for i in 0..7 {
        play_block(&mut run, if i % 2 == 0 { Signal::TooFast } else { Signal::TooSlow });
    }
    let at = chrono::Utc::now();
    let rec = run.run_record(800.0, at);
    assert_eq!((rec.dpi, rec.at), (800.0, at));
    let first: Vec<_> = rec.rows.iter().filter(|r| r.1 == 47.5).collect();
    assert!(first.contains(&&(TestKind::Flick, 47.5, 2, 0)), "{first:?}");
    assert!(first.contains(&&(TestKind::Tracking, 47.5, 2, 0)), "{first:?}");
    assert_eq!(rec.rows.iter().filter(|r| r.0 == TestKind::Flick).count(), 7, "one row per round and test");
}

// ---------- Runs without an answer ----------

/// Play every block, recording what `say(tested cm)` returns for each counted segment.
fn play_run(seed: u64, say: impl Fn(f64) -> Signal) -> CalibrationRun {
    let mut run = CalibrationRun::new(seed);
    skip_warmup(&mut run);
    while !run.is_completed() {
        let cm = run.test_cm(PLAYER_CM);
        let s = run.segment().unwrap();
        if s.counted {
            let band = (s.kind == TestKind::Flick).then_some(FlickBand::Mid);
            for _ in 0..3 {
                run.record(s.kind, band, say(cm));
            }
        }
        run.finish_segment();
    }
    run
}

#[test]
fn a_run_that_leans_one_way_everywhere_has_no_answer() {
    // Real play: flick said "too slow" even at 17 cm/360, and nothing crossed over.
    let r = play_run(50, |_| Signal::TooSlow).build_result(Vec::new(), None, chrono::Utc::now());
    assert!(r.no_answer);
}

#[test]
fn a_run_with_a_crossover_has_an_answer() {
    let say = |cm: f64| if cm < 40.0 { Signal::TooFast } else { Signal::TooSlow };
    let r = play_run(51, say).build_result(Vec::new(), None, chrono::Utc::now());
    assert!(!r.no_answer);
    assert!((30.0..=50.0).contains(&r.cm_360), "{}", r.cm_360);
}

#[test]
fn results_saved_before_have_an_answer() {
    let old = r#"{"cm_360":30.0,"confidence":0.5,"psa_cm":30.0,"stat_cm":null,
        "rounds":[],"samples":[],"at":"2026-11-16T12:58:26Z"}"#;
    let r: aim_trainer::data::CalibrationResult = serde_json::from_str(old).unwrap();
    assert!(!r.no_answer && !r.runs_disagree);
}

// ---------- Paired flick ----------

#[test]
fn counted_flick_is_paired() {
    let mut run = CalibrationRun::new(60);
    let warm = run.make_task();
    let mut warm = warm;
    warm.start();
    warm.update(0.001, aim());
    assert_eq!(warm.sens_factor(), 1.0, "warm-up flick");
    skip_warmup(&mut run);
    let mut seen_counted = false;
    while run.block_number() == 1 && !run.is_completed() {
        let s = run.segment().unwrap();
        let mut task = run.make_task();
        task.start();
        task.update(0.001, aim());
        if s.kind == TestKind::Flick && s.counted {
            assert_eq!(s.amount, Amount::Shots(18));
            assert!(is_side_factor(task.sens_factor()));
            seen_counted = true;
        } else if s.kind == TestKind::Flick {
            assert_eq!(task.sens_factor(), 1.0, "adaptation shots");
        }
        run.finish_segment();
    }
    assert!(seen_counted);
}

#[test]
fn flick_pairs_count_per_band() {
    let mut run = CalibrationRun::new(61);
    skip_warmup(&mut run);
    while run.segment().map(|s| (s.kind, s.counted)) != Some((TestKind::Flick, true)) {
        run.finish_segment();
    }
    let pair = PairResult { band: Some(FlickBand::Far), ..too_fast_pair() };
    run.record_pair(TestKind::Flick, pair);
    while run.block_number() == 1 {
        run.finish_segment();
    }
    let b = &run.scheduler().history[0];
    assert_eq!(b.counts(TestKind::Flick).too_fast, 1);
    assert_eq!(b.band_counts(FlickBand::Far).too_fast, 1);
    let rec = &run.diagnostics().rounds[0].pairs[0];
    assert_eq!((rec.test.as_str(), rec.band.as_deref()), ("Flicking", Some("far")));
}

#[test]
fn restarting_drops_flick_pairs() {
    let mut run = CalibrationRun::new(62);
    skip_warmup(&mut run);
    while run.segment().map(|s| (s.kind, s.counted)) != Some((TestKind::Flick, true)) {
        run.finish_segment();
    }
    run.record_pair(TestKind::Flick, PairResult { band: Some(FlickBand::Short), ..too_fast_pair() });
    run.restart_segment();
    while run.block_number() == 1 {
        run.finish_segment();
    }
    assert_eq!(run.scheduler().history[0].counts(TestKind::Flick).too_fast, 0);
}

#[test]
fn old_pair_records_load_without_a_band() {
    let json = r#"{"tested_cm_360":47.5,"signals":{},"pairs":[{"test":"Micro-adjustment","faster_cost":1.0,"slower_cost":0.5,"signal":"TooFast"}]}"#;
    let r: aim_trainer::data::RoundSignals = serde_json::from_str(json).unwrap();
    assert_eq!(r.pairs[0].band, None);
}

// ---------- Search plan and answers ----------

use aim_trainer::sensitivity::search_plan::SearchPlan;

#[test]
fn run_uses_plan_range_and_stores_start() {
    let plan = SearchPlan { min_cm: 15.0, max_cm: 30.0, start_cm: Some(27.0) };
    let setup = aim_trainer::data::setup::PlayerSetup::not_sure(33.0, chrono::Utc::now());
    let mut run = CalibrationRun::with_plan(1, plan, Some(setup.clone()), Some(33.0));
    assert_eq!(run.plan(), plan);
    skip_warmup(&mut run);
    assert_eq!(run.test_cm(40.0), 22.5, "block 1 is the midpoint; the start hint is ignored");
    assert_eq!(run.plan().start_cm, Some(27.0));
    let at = chrono::Utc::now();
    assert_eq!(run.run_record(800.0, at).search, (15.0, 30.0));
    let built = run.build_result(vec![], None, at);
    assert_eq!(built.plan, Some(plan));
    assert_eq!(built.setup, Some(setup));
    assert_eq!(built.own_cm_at_start, Some(33.0));
}

#[test]
fn old_run_record_json_defaults_search() {
    let old = r#"{"at":"2026-11-16T12:58:26Z","dpi":800.0,"rows":[]}"#;
    let r: aim_trainer::sensitivity::pooling::RunRecord = serde_json::from_str(old).unwrap();
    assert_eq!(r.search, (15.0, 80.0));
}

#[test]
fn pad_limited_flags_only_capped_results() {
    let old = r#"{"cm_360":30.0,"confidence":0.5,"psa_cm":30.0,"stat_cm":null,
        "rounds":[],"samples":[],"at":"2026-11-16T12:58:26Z"}"#;
    let mut r: aim_trainer::data::CalibrationResult = serde_json::from_str(old).unwrap();
    assert!(!r.pad_limited(), "no plan");
    r.plan = Some(SearchPlan { min_cm: 15.0, max_cm: 30.0, start_cm: None });
    r.cm_360 = 29.5;
    assert!(r.pad_limited());
    r.cm_360 = 31.0;
    assert!(!r.pad_limited(), "above the cap is not a cap hit (an older, wider pad)");
    r.plan = Some(SearchPlan { min_cm: 15.0, max_cm: 80.0, start_cm: None });
    r.cm_360 = 79.0;
    assert!(!r.pad_limited());
}
