use glam::Vec3;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::scoring::ScoreTracker;
use super::super::target::{Target, TargetState};
use super::micro_adjust::MISS_PENALTY_SECS;
use super::{Task, TaskMetrics};
use crate::sensitivity::paired::{abba, PairBook, PairResult, Side};

/// A paired target that times out costs this on top of its time and misses: as much as two
/// more misses. Not getting there in time is a bad result, not a missing one.
pub const EXPIRED_PENALTY_SECS: f64 = 0.6;

/// How far a flick goes, as an angle from where the player is aiming when the target appears.
/// Fast sensitivities favour far flicks and slow ones short flicks, so every calibration block
/// has the same mix and blocks at different sensitivities stay comparable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlickBand {
    Short,
    Mid,
    Far,
}

impl FlickBand {
    pub const ALL: [FlickBand; 3] = [FlickBand::Short, FlickBand::Mid, FlickBand::Far];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            FlickBand::Short => "short",
            FlickBand::Mid => "mid",
            FlickBand::Far => "far",
        }
    }

    /// Angle from the aim to the target, in degrees.
    pub fn range_deg(self) -> (f32, f32) {
        match self {
            // Every band stays inside the view (70° vertical FOV: ±51° across at 16:9, ±48° at
            // 16:10, ±43° at 4:3). A flick to a target the player can't see is a search: its
            // first move can't be planned, so it says nothing about the sensitivity.
            FlickBand::Short => (5.0, 12.0),
            FlickBand::Mid => (15.0, 25.0),
            FlickBand::Far => (30.0, 40.0),
        }
    }
}

/// `per_band` flicks of each band in a random order.
pub fn banded_order(per_band: usize, rng: &mut impl Rng) -> Vec<FlickBand> {
    let mut order: Vec<FlickBand> = FlickBand::ALL
        .iter()
        .flat_map(|&b| std::iter::repeat(b).take(per_band))
        .collect();
    order.shuffle(rng);
    order
}

/// Banded targets all sit this far from the eye, so a target's angular size never changes.
pub const TARGET_DISTANCE: f32 = 9.0;
/// Lowest target elevation from the eye: at 9 m this keeps a target clear of the floor.
const MIN_ELEVATION_DEG: f32 = -6.0;
const MAX_ELEVATION_DEG: f32 = 25.0;
/// Far flicks go mostly sideways: a 90° flick straight up would leave the room's view.
const FAR_MAX_TILT_DEG: f32 = 15.0;

/// A point `angle_deg` from `forward`, in screen direction `phi` (0 = right, π/2 = up), at
/// `TARGET_DISTANCE` from `origin`. A target that would be too low or too high is mirrored
/// vertically (same angle); if that is still out of range its elevation is clamped, which
/// changes the flick a little.
pub fn place_target(origin: Vec3, forward: Vec3, angle_deg: f32, phi: f32) -> Vec3 {
    let f = forward.normalize();
    let right = {
        let r = f.cross(Vec3::Y);
        if r.length_squared() < 1e-6 {
            Vec3::X
        } else {
            r.normalize()
        }
    };
    let up = right.cross(f).normalize();
    let (st, ct) = angle_deg.to_radians().sin_cos();
    let dir_for = |sin_phi: f32, cos_phi: f32| (f * ct + (right * cos_phi + up * sin_phi) * st).normalize();
    let elev = |d: Vec3| d.y.clamp(-1.0, 1.0).asin().to_degrees();
    let in_range = |d: Vec3| (MIN_ELEVATION_DEG..=MAX_ELEVATION_DEG).contains(&elev(d));

    let (sp, cp) = phi.sin_cos();
    let mut dir = dir_for(sp, cp);
    if !in_range(dir) {
        let mirrored = dir_for(-sp, cp);
        dir = if in_range(mirrored) {
            mirrored
        } else {
            // Neither fits: keep the heading, clamp the elevation.
            let e = elev(dir).clamp(MIN_ELEVATION_DEG, MAX_ELEVATION_DEG).to_radians();
            let mut flat = Vec3::new(dir.x, 0.0, dir.z);
            if flat.length_squared() < 1e-6 {
                flat = Vec3::new(f.x, 0.0, f.z);
            }
            if flat.length_squared() < 1e-6 {
                flat = Vec3::NEG_Z;
            }
            (flat.normalize() * e.cos() + Vec3::Y * e.sin()).normalize()
        };
    }
    origin + dir * TARGET_DISTANCE
}

