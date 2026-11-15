use glam::Vec3;
use rand::Rng;
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::scoring::ScoreTracker;
use super::super::target::Target;
use super::{Task, TaskMetrics};

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
    is_finished: bool,
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
            is_finished: false,
        }
    }

    fn spawn_single_target(&mut self) {
        if self.target_count >= self.max_targets {
            self.is_finished = true;
            return;
        }

        let mut rng = rand::thread_rng();
        // Wider horizontal and vertical distribution for flicking
        let x = rng.gen_range(-5.0..=5.0);
        let y = rng.gen_range(0.6..=2.6);
        let z = rng.gen_range(-11.0..=-8.0);

        let id = self.next_id;
        self.next_id += 1;
        self.target_count += 1;
        self.last_spawn_time = Instant::now();

        self.targets.push(Target::new(
            id,
            Vec3::new(x, y, z),
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
        self.is_finished = false;
        self.start_time = Instant::now();
        self.spawn_single_target();
    }

    fn update(&mut self, dt: f32, _crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }

        for target in &mut self.targets {
            target.update(dt);
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

        if !hit_any {
            self.score_tracker.record_miss();
        }

        hit_any
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

