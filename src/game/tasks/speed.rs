use glam::Vec3;
use rand::Rng;
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::scoring::ScoreTracker;
use super::super::target::Target;
use super::{AimTarget, Task, TaskMetrics};
use crate::habits::record::AimKind;

pub struct SpeedTask {
    targets: Vec<Target>,
    next_id: u64,
    score_tracker: ScoreTracker,
    target_count: usize,
    max_targets: usize,
    start_time: Instant,
    last_spawn_time: Instant,
    time_limit_per_target_secs: f32, // Decreases gradually to push speed limits
    is_finished: bool,
}

impl SpeedTask {
    pub fn new(max_targets: usize) -> Self {
        Self {
            targets: Vec::with_capacity(4),
            next_id: 1,
            score_tracker: ScoreTracker::new(),
            target_count: 0,
            max_targets,
            start_time: Instant::now(),
            last_spawn_time: Instant::now(),
            time_limit_per_target_secs: 1.2, // Fast pace
            is_finished: false,
        }
    }

    fn spawn_speed_target(&mut self) {
        if self.target_count >= self.max_targets {
            self.is_finished = true;
            return;
        }

        let mut rng = rand::thread_rng();
        let x = rng.gen_range(-4.0..=4.0);
        let y = rng.gen_range(0.8..=2.3);
        let z = rng.gen_range(-10.0..=-7.5);

        let id = self.next_id;
        self.next_id += 1;
        self.target_count += 1;
        self.last_spawn_time = Instant::now();

        // Target gets progressively faster (from 1.2s down to 0.55s)
        let speed_curve = 1.2 - (self.target_count as f32 / self.max_targets as f32) * 0.65;
        self.time_limit_per_target_secs = speed_curve.max(0.5);

        self.targets.push(Target::new(
            id,
            Vec3::new(x, y, z),
            0.32,
            Some(self.time_limit_per_target_secs),
        ));
    }
}

impl Task for SpeedTask {
    fn name(&self) -> &'static str {
        "Speed vs Precision"
    }

    fn description(&self) -> &'static str {
        "Targets shrink and vanish quickly with escalating speed. Tests reaction speed under high pressure."
    }

    fn start(&mut self) {
        self.targets.clear();
        self.score_tracker.reset();
        self.target_count = 0;
        self.is_finished = false;
        self.start_time = Instant::now();
        self.spawn_speed_target();
    }

    fn update(&mut self, dt: f32, _crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }

        for target in &mut self.targets {
            target.update(dt);
        }

        let had_alive = self.targets.iter().any(|t| t.is_alive());
        self.targets.retain(|t| t.current_render_radius().is_some());

        if !had_alive || self.targets.is_empty() {
            if self.target_count < self.max_targets {
                self.spawn_speed_target();
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

    fn aim_target(&self) -> Option<AimTarget> {
        let t = self.targets.iter().find(|t| t.is_alive())?;
        Some(AimTarget { id: t.id, center: t.position, radius: t.base_radius, kind: AimKind::Flick, band: None })
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
            custom_metric_label: "Average Speed",
            custom_metric_value: format!("{:.0} ms / target", self.score_tracker.average_reaction_time().as_millis()),
        }
    }
}

