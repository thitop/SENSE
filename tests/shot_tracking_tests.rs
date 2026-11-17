//! Shot trajectory classification, calibration shot recording, and task shot counting.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::{switching::SwitchingTask, Task};
use aim_trainer::sensitivity::path_recorder::{
    MousePathRecorder, MouseSample, ShotMovementPath, ShotTrajectoryPattern,
};
use aim_trainer::sensitivity::SensitivityEngine;
use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};

/// Build a path from `(ms since previous sample, dx, dy)` steps.
fn path(steps: &[(u64, f32, f32)]) -> ShotMovementPath {
    let start = Instant::now();
    let mut t = start;
    let mut pos = Vec2::ZERO;
    let samples = steps
        .iter()
        .map(|&(ms, dx, dy)| {
            t += Duration::from_millis(ms);
            let delta = Vec2::new(dx, dy);
            pos += delta;
            MouseSample { time: t, delta, accumulated_pos: pos }
        })
        .collect();
    ShotMovementPath::from_samples(samples, start, t)
}

fn repeat(n: usize, step: (u64, f32, f32)) -> Vec<(u64, f32, f32)> {
    vec![step; n]
}

#[test]
fn stopping_short_then_correcting_the_same_way_is_undershoot() {
    // Fast flick that covers ~75% of the way, a 150 ms stall, then a slower
    // correction in the same direction.
    let mut steps = repeat(10, (2, 15.0, 0.0));
    steps.push((150, 6.0, 0.0));
    steps.extend(repeat(7, (2, 6.0, 0.0)));
    assert_eq!(path(&steps).pattern, Some(ShotTrajectoryPattern::Undershoot));
}

#[test]
fn smooth_flick_that_decelerates_onto_target_is_direct() {
    let steps: Vec<_> = [2.0, 6.0, 12.0, 20.0, 24.0, 20.0, 12.0, 6.0, 2.0, 1.0]
        .iter()
        .map(|&dx| (2, dx, 0.0))
        .collect();
    assert_eq!(path(&steps).pattern, Some(ShotTrajectoryPattern::Direct));
}

#[test]
fn flying_past_and_coming_back_is_overshoot() {
    let mut steps = repeat(10, (2, 20.0, 0.0));
    steps.extend(repeat(5, (2, -12.0, 0.0)));
    assert_eq!(path(&steps).pattern, Some(ShotTrajectoryPattern::Overshoot));
}

#[test]
fn click_without_mouse_movement_still_counts_as_a_shot() {
    let mut r = MousePathRecorder::new();
    r.start_shot();
    r.record_sample(1.0, 0.0);
    let p = r.end_shot().expect("a click always ends a shot");

    let mut engine = SensitivityEngine::new(800.0);
    engine.record_calibration_shot(47.5, false, 300.0, &p);
    // Accuracy and reaction time are real data for the analyzer...
    assert_eq!(engine.analyzer.samples.len(), 1);
    // ...but with no trajectory to classify, it can't move the PSA search.
    assert_eq!(engine.psa.shots_in_round(), 0);
}

fn ray_to(p: Vec3) -> Ray {
    let origin = Vec3::new(0.0, 1.5, 0.0);
    Ray { origin, direction: (p - origin).normalize() }
}

#[test]
fn shooting_the_wrong_switching_target_is_one_miss() {
    use aim_trainer::game::hit_detection::intersect_ray_sphere;
    let mut t = SwitchingTask::new(25, 4, 0.32);
    // Targets spawn at random. Find a layout with an inactive target whose line of fire
    // doesn't also pass through the active one (that click would rightly count as a hit).
    let wrong = (0..100)
        .find_map(|_| {
            t.start();
            let targets = t.get_render_targets();
            // Inactive targets render smaller than the base radius, the active one larger.
            let (active, _) = *targets.iter().find(|(_, r)| *r > 0.32)?;
            targets
                .iter()
                .filter(|(_, r)| *r < 0.32)
                .map(|(p, _)| *p)
                .find(|p| intersect_ray_sphere(ray_to(*p), active, 0.32).is_none())
        })
        .expect("a clear shot at an inactive target");
    assert!(!t.on_click(ray_to(wrong)));
    let m = t.get_metrics();
    assert_eq!((m.shots_fired, m.misses), (1, 1));
}

#[test]
fn flick_counts_targets_that_time_out_unshot() {
    use aim_trainer::game::tasks::flick::FlickTask;
    let mut t = FlickTask::new(usize::MAX, 0.35).with_time_limit(0.0);
    t.start();
    assert_eq!(t.expired_targets(), 0);
    let ray = ray_to(Vec3::new(0.0, 1.5, -20.0));
    t.update(0.016, ray);
    assert_eq!(t.expired_targets(), 1, "the first target timed out");
    t.update(0.016, ray);
    assert_eq!(t.expired_targets(), 2, "its replacement timed out too");
}

#[test]
fn hit_targets_are_not_counted_as_timed_out() {
    use aim_trainer::game::tasks::flick::FlickTask;
    let mut t = FlickTask::new(usize::MAX, 0.35);
    t.start();
    let (pos, _) = t.get_render_targets()[0];
    assert!(t.on_click(ray_to(pos)));
    std::thread::sleep(Duration::from_millis(150)); // past the 120 ms hit animation
    t.update(0.016, ray_to(pos));
    assert_eq!(t.expired_targets(), 0);
}