/// Where a target was placed, for the calibration record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlickSpawn {
    /// `None` for unbanded (practice) targets.
    pub band: Option<FlickBand>,
    /// Angle from the aim at spawn to the target, in degrees.
    pub angle_deg: f32,
}

struct BandedSpawns {
    order: Vec<FlickBand>,
    seed: u64,
    rng: StdRng,
}

pub struct FlickTask {
    targets: Vec<Target>,
    next_id: u64,
    score_tracker: ScoreTracker,
    target_count: usize,
    max_targets: usize,
    target_radius: f32,
    start_time: Instant,
    last_spawn_time: Instant,
    time_limit_per_target_secs: f32,
    /// Targets that timed out without being hit, since `start`.
    expired_count: u32,
    is_finished: bool,
    banded: Option<BandedSpawns>,
    /// The aim as of the last update; banded targets are placed relative to it.
    aim: Ray,
    /// Spawn info of the targets still in `targets`, by target id.
    spawns: Vec<(u64, FlickSpawn)>,
    /// Paired runs: the side of each target and the pair costs, tagged with the pair's band.
    pairs: Option<(PairBook, Vec<Side>, Vec<FlickBand>)>,
    /// Angle shared by both targets of the pair in progress.
    pair_angle: f32,
    /// Seconds the live target has been up, and the misses fired at it.
    live_age: f32,
    live_misses: u32,
}

impl FlickTask {
    pub fn new(max_targets: usize, target_radius: f32) -> Self {
        Self {
            targets: Vec::with_capacity(4),
            next_id: 1,
            score_tracker: ScoreTracker::new(),
            target_count: 0,
            max_targets,
            target_radius,
            start_time: Instant::now(),
            last_spawn_time: Instant::now(),
            time_limit_per_target_secs: 2.0,
            expired_count: 0,
            is_finished: false,
            banded: None,
            aim: Ray { origin: Vec3::new(0.0, 1.5, 0.0), direction: Vec3::NEG_Z },
            spawns: Vec::new(),
            pairs: None,
            pair_angle: 0.0,
            live_age: 0.0,
            live_misses: 0,
        }
    }

    /// `pairs_per_band` pairs at each distance, pairs in a seeded random band order; both
    /// targets of a pair sit at the same angle (time to a target depends on its distance),
    /// sides in `A B B A` order with `first` leading.
    pub fn paired(pairs_per_band: usize, seed: u64, target_radius: f32, first: Side) -> Self {
        let bands = banded_order(pairs_per_band, &mut StdRng::seed_from_u64(seed ^ 0x5EED_F11C));
        let order: Vec<FlickBand> = bands.iter().flat_map(|&b| [b, b]).collect();
        let sides = abba(first, order.len());
        let mut t = Self::banded(order, seed, target_radius);
        t.pairs = Some((PairBook::new(sides.clone()).with_bands(bands.clone()), sides, bands));
        t
    }

    /// Record the live target's cost in its pair (paired runs only).
    fn record_live_cost(&mut self, extra_secs: f64) {
        let cost = self.live_age as f64 + self.live_misses as f64 * MISS_PENALTY_SECS + extra_secs;
        let slot = self.target_count.saturating_sub(1);
        if let Some((book, _, _)) = &mut self.pairs {
            book.record(slot, Some(cost));
        }
    }

    /// One target per entry of `order`, each placed that band's angle from the player's aim.
    /// The same `seed` gives the same angles and directions every time the task starts.
    pub fn banded(order: Vec<FlickBand>, seed: u64, target_radius: f32) -> Self {
        let mut t = Self::new(order.len(), target_radius);
        t.banded = Some(BandedSpawns { order, seed, rng: StdRng::seed_from_u64(seed) });
        t
    }

