//! Micro-adjustment: the degree-based shot classifier and seeded target placement.

use aim_trainer::engine::camera::Camera;
use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::micro_adjust::{MicroAdjustTask, MICRO_ANGLE_DEG};
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::combiner::{DirectionalSignal, Signal};
use aim_trainer::sensitivity::micro_signal::{classify_micro, deg_per_count};
use aim_trainer::sensitivity::path_recorder::MouseSample;
use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};

const CM_360: f64 = 30.0;

/// Mouse samples for `(ms since previous sample, dx°, dy°)` steps at `dpi`. Counts are whole
/// numbers like real raw input; the fraction carries to the next sample.
fn path(dpi: f64, steps: &[(u64, f32, f32)]) -> (Vec<MouseSample>, Instant, f32) {
    let dpc = deg_per_count(dpi, CM_360);
    let start = Instant::now();
    let (mut t, mut pos, mut carry) = (start, Vec2::ZERO, Vec2::ZERO);
    let mut samples = Vec::new();
    for &(ms, dx, dy) in steps {
        t += Duration::from_millis(ms);
        let want = Vec2::new(dx, dy) / dpc + carry;
        let delta = want.round();
        carry = want - delta;
        if delta == Vec2::ZERO {
            continue;
        }
        pos += delta;
        samples.push(MouseSample { time: t, delta, accumulated_pos: pos });
    }
    (samples, start, dpc)
}

/// `n` samples 1 ms apart moving `total` degrees along `dir`, speeding up then slowing down.
fn stroke(n: usize, total: f32, dir: Vec2) -> Vec<(u64, f32, f32)> {
    let weights: Vec<f32> = (0..n).map(|i| ((i as f32 + 0.5) / n as f32 * std::f32::consts::PI).sin()).collect();
    let sum: f32 = weights.iter().sum();
    weights
        .iter()
        .map(|w| {
            let d = dir.normalize() * total * w / sum;
            (1, d.x, d.y)
        })
        .collect()
}

fn pause(ms: u64, mut s: Vec<(u64, f32, f32)>) -> Vec<(u64, f32, f32)> {
    s[0].0 += ms;
    s
}

fn classify(dpi: f64, steps: &[(u64, f32, f32)], target: Vec2) -> Option<(Signal, u32, bool)> {
    let (samples, start, dpc) = path(dpi, steps);
    classify_micro(&samples, start, dpc, target).map(|m| (m.signal().unwrap(), m.features.submovements, m.features.crossed_center))
}

const RIGHT_3: Vec2 = Vec2::new(3.0, 0.0);

#[test]
fn one_smooth_move_onto_the_target_is_direct() {
    let steps = stroke(40, 3.0, Vec2::X);
    assert_eq!(classify(800.0, &steps, RIGHT_3), Some((Signal::Direct, 1, false)));
}

#[test]
fn going_past_the_centre_and_back_is_too_fast() {
    let mut steps = stroke(40, 4.2, Vec2::X);
    steps.extend(pause(60, stroke(20, -1.2, Vec2::X)));
    let (signal, _, crossed) = classify(800.0, &steps, RIGHT_3).unwrap();
    assert_eq!(signal, Signal::TooFast);
    assert!(crossed);
}

#[test]
fn passing_the_centre_slightly_without_coming_back_is_direct() {
    let steps = stroke(40, 3.2, Vec2::X);
    assert_eq!(classify(800.0, &steps, RIGHT_3).unwrap().0, Signal::Direct);
}

#[test]
fn creeping_up_in_three_steps_is_too_slow() {
    let mut steps = stroke(30, 1.6, Vec2::X);
    steps.extend(pause(80, stroke(20, 0.9, Vec2::X)));
    steps.extend(pause(80, stroke(15, 0.5, Vec2::X)));
    assert_eq!(classify(800.0, &steps, RIGHT_3), Some((Signal::TooSlow, 3, false)));
}

