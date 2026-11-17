//! Flick targets in distance bands: placement by angle from the aim, seeded order, and the
//! per-band estimates in the block search.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::flick::{banded_order, place_target, FlickBand, FlickTask, TARGET_DISTANCE};
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::blocks::BlockScheduler;
use aim_trainer::sensitivity::combiner::{Signal, TestKind};
use glam::Vec3;
use rand::{rngs::StdRng, Rng, SeedableRng};

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

fn level_aim() -> Ray {
    Ray { origin: EYE, direction: Vec3::NEG_Z }
}

fn angle_deg(a: Vec3, b: Vec3) -> f32 {
    a.normalize().dot(b.normalize()).clamp(-1.0, 1.0).acos().to_degrees()
}

fn elevation_deg(p: Vec3) -> f32 {
    ((p - EYE).normalize().y).asin().to_degrees()
}

// ---------- Bands and order ----------

#[test]
fn bands_cover_short_mid_and_far() {
    assert_eq!(FlickBand::Short.range_deg(), (5.0, 12.0));
    assert_eq!(FlickBand::Mid.range_deg(), (15.0, 25.0));
    assert_eq!(FlickBand::Far.range_deg(), (30.0, 40.0));
}

/// A camera looking along `dir`.
fn camera_along(dir: Vec3, aspect: f32) -> aim_trainer::engine::camera::Camera {
    let mut cam = aim_trainer::engine::camera::Camera::new(aspect);
    cam.position = EYE;
    cam.yaw = dir.z.atan2(dir.x);
    cam.pitch = dir.y.clamp(-1.0, 1.0).asin();
    cam
}

#[test]
fn every_banded_target_appears_on_screen() {
    for aspect in [16.0 / 9.0, 16.0 / 10.0, 4.0 / 3.0] {
        for seed in 0..40 {
            let order = banded_order(3, &mut StdRng::seed_from_u64(seed));
            let mut task = FlickTask::banded(order, seed, 0.35);
            task.start();
            let mut aim = level_aim();
            task.update(0.001, aim);
            while !task.is_complete() {
                let (pos, _) = *task.get_render_targets().last().unwrap();
                let cam = camera_along(aim.direction, aspect);
                let off = cam.offscreen_direction(pos);
                assert!(off.is_none(), "aspect {aspect}, seed {seed}: {:?} off screen", task.current_spawn());
                aim = Ray { origin: EYE, direction: (pos - EYE).normalize() };
                assert!(task.on_click(aim));
                task.update(0.001, aim);
            }
        }
    }
}

#[test]
fn order_has_equal_bands_and_is_seeded() {
    let order = banded_order(3, &mut StdRng::seed_from_u64(1));
    assert_eq!(order.len(), 9);
    for b in FlickBand::ALL {
        assert_eq!(order.iter().filter(|&&x| x == b).count(), 3);
    }
    assert_eq!(order, banded_order(3, &mut StdRng::seed_from_u64(1)));
    assert!((2..30).any(|s| banded_order(3, &mut StdRng::seed_from_u64(s)) != order));
}

// ---------- Placement ----------

#[test]
fn target_lands_the_asked_angle_from_a_level_aim() {
    let mut rng = StdRng::seed_from_u64(2);
    for _ in 0..500 {
        let want = rng.gen_range(5.0..100.0f32);
        let phi = rng.gen_range(0.0..std::f32::consts::TAU);
        let p = place_target(EYE, Vec3::NEG_Z, want, phi);
        let got = angle_deg(Vec3::NEG_Z, p - EYE);
        // Mirroring a too-low target upward keeps the angle; only clamping changes it.
        let e = elevation_deg(p);
        assert!((-6.01..=25.01).contains(&e), "elevation {e}");
        if (got - want).abs() > 0.05 {
            assert!((e - 25.0).abs() < 0.05 || (e + 6.0).abs() < 0.05, "want {want} got {got} elev {e}");
        }
        assert!(((p - EYE).length() - TARGET_DISTANCE).abs() < 1e-3);
    }
}

#[test]
fn target_stays_above_the_floor_when_aiming_down() {
    let down = Vec3::new(0.0, -40f32.to_radians().sin(), -40f32.to_radians().cos());
    for phi in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0] {
        let p = place_target(EYE, down, 10.0, phi);
        assert!(p.y > 0.5, "{p}");
    }
}

// ---------- Banded flick task ----------

fn shoot_all(task: &mut FlickTask) -> Vec<(FlickBand, f32)> {
    let mut seen = Vec::new();
    task.start();
    let mut aim = level_aim();
    task.update(0.001, aim);
    for _ in 0..100 {
        if task.is_complete() {
            break;
        }
        let spawn = task.current_spawn().expect("a live target");
        seen.push((spawn.band.expect("banded"), spawn.angle_deg));
        // A target that was just hit stays listed for its pop animation; the live one is last.
        let (pos, _) = *task.get_render_targets().last().unwrap();
        aim = Ray { origin: EYE, direction: (pos - EYE).normalize() };
        assert!(task.on_click(aim));
        task.update(0.001, aim);
    }
    seen
}

