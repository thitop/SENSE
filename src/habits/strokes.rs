//! Stroke features of one aimed movement, in degrees: where the first stroke landed, how many
//! strokes it took, whether the aim went past the target centre and came back.
//!
//! Stroke splitting and the thresholds are shared with `classify_micro`, so the two always agree.

use glam::Vec2;

use crate::game::tasks::flick::FlickBand;
use crate::sensitivity::micro_signal::{
    cross_tolerance_deg, split_strokes, MIN_MOVEMENT_DEG, MIN_STROKE_DEG,
};

/// Degrees the view moved in one input sample (y down), `t_ms` after the target appeared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DegSample {
    pub t_ms: f32,
    pub d: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeFeatures {
    /// Net progress along the target direction at the end of the first stroke / distance.
    pub first_landing: f32,
    /// Strokes of at least 0.05 degrees toward or away from the target.
    pub strokes: u8,
    /// Went past the centre by more than the tolerance and came back.
    pub crossed_center: bool,
    /// Furthest past the centre, in degrees (0 if it never got there).
    pub overshoot_deg: f32,
}

/// `samples` oldest first; `target_offset_deg` = the movement in degrees that would put the aim
/// on the target centre. `None` when the target is on the aim or the player barely moved.
pub fn stroke_features(samples: &[DegSample], target_offset_deg: Vec2) -> Option<StrokeFeatures> {
    let d = target_offset_deg.length();
    let path_deg: f32 = samples.iter().map(|s| s.d.length()).sum();
    // Written so that NaN fails the check.
    if !(d >= 1e-4 && d.is_finite() && path_deg >= MIN_MOVEMENT_DEG && path_deg.is_finite()) {
        return None;
    }
    let u = target_offset_deg / d;
    let tol = cross_tolerance_deg(d);
    let strokes = split_strokes(samples, u);
    let first = *strokes.first()?;
    let count = strokes.iter().filter(|p| p.abs() >= MIN_STROKE_DEG).count();

    let (mut p, mut max_p) = (0.0f32, f32::MIN);
    for s in samples {
        p += s.d.dot(u);
        max_p = max_p.max(p);
    }
    Some(StrokeFeatures {
        first_landing: first / d,
        strokes: count.min(u8::MAX as usize) as u8,
        crossed_center: max_p > d + tol && max_p - p > tol,
        overshoot_deg: (max_p - d).max(0.0),
    })
}

/// The flick band an angle belongs to, `None` between bands.
pub fn band_for_angle(angle_deg: f32) -> Option<FlickBand> {
    FlickBand::ALL.into_iter().find(|b| {
        let (lo, hi) = b.range_deg();
        (lo..=hi).contains(&angle_deg)
    })
}
