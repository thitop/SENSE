use super::path_recorder::ShotTrajectoryPattern;

#[derive(Clone, Debug)]
pub struct PsaRoundResult {
    pub round_number: usize,
    pub tested_cm_360: f64,
    pub overshoots: usize,
    pub undershoots: usize,
    pub directs: usize,
    pub total_shots: usize,
    pub conclusion: PsaRoundConclusion,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PsaRoundConclusion {
    TooFastNeedSlower, // Overshoot dominant -> decrease sensitivity (higher cm/360)
    TooSlowNeedFaster, // Undershoot dominant -> increase sensitivity (lower cm/360)
    BalancedNarrow,    // Balanced -> narrow both bounds
}

#[derive(Clone, Debug)]
pub struct PsaCalibrator {
    pub low_cm_360: f64,  // Fast sens boundary (e.g. 15 cm/360°)
    pub high_cm_360: f64, // Slow sens boundary (e.g. 80 cm/360°)
    pub current_test_cm_360: f64,
    pub round_number: usize,
    pub max_rounds: usize,
    pub convergence_threshold_cm: f64,
    pub history: Vec<PsaRoundResult>,
    pub is_completed: bool,
    pub recommended_cm_360: Option<f64>,

    // Current round shot accumulator
    round_overshoots: usize,
    round_undershoots: usize,
    round_directs: usize,
    shots_per_round: usize,
}

impl PsaCalibrator {
    pub fn new(initial_low_cm: f64, initial_high_cm: f64, max_rounds: usize) -> Self {
        let mid = (initial_low_cm + initial_high_cm) * 0.5;
        Self {
            low_cm_360: initial_low_cm,
            high_cm_360: initial_high_cm,
            current_test_cm_360: mid,
            round_number: 1,
            max_rounds,
            convergence_threshold_cm: 2.0,
            history: Vec::new(),
            is_completed: false,
            recommended_cm_360: None,
            round_overshoots: 0,
            round_undershoots: 0,
            round_directs: 0,
            shots_per_round: 10,
        }
    }

    pub fn record_shot(&mut self, pattern: ShotTrajectoryPattern) -> bool {
        match pattern {
            ShotTrajectoryPattern::Overshoot => self.round_overshoots += 1,
            ShotTrajectoryPattern::Undershoot => self.round_undershoots += 1,
            ShotTrajectoryPattern::Direct => self.round_directs += 1,
        }

        let total_recorded = self.round_overshoots + self.round_undershoots + self.round_directs;
        if total_recorded >= self.shots_per_round {
            self.finalize_round();
            true // Round finished
        } else {
            false
        }
    }

    fn finalize_round(&mut self) {
        let total = (self.round_overshoots + self.round_undershoots + self.round_directs).max(1);
        let overshoot_ratio = self.round_overshoots as f64 / total as f64;
        let undershoot_ratio = self.round_undershoots as f64 / total as f64;

        let conclusion = if overshoot_ratio > 0.45 {
            // Player is overshooting -> sensitivity is too high/fast -> increase cm/360° (slower)
            self.low_cm_360 = self.current_test_cm_360;
            PsaRoundConclusion::TooFastNeedSlower
        } else if undershoot_ratio > 0.45 {
            // Player is undershooting -> sensitivity is too low/slow -> decrease cm/360° (faster)
            self.high_cm_360 = self.current_test_cm_360;
            PsaRoundConclusion::TooSlowNeedFaster
        } else {
            // Fairly balanced -> narrow both sides symmetrically
            let quarter = (self.high_cm_360 - self.low_cm_360) * 0.25;
            self.low_cm_360 += quarter;
            self.high_cm_360 -= quarter;
            PsaRoundConclusion::BalancedNarrow
        };

        let result = PsaRoundResult {
            round_number: self.round_number,
            tested_cm_360: self.current_test_cm_360,
            overshoots: self.round_overshoots,
            undershoots: self.round_undershoots,
            directs: self.round_directs,
            total_shots: total,
            conclusion,
        };
        self.history.push(result);

        // Next iteration
        self.round_overshoots = 0;
        self.round_undershoots = 0;
        self.round_directs = 0;
        self.round_number += 1;

        let diff = (self.high_cm_360 - self.low_cm_360).abs();
        if self.round_number > self.max_rounds || diff <= self.convergence_threshold_cm {
            self.is_completed = true;
            self.recommended_cm_360 = Some((self.low_cm_360 + self.high_cm_360) * 0.5);
        } else {
            self.current_test_cm_360 = (self.low_cm_360 + self.high_cm_360) * 0.5;
        }
    }
}

