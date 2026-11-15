use glam::Vec3;
use rand::Rng;
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::scoring::ScoreTracker;
use super::super::target::Target;
use super::{Task, TaskMetrics};

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
    last_crosshair_dir: Vec3,
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
            last_crosshair_dir: Vec3::new(0.0, 0.0, -1.0),
        }
    }

    /// Spawns a small target very close to where the player is currently aiming (micro correction)
    fn spawn_micro_target(&mut self, crosshair_dir: Vec3) {
        if self.target_count >= self.max_targets {
            self.is_finished = true;
            return;
        }

        let mut rng = rand::thread_rng();
        // Generate small angular offset (between 1.5 and 5 degrees away from center)
        let angle_rad: f32 = rng.gen_range(1.5_f32..=5.0_f32).to_radians();
        let rot_angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);

        let distance = rng.gen_range(9.0..=12.0);
        let center_at_dist = Vec3::new(0.0, 1.5, 0.0) + crosshair_dir.normalize() * distance;

        // Orthogonal offset vector
        let offset_dist = distance * angle_rad.tan();
        let right = crosshair_dir.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(crosshair_dir).normalize_or_zero();

        let offset_vec = (right * rot_angle.cos() + up * rot_angle.sin()) * offset_dist;
        let mut final_pos = center_at_dist + offset_vec;

        // Clamp to room bounds
        final_pos.x = final_pos.x.clamp(-4.5, 4.5);
        final_pos.y = final_pos.y.clamp(0.6, 2.5);
        final_pos.z = final_pos.z.clamp(-12.0, -7.0);

        let id = self.next_id;
        self.next_id += 1;
        self.target_count += 1;
        self.last_spawn_time = Instant::now();

        self.targets.push(Target::new(id, final_pos, self.target_radius, None));
    }
}

impl Task for MicroAdjustTask {
    fn name(&self) -> &'static str {
        "Micro-Adjustment"
    }

    fn description(&self) -> &'static str {
        "Small head-sized targets spawn tightly clustered near your crosshair. Test fine fingertip/wrist precision."
    }

    fn start(&mut self) {
        self.targets.clear();
        self.score_tracker.reset();
        self.target_count = 0;
        self.is_finished = false;
        self.start_time = Instant::now();
        self.spawn_micro_target(Vec3::new(0.0, 0.0, -1.0));
    }

    fn update(&mut self, dt: f32, crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }

        self.last_crosshair_dir = crosshair_ray.direction;

        for target in &mut self.targets {
            target.update(dt);
        }

        self.targets.retain(|t| t.current_render_radius().is_some());

        if self.targets.is_empty() {
            if self.target_count < self.max_targets {
                self.spawn_micro_target(crosshair_ray.direction);
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
                break;
            }
        }

        if !hit {
            self.score_tracker.record_miss();
        }

        hit
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
            custom_metric_label: "Micro Accuracy",
            custom_metric_value: format!("{:.1}% ({} hits)", self.score_tracker.accuracy_percentage(), self.score_tracker.hits),
        }
    }
}

