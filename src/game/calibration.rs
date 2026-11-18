//! Calibration-session state and the small pure helpers the UI needs around it.

use crate::game::calibration_run::CalibrationRun;
use crate::game::tasks::TaskState;
use crate::sensitivity::combiner::TestKind;
use crate::sensitivity::games::GameProfile;
use crate::sensitivity::psa::{PsaCalibrator, PsaRoundConclusion};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMode {
    Practice,
    Calibration,
}

#[derive(Clone, Copy, Debug)]
pub struct PlaySession {
    pub mode: PlayMode,
    pub paused: bool,
}

impl PlaySession {
    /// Only an unpaused calibration run may feed the PSA calibrator and the analyzer.
    pub fn records_calibration(&self) -> bool {
        self.mode == PlayMode::Calibration && !self.paused
    }
}

/// A click counts as a calibration shot only while targets are live — never during the
/// 3-2-1 countdown, while paused, or in practice.
pub fn records_calibration_shot(session: PlaySession, task_state: TaskState) -> bool {
    session.records_calibration() && task_state == TaskState::Running
}

/// Esc in the menu returns to the game only when a run is in progress and setup isn't open;
/// otherwise there'd be nothing to play and the cursor would be captured for no reason.
pub fn esc_closes_menu(task_state: TaskState, onboarding_open: bool) -> bool {
    !onboarding_open && matches!(task_state, TaskState::Running | TaskState::Countdown { .. })
}

/// Score and play time of a calibration run that was paused and resumed. Each resume restarts
/// the target task, so finished segments are banked here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunTotals {
    pub score: u64,
    pub duration_secs: f32,
}

impl RunTotals {
    pub fn add_segment(&mut self, score: u64, duration_secs: f32) {
        self.score += score;
        self.duration_secs += duration_secs;
    }

    /// Totals including the segment still in progress.
    pub fn with_current(self, score: u64, duration_secs: f32) -> RunTotals {
        let mut t = self;
        t.add_segment(score, duration_secs);
        t
    }
}

/// Read-only view of calibration progress for the HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationProgress {
    pub round: usize,
    pub max_rounds: usize,
    pub testing_cm_360: f64,
    pub low_cm_360: f64,
    pub high_cm_360: f64,
    /// Flick-only search: shots in the round. Multi-test run: 1-based segment of the warm-up
    /// or block, out of `shots_per_round` segments.
    pub shot_in_round: usize,
    pub shots_per_round: usize,
    pub completed: bool,
    /// The test being played (multi-test run only).
    pub segment_label: Option<&'static str>,
    /// What that test wants from the player, shown during the countdown before it.
    pub segment_hint: Option<&'static str>,
    /// Playing the warm-up at the player's own sensitivity.
    pub warmup: bool,
}

impl CalibrationProgress {
    pub fn from_psa(psa: &PsaCalibrator) -> Self {
        Self {
            round: psa.round_number.min(psa.max_rounds),
            max_rounds: psa.max_rounds,
            testing_cm_360: psa.current_test_cm_360,
            low_cm_360: psa.low_cm_360,
            high_cm_360: psa.high_cm_360,
            shot_in_round: psa.shots_in_round(),
            shots_per_round: psa.shots_per_round(),
            completed: psa.is_completed,
            segment_label: None,
            segment_hint: None,
            warmup: false,
        }
    }

    /// The speed being tested, as the HUD shows it.
    pub fn speed_text(&self) -> String {
        format!("{:.1} cm", self.testing_cm_360)
    }

    /// `player_cm`: the sensitivity the warm-up runs at.
    pub fn from_run(run: &CalibrationRun, player_cm: f64) -> Self {
        let s = run.scheduler();
        Self {
            // No round is in progress during the warm-up.
            round: if run.in_warmup() { 0 } else { run.block_number() },
            max_rounds: run.blocks(),
            testing_cm_360: run.test_cm(player_cm),
            low_cm_360: s.low_cm_360,
            high_cm_360: s.high_cm_360,
            shot_in_round: (run.segment_index() + 1).min(run.segments_in_phase()),
            shots_per_round: run.segments_in_phase(),
            completed: run.is_completed(),
            segment_label: run.segment().map(|seg| seg.kind.label()),
            segment_hint: run.segment().map(|seg| segment_hint(seg.kind, seg.counted)),
            warmup: run.in_warmup(),
        }
    }
}

/// One line telling the player what the next calibration segment wants.
pub fn segment_hint(kind: TestKind, counted: bool) -> &'static str {
    match (kind, counted) {
        (TestKind::Flick, false) => "A few shots to get used to this sensitivity",
        (TestKind::Flick, true) => "Shoot each target as it appears",
        (TestKind::Micro, _) => "Small targets close to your crosshair",
        (TestKind::Tracking, _) => "Keep your crosshair on the moving target. No need to shoot",
        (TestKind::Recoil, _) => "Hold fire and pull down to stay on target",
        (TestKind::Placement, _) => "Aim at the corner before the target peeks",
        (TestKind::Strafe, _) => "Stop moving before you shoot",
    }
}

pub fn round_change_message(c: PsaRoundConclusion) -> &'static str {
    match c {
        PsaRoundConclusion::TooFastNeedSlower => "Sensitivity changed: slower",
        PsaRoundConclusion::TooSlowNeedFaster => "Sensitivity changed: faster",
        PsaRoundConclusion::BalancedNarrow => "Range narrowed",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl ConfidenceLevel {
    pub fn from_score(s: f32) -> Self {
        if s >= 0.75 {
            Self::High
        } else if s >= 0.5 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::High => "High confidence",
            Self::Medium => "Medium confidence",
            Self::Low => "Low confidence",
        }
    }
}

/// Lower cm/360 means a faster sensitivity.
pub fn delta_text(new_cm: f64, current_cm: f64) -> String {
    let diff = new_cm - current_cm;
    if diff.abs() < 0.05 {
        "Same as now".into()
    } else if diff < 0.0 {
        format!("{:.1} cm faster than now", -diff)
    } else {
        format!("{:.1} cm slower than now", diff)
    }
}

fn parse_finite(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

pub fn parse_dpi(s: &str) -> Result<f64, &'static str> {
    let v = parse_finite(s).ok_or("Enter a number")?;
    if (50.0..=50000.0).contains(&v) {
        Ok(v)
    } else {
        Err("DPI must be between 50 and 50000")
    }
}

/// Plausible range for a real sensitivity. Catches typos like "35" for "0.35" and keeps
/// non-finite values out of saved profiles (serde writes them as null, which won't load).
pub fn validate_cm_360(cm: f64) -> Result<f64, &'static str> {
    if (5.0..=300.0).contains(&cm) {
        Ok(cm)
    } else {
        Err("That's outside 5–300 cm/360°. Check the value.")
    }
}

pub fn parse_game_sens(s: &str) -> Result<f64, &'static str> {
    let v = parse_finite(s).ok_or("Enter a number")?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err("Sensitivity must be greater than 0")
    }
}

/// The player's main game, or the first known game if the saved id no longer exists.
pub fn primary_game<'a>(games: &'a [GameProfile], id: &str) -> Option<&'a GameProfile> {
    games.iter().find(|g| g.id == id).or_else(|| games.first())
}