#[test]
fn one_correction_is_normal() {
    let mut steps = stroke(30, 2.6, Vec2::X);
    steps.extend(pause(80, stroke(15, 0.4, Vec2::X)));
    assert_eq!(classify(800.0, &steps, RIGHT_3), Some((Signal::Direct, 2, false)));
}

#[test]
fn works_along_any_direction() {
    let target = Vec2::new(-2.0, -2.0); // up and to the left
    let dir = target.normalize();
    let mut steps = stroke(40, target.length() + 1.2, dir);
    steps.extend(pause(60, stroke(20, -1.2, dir)));
    assert_eq!(classify(800.0, &steps, target).unwrap().0, Signal::TooFast);
    // Moving the other way (away from the target) never counts as crossing it.
    assert_ne!(classify(800.0, &stroke(40, 4.0, -dir), target).unwrap().0, Signal::TooFast);
}

#[test]
fn same_hand_movement_gives_the_same_answer_at_any_dpi() {
    let mut slow = stroke(30, 1.6, Vec2::X);
    slow.extend(pause(80, stroke(20, 0.9, Vec2::X)));
    slow.extend(pause(80, stroke(15, 0.5, Vec2::X)));
    let mut fast = stroke(40, 4.2, Vec2::X);
    fast.extend(pause(60, stroke(20, -1.2, Vec2::X)));
    for dpi in [400.0, 800.0, 1600.0, 3200.0] {
        assert_eq!(classify(dpi, &slow, RIGHT_3).unwrap().0, Signal::TooSlow, "dpi {dpi}");
        assert_eq!(classify(dpi, &fast, RIGHT_3).unwrap().0, Signal::TooFast, "dpi {dpi}");
        assert_eq!(classify(dpi, &stroke(40, 3.0, Vec2::X), RIGHT_3).unwrap().0, Signal::Direct, "dpi {dpi}");
    }
}

fn two_strokes(first: f32, second: f32) -> Vec<(u64, f32, f32)> {
    let mut s = stroke(30, first, Vec2::X);
    s.extend(pause(80, stroke(15, second, Vec2::X)));
    s
}

#[test]
fn small_return_after_overshooting_is_seen_at_low_dpi() {
    // At 400 DPI the 0.6° return is ~1 count per sample. The count-based flick classifier
    // misses it there (Direct) but not at 3200 DPI (Overshoot).
    for dpi in [400.0, 3200.0] {
        assert_eq!(classify(dpi, &two_strokes(3.6, -0.6), RIGHT_3).unwrap().0, Signal::TooFast, "dpi {dpi}");
    }
}

#[test]
fn one_correction_is_normal_whatever_its_size() {
    // The flick classifier calls 2.4 + 0.6 an undershoot but 2.0 + 1.0 direct.
    for (a, b) in [(2.4, 0.6), (2.0, 1.0), (1.5, 1.5)] {
        assert_eq!(classify(800.0, &two_strokes(a, b), RIGHT_3).unwrap().0, Signal::Direct, "{a} + {b}");
    }
}

#[test]
fn tiny_final_nudges_are_not_creeping() {
    let mut steps = two_strokes(2.6, 0.2);
    steps.extend(pause(80, stroke(10, 0.2, Vec2::X)));
    assert_eq!(classify(800.0, &steps, RIGHT_3).unwrap().0, Signal::Direct);
}

#[test]
fn no_movement_says_nothing() {
    assert_eq!(classify(800.0, &[], RIGHT_3), None);
    assert_eq!(classify(800.0, &stroke(5, 0.02, Vec2::X), RIGHT_3), None);
}

#[test]
fn features_report_time_to_settle() {
    let mut steps = stroke(30, 2.6, Vec2::X);
    steps.extend(pause(80, stroke(15, 0.4, Vec2::X)));
    let (samples, start, dpc) = path(800.0, &steps);
    let m = classify_micro(&samples, start, dpc, RIGHT_3).unwrap();
    // Settles during the second stroke, which starts ~110 ms in.
    assert!(m.features.settle_ms > 100.0 && m.features.settle_ms < 140.0, "{}", m.features.settle_ms);
    assert!((m.features.target_deg - 3.0).abs() < 1e-4);
}

