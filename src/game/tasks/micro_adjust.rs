use glam::{Vec2, Vec3};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::scoring::ScoreTracker;
use super::super::target::Target;
use super::flick::place_target;
use super::{AimTarget, Task, TaskMetrics};
use crate::habits::record::AimKind;
use crate::sensitivity::paired::{abba, PairBook, PairResult, Side};

/// How far from the aim a micro target appears, in degrees. Below ~1.5° a head-sized target
/// (≈1° radius at 9 m) would already be under the crosshair.
pub const MICRO_ANGLE_DEG: (f32, f32) = (1.5, 5.0);

/// A miss on a paired target costs this many seconds on top of the time to hit it.
pub const MISS_PENALTY_SECS: f64 = 0.3;

/// Where a target was placed, for the shot classifier and the calibration record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MicroSpawn {
    /// Mouse movement in degrees that puts the aim on the target centre: x = right,
    /// y = down (mouse coordinates), like the raw-input samples.
    pub offset_deg: Vec2,
}

/// Mouse-space offset (degrees, y down) from `aim` to `target_dir`, matching how
/// `Camera::rotate` turns mouse counts into yaw and pitch.
pub fn mouse_offset_deg(aim: Vec3, target_dir: Vec3) -> Vec2 {
    let yaw_pitch = |v: Vec3| {
        let v = v.normalize();
        (v.z.atan2(v.x), v.y.clamp(-1.0, 1.0).asin())
    };
    let (ya, pa) = yaw_pitch(aim);
    let (yt, pt) = yaw_pitch(target_dir);
    let tau = std::f32::consts::TAU;
    let dyaw = (yt - ya + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    Vec2::new(dyaw.to_degrees(), -(pt - pa).to_degrees())
}

pub struct MicroAdjustTask {
    targets: Vec<Target>,
    next_id: u64,
    score_tracker: ScoreTracker,
    target_count: usize,
    max_targets: usize,
    target_radius: f32, // Smaller size (e.g. 0.15m ~ head size at range)
    start_time: Instant,
    last_spawn_time: Instant,
    is_finished: bool,
    /// `Some` for a seeded run: the same seed gives the same offsets every start.
    seed: Option<u64>,
    rng: StdRng,
    /// Spawn info of the targets still in `targets`, by target id.
    spawns: Vec<(u64, MicroSpawn)>,
    /// Paired runs: the side of each target (slot = target number) and their costs.
    pairs: Option<PairBook>,
    /// Angle shared by both targets of the pair in progress.
    pair_angle: f32,
    /// Seconds the live target has been up, and the misses fired at it.
    live_age: f32,
    live_misses: u32,
}

impl MicroAdjustTask {
    pub fn new(max_targets: usize, target_radius: f32) -> Self {
        Self {
            targets: Vec::with_capacity(2),
            next_id: 1,
            score_tracker: ScoreTracker::new(),
            target_count: 0,
            max_targets,
            target_radius,
            start_time: Instant::now(),
            last_spawn_time: Instant::now(),
            is_finished: false,
            seed: None,
            rng: StdRng::from_entropy(),
            spawns: Vec::new(),
            pairs: None,
            pair_angle: 0.0,
            live_age: 0.0,
            live_misses: 0,
        }
    }

    /// `pairs` pairs of targets, `first` side leading in `A B B A` order. Both targets of a
    /// pair sit at the same angle from the aim (time to a target depends on its distance).
    pub fn paired(pairs: usize, target_radius: f32, seed: u64, first: Side) -> Self {
        let mut t = Self::seeded(2 * pairs, target_radius, seed);
        t.pairs = Some(PairBook::new(abba(first, 2 * pairs)));
        t
    }

    /// Targets at reproducible angles and directions from the aim.
    pub fn seeded(max_targets: usize, target_radius: f32, seed: u64) -> Self {
        let mut t = Self::new(max_targets, target_radius);
        t.seed = Some(seed);
        t.rng = StdRng::seed_from_u64(seed);
        t
    }

    /// Offset of the live target, if there is one.
    pub fn current_spawn(&self) -> Option<MicroSpawn> {
        let live = self.targets.iter().find(|t| t.is_alive())?;
        self.spawns.iter().find(|(id, _)| *id == live.id).map(|(_, s)| *s)
    }

    /// Spawns a small target a few degrees from where the player is aiming.
    fn spawn_micro_target(&mut self, aim: Ray) {
        if self.target_count >= self.max_targets {
            self.is_finished = true;
            return;
        }

        let (lo, hi) = MICRO_ANGLE_DEG;
        let slot = self.target_count;
        let angle = if self.pairs.is_some() && slot % 2 == 1 {
            self.pair_angle
        } else {
            self.rng.gen_range(lo..=hi)
        };
        self.pair_angle = angle;
        self.live_age = 0.0;
        self.live_misses = 0;
        let phi = self.rng.gen_range(0.0..std::f32::consts::TAU);
        let position = place_target(aim.origin, aim.direction, angle, phi);
        let spawn = MicroSpawn { offset_deg: mouse_offset_deg(aim.direction, position - aim.origin) };

        let id = self.next_id;
        self.next_id += 1;
        self.target_count += 1;
        self.last_spawn_time = Instant::now();

        let targets = &self.targets;
        self.spawns.retain(|(sid, _)| targets.iter().any(|t| t.id == *sid));
        self.spawns.push((id, spawn));
        self.targets.push(Target::new(id, position, self.target_radius, None));
    }
}

impl Task for MicroAdjustTask {
    fn name(&self) -> &'static str {
        super::TaskType::MicroAdjust.task_name()
    }

    fn description(&self) -> &'static str {
        "Small head-sized targets spawn tightly clustered near your crosshair. Test fine fingertip/wrist precision."
    }

    fn start(&mut self) {
        self.targets.clear();
        self.spawns.clear();
        self.score_tracker.reset();
        self.target_count = 0;
        self.is_finished = false;
        self.start_time = Instant::now();
        if let Some(seed) = self.seed {
            self.rng = StdRng::seed_from_u64(seed);
        }
        if let Some(book) = &self.pairs {
            let order = (0..self.max_targets).filter_map(|i| book.side(i)).collect();
            self.pairs = Some(PairBook::new(order));
        }
        // Targets are placed relative to the aim, which the task first sees in `update` (the
        // player may have turned during the countdown). The first update spawns one.
    }

    fn update(&mut self, dt: f32, crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }

        for target in &mut self.targets {
            target.update(dt);
        }

        self.targets.retain(|t| t.current_render_radius().is_some());
        if self.targets.iter().any(|t| t.is_alive()) {
            self.live_age += dt;
        }

        // Next target as soon as the last one is shot; the hit one finishes its pop meanwhile.
        if !self.targets.iter().any(|t| t.is_alive()) {
            if self.target_count < self.max_targets {
                self.spawn_micro_target(crosshair_ray);
            } else {
                self.is_finished = true;
            }
        }
    }

    fn on_click(&mut self, ray: Ray) -> bool {
        if self.is_finished {
            return false;
        }

        let now = Instant::now();
        let mut hit = false;

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
                hit = true;
                if let Some(book) = &mut self.pairs {
                    let cost = self.live_age as f64 + self.live_misses as f64 * MISS_PENALTY_SECS;
                    book.record(self.target_count - 1, Some(cost));
                }
                break;
            }
        }

        if !hit {
            self.score_tracker.record_miss();
            self.live_misses += 1;
        }

        hit
    }

    fn is_complete(&self) -> bool {
        self.is_finished
    }

    fn aim_target(&self) -> Option<AimTarget> {
        let live = self.targets.iter().find(|t| t.is_alive())?;
        Some(AimTarget { id: live.id, center: live.position, radius: live.base_radius, kind: AimKind::Micro, band: None })
    }

    fn micro_spawn(&self) -> Option<MicroSpawn> {
        self.current_spawn()
    }

    fn sens_factor(&self) -> f32 {
        let Some(book) = &self.pairs else { return 1.0 };
        // The live target's slot; between targets, the next one's.
        let live = self.targets.iter().any(|t| t.is_alive());
        let slot = if live { self.target_count - 1 } else { self.target_count };
        book.side(slot).map_or(1.0, |s| s.factor())
    }

    fn take_pair_results(&mut self) -> Vec<PairResult> {
        self.pairs.as_mut().map(|b| b.take_finished()).unwrap_or_default()
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
            custom_metric_label: "Micro Accuracy",
            custom_metric_value: format!("{:.1}% ({} hits)", self.score_tracker.accuracy_percentage(), self.score_tracker.hits),
        }
    }
}
