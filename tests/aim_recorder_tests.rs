//! AimRecorder: live play becomes degree-based aim records.

use aim_trainer::game::tasks::AimTarget;
use aim_trainer::habits::record::{AimKind, AimLog, PlayContext};
use aim_trainer::habits::recorder::AimRecorder;
use aim_trainer::habits::strokes::band_for_angle;
use aim_trainer::sensitivity::paired::Side;
use aim_trainer::sensitivity::tracking_signal::TrackingAnalyzer;
use glam::{Vec2, Vec3};

fn dir(yaw_deg: f32, pitch_deg: f32) -> Vec3 {
    let (y, p) = (yaw_deg.to_radians(), pitch_deg.to_radians());
    Vec3::new(p.cos() * y.cos(), p.sin(), p.cos() * y.sin())
}

fn target(id: u64, yaw_deg: f32, kind: AimKind) -> AimTarget {
    AimTarget { id, center: dir(yaw_deg, 0.0) * 10.0, radius: 0.5, kind, band: None }
}

fn ctx(cm: f32) -> PlayContext {
    PlayContext { cm_360: cm, side: Some(Side::Faster), block: Some(1), warmup: false }
}

/// One finished flick target 10 degrees away, moved `total_counts` counts at `dpc` degrees each.
fn play_hit(r: &mut AimRecorder, id: u64, n: usize, dpc: f32, total_counts: f32) {
    let t = target(id, 10.0, AimKind::Flick);
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(t));
    for i in 0..n {
        r.push_mouse(0.05 + 0.2 * (i as f64 + 1.0) / n as f64, total_counts / n as f32, 0.0, dpc);
    }
    r.on_click(0.4, Vec3::ZERO, dir(10.0, 0.0), true);
}

#[test]
fn same_hand_movement_same_record_at_any_dpi() {
    let d = 7.0 / 800.0;
    let (mut a, mut b) = (AimRecorder::new(), AimRecorder::new());
    a.set_context(ctx(30.0));
    b.set_context(ctx(30.0));
    play_hit(&mut a, 1, 8, d, 800.0);
    play_hit(&mut b, 1, 8, d / 8.0, 6400.0);
    a.commit();
    b.commit();
    let (la, lb) = (a.take_log(), b.take_log());
    assert_eq!(la.targets.len(), 1);
    let (ra, rb) = (&la.targets[0], &lb.targets[0]);
    assert_eq!(ra.strokes, rb.strokes);
    assert!((ra.first_landing.unwrap() - rb.first_landing.unwrap()).abs() < 1e-3);
    assert!((ra.offset_deg[0] - rb.offset_deg[0]).abs() < 1e-4);
    assert_eq!(ra.time_to_hit_ms, rb.time_to_hit_ms);
}

#[test]
fn hit_records_time_and_landing() {
    let mut r = AimRecorder::new();
    r.set_context(ctx(30.0));
    play_hit(&mut r, 1, 8, 7.0 / 800.0, 800.0);
    r.commit();
    let rec = r.take_log().targets.remove(0);
    assert!((rec.time_to_hit_ms.unwrap() - 400.0).abs() < 1.0);
    assert!((rec.first_landing.unwrap() - 0.7).abs() < 0.02, "{:?}", rec.first_landing);
    assert!((rec.offset_deg[0] - 10.0).abs() < 1e-3);
    assert!(rec.offset_deg[1].abs() < 1e-3);
    assert!(rec.radius_deg > 2.5 && rec.radius_deg < 3.2);
}

#[test]
fn expired_target_has_no_time() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    r.track(1.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(2, 20.0, AimKind::Flick)));
    r.track(2.0, Vec3::ZERO, dir(0.0, 0.0), None);
    r.commit();
    let log = r.take_log();
    assert_eq!(log.targets.len(), 2);
    assert_eq!(log.targets[0].time_to_hit_ms, None);
}

#[test]
fn misses_capped_at_eight() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    for i in 0..11 {
        assert!(r.on_click(0.1 * i as f64, Vec3::ZERO, dir(9.0, 0.0), false).is_none());
    }
    r.on_click(2.0, Vec3::ZERO, dir(10.0, 0.0), true);
    r.commit();
    assert_eq!(r.take_log().targets[0].misses.len(), 8);
}

