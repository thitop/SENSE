use glam::{Vec2, Vec3};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::target::Target;
use super::micro_adjust::mouse_offset_deg;
use super::{Task, TaskMetrics};
use crate::sensitivity::paired::{abba, PairBook, PairResult, Side};
use crate::sensitivity::tracking_signal::{TrackingAnalyzer, TrackingWindow};

/// Where the target starts: 9 m in front of the eye.
const START: Vec3 = Vec3::new(0.0, 1.5, -9.0);
/// The target bounces within x = ±4.5 and y = 0.6..2.6.
const ROOM_HALF_X: f32 = 4.5;
const ROOM_Y: (f32, f32) = (0.6, 2.6);

/// A paired tracking run: four chunks, `A B B A`.
pub const PAIRED_SECS: f32 = 12.0;
pub const CHUNK_SECS: f32 = 3.0;
/// Right after the sensitivity switches the hand is still tuned to the old one.
pub const SWITCH_SETTLE_SECS: f32 = 0.4;
/// A chunk with less counted time than this has no cost (its pair is dropped). The seeded
/// path turns or bounces about three times a second, so a 3 s chunk keeps ~1.0–1.8 s.
pub const MIN_COUNTED_SECS: f32 = 0.75;

/// Mean angular error over one chunk, leaving out the moments after a sensitivity switch
/// and after the target turns, which say how fast the player reacts rather than how well the
/// sensitivity suits them.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChunkCost {
    sum: f64,
    counted: f32,
}

impl ChunkCost {
    pub fn push(&mut self, dt: f32, error_deg: f32, since_switch: f32, since_turn: f32) {
        if since_switch >= SWITCH_SETTLE_SECS && since_turn >= TrackingAnalyzer::SETTLE_S {
            self.sum += (error_deg * dt) as f64;
            self.counted += dt;
        }
    }

    /// Mean error in degrees, `None` below `MIN_COUNTED_SECS` of counted time.
    pub fn finish(&self) -> Option<f64> {
        (self.counted >= MIN_COUNTED_SECS).then(|| self.sum / self.counted as f64)
    }
}

pub struct TrackingTask {
    target: Option<Target>,
    target_radius: f32,
    target_speed: f32,
    direction: Vec3,
    direction_change_timer: f32,
    task_duration_secs: f32,
    start_time: Instant,
    total_active_time: f32,
    time_on_target: f32,
    is_on_target_current_frame: bool,
    accumulated_offset: f32,
    sample_count: u32,
    is_finished: bool,
    /// `Some` for a seeded run: same seed, same path every start.
    seed: Option<u64>,
    rng: StdRng,
    /// Direction segments so far; seeded runs alternate their two speeds by segment.
    segment: usize,
    since_turn: f32,
    prev_target_dir: Option<Vec3>,
    analyzer: TrackingAnalyzer,
    /// Paired runs: the side of each chunk and the chunk costs.
    pairs: Option<PairBook>,
    chunk: usize,
    since_switch: f32,
    chunk_cost: ChunkCost,
}

impl TrackingTask {
    pub fn new(duration_secs: f32, target_radius: f32, target_speed: f32) -> Self {
        Self {
            target: None,
            target_radius,
            target_speed,
            direction: Vec3::X,
            direction_change_timer: 0.0,
            task_duration_secs: duration_secs,
            start_time: Instant::now(),
            total_active_time: 0.0,
            time_on_target: 0.0,
            is_on_target_current_frame: false,
            accumulated_offset: 0.0,
            sample_count: 0,
            is_finished: false,
            seed: None,
            rng: StdRng::from_entropy(),
            segment: 0,
            since_turn: 0.0,
            prev_target_dir: None,
            analyzer: TrackingAnalyzer::new(Self::radius_deg(target_radius)),
            pairs: None,
            chunk: 0,
            since_switch: 0.0,
            chunk_cost: ChunkCost::default(),
        }
    }

