//! Micro-adjustment shots, judged in degrees and against where the target actually is.
//!
//! The flick classifier (`ShotMovementPath::from_samples`) works in mouse counts and doesn't know
//! the target. On 1.5–5° corrections that shows: at 400 DPI and 30 cm/360° a small return after
//! overshooting is ~1 count per sample, under its 1.5-count reversal threshold, so the same hand
//! movement is "direct" at 400 DPI and "overshoot" at 3200; and one correction after the first
//! stroke is "undershoot" or "direct" depending on its exact share. Here every threshold is in
//! degrees, overshoot means going past the target centre and coming back, and only repeated
//! stopping short (three strokes toward the target) counts as too slow.

use glam::Vec2;
use std::time::Instant;

use super::combiner::{DirectionalSignal, Signal};
use super::converter::SensitivityConverter;
use super::path_recorder::MouseSample;
use crate::habits::strokes::DegSample;

/// Degrees of view rotation per mouse count.
pub fn deg_per_count(dpi: f64, cm_360: f64) -> f32 {
    SensitivityConverter::cm_360_to_rad_per_count(dpi, cm_360).to_degrees()
}

/// Less total movement than this says nothing about the sensitivity.
pub(crate) const MIN_MOVEMENT_DEG: f32 = 0.1;
/// Going past the target centre by more than this (and back) is an overshoot; within it is
/// ordinary stopping noise. The larger of an absolute floor and a share of the distance.
const CROSS_TOLERANCE_MIN_DEG: f32 = 0.15;
const CROSS_TOLERANCE_SHARE: f32 = 0.10;
/// Speed is the path covered in this window, so single raw-input samples don't split a stroke.
const SPEED_WINDOW_MS: f32 = 10.0;
/// No samples for this long ends a stroke even without a visible slow-down.
const GAP_MS: f32 = 25.0;
/// A stroke starts above this share of the shot's peak speed and ends below the lower one.
const STROKE_ON_SHARE: f32 = 0.15;
const STROKE_OFF_SHARE: f32 = 0.08;
/// Ignore strokes that move less than this toward or away from the target.
pub(crate) const MIN_STROKE_DEG: f32 = 0.05;
/// This many strokes toward the target without going past it: the player kept stopping short.
const CREEP_STROKES: u32 = 3;

/// Per-stroke net progress along the unit direction `u` (degrees), oldest first. Speed is the
/// path covered in a short window; a stroke runs from above `STROKE_ON_SHARE` of the peak speed
/// to below `STROKE_OFF_SHARE`, or ends at a gap in the samples. Movement before a stroke starts
/// counts toward it.
pub(crate) fn split_strokes(samples: &[DegSample], u: Vec2) -> Vec<f32> {
    // Windowed speed (deg/ms) per sample.
    let speeds: Vec<f32> = (0..samples.len())
        .map(|i| {
            let ti = samples[i].t_ms;
            let covered: f32 = samples[..=i]
                .iter()
                .rev()
                .take_while(|s| ti - s.t_ms < SPEED_WINDOW_MS)
                .map(|s| s.d.length())
                .sum();
            covered / SPEED_WINDOW_MS
        })
        .collect();
    let peak = speeds.iter().cloned().fold(0.0f32, f32::max);
    let (on, off) = (peak * STROKE_ON_SHARE, peak * STROKE_OFF_SHARE);

    let mut strokes: Vec<f32> = Vec::new();
    let (mut in_stroke, mut current, mut pending) = (false, 0.0f32, 0.0f32);
    let mut prev_ms = 0.0f32;
    for (s, &v) in samples.iter().zip(&speeds) {
        let progress = s.d.dot(u);
        if in_stroke && s.t_ms - prev_ms > GAP_MS {
            strokes.push(current);
            (in_stroke, current) = (false, 0.0);
        }
        prev_ms = s.t_ms;
        if in_stroke {
            current += progress;
            if v < off {
                strokes.push(current);
                (in_stroke, current) = (false, 0.0);
            }
        } else if v >= on {
            (in_stroke, current, pending) = (true, pending + progress, 0.0);
        } else {
            pending += progress;
        }
    }
    if in_stroke {
        strokes.push(current);
    }
    strokes
}

