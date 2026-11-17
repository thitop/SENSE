//! Human-like simulated players for the paired micro and tracking tests.
//!
//! Real play showed the first judges leaning one way at every sensitivity: micro said "too
//! fast" 28 times and "too slow" never, tracking said "too slow" 66 times and "too fast" 3
//! times. These players carry the two habits behind that, a lagging tracker and a hand whose
//! moves fall about 10% short, and the tests check that (1) the old judges break on them the
//! same way, and (2) the paired judges still flip at the sensitivity the player does best at.
//!
//! "Best" is measured, not assumed: the cm/360 with the lowest mean cost, by brute force. A
//! hand that undershoots 10% really does best somewhat faster than the sensitivity it plans in.

use aim_trainer::engine::camera::Camera;
use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::flick::{FlickBand, FlickTask};
use aim_trainer::game::tasks::micro_adjust::{mouse_offset_deg, MicroAdjustTask};
use aim_trainer::game::tasks::tracking::TrackingTask;
use aim_trainer::game::tasks::Task;
use aim_trainer::sensitivity::blocks::BlockScheduler;
use aim_trainer::sensitivity::combiner::{DirectionalSignal, Signal, TestKind};
use aim_trainer::sensitivity::micro_signal::{classify_micro, deg_per_count};
use aim_trainer::sensitivity::paired::{PairResult, Side};
use aim_trainer::sensitivity::path_recorder::{MouseSample, ShotMovementPath};
use aim_trainer::sensitivity::psa::fit_crossover;
use aim_trainer::sensitivity::tracking_signal::TrackingWindow;
use glam::{Vec2, Vec3};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const DT: f32 = 0.001;
const DPI: f64 = 800.0;
/// The sensitivity the simulated hand plans its movements in.
const HAND_CM: f64 = 35.0;
const MICRO_RADIUS: f32 = 0.16;

