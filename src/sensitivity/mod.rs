pub mod analyzer;
pub mod converter;
pub mod games;
pub mod path_recorder;
pub mod psa;

pub use analyzer::StatisticalAnalyzer;
pub use converter::SensitivityConverter;
pub use games::{GameDatabase, GameProfile};
pub use path_recorder::{MousePathRecorder, ShotMovementPath, ShotTrajectoryPattern};
pub use psa::{PsaCalibrator, PsaRoundConclusion, PsaRoundResult};

use crate::data::{CalibrationResult, CalibrationRound};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct FinalSensitivityRecommendation {
    pub optimal_cm_360: f64,
    pub psa_result_cm_360: Option<f64>,
    pub statistical_peak_cm_360: Option<f64>,
    pub confidence_score: f32, // 0.0 - 1.0
    pub conversions: Vec<(String, f64)>, // (Game Name, In-Game Sens)
}

pub struct SensitivityEngine {
    pub db: GameDatabase,
    pub psa: PsaCalibrator,
    pub analyzer: StatisticalAnalyzer,
    pub path_recorder: MousePathRecorder,
    pub player_dpi: f64,
}

impl SensitivityEngine {
    pub fn new(player_dpi: f64) -> Self {
        Self {
            db: GameDatabase::new(),
            psa: PsaCalibrator::new(15.0, 80.0, 7),
            analyzer: StatisticalAnalyzer::new(),
            path_recorder: MousePathRecorder::new(),
            player_dpi: if player_dpi > 0.0 { player_dpi } else { 800.0 },
        }
    }

    /// Start a fresh calibration: new PSA search, no analyzer samples, no shot in progress.
    pub fn reset_calibration(&mut self) {
        self.psa = PsaCalibrator::new(15.0, 80.0, 7);
        self.analyzer = StatisticalAnalyzer::new();
        self.path_recorder = MousePathRecorder::new();
    }

    /// Snapshot the finished calibration for persisting on the profile.
    pub fn build_calibration_result(&self, at: DateTime<Utc>) -> CalibrationResult {
        let rec = self.compute_final_recommendation();
        CalibrationResult {
            cm_360: rec.optimal_cm_360,
            confidence: rec.confidence_score,
            psa_cm: rec.psa_result_cm_360,
            stat_cm: rec.statistical_peak_cm_360,
            rounds: self
                .psa
                .history
                .iter()
                .map(|r| CalibrationRound {
                    tested_cm_360: r.tested_cm_360,
                    conclusion: r.conclusion,
                })
                .collect(),
            samples: self
                .analyzer
                .samples
                .iter()
                .map(|s| (s.cm_360, s.composite_score))
                .collect(),
            at,
        }
    }

    /// Calculate combined optimal sensitivity using both PSA binary search and Statistical Curve Fitting
    pub fn compute_final_recommendation(&self) -> FinalSensitivityRecommendation {
        let psa_cm = self.psa.recommended_cm_360;
        let stat_cm = self.analyzer.find_optimal_cm_360();

        let (optimal_cm, confidence) = match (psa_cm, stat_cm) {
            (Some(p), Some(s)) => {
                // If both are available, blend them with 60% weight to PSA and 40% to Stats
                let diff = (p - s).abs();
                let conf = (1.0 - (diff / 25.0).clamp(0.0, 0.8)) as f32;
                (p * 0.60 + s * 0.40, conf)
            }
            (Some(p), None) => (p, 0.75),
            (None, Some(s)) => (s, 0.70),
            (None, None) => (40.0, 0.30), // Default 40 cm/360° fallback
        };

        // Convert to all supported games in database
        let mut conversions = Vec::new();
        for game in self.db.list() {
            let game_sens = SensitivityConverter::cm_360_to_game_sens(game, self.player_dpi, optimal_cm);
            conversions.push((game.name.clone(), game_sens));
        }

        FinalSensitivityRecommendation {
            optimal_cm_360: optimal_cm,
            psa_result_cm_360: psa_cm,
            statistical_peak_cm_360: stat_cm,
            confidence_score: confidence,
            conversions,
        }
    }
}

