//! Tracking scores time on target: clicks are never shots, and History shows time on target.

use aim_trainer::data::{AggregatedTaskMetrics, SessionRecord, TrackingStats};
use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::tracking::TrackingTask;
use aim_trainer::game::tasks::{Task, TaskManager, TaskState, TaskType};
use aim_trainer::ui::hud::tracking_live_text;
use aim_trainer::ui::screens::history::detail_stats;
use glam::Vec3;

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

/// Aim straight at the live target.
fn at_target(t: &TrackingTask) -> Ray {
    let (pos, _) = t.get_render_targets()[0];
    Ray { origin: EYE, direction: (pos - EYE).normalize() }
}

fn away() -> Ray {
    Ray { origin: EYE, direction: Vec3::Y }
}

fn started() -> TrackingTask {
    let mut t = TrackingTask::seeded(30.0, 0.35, 7);
    t.start();
    t
}

#[test]
fn clicks_are_not_shots() {
    let mut t = started();
    let r = at_target(&t);
    t.update(0.016, r);
    assert!(!t.takes_shots());
    assert!(!t.on_click(at_target(&t)), "a click on target is not a hit");
}

#[test]
fn time_on_target_builds_the_score_without_clicking() {
    let mut t = started();
    for _ in 0..60 {
        let r = at_target(&t);
        t.update(0.01, r);
    }
    for _ in 0..40 {
        t.update(0.01, away());
    }
    let s = t.tracking_stats().unwrap();
    assert!((s.on_target_pct - 60.0).abs() < 2.0, "{s:?}");
    assert!((s.time_on_target_secs - 0.6).abs() < 0.02, "{s:?}");
    assert_eq!(s.score(), (s.on_target_pct * 100.0) as u64);
}

#[test]
fn longest_stretch_resets_when_slipping_off() {
    let mut t = started();
    for _ in 0..30 {
        let r = at_target(&t);
        t.update(0.01, r);
    }
    t.update(0.01, away());
    for _ in 0..10 {
        let r = at_target(&t);
        t.update(0.01, r);
    }
    let s = t.tracking_stats().unwrap();
    assert!((s.longest_on_target_secs - 0.3).abs() < 0.02, "{s:?}");
}

#[test]
fn on_target_now_follows_the_crosshair() {
    let mut t = started();
    let r = at_target(&t);
    t.update(0.016, r);
    assert!(t.on_target_now());
    t.update(0.016, away());
    assert!(!t.on_target_now());
}

#[test]
fn start_resets_the_stats() {
    let mut t = started();
    for _ in 0..20 {
        let r = at_target(&t);
        t.update(0.01, r);
    }
    t.start();
    let s = t.tracking_stats().unwrap();
    assert_eq!(s.time_on_target_secs, 0.0);
    assert_eq!(s.longest_on_target_secs, 0.0);
}

#[test]
fn other_scenarios_still_take_shots() {
    let mut m = TaskManager::new();
    for task in [TaskType::Flick, TaskType::Switching, TaskType::MicroAdjust, TaskType::Speed, TaskType::MovingTargets] {
        m.set_task(task);
        assert!(m.takes_shots(), "{task:?}");
        assert!(m.tracking_stats().is_none(), "{task:?}");
    }
    m.set_task(TaskType::Tracking);
    assert!(!m.takes_shots());
    assert!(m.tracking_stats().is_none(), "not started yet");
    m.start_task_now();
    assert_eq!(m.state, TaskState::Running);
    assert!(m.tracking_stats().is_some());
}

const STATS: TrackingStats = TrackingStats { on_target_pct: 62.4, time_on_target_secs: 18.7, longest_on_target_secs: 3.4 };

#[test]
fn tracking_session_accuracy_is_time_on_target() {
    let m = AggregatedTaskMetrics::tracking(STATS);
    assert_eq!(m.accuracy_pct, 62.4);
    assert_eq!(m.total_score, 6240);
    assert_eq!(m.total_shots, 0);
    assert_eq!(m.tracking, Some(STATS));
}

