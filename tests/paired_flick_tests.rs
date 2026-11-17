//! Flick played as pairs: equal-angle targets in a band, one at a faster and one at a slower
//! sensitivity, judged by time to hit.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::flick::{FlickBand, FlickTask};
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::combiner::Signal;
use aim_trainer::sensitivity::paired::{abba, Side};
use glam::Vec3;

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

fn level() -> Ray {
    Ray { origin: EYE, direction: Vec3::NEG_Z }
}

/// A ray at the live target (the newest one listed; a shot one keeps popping a while).
fn at_target(task: &FlickTask) -> Ray {
    let (pos, _) = *task.get_render_targets().last().unwrap();
    Ray { origin: EYE, direction: (pos - EYE).normalize() }
}

/// Shoot every target straight away: (factor, band, angle) at each spawn.
fn play_all(task: &mut FlickTask) -> Vec<(f32, FlickBand, f32)> {
    task.start();
    let mut aim = level();
    task.update(0.001, aim);
    let mut seen = Vec::new();
    while !task.is_complete() {
        let s = task.flick_spawn().unwrap();
        seen.push((task.sens_factor(), s.band.unwrap(), s.angle_deg));
        aim = at_target(task);
        assert!(task.on_click(aim));
        task.update(0.001, aim);
    }
    seen
}

#[test]
fn three_pairs_per_band_is_eighteen_targets() {
    let mut task = FlickTask::paired(3, 1, 0.35, Side::Faster);
    assert_eq!(play_all(&mut task).len(), 18);
    let pairs = task.take_pair_results();
    assert_eq!(pairs.len(), 9);
    for band in FlickBand::ALL {
        assert_eq!(pairs.iter().filter(|p| p.band == Some(band)).count(), 3, "{band:?}");
    }
}

#[test]
fn factor_follows_abba() {
    let seen = play_all(&mut FlickTask::paired(3, 2, 0.35, Side::Faster));
    let want: Vec<f32> = abba(Side::Faster, 18).iter().map(|s| s.factor()).collect();
    assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), want);
}

#[test]
fn pairs_share_band_and_angle() {
    for seed in 1..10 {
        let seen = play_all(&mut FlickTask::paired(3, seed, 0.35, Side::Slower));
        for pair in seen.chunks(2) {
            assert_eq!(pair[0].1, pair[1].1, "seed {seed}");
            assert!((pair[0].2 - pair[1].2).abs() < 0.01, "seed {seed}: {pair:?}");
        }
        for (_, band, angle) in &seen {
            let (lo, hi) = band.range_deg();
            assert!(*angle <= hi + 0.1 && *angle >= lo * 0.5, "{band:?} {angle}");
        }
    }
}

#[test]
fn cost_is_time_plus_misses() {
    let mut task = FlickTask::paired(1, 3, 0.35, Side::Faster);
    task.start();
    task.update(0.001, level()); // slot 0 (faster)
    let band = task.flick_spawn().unwrap().band;
    for _ in 0..5 {
        task.update(0.1, level());
    }
    let away = Ray { origin: EYE, direction: Vec3::Y };
    assert!(!task.on_click(away));
    let ray = at_target(&task);
    assert!(task.on_click(ray));
    task.update(0.001, ray); // slot 1 (slower)
    for _ in 0..4 {
        task.update(0.1, ray);
    }
    let ray = at_target(&task);
    assert!(task.on_click(ray));
    task.update(0.001, ray);
    let r = task.take_pair_results();
    assert_eq!(r.len(), 1);
    assert!((r[0].faster_cost - 0.8).abs() < 0.01, "{r:?}");
    assert!((r[0].slower_cost - 0.4).abs() < 0.01, "{r:?}");
    assert_eq!((r[0].signal, r[0].band), (Signal::TooFast, band));
}

#[test]
fn expired_target_counts_as_slow() {
    let mut task = FlickTask::paired(1, 4, 0.35, Side::Faster).with_time_limit(0.01);
    task.start();
    task.update(0.001, level()); // slot 0 spawns
    std::thread::sleep(std::time::Duration::from_millis(20));
    task.update(0.001, level()); // slot 0 expires
    task.update(0.001, level()); // slot 1 spawns
    let ray = at_target(&task);
    assert!(task.on_click(ray));
    task.update(0.001, ray);
    let r = task.take_pair_results();
    assert_eq!(r.len(), 1, "an expired target keeps its pair");
    assert!(r[0].faster_cost >= 0.6, "{r:?}");
    assert_eq!(r[0].signal, Signal::TooFast);
}

#[test]
fn unpaired_and_banded_tasks_report_factor_one() {
    let order = aim_trainer::game::tasks::flick::banded_order(1, &mut <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(1));
    for mut task in [FlickTask::banded(order, 5, 0.35), FlickTask::new(3, 0.35)] {
        let seen = play_all_any(&mut task);
        assert!(seen.iter().all(|f| *f == 1.0));
        assert!(task.take_pair_results().is_empty());
    }
}

fn play_all_any(task: &mut FlickTask) -> Vec<f32> {
    task.start();
    let mut aim = level();
    task.update(0.001, aim);
    let mut seen = Vec::new();
    while !task.is_complete() {
        seen.push(task.sens_factor());
        aim = at_target(task);
        assert!(task.on_click(aim));
        task.update(0.001, aim);
    }
    seen
}

#[test]
fn an_expired_target_costs_at_least_its_whole_time_limit() {
    // Review: expiry runs on the wall clock, the cost on summed frame time (each frame capped
    // at 0.1 s). After a stall (window drag, driver hitch) an expired target cost less than a
    // normal hit. The spec pins it at time limit + misses + 0.6 s.
    let mut task = FlickTask::paired(1, 4, 0.35, Side::Faster).with_time_limit(0.05);
    task.start();
    task.update(0.001, level()); // slot 0 spawns
    std::thread::sleep(std::time::Duration::from_millis(60)); // a stall: no frames
    task.update(0.001, level()); // slot 0 expires; slot 1 spawns
    let ray = at_target(&task);
    assert!(task.on_click(ray));
    task.update(0.001, ray);
    let r = task.take_pair_results();
    assert!(r[0].faster_cost >= 0.05 + 0.6 - 1e-6, "{r:?}");
}