/// How far past the target centre still counts as stopping noise.
pub(crate) fn cross_tolerance_deg(distance_deg: f32) -> f32 {
    CROSS_TOLERANCE_MIN_DEG.max(CROSS_TOLERANCE_SHARE * distance_deg)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MicroFeatures {
    /// Angle from the aim to the target centre when it appeared.
    pub target_deg: f32,
    /// Strokes of at least `MIN_STROKE_DEG` toward or away from the target.
    pub submovements: u32,
    pub forward_strokes: u32,
    pub backward_strokes: u32,
    /// Went past the centre by more than the tolerance and came back.
    pub crossed_center: bool,
    /// Furthest past the centre, in degrees (0 if it never got there).
    pub overshoot_deg: f32,
    /// Time from the target appearing until the aim stayed within tolerance of the centre (or
    /// until the last movement, if it never did).
    pub settle_ms: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MicroShot {
    pub features: MicroFeatures,
    signal: Signal,
}

impl DirectionalSignal for MicroShot {
    fn signal(&self) -> Option<Signal> {
        Some(self.signal)
    }
}

/// Judge one micro-adjustment from its mouse samples (oldest first, counts with y down),
/// `start` = when the target appeared, `target_offset_deg` = the mouse movement in degrees that
/// would put the aim on the target centre (see `MicroSpawn::offset_deg`).
/// `None` when the player barely moved.
pub fn classify_micro(
    samples: &[MouseSample],
    start: Instant,
    deg_per_count: f32,
    target_offset_deg: Vec2,
) -> Option<MicroShot> {
    let d = target_offset_deg.length();
    let path_deg: f32 = samples.iter().map(|s| s.delta.length() * deg_per_count).sum();
    if d < 1e-4 || path_deg < MIN_MOVEMENT_DEG {
        return None;
    }
    let u = target_offset_deg / d;
    let tol = cross_tolerance_deg(d);
    let ms = |t: Instant| t.duration_since(start).as_secs_f32() * 1000.0;
    let deg_samples: Vec<DegSample> = samples
        .iter()
        .map(|s| DegSample { t_ms: ms(s.time), d: s.delta * deg_per_count })
        .collect();
    let strokes = split_strokes(&deg_samples, u);
    let forward = strokes.iter().filter(|&&p| p >= MIN_STROKE_DEG).count() as u32;
    let backward = strokes.iter().filter(|&&p| p <= -MIN_STROKE_DEG).count() as u32;

    // Progress toward the centre over time.
    let mut p = 0.0f32;
    let mut max_p = f32::MIN;
    let mut settle_ms = 0.0f32;
    let mut settled = false;
    for s in samples {
        p += s.delta.dot(u) * deg_per_count;
        max_p = max_p.max(p);
        let inside = (p - d).abs() <= tol;
        if inside && !settled {
            settle_ms = ms(s.time);
        }
        settled = inside;
    }
    if !settled {
        settle_ms = samples.last().map_or(0.0, |s| ms(s.time));
    }
    let crossed = max_p > d + tol && max_p - p > tol;

    let signal = if crossed {
        Signal::TooFast
    } else if forward >= CREEP_STROKES && backward == 0 {
        Signal::TooSlow
    } else {
        Signal::Direct
    };
    Some(MicroShot {
        features: MicroFeatures {
            target_deg: d,
            submovements: forward + backward,
            forward_strokes: forward,
            backward_strokes: backward,
            crossed_center: crossed,
            overshoot_deg: (max_p - d).max(0.0),
            settle_ms,
        },
        signal,
    })
}
