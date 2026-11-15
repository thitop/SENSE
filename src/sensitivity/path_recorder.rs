use glam::Vec2;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct MouseSample {
    pub time: Instant,
    pub delta: Vec2,
    pub accumulated_pos: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShotTrajectoryPattern {
    Direct,     // Clean direct path to target
    Overshoot,  // Passed the target and had to jerk back (sens too high)
    Undershoot, // Stalled/ran out of movement before reaching target and had to micro-step (sens too slow)
}

#[derive(Clone, Debug)]
pub struct ShotMovementPath {
    pub samples: Vec<MouseSample>,
    pub start_time: Instant,
    pub end_time: Instant,
    pub total_mouse_distance: f32,
    pub peak_velocity: f32,
    pub direction_changes: u32,
    pub pattern: ShotTrajectoryPattern,
}

pub struct MousePathRecorder {
    current_samples: Vec<MouseSample>,
    accumulated_pos: Vec2,
    recording: bool,
    start_time: Instant,
}

impl MousePathRecorder {
    pub fn new() -> Self {
        Self {
            current_samples: Vec::with_capacity(1024),
            accumulated_pos: Vec2::ZERO,
            recording: false,
            start_time: Instant::now(),
        }
    }

    pub fn start_shot(&mut self) {
        self.current_samples.clear();
        self.accumulated_pos = Vec2::ZERO;
        self.recording = true;
        self.start_time = Instant::now();
    }

    pub fn record_sample(&mut self, dx: f32, dy: f32) {
        if !self.recording {
            return;
        }

        let delta = Vec2::new(dx, dy);
        self.accumulated_pos += delta;

        self.current_samples.push(MouseSample {
            time: Instant::now(),
            delta,
            accumulated_pos: self.accumulated_pos,
        });
    }

    /// Complete current shot trajectory and analyze whether it was Overshoot, Undershoot, or Direct
    pub fn end_shot(&mut self) -> Option<ShotMovementPath> {
        if !self.recording || self.current_samples.len() < 3 {
            self.recording = false;
            return None;
        }

        self.recording = false;
        let end_time = Instant::now();

        let mut total_dist = 0.0;
        let mut peak_vel: f32 = 0.0;
        let mut dir_changes = 0;

        // Detect direction reversals (sign changes on primary movement axis)
        let primary_horizontal = self.accumulated_pos.x.abs() >= self.accumulated_pos.y.abs();

        let mut prev_sign = 0.0;
        for window in self.current_samples.windows(2) {
            let p1 = window[0].accumulated_pos;
            let p2 = window[1].accumulated_pos;
            let step = (p2 - p1).length();
            total_dist += step;

            let dt = window[1].time.duration_since(window[0].time).as_secs_f32();
            if dt > 0.0001 {
                let vel = step / dt;
                if vel > peak_vel {
                    peak_vel = vel;
                }
            }

            let primary_delta = if primary_horizontal {
                window[1].delta.x
            } else {
                window[1].delta.y
            };

            if primary_delta.abs() > 1.5 {
                let sign = primary_delta.signum();
                if prev_sign != 0.0 && sign != prev_sign {
                    dir_changes += 1;
                }
                prev_sign = sign;
            }
        }

        // Net displacement vs Total path travel
        let net_displacement = self.accumulated_pos.length();
        let path_efficiency = if total_dist > 0.0 {
            net_displacement / total_dist
        } else {
            1.0
        };

        // Pattern classification:
        // Overshoot: Trajectory flew past and turned back (dir_changes >= 1, path_efficiency < 0.75, overshoot on primary axis)
        // Undershoot: Stalled in the middle, multiple micro-steps in the same direction or took too long to accelerate
        let pattern = if dir_changes >= 1 && path_efficiency < 0.80 {
            ShotTrajectoryPattern::Overshoot
        } else if path_efficiency < 0.65 {
            ShotTrajectoryPattern::Undershoot
        } else {
            ShotTrajectoryPattern::Direct
        };

        Some(ShotMovementPath {
            samples: self.current_samples.clone(),
            start_time: self.start_time,
            end_time,
            total_mouse_distance: total_dist,
            peak_velocity: peak_vel,
            direction_changes: dir_changes,
            pattern,
        })
    }
}
