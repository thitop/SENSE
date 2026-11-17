//! Tracking: the window analyzer, the seeded target path, and a simulated tracker driving the
//! real task and camera.

use aim_trainer::engine::camera::Camera;
use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::micro_adjust::mouse_offset_deg;
use aim_trainer::game::tasks::tracking::TrackingTask;
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::combiner::{DirectionalSignal, Signal};
use aim_trainer::sensitivity::micro_signal::deg_per_count;
use aim_trainer::sensitivity::psa::fit_crossover;
use aim_trainer::sensitivity::tracking_signal::{TrackingAnalyzer, TrackingWindow};
use glam::{Vec2, Vec3};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::VecDeque;

const DT: f32 = 0.001;
const RADIUS_DEG: f32 = 2.2;

// ---------- Analyzer ----------

/// Feed `secs` of a target moving right at `speed` °/s with the aim a fixed `lead_deg` ahead
/// of it (negative = behind), `since_turn` counting from `turned_at`.
fn feed(a: &mut TrackingAnalyzer, secs: f32, speed: f32, lead_deg: f32, turned_at: f32) -> Vec<TrackingWindow> {
    let n = (secs / DT).round() as usize;
    for i in 0..n {
        let t = i as f32 * DT;
        // offset = aim → target; aim ahead (lead > 0) means the target is behind the aim.
        a.push(DT, Vec2::new(-lead_deg, 0.0), Vec2::new(speed, 0.0), (t - turned_at).max(0.0));
    }
    a.take_windows()
}

#[test]
fn half_second_windows() {
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    assert_eq!(feed(&mut a, 2.0, 20.0, 0.0, -1.0).len(), 4);
    assert!(a.take_windows().is_empty(), "windows are handed out once");
}

#[test]
fn behind_is_too_slow_ahead_is_too_fast_on_target_is_direct() {
    let sig = |lead| {
        let mut a = TrackingAnalyzer::new(RADIUS_DEG);
        feed(&mut a, 0.5, 20.0, lead, -1.0)[0].signal()
    };
    assert_eq!(sig(-1.0), Some(Signal::TooSlow));
    assert_eq!(sig(1.0), Some(Signal::TooFast));
    assert_eq!(sig(0.1), Some(Signal::Direct));
    assert_eq!(sig(-0.1), Some(Signal::Direct));
}

#[test]
fn lag_is_reported_in_time() {
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    let w = feed(&mut a, 0.5, 20.0, -2.0, -1.0)[0];
    assert!((w.lag_ms - 100.0).abs() < 1.0, "{}", w.lag_ms); // 2° behind at 20°/s
    assert!((w.mean_along_deg + 2.0).abs() < 1e-3);
}

#[test]
fn reaction_after_a_turn_is_left_out() {
    // Far behind only while the post-turn stretch is excluded, on target after: still direct.
    let settle = TrackingAnalyzer::SETTLE_S;
    let turned_at = -(settle - 0.15); // excluded for the first 0.15 s of the window
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    for i in 0..500 {
        let since = i as f32 * DT - turned_at;
        let lead = if since < settle { -3.0 } else { 0.0 };
        a.push(DT, Vec2::new(-lead, 0.0), Vec2::new(20.0, 0.0), since);
    }
    assert_eq!(a.take_windows()[0].signal(), Some(Signal::Direct));
}

#[test]
fn window_mostly_right_after_turns_says_nothing() {
    let settle = TrackingAnalyzer::SETTLE_S;
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    let w = feed(&mut a, 0.5, 20.0, -2.0, 0.25 - settle)[0]; // half the window counts
    assert_eq!(w.signal(), Some(Signal::TooSlow));
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    let w = feed(&mut a, 0.5, 20.0, -2.0, 0.35 - settle)[0]; // ~30% counts
    assert_eq!(w.signal(), None);
}

#[test]
fn still_target_says_nothing() {
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    assert_eq!(feed(&mut a, 0.5, 0.0, -2.0, -1.0)[0].signal(), None);
}

#[test]
fn direction_follows_the_target_not_the_screen() {
    // Target moving up-left, aim behind it (down-right of it).
    let v = Vec2::new(-14.0, -14.0); // y down: negative = up
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    for _ in 0..500 {
        let behind = -v.normalize() * 1.0; // aim relative to target
        a.push(DT, -behind, v, 1.0);
    }
    assert_eq!(a.take_windows()[0].signal(), Some(Signal::TooSlow));
}

