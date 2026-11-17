//! Tracking, cut into half-second windows that each say whether the aim ran ahead of the
//! target (too fast), fell behind it (too slow), or stayed with it.
//!
//! Only the signed error along the target's direction of motion votes. A sensitivity that is
//! too fast for the player's hand turns the same motion into more rotation, so the aim runs
//! ahead; too slow, it falls behind. The vote is symmetric around zero error, so it doesn't
//! move the balance point the search looks for. Jitter across the motion is measured and kept
//! as a feature but doesn't vote: it would only ever vote "too fast" and push every result
//! slower.
//!
//! Every player is behind for a moment after the target turns (reaction time). That stretch
//! is left out instead of subtracting a fixed lag, which would need a per-player value.

use glam::Vec2;

use super::combiner::{DirectionalSignal, Signal};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingWindow {
    /// Mean aim position along the target's motion, degrees: + ahead, − behind.
    pub mean_along_deg: f32,
    /// The same as time: how far behind the target the aim ran (negative = ahead).
    pub lag_ms: f32,
    /// RMS of the error across the motion after removing its window mean, degrees.
    pub cross_rms_deg: f32,
    /// Mean target speed over the counted samples, °/s.
    pub mean_speed_deg_s: f32,
    /// Share of the window that counted (the rest was right after a turn or a still target).
    pub coverage: f32,
    signal: Option<Signal>,
}

impl DirectionalSignal for TrackingWindow {
    fn signal(&self) -> Option<Signal> {
        self.signal
    }
}

pub struct TrackingAnalyzer {
    radius_deg: f32,
    elapsed: f32,
    counted_s: f32,
    along: Vec<f32>,
    cross: Vec<f32>,
    speed: Vec<f32>,
    windows: Vec<TrackingWindow>,
}

impl TrackingAnalyzer {
    pub const WINDOW_S: f32 = 0.5;
    /// Left out after each turn: ~150 ms to react plus ~200 ms to catch up. Shorter leaves the
    /// catch-up in, and since everyone is behind while catching up it leans every result faster.
    pub const SETTLE_S: f32 = 0.35;
    /// A window with less counted time than this share says nothing.
    const MIN_COVERAGE: f32 = 0.4;
    /// Within this share of the target's angular radius of zero along-error is "with it".
    const DEADBAND_SHARE: f32 = 0.15;
    /// Slower targets than this have no meaningful direction to be ahead or behind of.
    const MIN_SPEED_DEG_S: f32 = 3.0;

    /// `radius_deg`: the target's angular radius, which scales the deadband.
    pub fn new(radius_deg: f32) -> Self {
        Self {
            radius_deg,
            elapsed: 0.0,
            counted_s: 0.0,
            along: Vec::new(),
            cross: Vec::new(),
            speed: Vec::new(),
            windows: Vec::new(),
        }
    }

    /// One tick: `offset_deg` = mouse movement from the aim to the target centre (degrees,
    /// y down), `target_vel_deg_s` = the target's own motion in the same space,
    /// `since_turn_s` = time since its direction last changed.
    pub fn push(&mut self, dt: f32, offset_deg: Vec2, target_vel_deg_s: Vec2, since_turn_s: f32) {
        self.elapsed += dt;
        let speed = target_vel_deg_s.length();
        if since_turn_s >= Self::SETTLE_S && speed >= Self::MIN_SPEED_DEG_S {
            let along_dir = target_vel_deg_s / speed;
            let across_dir = Vec2::new(-along_dir.y, along_dir.x);
            let aim_rel = -offset_deg;
            self.along.push(aim_rel.dot(along_dir));
            self.cross.push(aim_rel.dot(across_dir));
            self.speed.push(speed);
            self.counted_s += dt;
        }
        // Within half a tick: summing f32 ticks lands just short of the window length.
        if self.elapsed + dt * 0.5 >= Self::WINDOW_S {
            self.finish_window();
        }
    }

    /// Windows finished since the last call.
    pub fn take_windows(&mut self) -> Vec<TrackingWindow> {
        std::mem::take(&mut self.windows)
    }

    fn finish_window(&mut self) {
        let n = self.along.len();
        let coverage = (self.counted_s / self.elapsed.max(1e-6)).min(1.0);
        let mean = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let mean_along = mean(&self.along);
        let mean_cross = mean(&self.cross);
        let mean_speed = mean(&self.speed);
        let cross_rms = if n == 0 {
            0.0
        } else {
            (self.cross.iter().map(|c| (c - mean_cross).powi(2)).sum::<f32>() / n as f32).sqrt()
        };
        let signal = if n == 0 || coverage < Self::MIN_COVERAGE {
            None
        } else {
            let deadband = Self::DEADBAND_SHARE * self.radius_deg;
            Some(if mean_along > deadband {
                Signal::TooFast
            } else if mean_along < -deadband {
                Signal::TooSlow
            } else {
                Signal::Direct
            })
        };
        self.windows.push(TrackingWindow {
            mean_along_deg: mean_along,
            lag_ms: if mean_speed > 0.0 { -mean_along / mean_speed * 1000.0 } else { 0.0 },
            cross_rms_deg: cross_rms,
            mean_speed_deg_s: mean_speed,
            coverage,
            signal,
        });
        self.elapsed = 0.0;
        self.counted_s = 0.0;
        self.along.clear();
        self.cross.clear();
        self.speed.clear();
    }
}