    /// `PAIRED_SECS` of seeded tracking in `CHUNK_SECS` chunks alternating a faster and a
    /// slower side, `first` leading in `A B B A` order.
    pub fn paired(target_radius: f32, seed: u64, first: Side) -> Self {
        let mut t = Self::seeded(PAIRED_SECS, target_radius, seed);
        t.pairs = Some(PairBook::new(abba(first, (PAIRED_SECS / CHUNK_SECS) as usize)));
        t
    }

    /// Close the chunk in progress into the pair book and start the next one.
    fn finish_chunk(&mut self) {
        if let Some(book) = &mut self.pairs {
            book.record(self.chunk, self.chunk_cost.finish());
        }
        self.chunk += 1;
        self.since_switch = 0.0;
        self.chunk_cost = ChunkCost::default();
    }

    /// Target speeds (m/s at ~9 m: ≈16 and ≈29 °/s) a seeded run alternates between.
    pub const SEEDED_SPEEDS: (f32, f32) = (2.5, 4.5);

    /// A reproducible path for calibration: straight segments at two alternating speeds, no
    /// wobble, so "ahead" and "behind" are clear.
    pub fn seeded(duration_secs: f32, target_radius: f32, seed: u64) -> Self {
        let mut t = Self::new(duration_secs, target_radius, Self::SEEDED_SPEEDS.0);
        t.seed = Some(seed);
        t.rng = StdRng::seed_from_u64(seed);
        t
    }

    fn radius_deg(radius: f32) -> f32 {
        (radius / (-START.z)).atan().to_degrees()
    }

    /// Half-second windows finished since the last call (see `TrackingAnalyzer`).
    pub fn take_windows(&mut self) -> Vec<TrackingWindow> {
        self.analyzer.take_windows()
    }

    fn speed(&self) -> f32 {
        match self.seed {
            Some(_) if self.segment % 2 == 1 => Self::SEEDED_SPEEDS.1,
            Some(_) => Self::SEEDED_SPEEDS.0,
            None => self.target_speed,
        }
    }

    fn pick_random_direction(&mut self) {
        // Move in horizontal/vertical 2D plane (X-Y)
        let angle: f32 = self.rng.gen_range(0.0..std::f32::consts::TAU);
        self.direction = Vec3::new(angle.cos(), angle.sin() * 0.5, 0.0).normalize();
        self.direction_change_timer = self.rng.gen_range(0.8..=1.8);
        self.segment += 1;
        self.since_turn = 0.0;
    }
}