#[test]
fn jitter_is_measured_but_does_not_vote() {
    let mut a = TrackingAnalyzer::new(RADIUS_DEG);
    for i in 0..500 {
        let wobble = if i % 20 < 10 { 0.8 } else { -0.8 };
        a.push(DT, Vec2::new(0.0, -wobble), Vec2::new(20.0, 0.0), 1.0);
    }
    let w = a.take_windows()[0];
    assert!((w.cross_rms_deg - 0.8).abs() < 0.05, "{}", w.cross_rms_deg);
    assert_eq!(w.signal(), Some(Signal::Direct));
}

// ---------- Seeded path ----------

fn target_path(seed: u64, secs: f32) -> Vec<Vec3> {
    let mut task = TrackingTask::seeded(secs, 0.35, seed);
    task.start();
    let ray = Ray { origin: Vec3::new(0.0, 1.5, 0.0), direction: Vec3::NEG_Z };
    let mut out = Vec::new();
    while !task.is_complete() {
        task.update(DT, ray);
        if let Some(&(p, _)) = task.get_render_targets().first() {
            out.push(p);
        }
    }
    out
}

#[test]
fn same_seed_same_path() {
    let a = target_path(1, 3.0);
    assert_eq!(a, target_path(1, 3.0));
    assert_ne!(a, target_path(2, 3.0));
}

#[test]
fn seeded_path_moves_at_two_speeds_and_stays_in_the_room() {
    let p = target_path(3, 8.0);
    let speeds: Vec<f32> = p.windows(2).map(|w| (w[1] - w[0]).length() / DT).collect();
    let slow = speeds.iter().filter(|&&s| (s - TrackingTask::SEEDED_SPEEDS.0).abs() < 0.01).count();
    let fast = speeds.iter().filter(|&&s| (s - TrackingTask::SEEDED_SPEEDS.1).abs() < 0.01).count();
    assert!(slow > 1000 && fast > 1000, "slow {slow} fast {fast} of {}", speeds.len());
    for q in &p {
        assert!(q.x.abs() <= 4.5 + 1e-3 && (0.6 - 1e-3..=2.6 + 1e-3).contains(&q.y), "{q}");
    }
}

#[test]
fn task_produces_windows() {
    let mut task = TrackingTask::seeded(2.0, 0.35, 4);
    task.start();
    let ray = Ray { origin: Vec3::new(0.0, 1.5, 0.0), direction: Vec3::NEG_Z };
    let mut windows = Vec::new();
    while !task.is_complete() {
        task.update(DT, ray);
        windows.extend(task.take_windows());
    }
    assert!((3..=4).contains(&windows.len()), "{}", windows.len());
}

// ---------- Simulated tracker ----------

/// How hard the simulated player corrects a visible error, per second. Humans close a pursuit
/// error within a few hundred ms (catch-up saccades); `SLOW_CORRECTOR` takes about twice that.
const TYPICAL_CORRECTOR: f32 = 8.0;
const SLOW_CORRECTOR: f32 = 4.0;