fn gauss(rng: &mut StdRng) -> f32 {
    let (u1, u2): (f32, f32) = (rng.gen::<f32>().max(1e-9), rng.gen());
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

fn first_side(seed: u64) -> Side {
    if seed % 2 == 0 {
        Side::Faster
    } else {
        Side::Slower
    }
}

// ---------- Tracker ----------

/// How much of the target's motion over their visual delay the player predicts.
/// - `LAGGING` (0.0) trails the target by speed × delay and breaks the old analyzer exactly
///   like real play did ("too slow" at every sensitivity). Its cost keeps falling as the
///   sensitivity gets faster (a lead from overturning cancels the lag, and this model never
///   oscillates), so it has no best sensitivity to find and isn't used for the paired checks.
/// - `PARTIAL` (0.5) lags too, but has a best sensitivity.
/// - `PREDICTING` (1.0) is the old simulation's perfect predictor.
const LAGGING: f32 = 0.0;
const PARTIAL: f32 = 0.5;
const PREDICTING: f32 = 1.0;
const TRACK_GAIN: f32 = 8.0;

/// One paired tracking segment (12 s) at `cm`. With `follow` the camera plays the task's
/// faster/slower side; without it both sides play at `cm`, so every chunk cost samples the
/// cost at `cm` (used to find the best sensitivity).
fn track(rng: &mut StdRng, cm: f64, seed: u64, prediction: f32, follow: bool) -> (Vec<TrackingWindow>, Vec<PairResult>) {
    const DELAY_TICKS: usize = 120;
    const MOTOR_NOISE: f32 = 0.15;
    let delay_s = DELAY_TICKS as f32 * DT;
    let dpc_hand = deg_per_count(DPI, HAND_CM);
    let mut cam = Camera::new(16.0 / 9.0);
    let mut task = TrackingTask::paired(0.35, seed, first_side(seed));
    task.start();
    let mut seen: VecDeque<(Vec2, Vec2)> = VecDeque::new();
    let mut believed: VecDeque<Vec2> = VecDeque::new();
    let mut believed_sum = Vec2::ZERO;
    let mut prev_dir: Option<Vec3> = None;
    let (mut windows, mut pairs) = (Vec::new(), Vec::new());
    while !task.is_complete() {
        let factor = if follow { task.sens_factor() as f64 } else { 1.0 };
        let rpc = deg_per_count(DPI, cm * factor).to_radians();
        let ray = Ray { origin: cam.position, direction: cam.forward() };
        task.update(DT, ray);
        windows.extend(task.take_tracking_windows());
        pairs.extend(task.take_pair_results());
        let Some(&(pos, _)) = task.get_render_targets().first() else { continue };
        let dir = pos - cam.position;
        let vel = prev_dir.map_or(Vec2::ZERO, |p| mouse_offset_deg(p, dir) / DT);
        prev_dir = Some(dir);
        seen.push_back((mouse_offset_deg(ray.direction, dir), vel));
        if seen.len() <= DELAY_TICKS {
            continue;
        }
        let (err, v) = seen.pop_front().unwrap();
        let predicted = err + v * delay_s * prediction - believed_sum;
        let noise = Vec2::new(rng.gen::<f32>() - 0.5, rng.gen::<f32>() - 0.5) * 2.0 * MOTOR_NOISE;
        let intended = (v + predicted * TRACK_GAIN) * (Vec2::ONE + noise);
        let step = intended * DT;
        believed.push_back(step);
        believed_sum += step;
        if believed.len() > DELAY_TICKS {
            believed_sum -= believed.pop_front().unwrap();
        }
        let counts = step / dpc_hand;
        cam.rotate(counts.x, counts.y, rpc);
    }
    pairs.extend(task.take_pair_results());
    (windows, pairs)
}

// ---------- Micro player ----------

/// Hand noise that doesn't shrink with the movement (tremor, finger resolution), in cm of
/// mouse travel. It is what makes very fast sensitivities hard for small targets.
const HAND_NOISE_CM: f32 = 0.02;

/// How the simulated hand closes on a target.
#[derive(Clone, Copy)]
struct Hand {
    /// Share of the seen distance the primary move covers.
    primary: f32,
    /// Share of what is left each correction covers.
    correction: f32,
}

const UNDERSHOOTING: Hand = Hand { primary: 0.9, correction: 0.9 };
/// Stops its main move well short and creeps in with small moves (what flick's old rule reads
/// as "too slow" whatever the sensitivity).
const CREEPING: Hand = Hand { primary: 0.6, correction: 0.5 };

/// Play a target task with the simulated hand at `cm`: 200 ms to react, a primary move, then
/// corrections, each followed by 100 ms to look, clicking once the crosshair is on the target.
/// A move lasts 100 ms + 100 ms per cm of hand travel (bigger hand moves at slow sensitivities
/// take longer) and lands with 8% proportional noise plus `HAND_NOISE_CM`. With `follow` the
/// camera plays the task's faster/slower side; without it both sides play at `cm`.
/// Returns the pairs, and the old judges' verdicts per target: micro's `classify_micro` when
/// the task has micro spawns, otherwise flick's shot-path rule.
fn play(
    rng: &mut StdRng,
    cm: f64,
    mut task: Box<dyn Task>,
    radius_m: f32,
    hand: Hand,
    follow: bool,
) -> (Vec<PairResult>, Vec<Option<Signal>>) {
    let dpc_hand = deg_per_count(DPI, HAND_CM);
    let radius_deg = (radius_m / 9.0).atan().to_degrees();
    let mut cam = Camera::new(16.0 / 9.0);
    task.start();
    let base = Instant::now();
    let (mut pairs, mut old) = (Vec::new(), Vec::new());
    let mut tick: u64 = 0;
    let mut live: Option<(Vec3, u64, Vec2)> = None; // target position, spawn tick, offset at spawn
    let mut wait: i64 = 0;
    let mut stroke: VecDeque<Vec2> = VecDeque::new();
    let (mut samples, mut acc): (Vec<MouseSample>, Vec2) = (Vec::new(), Vec2::ZERO);
    let mut carry = Vec2::ZERO;
    let mut moved = false;
    let mut factor = 1.0;
    while !task.is_complete() && tick < 400_000 {
        tick += 1;
        let ray = Ray { origin: cam.position, direction: cam.forward() };
        task.update(DT, ray);
        pairs.extend(task.take_pair_results());
        // Shot targets keep popping on the wall clock, which barely moves here: stop as soon
        // as the task is done instead of aiming at a target that is already gone.
        if task.is_complete() {
            break;
        }
        let Some(&(pos, _)) = task.get_render_targets().last() else { continue };
        if live.map(|l| l.0) != Some(pos) {
            let offset = mouse_offset_deg(cam.forward(), pos - cam.position);
            live = Some((pos, tick, offset));
            wait = 200;
            stroke.clear();
            samples.clear();
            acc = Vec2::ZERO;
            moved = false;
            factor = if follow { task.sens_factor() as f64 } else { 1.0 };
        }
        let error = mouse_offset_deg(cam.forward(), pos - cam.position);
        if let Some(step) = stroke.pop_front() {
            let want = step / dpc_hand + carry;
            let counts = want.round();
            carry = want - counts;
            if counts != Vec2::ZERO {
                acc += counts;
                let time = base + Duration::from_millis(tick);
                samples.push(MouseSample { time, delta: counts, accumulated_pos: acc });
            }
            cam.rotate(counts.x, counts.y, deg_per_count(DPI, cm * factor).to_radians());
            continue;
        }
        if wait > 0 {
            wait -= 1;
            continue;
        }
        if error.length() < radius_deg {
            let (_, spawned, offset) = live.unwrap();
            let start = base + Duration::from_millis(spawned);
            let verdict = if task.micro_spawn().is_some() {
                classify_micro(&samples, start, deg_per_count(DPI, cm * factor), offset).and_then(|m| m.signal())
            } else {
                ShotMovementPath::from_samples(samples.clone(), start, base + Duration::from_millis(tick)).signal()
            };
            old.push(verdict);
            assert!(task.on_click(ray), "on target but missed");
            continue;
        }
        // Plan the next move toward where the target looks, in the hand's degrees.
        let cover = if moved { hand.correction } else { hand.primary };
        moved = true;
        let planned = error * cover;
        let noise_deg = 0.08 * planned.length() + HAND_NOISE_CM * 360.0 / HAND_CM as f32;
        let total = planned + Vec2::new(gauss(rng), gauss(rng)) * noise_deg / std::f32::consts::SQRT_2;
        let hand_cm = total.length() * HAND_CM as f32 / 360.0;
        let ticks = (100.0 + 100.0 * hand_cm).round() as usize;
        let weights: Vec<f32> = (0..ticks).map(|i| ((i as f32 + 0.5) / ticks as f32 * std::f32::consts::PI).sin()).collect();
        let sum: f32 = weights.iter().sum();
        stroke.extend(weights.iter().map(|w| total * (w / sum)));
        wait = 100;
    }
    pairs.extend(task.take_pair_results());
    (pairs, old)
}

/// One paired micro segment (4 pairs) at `cm`.
fn micro(rng: &mut StdRng, cm: f64, seed: u64, follow: bool) -> (Vec<PairResult>, Vec<Option<Signal>>) {
    let task = Box::new(MicroAdjustTask::paired(4, MICRO_RADIUS, seed, first_side(seed)));
    play(rng, cm, task, MICRO_RADIUS, UNDERSHOOTING, follow)
}

const FLICK_RADIUS: f32 = 0.35;

/// One paired flick segment (3 pairs per band) at `cm`.
fn flick(rng: &mut StdRng, cm: f64, seed: u64, hand: Hand, follow: bool) -> (Vec<PairResult>, Vec<Option<Signal>>) {
    let task = Box::new(FlickTask::paired(3, seed, FLICK_RADIUS, first_side(seed)));
    play(rng, cm, task, FLICK_RADIUS, hand, follow)
}

/// Pairs of one flick band.
fn in_band(pairs: Vec<PairResult>, band: FlickBand) -> Vec<PairResult> {
    pairs.into_iter().filter(|p| p.band == Some(band)).collect()
}

// ---------- Best sensitivity and crossovers ----------

/// Mean chunk/target cost at `cm` over `runs` unpaired segments.
fn mean_cost(rng: &mut StdRng, cm: f64, runs: u64, cost: &mut impl FnMut(&mut StdRng, f64, u64) -> Vec<PairResult>) -> f64 {
    let all: Vec<f64> = (0..runs)
        .flat_map(|s| cost(rng, cm, 1000 + s))
        .flat_map(|p| [p.faster_cost, p.slower_cost])
        .collect();
    all.iter().sum::<f64>() / all.len() as f64
}

/// The cm/360 with the lowest cost: a 4% grid around the hand's sensitivity, then a parabola
/// in log cm through the points around the lowest one (cost curves can be flat at the bottom,
/// where the single lowest point is mostly noise).
fn best_cm(rng: &mut StdRng, runs: u64, mut cost: impl FnMut(&mut StdRng, f64, u64) -> Vec<PairResult>) -> f64 {
    let grid: Vec<f64> = (0..28).map(|i| HAND_CM * 0.6 * 1.04f64.powi(i)).collect();
    let costs: Vec<f64> = grid.iter().map(|&cm| mean_cost(rng, cm, runs, &mut cost)).collect();
    println!("cost curve: {:?}", grid.iter().zip(&costs).map(|(g, c)| (g.round(), (c * 100.0).round() / 100.0)).collect::<Vec<_>>());
    let i = (0..grid.len()).min_by(|&a, &b| costs[a].total_cmp(&costs[b])).unwrap();
    assert!(i > 2 && i < grid.len() - 3, "lowest cost at the edge of the grid: no best inside");
    // Least-squares parabola c = a·x² + b·x + k over ±6 grid steps, x = ln cm.
    let pts: Vec<(f64, f64)> =
        (i.saturating_sub(6)..=(i + 6).min(grid.len() - 1)).map(|j| (grid[j].ln(), costs[j])).collect();
    let mut m = [[0.0f64; 3]; 3];
    let mut v = [0.0f64; 3];
    for &(x, y) in &pts {
        let row = [x * x, x, 1.0];
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += row[r] * row[c];
            }
            v[r] += row[r] * y;
        }
    }
    let det3 = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let with_col = |c: usize| {
        let mut t = m;
        for r in 0..3 {
            t[r][c] = v[r];
        }
        det3(&t) / det3(&m)
    };
    let (a, b) = (with_col(0), with_col(1));
    let vertex = (-b / (2.0 * a)).exp();
    if a > 0.0 && (grid[0]..=grid[grid.len() - 1]).contains(&vertex) {
        vertex
    } else {
        grid[i]
    }
}