    /// Band and angle of the live target, if there is one.
    pub fn current_spawn(&self) -> Option<FlickSpawn> {
        let live = self.targets.iter().find(|t| t.is_alive())?;
        self.spawns.iter().find(|(id, _)| *id == live.id).map(|(_, s)| *s)
    }

    /// The next target's position and spawn info.
    fn next_position(&mut self) -> (Vec3, FlickSpawn) {
        let aim = self.aim;
        let index = self.target_count;
        let reuse_angle = self.pairs.is_some() && index % 2 == 1;
        let pair_angle = self.pair_angle;
        if let Some(b) = self.banded.as_mut() {
            let band = b.order[index];
            let (lo, hi) = band.range_deg();
            let angle = if reuse_angle { pair_angle } else { b.rng.gen_range(lo..=hi) };
            let draw_phi = |rng: &mut StdRng| {
                if band == FlickBand::Far {
                    let tilt = rng.gen_range(-FAR_MAX_TILT_DEG..=FAR_MAX_TILT_DEG).to_radians();
                    if rng.gen_bool(0.5) {
                        tilt
                    } else {
                        std::f32::consts::PI - tilt
                    }
                } else {
                    rng.gen_range(0.0..std::f32::consts::TAU)
                }
            };
            let mut pos = place_target(aim.origin, aim.direction, angle, draw_phi(&mut b.rng));
            let mut actual = aim.direction.angle_between(pos - aim.origin).to_degrees();
            // The second target of a pair must be as far as the first really was: a direction
            // that hits the floor or ceiling gets its elevation clamped, which shortens the
            // flick, so try a few other directions.
            for _ in 0..12 {
                if !reuse_angle || (actual - angle).abs() < 0.05 {
                    break;
                }
                pos = place_target(aim.origin, aim.direction, angle, draw_phi(&mut b.rng));
                actual = aim.direction.angle_between(pos - aim.origin).to_degrees();
            }
            // The pair's distance is what the first target actually got.
            self.pair_angle = actual;
            return (pos, FlickSpawn { band: Some(band), angle_deg: actual });
        }
        let mut rng = rand::thread_rng();
        // Wider horizontal and vertical distribution for flicking
        let x = rng.gen_range(-5.0..=5.0);
        let y = rng.gen_range(0.6..=2.6);
        let z = rng.gen_range(-11.0..=-8.0);
        let pos = Vec3::new(x, y, z);
        let actual = aim.direction.angle_between(pos - aim.origin).to_degrees();
        (pos, FlickSpawn { band: None, angle_deg: actual })
    }

    /// How long each target stays up before it disappears unshot.
    pub fn with_time_limit(mut self, secs: f32) -> Self {
        self.time_limit_per_target_secs = secs;
        self
    }

    fn spawn_single_target(&mut self) {
        if self.target_count >= self.max_targets {
            self.is_finished = true;
            return;
        }

        let (position, spawn) = self.next_position();
        self.live_age = 0.0;
        self.live_misses = 0;
        let id = self.next_id;
        self.next_id += 1;
        self.target_count += 1;
        self.last_spawn_time = Instant::now();

        let targets = &self.targets;
        self.spawns.retain(|(sid, _)| targets.iter().any(|t| t.id == *sid));
        self.spawns.push((id, spawn));
        self.targets.push(Target::new(
            id,
            position,
            self.target_radius,
            Some(self.time_limit_per_target_secs),
        ));
    }
}

