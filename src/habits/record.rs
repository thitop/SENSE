//! What gets stored about each target and tracking run. Every angle is in degrees, never mouse
//! counts, so records from different DPI and sensitivity settings can be compared.

use serde::{Deserialize, Serialize};

use crate::game::tasks::flick::FlickBand;
use crate::sensitivity::paired::Side;

/// Switching and Speed targets record as `Flick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AimKind {
    Flick,
    Micro,
    Moving,
}

/// The sensitivity setting a record was played at. `block == None` means practice.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayContext {
    /// The cm/360 actually played (block cm x side factor).
    pub cm_360: f32,
    pub side: Option<Side>,
    pub block: Option<u8>,
    pub warmup: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetRecord {
    pub kind: AimKind,
    /// Aim-to-target offset when the target appeared, `[x, y]` degrees, y down.
    pub offset_deg: [f32; 2],
    pub radius_deg: f32,
    /// Net progress at the end of the first stroke / distance (1.0 = on the centre).
    pub first_landing: Option<f32>,
    pub strokes: u8,
    pub crossed_center: bool,
    pub overshoot_deg: f32,
    pub time_to_hit_ms: Option<f32>,
    /// Missed shots as `[along, across]` the target direction, degrees from its centre; at most 8.
    pub misses: Vec<[f32; 2]>,
    pub band: Option<FlickBand>,
    pub ctx: PlayContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackingRecord {
    pub mean_along_deg: f32,
    pub lag_ms: f32,
    pub cross_rms_deg: f32,
    pub mean_speed_deg_s: f32,
    pub coverage: f32,
    pub radius_deg: f32,
    pub horizontal: bool,
    pub ctx: PlayContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct AimLog {
    pub targets: Vec<TargetRecord>,
    pub tracking: Vec<TrackingRecord>,
}

impl PlayContext {
    /// The setting being played: `cm_360` is the block's value, `factor` the paired side's
    /// speed factor (1.0 when there is none). `calibration` is `Some((block, counted))` during
    /// a calibration run, where only counted segments after the warm-up belong to a block;
    /// everything else there is warm-up. Practice passes `None`.
    pub fn for_play(cm_360: f64, factor: f64, calibration: Option<(u8, bool)>) -> Self {
        let side = if factor < 1.0 {
            Some(Side::Faster)
        } else if factor > 1.0 {
            Some(Side::Slower)
        } else {
            None
        };
        let (block, warmup) = match calibration {
            Some((block, true)) => (Some(block), false),
            Some((_, false)) => (None, true),
            None => (None, false),
        };
        Self { cm_360: (cm_360 * factor) as f32, side, block, warmup }
    }
}