const SWEEP: [f64; 7] = [0.7, 0.8, 0.9, 1.0, 1.1, 1.25, 1.4];

fn tally<'a>(signals: impl Iterator<Item = &'a Signal>) -> (usize, usize, usize) {
    let mut c = (0, 0, 0);
    for s in signals {
        match s {
            Signal::TooFast => c.0 += 1,
            Signal::TooSlow => c.1 += 1,
            Signal::Direct => c.2 += 1,
        }
    }
    c
}

/// Paired signals at each sweep point around `best`, and their logistic crossover.
fn paired_sweep(
    rng: &mut StdRng,
    best: f64,
    runs: u64,
    mut play: impl FnMut(&mut StdRng, f64, u64) -> Vec<PairResult>,
) -> (f64, Vec<(f64, (usize, usize, usize))>) {
    let rows: Vec<(f64, (usize, usize, usize))> = SWEEP
        .iter()
        .map(|r| {
            let cm = best * r;
            let pairs: Vec<PairResult> = (0..runs).flat_map(|s| play(rng, cm, s)).collect();
            (*r, tally(pairs.iter().map(|p| &p.signal)))
        })
        .collect();
    let fit: Vec<(f64, usize, usize)> = rows.iter().map(|(r, c)| (best * r, c.0, c.1)).collect();
    let c = fit_crossover(&fit).expect("a crossover");
    (c.cm_360 / best, rows)
}

