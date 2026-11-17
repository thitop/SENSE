//! Micro-adjustment played as pairs: one target at a faster and one at a slower sensitivity.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::micro_adjust::MicroAdjustTask;
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::combiner::Signal;
use aim_trainer::sensitivity::paired::{abba, Side};
use glam::Vec3;

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

fn level() -> Ray {
    Ray { origin: EYE, direction: Vec3::NEG_Z }
}

/// `micro_spawn` reports a mouse-space offset; its length differs from the true angle by
/// ~0.001° depending on direction, so "same angle" is checked to 0.01°.
/// A ray at the live target (the newest one listed; a shot one keeps popping for a while).
fn at_target(task: &MicroAdjustTask) -> Ray {
    let (pos, _) = *task.get_render_targets().last().unwrap();
    Ray { origin: EYE, direction: (pos - EYE).normalize() }
}

/// Shoot every target straight away, returning the factor and offset seen at each spawn.
fn play_all(task: &mut MicroAdjustTask) -> Vec<(f32, f32)> {
    task.start();
    task.update(0.001, level());
    let mut seen = Vec::new();
    while !task.is_complete() {
        seen.push((task.sens_factor(), task.micro_spawn().unwrap().offset_deg.length()));
        let ray = at_target(task);
        assert!(task.on_click(ray));
        task.update(0.001, ray);
    }
    seen
}

#[test]
fn paired_task_has_twice_the_pairs_in_targets() {
    let mut task = MicroAdjustTask::paired(4, 0.16, 1, Side::Faster);
    assert_eq!(play_all(&mut task).len(), 8);
    assert_eq!(task.take_pair_results().len(), 4);
}

#[test]
fn factor_follows_abba() {
    let mut task = MicroAdjustTask::paired(4, 0.16, 2, Side::Faster);
    let factors: Vec<f32> = play_all(&mut task).iter().map(|s| s.0).collect();
    let want: Vec<f32> = abba(Side::Faster, 8).iter().map(|s| s.factor()).collect();
    assert_eq!(factors, want);
}

#[test]
fn targets_in_a_pair_sit_at_the_same_angle() {
    let mut differs = false;
    for seed in 1..20 {
        let seen = play_all(&mut MicroAdjustTask::paired(2, 0.16, seed, Side::Slower));
        assert!((seen[0].1 - seen[1].1).abs() < 1e-2, "seed {seed}: {seen:?}");
        assert!((seen[2].1 - seen[3].1).abs() < 1e-2, "seed {seed}: {seen:?}");
        differs |= (seen[1].1 - seen[2].1).abs() > 0.05;
    }
    assert!(differs, "pairs draw their own angle");
}

#[test]
fn cost_is_time_to_hit_plus_miss_penalty() {
    let mut task = MicroAdjustTask::paired(1, 0.16, 3, Side::Faster);
    task.start();
    task.update(0.001, level()); // spawns slot 0 (faster)
    for _ in 0..5 {
        task.update(0.1, level());
    }
    let away = Ray { origin: EYE, direction: Vec3::Y };
    assert!(!task.on_click(away));
    assert!(task.on_click(at_target(&task)));
    task.update(0.001, level()); // spawns slot 1 (slower)
    for _ in 0..4 {
        task.update(0.1, level());
    }
    assert!(task.on_click(at_target(&task)));
    task.update(0.001, level());
    let r = task.take_pair_results();
    assert_eq!(r.len(), 1);
    assert!((r[0].faster_cost - 0.8).abs() < 0.01, "{r:?}");
    assert!((r[0].slower_cost - 0.4).abs() < 0.01, "{r:?}");
    assert_eq!(r[0].signal, Signal::TooFast);
}

#[test]
fn seeded_and_unpaired_tasks_report_factor_one() {
    let mut task = MicroAdjustTask::seeded(6, 0.16, 3);
    let seen = play_all(&mut task);
    assert!(seen.iter().all(|s| s.0 == 1.0));
    assert!(task.take_pair_results().is_empty());
}
