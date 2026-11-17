//! Tracking played as pairs of 3 s chunks at a faster and a slower sensitivity.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::tracking::{ChunkCost, TrackingTask};
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::combiner::Signal;
use aim_trainer::sensitivity::paired::{abba, PairBook, PairResult, Side};
use glam::{Quat, Vec3};

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

/// Play a whole paired task, aiming `off_deg(side, seconds into the chunk)` degrees to the
/// side of the target every tick. Returns the factor seen per chunk and the pair results.
fn play(dt: f32, first: Side, off_deg: impl Fn(f32, f32) -> f32) -> (Vec<f32>, Vec<PairResult>) {
    let mut task = TrackingTask::paired(0.35, 7, first);
    task.start();
    let (mut factors, mut results) = (Vec::new(), Vec::new());
    let mut t = 0.0f32;
    let mut aim = Vec3::NEG_Z;
    while !task.is_complete() {
        let f = task.sens_factor();
        let chunk = (t / 3.0) as usize;
        if factors.len() == chunk && chunk < 4 {
            factors.push(f);
        }
        task.update(dt, Ray { origin: EYE, direction: aim });
        t += dt;
        results.extend(task.take_pair_results());
        if let Some(&(pos, _)) = task.get_render_targets().first() {
            let into = t - (t / 3.0).floor() * 3.0;
            let angle = off_deg(task.sens_factor(), into).to_radians();
            aim = Quat::from_rotation_y(angle).mul_vec3((pos - EYE).normalize());
        }
    }
    results.extend(task.take_pair_results());
    (factors, results)
}

#[test]
fn four_chunks_two_pairs() {
    let (factors, results) = play(0.001, Side::Faster, |_, _| 0.0);
    let want: Vec<f32> = abba(Side::Faster, 4).iter().map(|s| s.factor()).collect();
    assert_eq!(factors, want);
    assert_eq!(results.len(), 2, "{results:?}");
}

#[test]
fn cost_is_mean_angular_error() {
    let faster = Side::Faster.factor();
    let (_, results) = play(0.001, Side::Slower, |f, _| if f == faster { 1.0 } else { 0.5 });
    assert_eq!(results.len(), 2, "{results:?}");
    for r in &results {
        assert!((r.faster_cost - 1.0).abs() < 0.05 && (r.slower_cost - 0.5).abs() < 0.05, "{r:?}");
        assert_eq!(r.signal, Signal::TooFast);
    }
}

#[test]
fn settle_after_switch_is_left_out() {
    let (_, results) = play(0.001, Side::Faster, |_, into| if into < 0.3 { 5.0 } else { 0.0 });
    assert_eq!(results.len(), 2);
    for r in &results {
        assert!(r.faster_cost < 0.05 && r.slower_cost < 0.05, "{r:?}");
    }
}

#[test]
fn turn_heavy_chunk_counts_too_little() {
    let mut busy = ChunkCost::default();
    let mut calm = ChunkCost::default();
    for i in 0..3000 {
        let t = i as f32 * 0.001;
        // A turn every 0.4 s leaves 0.05 s of each counted: ~0.35 s per chunk.
        busy.push(0.001, 1.0, t, t % 0.4);
        calm.push(0.001, 1.0, t, 10.0);
    }
    assert_eq!(busy.finish(), None);
    assert!((calm.finish().unwrap() - 1.0).abs() < 1e-3);
}

#[test]
fn dropped_chunk_keeps_the_other_pair() {
    let mut book = PairBook::new(abba(Side::Faster, 4));
    book.record(0, None);
    book.record(1, Some(1.0));
    book.record(2, Some(1.0));
    book.record(3, Some(1.0));
    assert_eq!(book.take_finished().len(), 1);
}

#[test]
fn works_at_30_fps() {
    let (factors, results) = play(1.0 / 30.0, Side::Faster, |_, _| 0.0);
    assert_eq!(factors.len(), 4);
    assert_eq!(results.len(), 2, "{results:?}");
}

#[test]
fn unpaired_tracking_reports_factor_one() {
    let mut task = TrackingTask::seeded(2.0, 0.35, 1);
    task.start();
    task.update(0.001, Ray { origin: EYE, direction: Vec3::NEG_Z });
    assert_eq!(task.sens_factor(), 1.0);
    assert!(task.take_pair_results().is_empty());
}
