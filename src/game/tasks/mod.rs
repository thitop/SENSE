pub mod flick;
pub mod micro_adjust;
pub mod speed;
pub mod switching;
pub mod tracking;

use glam::Vec3;
use std::time::Instant;

use super::hit_detection::Ray;
use flick::FlickTask;
use micro_adjust::MicroAdjustTask;
use speed::SpeedTask;
use switching::SwitchingTask;
use tracking::TrackingTask;

#[derive(Clone, Debug)]
pub struct TaskMetrics {
    pub task_name: String,
    pub score: u64,
    pub accuracy: f32,
    pub avg_reaction_time_ms: f32,
    pub shots_fired: u32,
    pub hits: u32,
    pub misses: u32,
    pub duration_secs: f32,
    pub custom_metric_label: &'static str,
    pub custom_metric_value: String,
}

pub trait Task: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn start(&mut self);
    fn update(&mut self, dt: f32, crosshair_ray: Ray);
    fn on_click(&mut self, ray: Ray) -> bool;
    fn is_complete(&self) -> bool;
    fn get_render_targets(&self) -> Vec<(Vec3, f32)>;
    fn get_metrics(&self) -> TaskMetrics;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskType {
    Flick,
    Tracking,
    Switching,
    MicroAdjust,
    Speed,
    /// Endless flick targets; the game loop ends it when PSA calibration completes.
    Calibration,
}

/// Flick targets with no target limit, shown as "Calibration".
struct CalibrationTask(FlickTask);

impl Task for CalibrationTask {
    fn name(&self) -> &'static str {
        "Calibration"
    }
    fn description(&self) -> &'static str {
        "Shoot targets while the sensitivity is narrowed down round by round."
    }
    fn start(&mut self) {
        self.0.start()
    }
    fn update(&mut self, dt: f32, crosshair_ray: Ray) {
        self.0.update(dt, crosshair_ray)
    }
    fn on_click(&mut self, ray: Ray) -> bool {
        self.0.on_click(ray)
    }
    fn is_complete(&self) -> bool {
        false
    }
    fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        self.0.get_render_targets()
    }
    fn get_metrics(&self) -> TaskMetrics {
        let mut m = self.0.get_metrics();
        m.task_name = "Calibration".into();
        m
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskState {
    NotStarted,
    Countdown { remaining_secs: f32 },
    Running,
    Finished,
}

pub struct TaskManager {
    current_task: Box<dyn Task>,
    pub current_type: TaskType,
    pub state: TaskState,
    pub countdown_duration: f32,
    pub last_completed_metrics: Option<TaskMetrics>,
}

impl TaskManager {
    pub fn new() -> Self {
        let default_task = Box::new(FlickTask::new(30, 0.35));
        Self {
            current_task: default_task,
            current_type: TaskType::Flick,
            state: TaskState::NotStarted,
            countdown_duration: 3.0,
            last_completed_metrics: None,
        }
    }

    pub fn set_task(&mut self, task_type: TaskType) {
        self.current_type = task_type;
        self.current_task = match task_type {
            TaskType::Flick => Box::new(FlickTask::new(30, 0.35)),
            TaskType::Tracking => Box::new(TrackingTask::new(30.0, 0.35, 3.8)),
            TaskType::Switching => Box::new(SwitchingTask::new(25, 4, 0.32)),
            TaskType::MicroAdjust => Box::new(MicroAdjustTask::new(30, 0.16)),
            TaskType::Speed => Box::new(SpeedTask::new(30)),
            TaskType::Calibration => Box::new(CalibrationTask(FlickTask::new(usize::MAX, 0.35))),
        };
        self.state = TaskState::NotStarted;
    }

    pub fn start_task_with_countdown(&mut self) {
        self.state = TaskState::Countdown {
            remaining_secs: self.countdown_duration,
        };
    }

    pub fn update(&mut self, dt: f32, crosshair_ray: Ray) {
        match self.state {
            TaskState::NotStarted => {}
            TaskState::Countdown { ref mut remaining_secs } => {
                *remaining_secs -= dt;
                if *remaining_secs <= 0.0 {
                    self.current_task.start();
                    self.state = TaskState::Running;
                }
            }
            TaskState::Running => {
                self.current_task.update(dt, crosshair_ray);
                if self.current_task.is_complete() {
                    self.last_completed_metrics = Some(self.current_task.get_metrics());
                    self.state = TaskState::Finished;
                }
            }
            TaskState::Finished => {}
        }
    }

    pub fn on_click(&mut self, ray: Ray) -> bool {
        if self.state == TaskState::Running {
            self.current_task.on_click(ray)
        } else {
            false
        }
    }

    pub fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        if self.state == TaskState::Running {
            self.current_task.get_render_targets()
        } else {
            vec![]
        }
    }

    pub fn current_task_name(&self) -> &'static str {
        self.current_task.name()
    }

    pub fn current_task_description(&self) -> &'static str {
        self.current_task.description()
    }
}

