//! Cues for targets outside the view: detecting new spawns, where they are, sound panning,
//! edge arrows, and segments that start without a countdown.

use aim_trainer::engine::audio::pan_gains;
use aim_trainer::engine::camera::Camera;
use aim_trainer::game::spawn_cue::SpawnWatcher;
use aim_trainer::game::tasks::{TaskManager, TaskState, TaskType};
use aim_trainer::ui::hud::edge_point;
use glam::{Vec2, Vec3};

fn camera() -> Camera {
    Camera::new(16.0 / 9.0)
}

/// A point 9 m away, `yaw_deg` to the right and `pitch_deg` up from where the camera looks.
fn at(cam: &Camera, yaw_deg: f32, pitch_deg: f32) -> Vec3 {
    let (y, p) = (yaw_deg.to_radians(), pitch_deg.to_radians());
    let dir = cam.forward() * y.cos() * p.cos() + cam.right() * y.sin() * p.cos() + cam.up() * p.sin();
    cam.position + dir * 9.0
}

#[test]
fn targets_in_view_have_no_offscreen_direction() {
    let cam = camera();
    for (yaw, pitch) in [(0.0, 0.0), (30.0, 0.0), (-45.0, 0.0), (0.0, 30.0), (20.0, -20.0)] {
        assert_eq!(cam.offscreen_direction(at(&cam, yaw, pitch)), None, "{yaw}°, {pitch}°");
    }
}

#[test]
fn far_flicks_are_outside_the_view_and_point_the_right_way() {
    let cam = camera();
    let right = cam.offscreen_direction(at(&cam, 70.0, 0.0)).unwrap();
    assert!(right.x > 0.99, "{right}");
    let left = cam.offscreen_direction(at(&cam, -100.0, 5.0)).unwrap();
    assert!(left.x < -0.95 && left.y > 0.0, "{left}");
    let up = cam.offscreen_direction(at(&cam, 0.0, 50.0)).unwrap();
    assert!(up.y > 0.99, "{up}");
}

#[test]
fn a_target_behind_still_gets_a_direction() {
    let cam = camera();
    let dir = cam.offscreen_direction(cam.position - cam.forward() * 9.0).unwrap();
    assert!((dir.length() - 1.0).abs() < 1e-4);
    let behind_left = cam.offscreen_direction(at(&cam, -170.0, 0.0)).unwrap();
    assert!(behind_left.x < -0.99, "turn left, the short way: {behind_left}");
}

#[test]
fn the_watcher_reports_each_spawn_once() {
    let mut w = SpawnWatcher::default();
    let a = Vec3::new(1.0, 1.5, -9.0);
    assert_eq!(w.update(&[(a, 0.35)]), vec![a]);
    assert!(w.update(&[(a, 0.35)]).is_empty());
    let b = Vec3::new(-3.0, 2.0, -8.0);
    assert_eq!(w.update(&[(b, 0.35)]), vec![b], "the next target replaces the shot one");
    assert!(w.update(&[]).is_empty());
}

#[test]
fn a_moving_target_is_not_a_new_spawn() {
    let mut w = SpawnWatcher::default();
    let mut p = Vec3::new(0.0, 1.5, -9.0);
    w.update(&[(p, 0.35)]);
    // 4.5 m/s at 60 FPS.
    for _ in 0..120 {
        p.x += 4.5 / 60.0;
        assert!(w.update(&[(p, 0.35)]).is_empty());
    }
}

#[test]
fn clearing_the_watcher_reports_current_targets_again() {
    let mut w = SpawnWatcher::default();
    let a = Vec3::new(1.0, 1.5, -9.0);
    w.update(&[(a, 0.35)]);
    w.clear();
    assert_eq!(w.update(&[(a, 0.35)]), vec![a]);
}

#[test]
fn panning_keeps_loudness_and_follows_the_side() {
    for pan in [-1.0, -0.4, 0.0, 0.7, 1.0] {
        let (l, r) = pan_gains(pan);
        assert!((l * l + r * r - 1.0).abs() < 1e-5, "{pan}");
    }
    let (l, r) = pan_gains(-1.0);
    assert!(l > 0.99 && r < 0.01);
    let (l, r) = pan_gains(1.0);
    assert!(r > 0.99 && l < 0.01);
    assert_eq!(pan_gains(5.0), pan_gains(1.0));
}

#[test]
fn arrows_sit_on_the_inset_edge_toward_the_target() {
    let size = egui::vec2(1280.0, 720.0);
    let m = 40.0;
    let p = edge_point(size, Vec2::X, m);
    assert!((p.x - (1280.0 - m)).abs() < 1e-3 && (p.y - 360.0).abs() < 1e-3);
    let p = edge_point(size, Vec2::Y, m);
    assert!((p.y - m).abs() < 1e-3, "up is the top of the screen: {p:?}");
    let p = edge_point(size, Vec2::new(-1.0, -1.0).normalize(), m);
    assert!((p.y - (720.0 - m)).abs() < 1e-3 && p.x > m, "hits the bottom first: {p:?}");
}

#[test]
fn a_task_can_start_without_a_countdown() {
    let mut tm = TaskManager::new();
    tm.set_task(TaskType::Flick);
    tm.start_task_now();
    assert_eq!(tm.state, TaskState::Running);
}
