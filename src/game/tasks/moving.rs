use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::time::Instant;

use super::super::hit_detection::{intersect_ray_sphere, Ray};
use super::super::scoring::ScoreTracker;
use super::super::target::Target;
use super::{AimTarget, Task, TaskMetrics};
use crate::habits::record::AimKind;

/// Room the target moves in (same bounds as tracking), 9 m in front of the eye.
const X_LIMIT: f32 = 4.5;
const Y_RANGE: (f32, f32) = (0.6, 2.6);
const DEPTH: f32 = -9.0;
/// Spawns keep a margin from the walls so the target doesn't bounce straight away.
const SPAWN_X: f32 = 4.0;
const SPAWN_Y: (f32, f32) = (0.9, 2.3);

/// One moving target at a time. Shooting it pops it and a new moving target appears elsewhere;
/// the run ends after `max_kills` kills. Targets don't time out.
pub struct MovingTargetTask {
    targets: Vec<Target>,
    next_id: u64,
    score_tracker: ScoreTracker,
    kills: usize,
    max_kills: usize,
    target_radius: f32,
    /// m/s along the current direction.
    speed: f32,
    direction: Vec3,
    turn_timer: f32,
    /// Where the last target was shot; the next one spawns away from it.
    last_kill: Option<Vec3>,
    start_time: Instant,
    seed: Option<u64>,
    rng: StdRng,
    is_finished: bool,
}

impl MovingTargetTask {
    /// Metres between a shot target and the next one, so each kill needs a new flick.
    pub const MIN_RESPAWN_DISTANCE: f32 = 1.5;

    pub fn new(max_kills: usize, target_radius: f32, speed: f32) -> Self {
        Self {
            targets: Vec::with_capacity(2),
            next_id: 1,
            score_tracker: ScoreTracker::new(),
            kills: 0,
            max_kills,
            target_radius,
            speed,
            direction: Vec3::X,
            turn_timer: 0.0,
            last_kill: None,
            start_time: Instant::now(),
            seed: None,
            rng: StdRng::from_entropy(),
            is_finished: false,
        }
    }

    /// Same spawns and paths every start (given the same shots).
    pub fn seeded(max_kills: usize, target_radius: f32, speed: f32, seed: u64) -> Self {
        let mut t = Self::new(max_kills, target_radius, speed);
        t.seed = Some(seed);
        t.rng = StdRng::seed_from_u64(seed);
        t
    }

    /// Position and radius of the target that can be shot, if any.
    pub fn live_target(&self) -> Option<(Vec3, f32)> {
        self.targets.iter().find(|t| t.is_alive()).map(|t| (t.position, t.base_radius))
    }

    fn pick_direction(&mut self) {
        // Mostly sideways, like tracking: the room is wider than it is tall.
        let angle: f32 = self.rng.gen_range(0.0..std::f32::consts::TAU);
        self.direction = Vec3::new(angle.cos(), angle.sin() * 0.5, 0.0).normalize();
        self.turn_timer = self.rng.gen_range(0.8..=1.8);
    }

    fn spawn(&mut self) {
        let mut position = Vec3::ZERO;
        for _ in 0..32 {
            position = Vec3::new(
                self.rng.gen_range(-SPAWN_X..=SPAWN_X),
                self.rng.gen_range(SPAWN_Y.0..=SPAWN_Y.1),
                DEPTH,
            );
            if self.last_kill.map_or(true, |k| k.distance(position) >= Self::MIN_RESPAWN_DISTANCE) {
                break;
            }
        }
        if let Some(k) = self.last_kill {
            if k.distance(position) < Self::MIN_RESPAWN_DISTANCE {
                // 32 unlucky draws: use the far side wall, at least 4 m from the kill.
                position.x = if k.x >= 0.0 { -SPAWN_X } else { SPAWN_X };
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.targets.push(Target::new(id, position, self.target_radius, None));
        self.pick_direction();
    }
}

impl Task for MovingTargetTask {
    fn name(&self) -> &'static str {
        "Moving Targets"
    }

    fn description(&self) -> &'static str {
        "A target moves around the room. Shoot it and a new moving target appears somewhere else."
    }

    fn start(&mut self) {
        self.targets.clear();
        self.next_id = 1;
        self.score_tracker.reset();
        self.kills = 0;
        self.last_kill = None;
        self.is_finished = false;
        self.start_time = Instant::now();
        if let Some(seed) = self.seed {
            self.rng = StdRng::seed_from_u64(seed);
        }
        self.spawn();
    }

    fn update(&mut self, dt: f32, _crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }
        // The hit pop runs on its own clock; only the live target moves.
        for target in &mut self.targets {
            target.update(dt);
        }
        self.targets.retain(|t| t.current_render_radius().is_some());

        if self.kills >= self.max_kills {
            self.is_finished = true;
            return;
        }
        if self.live_target().is_none() {
            self.spawn();
        }

        self.turn_timer -= dt;
        if self.turn_timer <= 0.0 {
            self.pick_direction();
        }
        let velocity = self.direction * self.speed;
        if let Some(target) = self.targets.iter_mut().find(|t| t.is_alive()) {
            target.position += velocity * dt;
            if target.position.x.abs() > X_LIMIT {
                self.direction.x = -self.direction.x;
                target.position.x = target.position.x.clamp(-X_LIMIT, X_LIMIT);
            }
            if target.position.y < Y_RANGE.0 || target.position.y > Y_RANGE.1 {
                self.direction.y = -self.direction.y;
                target.position.y = target.position.y.clamp(Y_RANGE.0, Y_RANGE.1);
            }
        }
    }

    fn on_click(&mut self, ray: Ray) -> bool {
        if self.is_finished {
            return false;
        }
        let now = Instant::now();
        let hit = self.targets.iter_mut().filter(|t| t.is_alive()).find_map(|t| {
            intersect_ray_sphere(ray, t.position, t.base_radius).map(|(_, _, offset)| (t, offset))
        });
        match hit {
            Some((target, offset)) => {
                self.score_tracker.record_hit(
                    target.id,
                    now.duration_since(target.spawn_time),
                    offset,
                    target.base_radius,
                );
                target.hit();
                self.last_kill = Some(target.position);
                self.kills += 1;
                true
            }
            None => {
                self.score_tracker.record_miss();
                false
            }
        }
    }

    fn aim_target(&self) -> Option<AimTarget> {
        let t = self.targets.iter().find(|t| t.is_alive())?;
        Some(AimTarget { id: t.id, center: t.position, radius: t.base_radius, kind: AimKind::Moving, band: None })
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
        let avg_ms = self.score_tracker.average_reaction_time().as_millis() as f32;
        TaskMetrics {
            task_name: self.name().to_string(),
            score: self.score_tracker.total_score,
            accuracy: self.score_tracker.accuracy_percentage(),
            avg_reaction_time_ms: avg_ms,
            shots_fired: self.score_tracker.total_shots,
            hits: self.score_tracker.hits,
            misses: self.score_tracker.misses,
            duration_secs: self.start_time.elapsed().as_secs_f32(),
            custom_metric_label: "Targets Down",
            custom_metric_value: format!("{}/{} ({:.0} ms each)", self.kills, self.max_kills, avg_ms),
        }
    }
}