// ---------- The old judges break the way real play did ----------

#[test]
fn lagging_tracker_breaks_the_old_analyzer() {
    let mut rng = StdRng::seed_from_u64(1);
    for r in [0.7, 1.0, 1.4] {
        let ws: Vec<TrackingWindow> = (0..3).flat_map(|s| track(&mut rng, HAND_CM * r, s, LAGGING, true).0).collect();
        let (f, s, d) = tally(ws.iter().filter_map(|w| w.signal()).collect::<Vec<_>>().iter());
        println!("old tracking at {r}× hand: fast {f} slow {s} direct {d}");
        assert!(s >= 4 * f.max(1), "{r}: fast {f} slow {s}");
    }
}

#[test]
fn undershooting_hand_breaks_the_old_micro_rule() {
    let mut rng = StdRng::seed_from_u64(2);
    for r in [0.7, 1.0, 1.4] {
        let old: Vec<Signal> = (0..6).flat_map(|s| micro(&mut rng, HAND_CM * r, s, true).1).flatten().collect();
        let (f, s, d) = tally(old.iter());
        println!("old micro at {r}× hand: fast {f} slow {s} direct {d}");
        // Real play: "too slow" never (0 of 28). Here: at most one target in twenty, even
        // when the sensitivity is far too slow.
        assert!(s * 20 <= f + s + d, "{r}: fast {f} slow {s} direct {d}");
    }
}

