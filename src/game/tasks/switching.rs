use glam::Vec3;
use rand::Rng;
use std::time::Instant;

use super::super::hit_detection::Ray;
use super::super::scoring::ScoreTracker;
use super::super::target::Target;
use super::{Task, TaskMetrics};

pub struct SwitchingTask {
    targets: Vec<Target>,
    active_target_id: Option<u64>,
    score_tracker: ScoreTracker,
    next_id: u64,
    switch_count: usize,
    max_switches: usize,
    simultaneous_targets: usize,
    target_radius: f32,
    start_time: Instant,
    last_switch_time: Instant,
    is_finished: bool,
}

impl SwitchingTask {
    pub fn new(max_switches: usize, simultaneous_targets: usize, target_radius: f32) -> Self {
        Self {
            targets: Vec::with_capacity(simultaneous_targets + 2),
            active_target_id: None,
            score_tracker: ScoreTracker::new(),
            next_id: 1,
            switch_count: 0,
            max_switches,
            simultaneous_targets,
            target_radius,
            start_time: Instant::now(),
            last_switch_time: Instant::now(),
            is_finished: false,
        }
    }

    fn spawn_new_target(&mut self) -> u64 {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(-4.5..=4.5);
        let y = rng.gen_range(0.8..=2.4);
        let z = rng.gen_range(-11.0..=-8.0);

        let id = self.next_id;
        self.next_id += 1;

        self.targets.push(Target::new(id, Vec3::new(x, y, z), self.target_radius, None));
        id
    }

    fn select_next_active(&mut self) {
        if self.switch_count >= self.max_switches {
            self.is_finished = true;
            return;
        }

        let alive_ids: Vec<u64> = self.targets.iter().filter(|t| t.is_alive()).map(|t| t.id).collect();
        if !alive_ids.is_empty() {
            let mut rng = rand::thread_rng();
            let idx = rng.gen_range(0..alive_ids.len());
            self.active_target_id = Some(alive_ids[idx]);
            self.last_switch_time = Instant::now();
        }
    }
}

impl Task for SwitchingTask {
    fn name(&self) -> &'static str {
        "Target Switching"
    }

    fn description(&self) -> &'static str {
        "Multiple targets are present. One is highlighted as the primary objective. Rapidly eliminate the active target and snap to the next."
    }

    fn start(&mut self) {
        self.targets.clear();
        self.score_tracker.reset();
        self.switch_count = 0;
        self.is_finished = false;
        self.start_time = Instant::now();

        for _ in 0..self.simultaneous_targets {
            self.spawn_new_target();
        }
        self.select_next_active();
    }

    fn update(&mut self, dt: f32, _crosshair_ray: Ray) {
        if self.is_finished {
            return;
        }

        for target in &mut self.targets {
            target.update(dt);
        }

        self.targets.retain(|t| t.current_render_radius().is_some());

        // Maintain constant number of targets on screen
        while self.targets.iter().filter(|t| t.is_alive()).count() < self.simultaneous_targets {
            self.spawn_new_target();
        }

        if self.active_target_id.is_none() || !self.targets.iter().any(|t| Some(t.id) == self.active_target_id && t.is_alive()) {
            self.select_next_active();
        }
    }

    fn on_click(&mut self, ray: Ray) -> bool {
        if self.is_finished {
            return false;
        }

        let now = Instant::now();
        let mut hit_active = false;

        for target in &mut self.targets {
            if !target.is_alive() {
                continue;
            }

            if let Some((_, _, offset)) =
                super::super::hit_detection::intersect_ray_sphere(ray, target.position, target.base_radius)
            {
                if Some(target.id) == self.active_target_id {
                    let reaction_time = now.duration_since(self.last_switch_time);
                    self.score_tracker.record_hit(
                        target.id,
                        reaction_time,
                        offset,
                        target.base_radius,
                    );
                    target.hit();
                    self.switch_count += 1;
                    hit_active = true;
                    self.select_next_active();
                } else {
                    // Shot the wrong target (penalty)
                    self.score_tracker.record_miss();
                }
                break;
            }
        }

        if !hit_active {
            self.score_tracker.record_miss();
        }

        hit_active
    }

    fn is_complete(&self) -> bool {
        self.is_finished
    }

    fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        self.targets
            .iter()
            .filter_map(|t| {
                t.current_render_radius().map(|r| {
                    // Active highlighted target is rendered slightly larger (1.15x) for clarity
                    let scale = if Some(t.id) == self.active_target_id { 1.15 } else { 0.85 };
                    (t.position, r * scale)
                })
            })
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
            custom_metric_label: "Switches Completed",
            custom_metric_value: format!("{}/{}", self.switch_count, self.max_switches),
        }
    }
}