/// A player tracking the target with a 120 ms visual delay. Like real pursuit, they predict
/// past the delay with an efference copy: what they see, plus how far the target moved since,
/// minus how far they believe they turned since. Their hand moves as if the sensitivity were
/// `pref` cm/360, so that belief is wrong when it isn't: at a faster sensitivity they turned
/// more than they think and run ahead; at a slower one they fall behind.
fn simulate(rng: &mut StdRng, pref: f64, cm: f64, seed: u64, secs: f32, gain: f32) -> Vec<TrackingWindow> {
    const DELAY_TICKS: usize = 120;
    const MOTOR_NOISE: f32 = 0.15;
    let delay_s = DELAY_TICKS as f32 * DT;
    let dpi = 800.0;
    let (dpc_pref, rpc_cm) = (deg_per_count(dpi, pref), deg_per_count(dpi, cm).to_radians());

    let mut cam = Camera::new(16.0 / 9.0);
    let mut task = TrackingTask::seeded(secs, 0.35, seed);
    task.start();
    let mut seen: VecDeque<(Vec2, Vec2)> = VecDeque::new();
    // Rotation they believe they made each tick over the last delay (degrees, at `pref`).
    let mut believed: VecDeque<Vec2> = VecDeque::new();
    let mut believed_sum = Vec2::ZERO;
    let mut prev_dir: Option<Vec3> = None;
    let mut windows = Vec::new();
    while !task.is_complete() {
        let ray = Ray { origin: cam.position, direction: cam.forward() };
        task.update(DT, ray);
        windows.extend(task.take_windows());
        let Some(&(pos, _)) = task.get_render_targets().first() else { continue };
        let dir = pos - cam.position;
        let vel = prev_dir.map_or(Vec2::ZERO, |p| mouse_offset_deg(p, dir) / DT);
        prev_dir = Some(dir);
        seen.push_back((mouse_offset_deg(ray.direction, dir), vel));
        if seen.len() <= DELAY_TICKS {
            continue;
        }
        let (err, v) = seen.pop_front().unwrap();
        let predicted = err + v * delay_s - believed_sum;
        let noise = Vec2::new(rng.gen::<f32>() - 0.5, rng.gen::<f32>() - 0.5) * 2.0 * MOTOR_NOISE;
        let intended = (v + predicted * gain) * (Vec2::ONE + noise); // °/s at their preferred sens
        let step = intended * DT;
        believed.push_back(step);
        believed_sum += step;
        if believed.len() > DELAY_TICKS {
            believed_sum -= believed.pop_front().unwrap();
        }
        let counts = step / dpc_pref;
        cam.rotate(counts.x, counts.y, rpc_cm);
    }
    windows
}

fn counts(ws: &[TrackingWindow]) -> (usize, usize, usize) {
    let c = |s| ws.iter().filter(|w| w.signal() == Some(s)).count();
    (c(Signal::TooFast), c(Signal::TooSlow), c(Signal::Direct))
}

#[test]
fn simulated_tracker_reads_fast_and_slow_the_right_way() {
    let mut rng = StdRng::seed_from_u64(10);
    let (f, s, _) = counts(&simulate(&mut rng, 35.0, 24.0, 1, 8.0, TYPICAL_CORRECTOR));
    assert!(f > 3 * s.max(1), "faster than preferred: fast {f} slow {s}");
    let (f, s, _) = counts(&simulate(&mut rng, 35.0, 50.0, 1, 8.0, TYPICAL_CORRECTOR));
    assert!(s > 3 * f.max(1), "slower than preferred: fast {f} slow {s}");
}

/// Tracking-only crossover over seven 8 s blocks spread around `pref`.
fn tracking_crossover(rng: &mut StdRng, pref: f64, gain: f32) -> f64 {
    let rounds: Vec<(f64, usize, usize)> = [0.6, 0.75, 0.9, 1.0, 1.1, 1.3, 1.6]
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let cm = pref * r;
            let (f, s, _) = counts(&simulate(rng, pref, cm, i as u64, 8.0, gain));
            (cm, f, s)
        })
        .collect();
    let c = fit_crossover(&rounds).expect("a crossover");
    println!("pref {pref} gain {gain}: crossover {:.1} ±{:.0}%  {rounds:?}", c.cm_360, c.rel_uncertainty * 100.0);
    c.cm_360
}

#[test]
fn simulated_tracker_balances_near_its_preferred_sensitivity() {
    let mut rng = StdRng::seed_from_u64(11);
    for pref in [25.0, 35.0, 50.0] {
        let c = tracking_crossover(&mut rng, pref, TYPICAL_CORRECTOR);
        assert!((c / pref - 1.0).abs() < 0.10, "pref {pref}: {c:.1}");
    }
}

#[test]
fn slow_correctors_lean_faster_but_not_far() {
    // A player who closes errors slowly is still behind when the post-turn exclusion ends, so
    // tracking reads a little "too slow" for them. Bounded, not zero: see the spec, §3.3.
    let mut rng = StdRng::seed_from_u64(12);
    for pref in [25.0, 35.0, 50.0] {
        let r = tracking_crossover(&mut rng, pref, SLOW_CORRECTOR) / pref;
        assert!((0.80..=1.05).contains(&r), "pref {pref}: ratio {r:.2}");
    }
}