// ---------- The paired judges flip at the best sensitivity ----------

#[test]
fn paired_tracking_flips_at_the_best_sensitivity() {
    for prediction in [PREDICTING, PARTIAL] {
        let mut rng = StdRng::seed_from_u64(3);
        let best = best_cm(&mut rng, 2, |rng, cm, s| track(rng, cm, s, prediction, false).1);
        let (ratio, rows) = paired_sweep(&mut rng, best, 6, |rng, cm, s| track(rng, cm, s, prediction, true).1);
        println!("tracking prediction {prediction}: best {best:.1}, crossover {ratio:.3}× best, {rows:?}");
        assert!((ratio - 1.0).abs() <= 0.05, "prediction {prediction}: {ratio:.3}");
    }
}

#[test]
fn paired_micro_flips_at_the_best_sensitivity() {
    let mut rng = StdRng::seed_from_u64(4);
    let best = best_cm(&mut rng, 8, |rng, cm, s| micro(rng, cm, s, false).0);
    let (ratio, rows) = paired_sweep(&mut rng, best, 12, |rng, cm, s| micro(rng, cm, s, true).0);
    println!("micro: best {best:.1} (hand {HAND_CM}), crossover {ratio:.3}× best, {rows:?}");
    assert!((ratio - 1.0).abs() <= 0.05, "{ratio:.3}");
}

#[test]
fn paired_signals_point_the_right_way_far_from_the_best() {
    let mut rng = StdRng::seed_from_u64(5);
    let best_t = best_cm(&mut rng, 2, |rng, cm, s| track(rng, cm, s, PARTIAL, false).1);
    let best_m = best_cm(&mut rng, 8, |rng, cm, s| micro(rng, cm, s, false).0);
    for (name, best) in [("tracking", best_t), ("micro", best_m)] {
        for (r, want) in [(0.7, Signal::TooFast), (1.4, Signal::TooSlow)] {
            let pairs: Vec<PairResult> = (0..8)
                .flat_map(|s| match name {
                    "tracking" => track(&mut rng, best * r, s, PARTIAL, true).1,
                    _ => micro(&mut rng, best * r, s, true).0,
                })
                .collect();
            let (f, s, _) = tally(pairs.iter().map(|p| &p.signal));
            let right = if want == Signal::TooFast { f } else { s };
            println!("{name} at {r}× best: fast {f} slow {s}");
            assert!(right * 10 >= (f + s) * 6, "{name} at {r}: fast {f} slow {s}");
        }
    }
}

#[test]
#[ignore = "the simulated hand doesn't adapt to the sensitivity, so at 0.5× it still overshoots"]
fn creeping_hand_breaks_the_old_flick_rule() {
    // Real play: flick said "too slow" 69 times to 28 even at 16.5–17.5 cm/360. Not reproduced
    // here: this hand plans every move in its own sensitivity, so at a fast one its creeping
    // primary move (60%) still lands past the target (fast 22, slow 13, direct 37 at 0.5×).
    // Real players re-tune their movement to the sensitivity within a few shots; modelling
    // that is out of scope. The paired judge doesn't depend on movement shape either way.
    let mut rng = StdRng::seed_from_u64(20);
    for r in [0.5, 1.0] {
        let old: Vec<Signal> =
            (0..4).flat_map(|s| flick(&mut rng, HAND_CM * r, s, CREEPING, true).1).flatten().collect();
        let (f, s, d) = tally(old.iter());
        println!("old flick, creeping hand at {r}× hand: fast {f} slow {s} direct {d}");
        assert!(s >= 2 * f.max(1), "{r}: fast {f} slow {s}");
    }
}

#[test]
fn paired_flick_flips_at_the_best_sensitivity() {
    for band in FlickBand::ALL {
        let mut rng = StdRng::seed_from_u64(21);
        let best = best_cm(&mut rng, 6, |rng, cm, s| in_band(flick(rng, cm, s, UNDERSHOOTING, false).0, band));
        let (ratio, rows) =
            paired_sweep(&mut rng, best, 10, |rng, cm, s| in_band(flick(rng, cm, s, UNDERSHOOTING, true).0, band));
        println!("flick {band:?}: best {best:.1}, crossover {ratio:.3}× best, {rows:?}");
        assert!((ratio - 1.0).abs() <= 0.05, "{band:?}: {ratio:.3}");
    }
}