#[test]
fn old_sessions_load_without_tracking_stats() {
    let m: AggregatedTaskMetrics = serde_json::from_str(
        r#"{"total_shots":0,"hits":0,"misses":0,"accuracy_pct":0.0,"avg_reaction_time_ms":0.0,
            "median_reaction_time_ms":0.0,"avg_offset_m":0.0,"total_score":5000,"max_streak":0,"points":[]}"#,
    )
    .unwrap();
    assert!(m.tracking.is_none());
    let json = serde_json::to_string(&AggregatedTaskMetrics::compute(Vec::new(), 0, 0)).unwrap();
    assert!(!json.contains("tracking"));
    let back: AggregatedTaskMetrics = serde_json::from_str(&serde_json::to_string(&AggregatedTaskMetrics::tracking(STATS)).unwrap()).unwrap();
    assert_eq!(back.tracking, Some(STATS));
}

#[test]
fn history_shows_time_on_target_for_tracking() {
    let s = SessionRecord {
        session_id: "t".into(),
        task_type: "Smooth Tracking".into(),
        timestamp: chrono::Utc::now(),
        duration_secs: 30.0,
        tested_cm_360: 34.2,
        tested_dpi: 800.0,
        metrics: AggregatedTaskMetrics::tracking(STATS),
        aim: None,
        habits: None,
    };
    let labels: Vec<&str> = detail_stats(&s).iter().map(|d| d.label).collect();
    assert_eq!(labels, ["Score", "On target", "Time on target", "Longest on target", "Sensitivity", "Duration"]);
    let on = &detail_stats(&s)[1];
    assert_eq!(on.value, "62");
}

#[test]
fn live_bar_text() {
    assert_eq!(tracking_live_text(&STATS), "Score 6240 · 62% on target");
}

mod ticker {
    use aim_trainer::engine::audio::{Sfx, TrackingTicker};

    /// Ticks over `frames` frames of `dt`, all on target.
    fn ticks(t: &mut TrackingTicker, frames: usize, dt: f32, on: bool) -> usize {
        (0..frames).filter(|_| t.update(dt, on)).count()
    }

    #[test]
    fn ticks_at_once_then_ten_times_a_second() {
        let mut t = TrackingTicker::default();
        assert!(t.update(0.016, true), "first frame on target ticks");
        // 1 s more at ~60 fps: 10 more ticks.
        assert_eq!(ticks(&mut t, 60, 1.0 / 60.0, true), 10);
    }

    #[test]
    fn silent_off_target_and_ticks_again_on_return() {
        let mut t = TrackingTicker::default();
        assert_eq!(ticks(&mut t, 30, 0.016, false), 0);
        assert!(t.update(0.016, true));
        assert!(!t.update(0.016, true));
        assert!(!t.update(0.016, false));
        assert!(t.update(0.016, true), "getting back on ticks right away");
    }

    #[test]
    fn ticks_keep_pace_at_low_frame_rates() {
        let mut t = TrackingTicker::default();
        t.update(0.0, true);
        // 30 fps for 1 s: still 10 ticks.
        assert_eq!(ticks(&mut t, 30, 1.0 / 30.0, true), 10);
    }

    #[test]
    fn tick_sound_is_shorter_than_the_interval() {
        assert!(Sfx::TrackTick.max_secs() < TrackingTicker::INTERVAL_SECS);
    }
}

/// Calibration plays the tracking test as its own task: clicks there must not be shots
/// (no hit/miss sound, nothing recorded), while flick and micro segments still take them.
#[test]
fn calibration_tracking_segments_ignore_clicks() {
    use aim_trainer::game::calibration_run::{CalibrationRun, SegmentEnd};
    use aim_trainer::sensitivity::combiner::TestKind;

    let mut run = CalibrationRun::new(7);
    let mut tracking_segments = 0;
    for _ in 0..200 {
        let Some(seg) = run.segment() else { break };
        let task = run.make_task();
        assert_eq!(task.takes_shots(), seg.kind != TestKind::Tracking, "{:?}", seg);
        if seg.kind == TestKind::Tracking {
            tracking_segments += 1;
        }
        if run.finish_segment() == SegmentEnd::Completed {
            break;
        }
    }
    assert!(tracking_segments >= 2, "warm-up and counted tracking were both checked");
}
