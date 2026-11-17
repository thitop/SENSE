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
    /// `None` when there was too little mouse movement to classify the trajectory.
    pub pattern: Option<ShotTrajectoryPattern>,
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
        if !self.recording {
            return None;
        }

        self.recording = false;
        Some(ShotMovementPath::from_samples(
            self.current_samples.clone(),
            self.start_time,
            Instant::now(),
        ))
    }
}

/// Fewer mouse samples than this can't show an overshoot or undershoot.
const MIN_CLASSIFIABLE_SAMPLES: usize = 3;
/// The main flick has ended once speed falls below this share of its peak.
const FLICK_END_SPEED_RATIO: f32 = 0.25;
/// Undershoot when at least this share of the net movement comes after the flick ended.
const UNDERSHOOT_CORRECTION_SHARE: f32 = 0.15;

/// Share of the net primary-axis movement made after the main flick slowed down.
/// A natural deceleration onto the target leaves only a small tail; stopping short
/// and then correcting forward leaves a large one.
fn correction_share(samples: &[MouseSample], start_time: Instant, primary_horizontal: bool) -> f32 {
    let primary = |v: Vec2| if primary_horizontal { v.x } else { v.y };
    let net = samples.last().map(|s| primary(s.accumulated_pos)).unwrap_or(0.0);
    if net.abs() < 1.0 {
        return 0.0;
    }

    let mut prev_time = start_time;
    let speeds: Vec<f32> = samples
        .iter()
        .map(|s| {
            let dt = s.time.duration_since(prev_time).as_secs_f32().max(0.0005);
            prev_time = s.time;
            s.delta.length() / dt
        })
        .collect();

    let Some(peak_idx) = speeds
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
    else {
        return 0.0;
    };
    let slow = speeds[peak_idx] * FLICK_END_SPEED_RATIO;
    let Some(flick_end) = (peak_idx + 1..samples.len()).find(|&i| speeds[i] < slow) else {
        return 0.0;
    };

    let after: f32 = samples[flick_end..].iter().map(|s| primary(s.delta)).sum();
    (after / net).max(0.0)
}

impl ShotMovementPath {
    /// Analyze one shot's mouse samples (oldest first) and classify the trajectory.
    pub fn from_samples(samples: Vec<MouseSample>, start_time: Instant, end_time: Instant) -> Self {
        let accumulated_pos = samples.last().map(|s| s.accumulated_pos).unwrap_or(Vec2::ZERO);

        let mut total_dist = 0.0;
        let mut peak_vel: f32 = 0.0;
        let mut dir_changes = 0;

        // Detect direction reversals (sign changes on primary movement axis)
        let primary_horizontal = accumulated_pos.x.abs() >= accumulated_pos.y.abs();

        let mut prev_sign = 0.0;
        for window in samples.windows(2) {
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
        let net_displacement = accumulated_pos.length();
        let path_efficiency = if total_dist > 0.0 {
            net_displacement / total_dist
        } else {
            1.0
        };

        // Pattern classification:
        // Overshoot: Trajectory flew past and turned back (dir_changes >= 1, path_efficiency < 0.80)
        // Undershoot: The main flick stalled short of the target and a later correction kept
        // going the same way (a straight path, so path efficiency alone can't see it).
        let pattern = if samples.len() < MIN_CLASSIFIABLE_SAMPLES {
            None
        } else if dir_changes >= 1 && path_efficiency < 0.80 {
            Some(ShotTrajectoryPattern::Overshoot)
        } else if path_efficiency < 0.65
            || correction_share(&samples, start_time, primary_horizontal) >= UNDERSHOOT_CORRECTION_SHARE
        {
            Some(ShotTrajectoryPattern::Undershoot)
        } else {
            Some(ShotTrajectoryPattern::Direct)
        };

        ShotMovementPath {
            samples,
            start_time,
            end_time,
            total_mouse_distance: total_dist,
            peak_velocity: peak_vel,
            direction_changes: dir_changes,
            pattern,
        }
    }
}
