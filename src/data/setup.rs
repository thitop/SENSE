use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AimProblem {
    Overshoot,
    CantTurnFast,
    ShakySmall,
    RunOutOfPad,
}

impl AimProblem {
    /// +1 points to a slower sensitivity, -1 to a faster one.
    pub fn lean(self) -> i8 {
        match self {
            AimProblem::Overshoot | AimProblem::ShakySmall => 1,
            AimProblem::CantTurnFast | AimProblem::RunOutOfPad => -1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grip {
    Palm,
    Claw,
    Fingertip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pivot {
    Wrist,
    Fingers,
    Arm,
    Shoulder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadSize {
    Small,
    Medium,
    Large,
}

/// What the player told us about their aim and gear. Empty `problems` / `None` mean "Not sure".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerSetup {
    pub problems: Vec<AimProblem>,
    pub grip: Option<Grip>,
    pub pivot: Option<Pivot>,
    pub pad: Option<PadSize>,
    /// The sensitivity (cm/360) in use when the problems were answered.
    pub answered_at_cm: f64,
    pub answered_at: DateTime<Utc>,
}

impl PlayerSetup {
    /// What "Skip all" saves.
    pub fn not_sure(at_cm: f64, at: DateTime<Utc>) -> Self {
        Self { problems: Vec::new(), grip: None, pivot: None, pad: None, answered_at_cm: at_cm, answered_at: at }
    }

    /// Whether the problems answer still describes `own_cm` (within 5%).
    pub fn q1_current(&self, own_cm: f64) -> bool {
        // The epsilon keeps an exact 5% move (40 -> 42) on the current side despite float error.
        (own_cm / self.answered_at_cm - 1.0).abs() <= 0.05 + 1e-9
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionsNeeded {
    All,
    Q1Only,
}