// ---------- Task ----------

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

fn aim_toward(yaw_deg: f32, pitch_deg: f32) -> Ray {
    let mut cam = Camera::new(16.0 / 9.0);
    cam.yaw = yaw_deg.to_radians();
    cam.pitch = pitch_deg.to_radians();
    Ray { origin: EYE, direction: cam.forward() }
}

/// Spawn offsets of a whole seeded run, aiming with `aim` throughout.
fn offsets(seed: u64, aim: Ray) -> Vec<Vec2> {
    let mut task = MicroAdjustTask::seeded(6, 0.16, seed);
    task.start();
    assert!(task.get_render_targets().is_empty(), "first target waits for the aim");
    let mut out = Vec::new();
    for _ in 0..20 {
        task.update(0.001, aim);
        if task.is_complete() {
            break;
        }
        let Some(spawn) = task.current_spawn() else { continue };
        if out.last() != Some(&spawn.offset_deg) {
            out.push(spawn.offset_deg);
            let (pos, _) = *task.get_render_targets().last().unwrap();
            assert!(task.on_click(Ray { origin: EYE, direction: (pos - EYE).normalize() }));
        }
    }
    out
}

#[test]
fn targets_sit_a_micro_angle_from_the_aim() {
    let (lo, hi) = MICRO_ANGLE_DEG;
    let got = offsets(1, aim_toward(-90.0, 0.0));
    assert_eq!(got.len(), 6);
    for o in got {
        assert!(o.length() >= lo - 0.05 && o.length() <= hi + 0.05, "{o}");
    }
}

#[test]
fn same_seed_same_targets_relative_to_the_aim() {
    let a = offsets(2, aim_toward(-90.0, 0.0));
    let b = offsets(2, aim_toward(-90.0, 0.0));
    let turned = offsets(2, aim_toward(30.0, 5.0));
    assert_eq!(a, b);
    for (x, y) in a.iter().zip(&turned) {
        assert!((*x - *y).length() < 0.05, "{x} vs {y}");
    }
    assert_ne!(a, offsets(3, aim_toward(-90.0, 0.0)));
}

#[test]
fn offset_is_the_mouse_movement_that_reaches_the_target() {
    let dpi = 800.0;
    let rad_per_count = deg_per_count(dpi, CM_360).to_radians();
    for (seed, yaw, pitch) in [(4, -90.0, 0.0), (5, 120.0, 10.0), (6, -10.0, -3.0)] {
        let aim = aim_toward(yaw, pitch);
        let mut task = MicroAdjustTask::seeded(1, 0.16, seed);
        task.start();
        task.update(0.001, aim);
        let off = task.current_spawn().unwrap().offset_deg;
        let (pos, _) = task.get_render_targets()[0];

        let mut cam = Camera::new(16.0 / 9.0);
        cam.yaw = yaw.to_radians();
        cam.pitch = pitch.to_radians();
        let counts = off / rad_per_count.to_degrees();
        cam.rotate(counts.x, counts.y, rad_per_count);
        let miss = cam.forward().angle_between(pos - EYE).to_degrees();
        assert!(miss < 0.05, "seed {seed}: {miss}° off after moving {off}");
    }
}

#[test]
fn practice_task_still_runs_unseeded() {
    let mut task = MicroAdjustTask::new(3, 0.16);
    task.start();
    task.update(0.001, aim_toward(-90.0, 0.0));
    assert_eq!(task.get_render_targets().len(), 1);
    let o = task.current_spawn().unwrap().offset_deg.length();
    assert!(o >= MICRO_ANGLE_DEG.0 - 0.05 && o <= MICRO_ANGLE_DEG.1 + 0.05, "{o}");
}
