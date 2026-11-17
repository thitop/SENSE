use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Fit over every round, set when the calibration completes (if the data allow one).
    pub crossover: Option<Crossover>,
    /// The range the search started with; a recommendation never leaves it.
    search_min_cm: f64,
    search_max_cm: f64,

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
            crossover: None,
            search_min_cm: initial_low_cm,
            search_max_cm: initial_high_cm,
            round_overshoots: 0,
            round_undershoots: 0,
            round_directs: 0,
            shots_per_round: 10,
        }
    }

    /// How much to trust `recommended_cm_360`, 0..1, from the fit's ~95% interval: ±12.5% or
    /// tighter reads as high (≥ 0.75), ±25% as medium (0.5), wider or no fit as low. With 70
    /// shots a typical player lands around ±20%. An answer against the edge of the search range
    /// stays low: the real one may lie outside it.
    pub fn confidence(&self) -> f32 {
        const UNCERTAINTY_FOR_ZERO: f64 = 0.50;
        let Some(r) = self.recommended_cm_360 else {
            return 0.0;
        };
        let mut conf = match self.crossover {
            Some(c) => (1.0 - c.rel_uncertainty / UNCERTAINTY_FOR_ZERO).clamp(0.05, 0.95),
            None => 0.3,
        };
        let at_edge = r <= self.search_min_cm * 1.03 || r >= self.search_max_cm * 0.97;
        if at_edge {
            conf = conf.min(0.3);
        }
        conf as f32
    }

    /// Shots recorded so far in the round in progress.
    pub fn shots_in_round(&self) -> usize {
        self.round_overshoots + self.round_undershoots + self.round_directs
    }

    pub fn shots_per_round(&self) -> usize {
        self.shots_per_round
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

    /// Rounds that bisect the range before the search starts following the fit.
    const BISECTION_ROUNDS: usize = 2;
    /// Once following the fit, a round only moves a bound to within 3% of the tested value,
    /// so one noisy round can't shut the answer out of the range.
    const ADAPTIVE_BOUND_MARGIN: f64 = 0.03;

    fn rounds_for_fit(&self) -> Vec<(f64, usize, usize)> {
        self.history
            .iter()
            .map(|r| (r.tested_cm_360, r.overshoots, r.undershoots))
            .collect()
    }

    fn finalize_round(&mut self) {
        let total = (self.round_overshoots + self.round_undershoots + self.round_directs).max(1);
        let adaptive = self.history.len() >= Self::BISECTION_ROUNDS;
        let tested = self.current_test_cm_360;

        // Clean (direct) shots say nothing about which way the sensitivity is off, so only the
        // misses decide the direction. Comparing against all shots would call most rounds near
        // the answer "balanced", and a balanced round never moves the test point, so the search
        // would stop at the first midpoint that happened to land near the answer.
        let conclusion = if self.round_overshoots > self.round_undershoots {
            // Player is overshooting -> sensitivity is too high/fast -> increase cm/360° (slower)
            self.low_cm_360 = if adaptive {
                self.low_cm_360.max(tested * (1.0 - Self::ADAPTIVE_BOUND_MARGIN)).min(self.high_cm_360)
            } else {
                tested
            };
            PsaRoundConclusion::TooFastNeedSlower
        } else if self.round_undershoots > self.round_overshoots {
            // Player is undershooting -> sensitivity is too low/slow -> decrease cm/360° (faster)
            self.high_cm_360 = if adaptive {
                self.high_cm_360.min(tested * (1.0 + Self::ADAPTIVE_BOUND_MARGIN)).max(self.low_cm_360)
            } else {
                tested
            };
            PsaRoundConclusion::TooSlowNeedFaster
        } else {
            // Even misses. While bisecting, narrow both sides; once following the fit, the
            // round's shots move the next test point through the fit instead.
            if !adaptive {
                let quarter = (self.high_cm_360 - self.low_cm_360) * 0.25;
                self.low_cm_360 += quarter;
                self.high_cm_360 -= quarter;
            }
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
            // Bisection alone keeps only the last step; the fit weighs every round's shots.
            self.crossover = fit_crossover(&self.rounds_for_fit());
            let midpoint = (self.low_cm_360 + self.high_cm_360) * 0.5;
            let estimate = self.crossover.map_or(midpoint, |c| c.cm_360);
            self.recommended_cm_360 = Some(estimate.clamp(self.search_min_cm, self.search_max_cm));
        } else {
            let midpoint = (self.low_cm_360 + self.high_cm_360) * 0.5;
            // After the first rounds, test where the fit puts the answer: shots there carry the
            // most information about it.
            self.current_test_cm_360 = if self.history.len() >= Self::BISECTION_ROUNDS {
                fit_crossover(&self.rounds_for_fit())
                    .map_or(midpoint, |c| c.cm_360)
                    .clamp(self.low_cm_360, self.high_cm_360)
            } else {
                midpoint
            };
        }
    }
}


