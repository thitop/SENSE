use aim_trainer::data::history::{SessionHistory, SessionRecord};
use aim_trainer::data::metrics::AggregatedTaskMetrics;
use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::analysis::{Habit, Measure};
use aim_trainer::habits::record::*;
use aim_trainer::habits::trend::*;
use chrono::{DateTime, Duration, TimeZone, Utc};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}
fn at(days: i64) -> DateTime<Utc> {
    t0() + Duration::days(days)
}
fn flick(landing: f32, strokes: u8) -> TargetRecord {
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
        band: Some(FlickBand::Far),
        ctx: PlayContext::default(),
    }
}
fn flick_log(landing: f32) -> AimLog {
    AimLog { targets: (0..10).map(|_| flick(landing, 1)).collect(), tracking: vec![] }
}
fn strokes_log(strokes: u8) -> AimLog {
    AimLog { targets: (0..12).map(|_| flick(1.0, strokes)).collect(), tracking: vec![] }
}
fn track_log(lag: f32) -> AimLog {
    let w = TrackingRecord {
        mean_along_deg: 0.0,
        lag_ms: lag,
        cross_rms_deg: 0.1,
        mean_speed_deg_s: 10.0,
        coverage: 1.0,
        radius_deg: 1.0,
        horizontal: true,
        ctx: PlayContext::default(),
    };
    AimLog { targets: vec![], tracking: (0..12).map(|_| w.clone()).collect() }
}
fn micro_log(share: f32) -> AimLog {
    let crossed = (share * 100.0).round() as usize;
    let targets = (0..100)
        .map(|i| TargetRecord {
            kind: AimKind::Micro,
            crossed_center: i < crossed,
            band: None,
            ..flick(1.0, 1)
        })
        .collect();
    AimLog { targets, tracking: vec![] }
}
fn rec(id: &str, task: &str, ts: DateTime<Utc>, cm: f64, aim: Option<AimLog>) -> SessionRecord {
    SessionRecord {
        session_id: id.into(),
        task_type: task.into(),
        timestamp: ts,
        duration_secs: 60.0,
        tested_cm_360: cm,
        tested_dpi: 800.0,
        metrics: AggregatedTaskMetrics::default(),
        aim,
        habits: None,
    }
}
fn hist(sessions: Vec<SessionRecord>) -> SessionHistory {
    SessionHistory { profile_id: "p".into(), sessions }
}
const FAR: Habit = Habit::FlickShort(FlickBand::Far);

#[test]
fn progress_counts_sessions_after_calibration_near_result() {
    let h = hist(vec![
        rec("a", "Flick", at(5), 40.0, Some(flick_log(0.7))), // before since
        rec("b", "Flick", at(12), 44.0, Some(flick_log(0.8))), // +10%
        rec("c", "Flick", at(11), 36.0, Some(flick_log(0.75))), // -10%
        rec("d", "Flick", at(13), 52.0, Some(flick_log(0.5))), // +30%
        rec("e", "Flick", at(14), 40.0, Some(flick_log(0.9))),
        rec("cal", "Calibration", at(15), 40.0, Some(flick_log(0.1))),
    ]);
    let p = progress(&h, FAR, at(10), 40.0);
    assert_eq!(p.points.len(), 3);
    assert!(!p.practising_elsewhere);
    assert!((p.points[0] - 0.75).abs() < 1e-4);
    assert!((p.points[1] - 0.8).abs() < 1e-4);
    assert!((p.points[2] - 0.9).abs() < 1e-4);
    // strictly after: a session exactly at `since` does not count
    let h2 = hist(vec![rec("x", "Flick", at(10), 40.0, Some(flick_log(0.8)))]);
    assert!(progress(&h2, FAR, at(10), 40.0).points.is_empty());
}

#[test]
fn practising_elsewhere_when_only_far_sessions() {
    let h = hist(vec![
        rec("a", "Flick", at(12), 60.0, Some(flick_log(0.8))),
        rec("b", "Flick", at(13), 20.0, Some(flick_log(0.8))),
    ]);
    let p = progress(&h, FAR, at(10), 40.0);
    assert!(p.points.is_empty());
    assert!(p.practising_elsewhere);
    let none = progress(&hist(vec![]), FAR, at(10), 40.0);
    assert!(none.points.is_empty() && !none.practising_elsewhere);
    // a far session without a value for the measure does not count
    let h3 = hist(vec![rec("a", "Flick", at(12), 60.0, Some(AimLog::default()))]);
    assert!(!progress(&h3, FAR, at(10), 40.0).practising_elsewhere);
}