// ---------- The whole search with human-like players ----------

/// Share of TooFast / TooSlow among paired signals at each sweep ratio, measured once.
struct Rates(Vec<(f64, f64, f64)>);

impl Rates {
    fn measure(rng: &mut StdRng, runs: u64, mut play: impl FnMut(&mut StdRng, f64, u64) -> Vec<PairResult>, best: f64) -> Self {
        let grid = [0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.25, 1.4, 1.6, 1.8];
        Rates(
            grid.iter()
                .map(|&r| {
                    let pairs: Vec<PairResult> = (0..runs).flat_map(|s| play(rng, best * r, s)).collect();
                    let (f, s, _) = tally(pairs.iter().map(|p| &p.signal));
                    let n = pairs.len().max(1) as f64;
                    (r, f as f64 / n, s as f64 / n)
                })
                .collect(),
        )
    }

    /// A signal at `ratio` = tested / best, interpolating in log ratio.
    fn draw(&self, rng: &mut StdRng, ratio: f64) -> Signal {
        let v = &self.0;
        let r = ratio.clamp(v[0].0, v[v.len() - 1].0);
        let i = v.iter().position(|p| p.0 >= r).unwrap().max(1);
        let (a, b) = (v[i - 1], v[i]);
        let t = (r.ln() - a.0.ln()) / (b.0.ln() - a.0.ln());
        let (pf, ps) = (a.1 + t * (b.1 - a.1), a.2 + t * (b.2 - a.2));
        let u: f64 = rng.gen();
        if u < pf {
            Signal::TooFast
        } else if u < pf + ps {
            Signal::TooSlow
        } else {
            Signal::Direct
        }
    }
}

/// Same model as the other simulations: faster than `best` leans flick misses "too fast".
fn flick_signal(rng: &mut StdRng, best: f64, cm: f64) -> Signal {
    let mismatch = (best / cm).ln();
    let p_direct = 0.45 * (-(mismatch * mismatch) / (2.0 * 0.25f64.powi(2))).exp() + 0.15;
    let p_fast = 1.0 / (1.0 + (-4.0 * mismatch).exp());
    if rng.gen::<f64>() < p_direct {
        Signal::Direct
    } else if rng.gen::<f64>() < p_fast {
        Signal::TooFast
    } else {
        Signal::TooSlow
    }
}

/// Paired-test rates for the whole-search check.
struct PairedRates<'a> {
    flick: Option<&'a Rates>,
    micro: &'a Rates,
    tracking: &'a Rates,
}

