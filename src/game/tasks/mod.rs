pub mod flick;
pub mod micro_adjust;
pub mod moving;
pub mod speed;
pub mod switching;
pub mod tracking;

use crate::sensitivity::paired::PairResult;
use glam::Vec3;
use std::time::Instant;

use super::hit_detection::Ray;
use crate::sensitivity::tracking_signal::TrackingWindow;
use flick::{FlickSpawn, FlickTask};
use micro_adjust::{MicroAdjustTask, MicroSpawn};
use moving::MovingTargetTask;
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
    /// Targets that timed out without being shot since `start`.
    fn expired_targets(&self) -> u32 {
        0
    }
    fn get_render_targets(&self) -> Vec<(Vec3, f32)>;
    fn get_metrics(&self) -> TaskMetrics;

    // What calibration needs to judge a shot. Tasks that aren't calibration tests keep the
    // defaults.

    /// Band and angle of the live flick target.
    fn flick_spawn(&self) -> Option<FlickSpawn> {
        None
    }
    /// Offset of the live micro-adjustment target.
    fn micro_spawn(&self) -> Option<MicroSpawn> {
        None
    }
    /// Tracking windows finished since the last call.
    fn take_tracking_windows(&mut self) -> Vec<TrackingWindow> {
        Vec::new()
    }
    /// Multiplier on the tested cm/360 for what is being played right now: a paired test
    /// alternates a faster and a slower side. Everything else plays at 1.0.
    fn sens_factor(&self) -> f32 {
        1.0
    }
    /// Pairs finished since the last call.
    fn take_pair_results(&mut self) -> Vec<PairResult> {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskType {
    Flick,
    Tracking,
    Switching,
    MicroAdjust,
    Speed,
    /// One moving target at a time; a new one appears when it is shot.
    MovingTargets,
    /// A calibration segment; the task itself comes from `CalibrationRun::make_task`.
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
    fn expired_targets(&self) -> u32 {
        self.0.expired_targets()
    }
    fn get_render_targets(&self) -> Vec<(Vec3, f32)> {
        self.0.get_render_targets()
    }
    fn get_metrics(&self) -> TaskMetrics {
        let mut m = self.0.get_metrics();
        m.task_name = "Calibration".into();
        m
    }
    fn flick_spawn(&self) -> Option<FlickSpawn> {
        self.0.flick_spawn()
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
            TaskType::MovingTargets => Box::new(MovingTargetTask::new(30, 0.35, 3.0)),
            TaskType::Calibration => Box::new(CalibrationTask(FlickTask::new(usize::MAX, 0.35))),
        };
        self.state = TaskState::NotStarted;
    }

    /// Run a task built elsewhere (a calibration segment) as `task_type`.
    pub fn set_custom_task(&mut self, task: Box<dyn Task>, task_type: TaskType) {
        self.current_type = task_type;
        self.current_task = task;
        self.state = TaskState::NotStarted;
    }

    pub fn current_flick_spawn(&self) -> Option<FlickSpawn> {
        self.current_task.flick_spawn()
    }

    pub fn current_micro_spawn(&self) -> Option<MicroSpawn> {
        self.current_task.micro_spawn()
    }

    pub fn take_tracking_windows(&mut self) -> Vec<TrackingWindow> {
        self.current_task.take_tracking_windows()
    }

    /// The running task's sensitivity multiplier; 1.0 while counting down or stopped.
    pub fn sens_factor(&self) -> f32 {
        if self.state == TaskState::Running {
            self.current_task.sens_factor()
        } else {
            1.0
        }
    }

    pub fn take_pair_results(&mut self) -> Vec<PairResult> {
        self.current_task.take_pair_results()
    }

    pub fn start_task_with_countdown(&mut self) {
        self.state = TaskState::Countdown {
            remaining_secs: self.countdown_duration,
        };
    }

    /// Start the task with no countdown: targets appear on the next update.
    pub fn start_task_now(&mut self) {
        self.current_task.start();
        self.state = TaskState::Running;
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

    /// Drop a run that is counting down or running, keeping the same task type ready to start.
    /// Returns whether there was a run to drop.
    pub fn abandon(&mut self) -> bool {
        let in_progress = matches!(self.state, TaskState::Countdown { .. } | TaskState::Running);
        if in_progress {
            self.set_task(self.current_type);
        }
        in_progress
    }

    /// Targets of the current run that timed out without being shot.
    pub fn expired_targets(&self) -> u32 {
        self.current_task.expired_targets()
    }

    /// Metrics of the task in progress (the calibration task never finishes on its own).
    pub fn current_metrics(&self) -> TaskMetrics {
        self.current_task.get_metrics()
    }

    pub fn current_task_name(&self) -> &'static str {
        self.current_task.name()
    }

    pub fn current_task_description(&self) -> &'static str {
        self.current_task.description()
    }
}