#[test]
fn progress_keeps_last_ten() {
    let h = hist(
        (0..15)
            .rev() // insertion order is not time order
            .map(|i| {
                rec(&i.to_string(), "Flick", at(11 + i), 40.0, Some(flick_log(0.5 + i as f32 * 0.01)))
            })
            .collect(),
    );
    let p = progress(&h, FAR, at(10), 40.0);
    assert_eq!(p.points.len(), 10);
    assert!((p.points[0] - 0.55).abs() < 1e-4);
    assert!((p.points[9] - 0.64).abs() < 1e-4);
}

#[test]
fn rows_compare_with_previous_five_same_scenario() {
    let mut s = vec![];
    for i in 0..6 {
        s.push(rec(&format!("f{i}"), "Flick", at(i), 40.0, Some(flick_log(0.60 + i as f32 * 0.02))));
    }
    s.push(rec("t", "Tracking", at(6), 40.0, Some(flick_log(0.1))));
    s.push(rec("now", "Flick", at(7), 40.0, Some(flick_log(0.9))));
    s.push(rec("later", "Flick", at(8), 40.0, Some(flick_log(0.99))));
    let h = hist(s);
    let now = h.sessions.iter().find(|r| r.session_id == "now").unwrap();
    let rows = session_rows(&h, now);
    let row =
        rows.iter().find(|r| r.value.measure == Measure::FlickLanding(FlickBand::Far)).unwrap();
    // five most recent Flick before: f1..f5 = 0.62..0.70, mean 0.66
    assert!((row.previous_mean.unwrap() - 0.66).abs() < 1e-4);
    assert_eq!(row.change, Some(Change::Better));
}

#[test]
fn no_previous_means_no_change() {
    let h = hist(vec![rec("only", "Flick", at(1), 40.0, Some(flick_log(0.9)))]);
    let rows = session_rows(&h, &h.sessions[0]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].previous_mean, None);
    assert_eq!(rows[0].change, None);
}

fn change_for(task: &str, prev: AimLog, now: AimLog, m: Measure) -> Option<Change> {
    let h = hist(vec![
        rec("p", task, at(1), 40.0, Some(prev)),
        rec("n", task, at(2), 40.0, Some(now)),
    ]);
    let rows = session_rows(&h, &h.sessions[1]);
    rows.iter().find(|r| r.value.measure == m).unwrap().change
}

#[test]
fn change_uses_badness() {
    let far = Measure::FlickLanding(FlickBand::Far);
    assert_eq!(change_for("Flick", flick_log(0.80), flick_log(0.90), far), Some(Change::Better));
    assert_eq!(change_for("Flick", flick_log(1.2), flick_log(1.4), far), Some(Change::Worse));
    assert_eq!(
        change_for("Tracking", track_log(-30.0), track_log(-60.0), Measure::TrackingLag),
        Some(Change::Worse)
    );
    assert_eq!(
        change_for("Micro", micro_log(0.50), micro_log(0.52), Measure::MicroOvershoot),
        Some(Change::Same)
    );
    assert_eq!(
        change_for("Flick", strokes_log(4), strokes_log(3), Measure::FlickStrokes),
        Some(Change::Better)
    );
}

#[test]
fn zero_previous_badness_is_same_only_when_equal() {
    let m = Measure::MicroOvershoot;
    assert_eq!(change_for("Micro", micro_log(0.0), micro_log(0.0), m), Some(Change::Same));
    assert_eq!(change_for("Micro", micro_log(0.0), micro_log(0.2), m), Some(Change::Worse));
}

#[test]
fn sessions_without_aim_are_skipped() {
    let h = hist(vec![
        rec("a", "Flick", at(12), 40.0, None),
        rec("b", "Flick", at(13), 40.0, Some(flick_log(0.8))),
        rec("c", "Flick", at(14), 40.0, None),
    ]);
    let p = progress(&h, FAR, at(10), 40.0);
    assert_eq!(p.points.len(), 1);
    assert!(session_rows(&h, &h.sessions[0]).is_empty());
    let rows = session_rows(&h, &h.sessions[1]);
    assert!(rows.iter().all(|r| r.previous_mean.is_none()));
}