impl Task for FlickTask {
    fn name(&self) -> &'static str {
        "Flick Shot"
    }

    fn description(&self) -> &'static str {
        "One target appears at a random location. Flick and click it as fast and accurately as possible."
    }

    fn start(&mut self) {
        self.targets.clear();
        self.score_tracker.reset();
        self.target_count = 0;
        self.expired_count = 0;
        self.is_finished = false;
        self.start_time = Instant::now();
        self.spawns.clear();
        self.live_age = 0.0;
        self.live_misses = 0;
        if let Some((_, sides, bands)) = &self.pairs {
            let book = PairBook::new(sides.clone()).with_bands(bands.clone());
            self.pairs = Some((book, sides.clone(), bands.clone()));
        }
        if let Some(b) = self.banded.as_mut() {
            b.rng = StdRng::seed_from_u64(b.seed);
            // Placed relative to the aim, which the task first sees in `update`; the player
            // may have turned during the countdown. The first update spawns it.
            return;
        }
        self.spawn_single_target();
    }

    fn update(&mut self, dt: f32, crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }
        self.aim = crosshair_ray;

        if self.targets.iter().any(|t| t.is_alive()) {
            self.live_age += dt;
        }
        let mut expired = false;
        for target in &mut self.targets {
            let was_alive = target.is_alive();
            target.update(dt);
            if was_alive && target.state == TargetState::Despawned {
                self.expired_count += 1;
                expired = true;
            }
        }
        if expired {
            // Expiry runs on the wall clock while `live_age` sums frames (each capped), so
            // after a stall the age can fall short: an expired target took its whole limit.
            self.live_age = self.live_age.max(self.time_limit_per_target_secs);
            self.record_live_cost(EXPIRED_PENALTY_SECS);
        }

        // Check if alive target timed out
        let had_alive = self.targets.iter().any(|t| t.is_alive());
        self.targets.retain(|t| t.current_render_radius().is_some());

        if !had_alive || self.targets.is_empty() {
            if self.target_count < self.max_targets {
                self.spawn_single_target();
            } else {
                self.is_finished = true;
            }
        }
    }

    fn on_click(&mut self, ray: Ray) -> bool {
        if self.is_finished {
            return false;
        }

        let mut hit_any = false;
        let now = Instant::now();

        for target in &mut self.targets {
            if !target.is_alive() {
                continue;
            }

            if let Some((_, _, offset)) =
                super::super::hit_detection::intersect_ray_sphere(ray, target.position, target.base_radius)
            {
                let reaction_time = now.duration_since(target.spawn_time);
                self.score_tracker.record_hit(
                    target.id,
                    reaction_time,
                    offset,
                    target.base_radius,
                );
                target.hit();
                hit_any = true;
                break;
            }
        }
        if hit_any {
            self.record_live_cost(0.0);
        } else if self.targets.iter().any(|t| t.is_alive()) {
            self.live_misses += 1;
        }

        if !hit_any {
            self.score_tracker.record_miss();
        }

        hit_any
    }

    fn expired_targets(&self) -> u32 {
        self.expired_count
    }

    fn flick_spawn(&self) -> Option<FlickSpawn> {
        self.current_spawn()
    }

    fn sens_factor(&self) -> f32 {
        let Some((_, sides, _)) = &self.pairs else { return 1.0 };
        // The live target's slot; between targets, the next one's.
        let live = self.targets.iter().any(|t| t.is_alive());
        let slot = if live { self.target_count - 1 } else { self.target_count };
        sides.get(slot).map_or(1.0, |s| s.factor())
    }

    fn take_pair_results(&mut self) -> Vec<PairResult> {
        self.pairs.as_mut().map(|(b, _, _)| b.take_finished()).unwrap_or_default()
    }

    fn is_complete(&self) -> bool {
        self.is_finished
    }

    fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        self.targets
            .iter()
            .filter_map(|t| t.current_render_radius().map(|r| (t.position, r)))
            .collect()
    }

    fn get_metrics(&self) -> TaskMetrics {
        TaskMetrics {
            task_name: self.name().to_string(),
            score: self.score_tracker.total_score,
            accuracy: self.score_tracker.accuracy_percentage(),
            avg_reaction_time_ms: self.score_tracker.average_reaction_time().as_millis() as f32,
            shots_fired: self.score_tracker.total_shots,
            hits: self.score_tracker.hits,
            misses: self.score_tracker.misses,
            duration_secs: self.start_time.elapsed().as_secs_f32(),
            custom_metric_label: "Targets Cleared",
            custom_metric_value: format!("{}/{}", self.score_tracker.hits, self.max_targets),
        }
    }
}

