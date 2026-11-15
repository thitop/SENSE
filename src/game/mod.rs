pub mod hit_detection;
pub mod scoring;
pub mod target;
pub mod tasks;

use glam::Vec3;
use rand::Rng;
use std::time::Instant;

use hit_detection::{intersect_ray_sphere, Ray, RayHit};
use scoring::ScoreTracker;
use target::{Target, TargetState};

pub use tasks::{Task, TaskManager, TaskMetrics, TaskState, TaskType};

pub struct TargetManager {
    targets: Vec<Target>,
    next_id: u64,
    pub score_tracker: ScoreTracker,
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl TargetManager {
    pub fn new() -> Self {
        Self {
            targets: Vec::with_capacity(32),
            next_id: 1,
            score_tracker: ScoreTracker::new(),
            min_x: -4.0,
            max_x: 4.0,
            min_y: 0.8,
            max_y: 2.4,
            min_z: -12.0,
            max_z: -7.0,
        }
    }

    pub fn spawn_random_target(&mut self, radius: f32, lifetime_secs: Option<f32>) -> u64 {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(self.min_x..=self.max_x);
        let y = rng.gen_range(self.min_y..=self.max_y);
        let z = rng.gen_range(self.min_z..=self.max_z);

        let id = self.next_id;
        self.next_id += 1;

        let target = Target::new(id, Vec3::new(x, y, z), radius, lifetime_secs);
        self.targets.push(target);
        id
    }

    pub fn active_target_count(&self) -> usize {
        self.targets.iter().filter(|t| t.is_alive()).count()
    }

    pub fn update(&mut self, dt: f32) {
        for target in &mut self.targets {
            target.update(dt);
        }

        self.targets.retain(|t| t.state != TargetState::Despawned);
    }

    pub fn raycast(&self, ray: Ray) -> Option<RayHit> {
        let mut closest_hit: Option<RayHit> = None;

        for target in &self.targets {
            if !target.is_alive() {
                continue;
            }

            if let Some((dist, hit_pt, offset)) =
                intersect_ray_sphere(ray, target.position, target.base_radius)
            {
                if closest_hit.as_ref().map_or(true, |c| dist < c.distance) {
                    closest_hit = Some(RayHit {
                        target_id: target.id,
                        distance: dist,
                        hit_point: hit_pt,
                        offset_from_center: offset,
                    });
                }
            }
        }

        closest_hit
    }

    pub fn process_click(&mut self, ray: Ray) -> Option<RayHit> {
        if let Some(hit) = self.raycast(ray) {
            let now = Instant::now();
            let mut reaction_time = std::time::Duration::ZERO;
            let mut radius = 0.3;

            for target in &mut self.targets {
                if target.id == hit.target_id && target.is_alive() {
                    reaction_time = now.duration_since(target.spawn_time);
                    radius = target.base_radius;
                    target.hit();
                    break;
                }
            }

            self.score_tracker.record_hit(
                hit.target_id,
                reaction_time,
                hit.offset_from_center,
                radius,
            );

            Some(hit)
        } else {
            self.score_tracker.record_miss();
            None
        }
    }

    pub fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        self.targets
            .iter()
            .filter_map(|t| t.current_render_radius().map(|r| (t.position, r)))
            .collect()
    }
}
