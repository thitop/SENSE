use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::sensitivity::psa::PsaRoundConclusion;

/// One PSA round of a finished calibration, kept so the result screen can redraw its graph.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CalibrationRound {
    pub tested_cm_360: f64,
    pub conclusion: PsaRoundConclusion,
}

/// A completed calibration run, persisted on the player profile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalibrationResult {
    pub cm_360: f64,
    pub confidence: f32,
    pub psa_cm: Option<f64>,
    pub stat_cm: Option<f64>,
    pub rounds: Vec<CalibrationRound>,
    /// (cm/360, composite score) per recorded shot.
    pub samples: Vec<(f64, f64)>,
    pub at: DateTime<Utc>,
}