#[test]
fn miss_position_is_along_and_across() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    r.on_click(0.1, Vec3::ZERO, dir(9.0, 1.0), false);
    r.on_click(0.3, Vec3::ZERO, dir(10.0, 0.0), true);
    r.commit();
    let m = r.take_log().targets[0].misses[0];
    assert!((m[0] + 1.0).abs() < 0.02 && (m[1] + 1.0).abs() < 0.02, "{m:?}");
}

#[test]
fn miss_on_expiring_target_is_kept() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    r.on_click(0.1, Vec3::ZERO, dir(9.0, 0.0), false);
    r.track(1.0, Vec3::ZERO, dir(0.0, 0.0), None);
    r.commit();
    let rec = r.take_log().targets.remove(0);
    assert_eq!(rec.misses.len(), 1);
    assert_eq!(rec.time_to_hit_ms, None);
}

#[test]
fn hit_returns_shot_offset_at_target_depth() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    let s = r.on_click(0.2, Vec3::ZERO, dir(9.0, 0.0), true).unwrap();
    assert!((s.radius_m - 0.5).abs() < 1e-6);
    let expect = 10.0 * 1.0f32.to_radians().sin();
    assert!((s.offset_m - expect).abs() < 1e-3, "{}", s.offset_m);
    assert!(r.on_click(0.3, Vec3::ZERO, dir(9.0, 0.0), true).is_none());
}

#[test]
fn click_without_target_is_none() {
    let mut r = AimRecorder::new();
    assert!(r.on_click(0.0, Vec3::ZERO, dir(0.0, 0.0), false).is_none());
}

#[test]
fn discarded_segment_does_not_count() {
    let mut r = AimRecorder::new();
    let mut id = 0;
    for (n, keep) in [(3, true), (2, false), (2, true)] {
        for _ in 0..n {
            id += 1;
            play_hit(&mut r, id, 8, 0.00875, 800.0);
        }
        if keep {
            r.commit()
        } else {
            r.discard_pending()
        }
    }
    assert_eq!(r.take_log().targets.len(), 5);
}

#[test]
fn discard_drops_target_in_progress() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    r.discard_pending();
    r.track(0.1, Vec3::ZERO, dir(0.0, 0.0), None);
    r.commit();
    assert!(r.take_log().targets.is_empty());
}

#[test]
fn take_log_empties_and_drops_current() {
    let mut r = AimRecorder::new();
    play_hit(&mut r, 1, 8, 0.00875, 800.0);
    r.commit();
    r.track(1.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(2, 10.0, AimKind::Flick)));
    assert_eq!(r.take_log().targets.len(), 1);
    r.track(2.0, Vec3::ZERO, dir(0.0, 0.0), None);
    r.commit();
    assert!(r.take_log().targets.is_empty());
}

#[test]
fn reset_drops_everything() {
    let mut r = AimRecorder::new();
    play_hit(&mut r, 1, 8, 0.00875, 800.0);
    r.commit();
    play_hit(&mut r, 2, 8, 0.00875, 800.0);
    r.reset();
    r.commit();
    let log = r.take_log();
    assert!(log.targets.is_empty() && log.tracking.is_empty());
}

#[test]
fn context_is_captured_at_begin() {
    let mut r = AimRecorder::new();
    r.set_context(ctx(30.0));
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    r.set_context(ctx(40.0));
    r.on_click(0.3, Vec3::ZERO, dir(10.0, 0.0), true);
    r.commit();
    assert_eq!(r.take_log().targets[0].ctx.cm_360, 30.0);
}

#[test]
fn degenerate_target_is_finite_and_round_trips() {
    let mut r = AimRecorder::new();
    r.set_context(ctx(30.0));
    let t = target(1, 0.0, AimKind::Micro);
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(t));
    r.on_click(0.3, Vec3::ZERO, dir(0.0, 0.0), true);
    r.commit();
    let log = r.take_log();
    let rec = &log.targets[0];
    assert_eq!(rec.first_landing, None);
    assert_eq!(rec.strokes, 0);
    assert!(!rec.crossed_center);
    assert_eq!(rec.overshoot_deg, 0.0);
    assert!(rec.offset_deg.iter().all(|v| v.is_finite()) && rec.radius_deg.is_finite());
    let json = serde_json::to_string(&log).unwrap();
    let back: AimLog = serde_json::from_str(&json).unwrap();
    assert_eq!(back, log);
}