impl Task for TrackingTask {
    fn name(&self) -> &'static str {
        "Smooth Tracking"
    }

    fn description(&self) -> &'static str {
        "Keep your crosshair smoothly locked onto the moving yellow sphere as it changes directions."
    }

    fn start(&mut self) {
        self.total_active_time = 0.0;
        self.time_on_target = 0.0;
        self.accumulated_offset = 0.0;
        self.sample_count = 0;
        self.is_finished = false;
        self.start_time = Instant::now();
        if let Some(seed) = self.seed {
            self.rng = StdRng::seed_from_u64(seed);
        }
        self.segment = 0;
        self.prev_target_dir = None;
        self.analyzer = TrackingAnalyzer::new(Self::radius_deg(self.target_radius));
        if let Some(book) = &self.pairs {
            let n = (self.task_duration_secs / CHUNK_SECS) as usize;
            self.pairs = Some(PairBook::new((0..n).filter_map(|i| book.side(i)).collect()));
        }
        self.chunk = 0;
        self.since_switch = 0.0;
        self.chunk_cost = ChunkCost::default();

        self.target = Some(Target::new(
            1,
            START,
            self.target_radius,
            None,
        ));
        self.pick_random_direction();
    }

    fn update(&mut self, dt: f32, crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }

        self.total_active_time += dt;
        // Within half a tick: summing f32 ticks lands just short of a boundary.
        if self.pairs.is_some() && self.total_active_time + dt * 0.5 >= (self.chunk + 1) as f32 * CHUNK_SECS {
            self.finish_chunk();
        }
        if self.total_active_time >= self.task_duration_secs {
            self.is_finished = true;
            return;
        }
        self.since_switch += dt;

        // Update direction switch
        self.direction_change_timer -= dt;
        if self.direction_change_timer <= 0.0 {
            self.pick_random_direction();
        }

        self.since_turn += dt;
        let speed = self.speed();
        let seeded = self.seed.is_some();
        if let Some(target) = &mut self.target {
            // Practice adds a vertical wobble; seeded runs move in straight segments.
            let oscillation = if seeded { 0.0 } else { (self.total_active_time * 2.5).sin() * 0.5 };
            let current_vel = (self.direction + Vec3::new(0.0, oscillation, 0.0)).normalize() * speed;
            target.position += current_vel * dt;

            // Bounce within the room.
            let half_x = ROOM_HALF_X;
            let (y_lo, y_hi) = ROOM_Y;
            if target.position.x < START.x - half_x || target.position.x > START.x + half_x {
                self.direction.x = -self.direction.x;
                target.position.x = target.position.x.clamp(START.x - half_x, START.x + half_x);
                self.since_turn = 0.0;
            }
            if target.position.y < y_lo || target.position.y > y_hi {
                self.direction.y = -self.direction.y;
                target.position.y = target.position.y.clamp(y_lo, y_hi);
                self.since_turn = 0.0;
            }

            // Where the target is and how it moved, in the mouse's degrees.
            let dir = target.position - crosshair_ray.origin;
            let vel = self
                .prev_target_dir
                .map_or(Vec2::ZERO, |p| mouse_offset_deg(p, dir) / dt.max(1e-6));
            self.prev_target_dir = Some(dir);
            let offset = mouse_offset_deg(crosshair_ray.direction, dir);
            self.analyzer.push(dt, offset, vel, self.since_turn);
            if self.pairs.is_some() {
                self.chunk_cost.push(dt, offset.length(), self.since_switch, self.since_turn);
            }

            // Raycast check: is crosshair on the sphere right now?
            if let Some((_, _, offset)) =
                super::super::hit_detection::intersect_ray_sphere(crosshair_ray, target.position, target.base_radius)
            {
                self.time_on_target += dt;
                self.is_on_target_current_frame = true;
                self.accumulated_offset += offset;
                self.sample_count += 1;
            } else {
                self.is_on_target_current_frame = false;
            }
        }
    }

    fn on_click(&mut self, _ray: Ray) -> bool {
        // Continuous tracking doesn't require clicking
        self.is_on_target_current_frame
    }

    fn is_complete(&self) -> bool {
        self.is_finished
    }

    fn take_tracking_windows(&mut self) -> Vec<TrackingWindow> {
        self.take_windows()
    }

    fn sens_factor(&self) -> f32 {
        self.pairs.as_ref().and_then(|b| b.side(self.chunk)).map_or(1.0, |s| s.factor())
    }

    fn take_pair_results(&mut self) -> Vec<PairResult> {
        self.pairs.as_mut().map(|b| b.take_finished()).unwrap_or_default()
    }

    fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        if let Some(target) = &self.target {
            vec![(target.position, target.base_radius)]
        } else {
            vec![]
        }
    }

    fn get_metrics(&self) -> TaskMetrics {
        let tracking_accuracy = if self.total_active_time > 0.0 {
            (self.time_on_target / self.total_active_time) * 100.0
        } else {
            0.0
        };

        let avg_offset = if self.sample_count > 0 {
            self.accumulated_offset / self.sample_count as f32
        } else {
            0.0
        };

        let score = (tracking_accuracy * 100.0) as u64;

        TaskMetrics {
            task_name: self.name().to_string(),
            score,
            accuracy: tracking_accuracy,
            avg_reaction_time_ms: 0.0,
            shots_fired: self.sample_count,
            hits: (tracking_accuracy * 10.0) as u32,
            misses: ((100.0 - tracking_accuracy) * 10.0) as u32,
            duration_secs: self.total_active_time,
            custom_metric_label: "Average Offset from Center",
            custom_metric_value: format!("{:.3}m ({:.1}% on target)", avg_offset, tracking_accuracy),
        }
    }
}