#[test]
fn banded_task_follows_the_order_and_stays_in_band() {
    let order = banded_order(3, &mut StdRng::seed_from_u64(3));
    let mut task = FlickTask::banded(order.clone(), 77, 0.35);
    let seen = shoot_all(&mut task);
    assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), order);
    assert!(task.is_complete());
    for (band, angle) in seen {
        let (lo, hi) = band.range_deg();
        // Clamping at the floor/ceiling can only shorten a flick a little.
        assert!(angle <= hi + 0.1 && angle >= lo * 0.5, "{band:?} {angle}");
    }
}

#[test]
fn same_seed_same_targets() {
    let order = banded_order(3, &mut StdRng::seed_from_u64(4));
    let a = shoot_all(&mut FlickTask::banded(order.clone(), 5, 0.35));
    let b = shoot_all(&mut FlickTask::banded(order.clone(), 5, 0.35));
    let c = shoot_all(&mut FlickTask::banded(order, 6, 0.35));
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn unbanded_task_reports_no_band() {
    let mut task = FlickTask::new(3, 0.35);
    task.start();
    task.update(0.001, level_aim());
    assert_eq!(task.current_spawn().and_then(|s| s.band), None);
}

// ---------- Per-band estimates ----------

fn simulated_signal(rng: &mut StdRng, pref: f64, cm: f64, clarity: f64) -> Signal {
    let mismatch = (pref / cm).ln();
    let p_direct = 0.45 * (-(mismatch * mismatch) / (2.0 * 0.25f64.powi(2))).exp() + 0.15;
    let p_fast_given_miss = 1.0 / (1.0 + (-clarity * mismatch).exp());
    if rng.gen::<f64>() < p_direct {
        Signal::Direct
    } else if rng.gen::<f64>() < p_fast_given_miss {
        Signal::TooFast
    } else {
        Signal::TooSlow
    }
}

/// Flicks prefer `band_prefs` (short, mid, far); every other test prefers the mid value.
fn run(rng: &mut StdRng, band_prefs: [f64; 3]) -> BlockScheduler {
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    let other = band_prefs[1];
    while !s.is_completed {
        let cm = s.current_cm_360;
        for band in banded_order(3, rng) {
            s.record_flick(band, simulated_signal(rng, band_prefs[band.index()], cm, 4.0));
        }
        for (kind, n, clarity) in [
            (TestKind::Micro, 6, 4.0),
            (TestKind::Tracking, 16, 1.5),
            (TestKind::Recoil, 4, 4.0),
            (TestKind::Placement, 4, 1.5),
        ] {
            for _ in 0..n {
                s.record(kind, simulated_signal(rng, other, cm, clarity));
            }
        }
        s.end_block();
    }
    s
}

#[test]
fn flick_bands_add_up_to_the_flick_test() {
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    s.record_flick(FlickBand::Short, Signal::TooFast);
    s.record_flick(FlickBand::Far, Signal::TooSlow);
    s.record_flick(FlickBand::Far, Signal::Direct);
    s.end_block();
    let b = &s.history[0];
    assert_eq!(b.band_counts(FlickBand::Short).too_fast, 1);
    assert_eq!(b.band_counts(FlickBand::Far).too_slow, 1);
    assert_eq!(b.band_counts(FlickBand::Far).direct, 1);
    let flick = b.counts(TestKind::Flick);
    assert_eq!((flick.too_fast, flick.too_slow, flick.direct), (1, 1, 1));
}

#[test]
fn band_estimates_follow_each_band() {
    let mut rng = StdRng::seed_from_u64(20);
    let mut ordered = 0;
    let runs = 200;
    for _ in 0..runs {
        let fit = run(&mut rng, [45.0, 34.0, 26.0]).flick_bands();
        let Some(fit) = fit else { continue };
        let cm = |b| fit.estimates.iter().find(|e| e.0 == b).map(|e| e.1.cm_360);
        if let (Some(s), Some(f)) = (cm(FlickBand::Short), cm(FlickBand::Far)) {
            ordered += (s > f) as usize;
        }
    }
    println!("short slower than far: {ordered}/{runs}");
    assert!(ordered as f64 >= runs as f64 * 0.9, "{ordered}/{runs}");
}

/// Share of runs where short and far flicks are reported as preferring different sensitivities.
fn distance_flag_rate(rng: &mut StdRng, prefs: [f64; 3], runs: usize) -> f64 {
    let flagged = (0..runs)
        .filter(|_| run(rng, prefs).flick_bands().is_some_and(|f| f.distance_conflict))
        .count();
    flagged as f64 / runs as f64
}

#[test]
fn short_and_far_disagreeing_is_reported() {
    let rate = distance_flag_rate(&mut StdRng::seed_from_u64(21), [50.0, 35.0, 25.0], 300);
    println!("2x short/far gap flagged: {rate:.2}");
    assert!(rate >= 0.8, "{rate:.2}");
}

#[test]
fn agreeing_bands_are_rarely_reported() {
    let rate = distance_flag_rate(&mut StdRng::seed_from_u64(22), [35.0, 35.0, 35.0], 300);
    println!("false distance flags: {rate:.2}");
    assert!(rate <= 0.06, "{rate:.2}");
}
