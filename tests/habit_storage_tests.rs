use aim_trainer::data::{AggregatedTaskMetrics, SessionHistory, SessionRecord};
use aim_trainer::habits::record::{AimKind, AimLog, PlayContext, TargetRecord, TrackingRecord};
use aim_trainer::sensitivity::paired::Side;

const OLD: &str = r#"{"profile_id":"p","sessions":[{"session_id":"s1","task_type":"Flick",
"timestamp":"2026-01-01T00:00:00Z","duration_secs":30.0,"tested_cm_360":40.0,"tested_dpi":800.0,
"metrics":{"total_shots":2,"hits":1,"misses":1,"accuracy_pct":50.0,"avg_reaction_time_ms":300.0,
"median_reaction_time_ms":300.0,"avg_offset_m":0.1,"total_score":100,"max_streak":1,
"points":[
{"shot_number":1,"hit":true,"reaction_time_ms":300.0,"offset_from_center_m":0.1,"target_radius_m":0.35,"precision_percent":85.0,"mouse_distance_px":100.0,"peak_velocity_px_s":900.0},
{"shot_number":2,"hit":false,"reaction_time_ms":400.0,"offset_from_center_m":0.1,"target_radius_m":0.35,"precision_percent":0.0,"mouse_distance_px":80.0,"peak_velocity_px_s":700.0}]}}]}"#;

fn ctx() -> PlayContext {
    PlayContext { cm_360: 34.0, side: Some(Side::Faster), block: Some(2), warmup: false }
}

fn session(aim: Option<AimLog>) -> SessionRecord {
    SessionRecord {
        session_id: "s".into(),
        task_type: "Flick".into(),
        timestamp: chrono::Utc::now(),
        duration_secs: 30.0,
        tested_cm_360: 34.0,
        tested_dpi: 800.0,
        metrics: AggregatedTaskMetrics::default(),
        aim,
    }
}

#[test]
fn old_history_loads() {
    let h: SessionHistory = serde_json::from_str(OLD).unwrap();
    assert!(h.sessions[0].aim.is_none());
    assert_eq!(h.sessions[0].metrics.points.len(), 2);
}

#[test]
fn session_with_aim_round_trips() {
    let log = AimLog {
        targets: vec![TargetRecord {
            kind: AimKind::Flick,
            offset_deg: [10.0, -3.0],
            radius_deg: 1.5,
            first_landing: Some(0.9),
            strokes: 2,
            crossed_center: true,
            overshoot_deg: 0.4,
            time_to_hit_ms: Some(420.0),
            misses: vec![[0.5, 0.2]],
            band: None,
            ctx: ctx(),
        }],
        tracking: vec![TrackingRecord {
            mean_along_deg: 0.1,
            lag_ms: 20.0,
            cross_rms_deg: 0.3,
            mean_speed_deg_s: 30.0,
            coverage: 0.8,
            radius_deg: 1.0,
            horizontal: true,
            ctx: ctx(),
        }],
    };
    let s = session(Some(log.clone()));
    let back: SessionRecord = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back.aim, Some(log));
}

#[test]
fn session_without_aim_omits_field() {
    let json = serde_json::to_string(&session(None)).unwrap();
    assert!(!json.contains("\"aim\""));
}

#[test]
fn play_context_from_mode_and_factor() {
    let c = PlayContext::for_play(40.0, 0.9, Some((3, true)));
    assert_eq!((c.side, c.block, c.warmup), (Some(Side::Faster), Some(3), false));
    assert!((c.cm_360 - 36.0).abs() < 1e-4);
    let c = PlayContext::for_play(40.0, 1.1, Some((1, false)));
    assert_eq!((c.side, c.block, c.warmup), (Some(Side::Slower), None, true));
    let c = PlayContext::for_play(40.0, 1.0, None);
    assert_eq!((c.side, c.block, c.warmup), (None, None, false));
}