/// Where a player's misses switch from mostly overshooting to mostly undershooting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossover {
    /// The sensitivity where overshoots and undershoots are equally likely.
    pub cm_360: f64,
    /// Half-width of the ~95% interval around `cm_360`, as a fraction of it (0.1 = ±10%).
    pub rel_uncertainty: f64,
}

/// Fit P(overshoot | miss) = 1 / (1 + e^-(a + b·(ln cm − mean))) to `(cm/360, overshoots,
/// undershoots)` per round, and return the 50% point with its uncertainty. A small penalty on
/// the slope keeps the fit finite when the rounds separate perfectly (all overshoots on one
/// side, all undershoots on the other). Returns `None` without misses on both sides, or when
/// overshoots don't become less likely as the sensitivity gets slower.
pub fn fit_crossover(rounds: &[(f64, usize, usize)]) -> Option<Crossover> {
    const SLOPE_PENALTY: f64 = 0.05;
    let data: Vec<(f64, f64, f64)> = rounds
        .iter()
        .filter(|&&(cm, o, u)| cm > 0.0 && o + u > 0)
        .map(|&(cm, o, u)| (cm.ln(), o as f64, (o + u) as f64))
        .collect();
    let overs: f64 = data.iter().map(|d| d.1).sum();
    let misses: f64 = data.iter().map(|d| d.2).sum();
    if data.len() < 2 || overs == 0.0 || overs == misses {
        return None;
    }
    let mean_x = data.iter().map(|d| d.0 * d.2).sum::<f64>() / misses;

    // Newton-Raphson on the penalized log-likelihood.
    let (mut a, mut b) = (0.0f64, 0.0f64);
    let mut info = [[0.0f64; 2]; 2];
    for _ in 0..100 {
        let (mut ga, mut gb) = (0.0, -SLOPE_PENALTY * b);
        info = [[1e-9, 0.0], [0.0, SLOPE_PENALTY]];
        for &(x, y, n) in &data {
            let xc = x - mean_x;
            let p = 1.0 / (1.0 + (-(a + b * xc)).exp());
            let w = n * p * (1.0 - p);
            ga += y - n * p;
            gb += (y - n * p) * xc;
            info[0][0] += w;
            info[0][1] += w * xc;
            info[1][1] += w * xc * xc;
        }
        info[1][0] = info[0][1];
        let det = info[0][0] * info[1][1] - info[0][1] * info[1][0];
        if det.abs() < 1e-12 {
            return None;
        }
        let da = (info[1][1] * ga - info[0][1] * gb) / det;
        let db = (info[0][0] * gb - info[1][0] * ga) / det;
        a += da;
        b += db;
        if da.abs() < 1e-9 && db.abs() < 1e-9 {
            break;
        }
    }
    // Faster (smaller cm) must mean more overshoots.
    if !(b < 0.0) || !a.is_finite() {
        return None;
    }

    // Covariance = inverse information; delta method for x* = mean − a/b.
    let det = info[0][0] * info[1][1] - info[0][1] * info[1][0];
    let (var_a, var_b, cov) = (info[1][1] / det, info[0][0] / det, -info[0][1] / det);
    let (da, db) = (-1.0 / b, a / (b * b));
    let var_x = da * da * var_a + db * db * var_b + 2.0 * da * db * cov;
    let x_star = mean_x - a / b;
    Some(Crossover {
        cm_360: x_star.exp(),
        rel_uncertainty: 1.96 * var_x.max(0.0).sqrt(),
    })
}
