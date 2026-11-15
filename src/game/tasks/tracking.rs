use glam::Vec3;
use rand::Rng;
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::target::Target;
use super::{Task, TaskMetrics};

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
        }
    }

    fn pick_random_direction(&mut self) {
        let mut rng = rand::thread_rng();
        // Move in horizontal/vertical 2D plane (X-Y)
        let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
        self.direction = Vec3::new(angle.cos(), angle.sin() * 0.5, 0.0).normalize();
        self.direction_change_timer = rng.gen_range(0.8..=1.8);
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

        self.target = Some(Target::new(
            1,
            Vec3::new(0.0, 1.5, -9.0),
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
        if self.total_active_time >= self.task_duration_secs {
            self.is_finished = true;
            return;
        }

        // Update direction switch
        self.direction_change_timer -= dt;
        if self.direction_change_timer <= 0.0 {
            self.pick_random_direction();
        }

        if let Some(target) = &mut self.target {
            // Apply motion with sine wave oscillation
            let oscillation = (self.total_active_time * 2.5).sin() * 0.5;
            let current_vel = (self.direction + Vec3::new(0.0, oscillation, 0.0)).normalize() * self.target_speed;
            target.position += current_vel * dt;

            // Bounce within room bounds
            if target.position.x < -4.5 || target.position.x > 4.5 {
                self.direction.x = -self.direction.x;
                target.position.x = target.position.x.clamp(-4.5, 4.5);
            }
            if target.position.y < 0.6 || target.position.y > 2.6 {
                self.direction.y = -self.direction.y;
                target.position.y = target.position.y.clamp(0.6, 2.6);
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
