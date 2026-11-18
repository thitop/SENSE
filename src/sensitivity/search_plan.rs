use crate::data::setup::{Grip, PadSize, PlayerSetup, Pivot};
use crate::data::PlayerProfile;
use crate::sensitivity::pooling::SEARCH_CM;
use serde::{Deserialize, Serialize};

/// The longest sweep (cm/360) each mousepad size comfortably allows. `None` is "Not sure".
pub const PAD_MAX_CM: [(Option<PadSize>, f64); 4] = [
    (Some(PadSize::Small), 30.0),
    (Some(PadSize::Medium), 52.0),
    (Some(PadSize::Large), 80.0),
    (None, 80.0),
];

pub fn pad_max_cm(pad: Option<PadSize>) -> f64 {
    PAD_MAX_CM.iter().find(|(p, _)| *p == pad).map_or(SEARCH_CM.1, |&(_, cm)| cm)
}

/// Where a calibration looks: the allowed range and, when known, where to start.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchPlan {
    pub min_cm: f64,
    pub max_cm: f64,
    /// Recorded for History only; the block search ignores it (see `BlockScheduler::with_start`).
    pub start_cm: Option<f64>,
}

impl Default for SearchPlan {
    fn default() -> Self {
        Self { min_cm: SEARCH_CM.0, max_cm: SEARCH_CM.1, start_cm: None }
    }
}

fn pivot_base(p: Pivot) -> f64 {
    match p {
        Pivot::Fingers => 28.0,
        Pivot::Wrist => 32.0,
        Pivot::Arm => 45.0,
        Pivot::Shoulder => 55.0,
    }
}

fn grip_base(g: Grip) -> f64 {
    match g {
        Grip::Fingertip => 32.0,
        Grip::Claw => 40.0,
        Grip::Palm => 45.0,
    }
}

impl SearchPlan {
    /// `own_cm` is `Some` only when the player's sensitivity is known.
    pub fn new(setup: Option<&PlayerSetup>, own_cm: Option<f64>) -> SearchPlan {
        let min_cm = SEARCH_CM.0;
        let max_cm = pad_max_cm(setup.and_then(|s| s.pad));
        let base = own_cm.or_else(|| {
            let s = setup?;
            s.pivot.map(pivot_base).or_else(|| s.grip.map(grip_base))
        });
        let start_cm = base.map(|base| {
            let net: i32 = match setup {
                Some(s) if own_cm.is_none_or(|cm| s.q1_current(cm)) => {
                    s.problems.iter().map(|p| i32::from(p.lean())).sum()
                }
                _ => 0,
            };
            let leaned = match net {
                n if n > 0 => base * 1.12f64.powi(n),
                n if n < 0 => base * 0.89f64.powi(-n),
                _ => base,
            };
            leaned.clamp(min_cm * 1.10, max_cm * 0.90)
        });
        SearchPlan { min_cm, max_cm, start_cm }
    }

    pub fn for_profile(p: &PlayerProfile) -> SearchPlan {
        Self::new(p.setup.as_ref(), p.sens_known().then_some(p.current_cm_360))
    }
}