/// `paired` = None: flick only, judged the old way (`flick_signal`, 9 shots per block).
fn search(rng: &mut StdRng, best: f64, paired: Option<PairedRates>) -> BlockScheduler {
    let mut s = BlockScheduler::new(15.0, 80.0, 7);
    while !s.is_completed {
        let cm = s.current_cm_360;
        match paired.as_ref().and_then(|p| p.flick) {
            Some(rates) => {
                for _ in 0..9 {
                    s.record(TestKind::Flick, rates.draw(rng, cm / best));
                }
            }
            None => {
                for _ in 0..9 {
                    s.record(TestKind::Flick, flick_signal(rng, best, cm));
                }
            }
        }
        if let Some(PairedRates { micro, tracking, .. }) = paired.as_ref() {
            for _ in 0..4 {
                s.record(TestKind::Micro, micro.draw(rng, cm / best));
            }
            for _ in 0..2 {
                s.record(TestKind::Tracking, tracking.draw(rng, cm / best));
            }
        }
        s.end_block();
    }
    s
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

#[test]
fn block_search_with_human_like_players() {
    let mut rng = StdRng::seed_from_u64(6);
    let best_t = best_cm(&mut rng, 2, |rng, cm, s| track(rng, cm, s, PARTIAL, false).1);
    let best_m = best_cm(&mut rng, 8, |rng, cm, s| micro(rng, cm, s, false).0);
    let tracking = Rates::measure(&mut rng, 4, |rng, cm, s| track(rng, cm, s, PARTIAL, true).1, best_t);
    let micro_rates = Rates::measure(&mut rng, 8, |rng, cm, s| micro(rng, cm, s, true).0, best_m);
    let err = |r: f64, best: f64| (r - best).abs() / best * 100.0;
    for best in [25.0, 40.0, 60.0] {
        let (mut all, mut flick, mut per_test, mut excluded) = (Vec::new(), Vec::new(), [Vec::new(), Vec::new()], 0);
        for _ in 0..200 {
            let s = search(&mut rng, best, Some(PairedRates { flick: None, micro: &micro_rates, tracking: &tracking }));
            all.push(err(s.recommended_cm_360.unwrap(), best));
            excluded += usize::from(!s.excluded_tests().is_empty());
            for (i, k) in [TestKind::Micro, TestKind::Tracking].into_iter().enumerate() {
                if let Some(e) = s.estimates().into_iter().find(|e| e.kind == k) {
                    per_test[i].push((e.crossover.cm_360 / best).ln() * 100.0);
                }
            }
            flick.push(err(search(&mut rng, best, None).recommended_cm_360.unwrap(), best));
        }
        let (a, f) = (median(all), median(flick));
        // Signed (log %) for bias, absolute for spread.
        let bias = |v: &Vec<f64>| median(v.clone());
        let spread = |v: &Vec<f64>| median(v.iter().map(|x| x.abs()).collect());
        let (mb, tb, m, t) = (bias(&per_test[0]), bias(&per_test[1]), spread(&per_test[0]), spread(&per_test[1]));
        println!("best {best}: all tests {a:.1}% vs flick only {f:.1}%; micro alone bias {mb:+.1}% spread {m:.1}%, tracking alone bias {tb:+.1}% spread {t:.1}%; runs with a test left out {excluded}/200");
        assert!(a <= 7.0 && a <= f, "best {best}: {a:.1}% vs flick only {f:.1}%");
        assert!(excluded <= 20, "best {best}: {excluded}/200 left a test out");
        // Each paired test's own estimate. Its leftover lean (≈ −6% micro, +6% tracking) is not
        // the shared slope: per-test slopes at any strength left it within a point or two. It
        // comes from fitting near-step signals (tracking pairs read 11 of 11 one way a step
        // from the best) with a logistic over tested points that cluster on one side. The
        // combined result is what players get, and it lands within ~3%.
        // The lean moves ±2% between random streams (tracking +7.0% → +9.2% after an unrelated
        // fit change), so the bound leaves room for that.
        assert!(mb.abs() <= 10.0 && tb.abs() <= 10.0, "best {best}: micro bias {mb:+.1}% tracking bias {tb:+.1}%");
    }
}

#[test]
fn block_search_with_every_test_paired() {
    let mut rng = StdRng::seed_from_u64(22);
    let best_t = best_cm(&mut rng, 2, |rng, cm, s| track(rng, cm, s, PARTIAL, false).1);
    let best_m = best_cm(&mut rng, 8, |rng, cm, s| micro(rng, cm, s, false).0);
    let best_f = best_cm(&mut rng, 4, |rng, cm, s| flick(rng, cm, s, UNDERSHOOTING, false).0);
    let tracking = Rates::measure(&mut rng, 4, |rng, cm, s| track(rng, cm, s, PARTIAL, true).1, best_t);
    let micro_rates = Rates::measure(&mut rng, 8, |rng, cm, s| micro(rng, cm, s, true).0, best_m);
    let flick_rates = Rates::measure(&mut rng, 6, |rng, cm, s| flick(rng, cm, s, UNDERSHOOTING, true).0, best_f);
    let err = |r: f64, best: f64| (r - best).abs() / best * 100.0;
    for best in [25.0, 40.0, 60.0] {
        let (mut all, mut old_flick, mut none) = (Vec::new(), Vec::new(), 0);
        for _ in 0..200 {
            let rates = PairedRates { flick: Some(&flick_rates), micro: &micro_rates, tracking: &tracking };
            let s = search(&mut rng, best, Some(rates));
            match s.recommendation(None) {
                Some(_) => all.push(err(s.recommended_cm_360.unwrap(), best)),
                None => none += 1,
            }
            old_flick.push(err(search(&mut rng, best, None).recommended_cm_360.unwrap(), best));
        }
        let (a, f) = (median(all), median(old_flick));
        println!("best {best}: every test paired {a:.1}% vs old flick only {f:.1}%; runs with no answer {none}/200");
        assert!(a <= 4.5 && a < f, "best {best}: {a:.1}% vs {f:.1}%");
        assert!(none <= 10, "best {best}: {none}/200 without an answer");
    }
}