#[test]
fn non_finite_input_never_reaches_a_record() {
    let mut r = AimRecorder::new();
    let t = AimTarget {
        id: 1,
        center: Vec3::NAN,
        radius: f32::NAN,
        kind: AimKind::Flick,
        band: None,
    };
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(t));
    r.push_mouse(0.1, f32::NAN, 1.0, 0.01);
    r.on_click(0.2, Vec3::ZERO, Vec3::NAN, false);
    r.on_click(0.3, Vec3::ZERO, dir(0.0, 0.0), true);
    r.commit();
    let log = r.take_log();
    let rec = &log.targets[0];
    assert!(rec.offset_deg.iter().all(|v| v.is_finite()));
    assert!(rec.radius_deg.is_finite());
    assert!(rec.misses.iter().flatten().all(|v| v.is_finite()));
    assert!(rec.time_to_hit_ms.map_or(true, f32::is_finite));
    serde_json::to_string(&log).unwrap();
}

#[test]
fn moving_target_has_no_landing() {
    let mut r = AimRecorder::new();
    r.track(0.0, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Moving)));
    for i in 0..8 {
        r.push_mouse(0.05 + 0.03 * i as f64, 100.0, 0.0, 0.00875);
    }
    r.on_click(0.4, Vec3::ZERO, dir(10.0, 0.0), true);
    r.commit();
    let rec = r.take_log().targets.remove(0);
    assert_eq!(rec.first_landing, None);
    assert_eq!(rec.kind, AimKind::Moving);
    assert_eq!(rec.band, None);
}

#[test]
fn finished_id_is_not_begun_again() {
    let mut r = AimRecorder::new();
    play_hit(&mut r, 1, 8, 0.00875, 800.0);
    r.track(0.5, Vec3::ZERO, dir(0.0, 0.0), Some(target(1, 10.0, AimKind::Flick)));
    r.track(0.6, Vec3::ZERO, dir(0.0, 0.0), None);
    r.commit();
    assert_eq!(r.take_log().targets.len(), 1);
}

#[test]
fn flick_band_comes_from_angle_when_target_has_none() {
    let mut r = AimRecorder::new();
    play_hit(&mut r, 1, 8, 0.00875, 800.0);
    r.commit();
    let rec = r.take_log().targets.remove(0);
    assert_eq!(rec.band, band_for_angle(10.0));
}

#[test]
fn tracking_windows_take_context() {
    let mut r = AimRecorder::new();
    r.set_context(ctx(33.0));
    let mut an = TrackingAnalyzer::new(1.5);
    let mut ws = Vec::new();
    for _ in 0..2000 {
        an.push(1.0 / 60.0, Vec2::new(-0.5, 0.0), Vec2::new(20.0, 0.0), 1.0);
        ws.extend(an.take_windows());
        if !ws.is_empty() {
            break;
        }
    }
    let w = ws[0];
    let mut bad = w;
    bad.lag_ms = f32::NAN;
    r.push_tracking(&[w, bad]);
    r.commit();
    let log = r.take_log();
    assert_eq!(log.tracking.len(), 2);
    let t = &log.tracking[0];
    assert_eq!(
        (t.mean_along_deg, t.lag_ms, t.cross_rms_deg),
        (w.mean_along_deg, w.lag_ms, w.cross_rms_deg)
    );
    assert_eq!(
        (t.mean_speed_deg_s, t.coverage, t.radius_deg, t.horizontal),
        (w.mean_speed_deg_s, w.coverage, w.radius_deg, w.horizontal)
    );
    assert!(t.mean_speed_deg_s > 0.0 && t.radius_deg == 1.5);
    assert_eq!(t.ctx.cm_360, 33.0);
    assert_eq!(log.tracking[1].lag_ms, 0.0);
}
