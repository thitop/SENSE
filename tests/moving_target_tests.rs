//! Moving targets practice: one target moves around; shooting it brings a new moving target
//! somewhere else, until the run's kill count is reached.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::moving::MovingTargetTask;
use aim_trainer::game::tasks::{Task, TaskManager, TaskState, TaskType};
use glam::Vec3;

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);
const DT: f32 = 1.0 / 120.0;

fn ray_at(p: Vec3) -> Ray {
    Ray { origin: EYE, direction: (p - EYE).normalize() }
}

fn away() -> Ray {
    Ray { origin: EYE, direction: Vec3::Y }
}

fn started(kills: usize) -> MovingTargetTask {
    let mut t = MovingTargetTask::seeded(kills, 0.35, 3.0, 7);
    t.start();
    t.update(DT, away());
    t
}

fn live(t: &MovingTargetTask) -> Vec3 {
    t.live_target().expect("a live target").0
}

#[test]
fn there_is_one_target_and_it_moves() {
    let mut t = started(30);
    let a = live(&t);
    for _ in 0..60 {
        t.update(DT, away());
    }
    let b = live(&t);
    // Half a second at 3 m/s, minus any bounce: well over a metre of travel is ~1.5 m.
    assert!(a.distance(b) > 0.5, "moved only {:.3} m", a.distance(b));
    assert_eq!(t.get_render_targets().len(), 1);
}

#[test]
fn a_miss_leaves_the_target_up() {
    let mut t = started(30);
    let before = live(&t);
    assert!(!t.on_click(away()));
    t.update(DT, away());
    assert!(live(&t).distance(before) < 0.1, "same target, moved one tick");
    assert_eq!(t.get_metrics().misses, 1);
    assert_eq!(t.get_metrics().hits, 0);
}

#[test]
fn shooting_it_brings_a_new_moving_target_somewhere_else() {
    let mut t = started(30);
    let shot = live(&t);
    assert!(t.on_click(ray_at(shot)));
    assert!(t.live_target().is_none() || live(&t).distance(shot) >= MovingTargetTask::MIN_RESPAWN_DISTANCE);
    t.update(DT, away());
    let next = live(&t);
    assert!(
        next.distance(shot) >= MovingTargetTask::MIN_RESPAWN_DISTANCE,
        "new target {:.2} m from the shot one",
        next.distance(shot)
    );
    // The shot target pops where it was; only the new one is alive and it moves too.
    for _ in 0..30 {
        t.update(DT, away());
    }
    assert!(live(&t).distance(next) > 0.3);
    assert_eq!(t.get_metrics().hits, 1);
}

#[test]
fn the_run_ends_after_the_kill_count() {
    let mut t = started(5);
    for i in 0..5 {
        assert!(!t.is_complete(), "finished early after {i} kills");
        let p = live(&t);
        assert!(t.on_click(ray_at(p)));
        t.update(DT, away());
    }
    assert!(t.is_complete());
    let m = t.get_metrics();
    assert_eq!(m.hits, 5);
    assert_eq!(m.shots_fired, 5);
    assert!(m.score > 0);
    assert_eq!(m.custom_metric_value.split('/').next().unwrap().trim(), "5");
}

#[test]
fn targets_never_time_out_and_stay_in_the_room() {
    let mut t = started(30);
    for _ in 0..(60.0 / DT) as usize {
        t.update(DT, away());
        let p = live(&t);
        assert!(p.x.abs() <= 4.5 + 1e-3 && (0.6 - 1e-3..=2.6 + 1e-3).contains(&p.y), "{p}");
    }
    assert!(!t.is_complete());
    assert_eq!(t.expired_targets(), 0);
}

#[test]
fn seeded_runs_repeat_the_same_path() {
    let path = || {
        let mut t = started(30);
        (0..200).map(|_| {
            t.update(DT, away());
            live(&t)
        }).collect::<Vec<_>>()
    };
    assert_eq!(path(), path());
}

#[test]
fn it_is_a_practice_task() {
    let mut m = TaskManager::new();
    m.set_task(TaskType::MovingTargets);
    assert_eq!(m.current_task_name(), "Moving Targets");
    m.start_task_now();
    m.update(DT, away());
    assert_eq!(m.state, TaskState::Running);
    assert_eq!(m.get_render_targets().len(), 1);
}
