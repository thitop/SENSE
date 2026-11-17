//! Signals shared by every calibration test, and the combination of each test's estimate
//! into one recommended sensitivity.

use serde::{Deserialize, Serialize};

use super::path_recorder::{ShotMovementPath, ShotTrajectoryPattern};
use super::psa::Crossover;

/// The calibration tests. `Strafe` is diagnostic only and never moves the sensitivity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TestKind {
    Placement,
    Flick,
    Tracking,
    Micro,
    Recoil,
    Strafe,
}

impl TestKind {
    pub const ALL: [TestKind; 6] = [
        TestKind::Placement,
        TestKind::Flick,
        TestKind::Tracking,
        TestKind::Micro,
        TestKind::Recoil,
        TestKind::Strafe,
    ];

    /// Position in `ALL`, for per-test arrays.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            TestKind::Placement => "Crosshair placement",
            TestKind::Flick => "Flicking",
            TestKind::Tracking => "Tracking",
            TestKind::Micro => "Micro-adjustment",
            TestKind::Recoil => "Recoil control",
            TestKind::Strafe => "Counter-strafing",
        }
    }
}

/// What one sample says about the sensitivity it was taken at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    /// The player went past where they meant to stop: slower would help.
    TooFast,
    /// The player stopped short or lagged behind: faster would help.
    TooSlow,
    /// Clean; says nothing about direction.
    Direct,
}

impl From<ShotTrajectoryPattern> for Signal {
    fn from(p: ShotTrajectoryPattern) -> Self {
        match p {
            ShotTrajectoryPattern::Overshoot => Signal::TooFast,
            ShotTrajectoryPattern::Undershoot => Signal::TooSlow,
            ShotTrajectoryPattern::Direct => Signal::Direct,
        }
    }
}

/// Anything a test measures that can say "too fast" or "too slow": a flick shot, a tracking
/// window, a recoil spray. `None` when the sample is too small to tell.
pub trait DirectionalSignal {
    fn signal(&self) -> Option<Signal>;
}

impl DirectionalSignal for ShotMovementPath {
    fn signal(&self) -> Option<Signal> {
        self.pattern.map(Signal::from)
    }
}

/// How much each test counts toward the result. Counter-strafing is fixed at zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightSet {
    pub placement: f64,
    pub flick: f64,
    pub tracking: f64,
    pub micro: f64,
    pub recoil: f64,
}

impl WeightSet {
    /// The weights the search runs on and the neutral result uses.
    pub fn neutral() -> Self {
        Self {
            placement: 0.3,
            flick: 1.0,
            tracking: 1.0,
            micro: 1.0,
            recoil: 0.6,
        }
    }

    /// The same weights with `kind` left out.
    pub fn without(mut self, kind: TestKind) -> Self {
        match kind {
            TestKind::Placement => self.placement = 0.0,
            TestKind::Flick => self.flick = 0.0,
            TestKind::Tracking => self.tracking = 0.0,
            TestKind::Micro => self.micro = 0.0,
            TestKind::Recoil => self.recoil = 0.0,
            TestKind::Strafe => {}
        }
        self
    }

    pub fn get(&self, kind: TestKind) -> f64 {
        let w = match kind {
            TestKind::Placement => self.placement,
            TestKind::Flick => self.flick,
            TestKind::Tracking => self.tracking,
            TestKind::Micro => self.micro,
            TestKind::Recoil => self.recoil,
            TestKind::Strafe => 0.0,
        };
        if w.is_finite() {
            w.max(0.0)
        } else {
            0.0
        }
    }
}

/// One test's estimate of the sensitivity it favours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestEstimate {
    pub kind: TestKind,
    pub crossover: Crossover,
}

/// How many standard deviations apart two tests must be to count as disagreeing. Five scored
/// tests make ten pairs, so 2σ would flag about a quarter of players whose tests all agree;
/// 2.8σ keeps that near 2% in simulation and still flags a 2× gap ~95% of the time.
const CONFLICT_Z: f64 = 2.8;

/// Floor on a test's standard deviation in ln(cm/360) when weighting, so one lucky fit can't
/// take all the weight.
const MIN_SIGMA: f64 = 0.01;

/// Each test's estimate in ln(cm/360) with the covariance between them. Estimates from one
/// joint fit share parameters and are correlated; the combination accounts for that.
#[derive(Clone, Debug, PartialEq)]
pub struct EstimateSet {
    kinds: Vec<TestKind>,
    ln_cm: Vec<f64>,
    cov: Vec<Vec<f64>>,
    /// From a joint fit: each test's offset (logit units) and their covariance. How
    /// differently two tests' misses lean at the sensitivities actually tested; unlike the
    /// gap in cm it doesn't depend on extrapolating past the tested range.
    offsets: Option<(Vec<f64>, Vec<Vec<f64>>)>,
}

/// A combined sensitivity with the half-width of its ~95% interval as a fraction (0.1 = ±10%),
/// the same scale as `Crossover::rel_uncertainty`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Combined {
    pub cm_360: f64,
    pub rel_uncertainty: f64,
}

impl EstimateSet {
    pub fn new(kinds: Vec<TestKind>, ln_cm: Vec<f64>, cov: Vec<Vec<f64>>) -> Self {
        Self { kinds, ln_cm, cov, offsets: None }
    }

    /// Attach the joint fit's per-test offsets, which `conflicts` then compares.
    pub fn with_offsets(mut self, offsets: Vec<f64>, cov: Vec<Vec<f64>>) -> Self {
        self.offsets = Some((offsets, cov));
        self
    }

    /// Estimates with nothing in common (separate fits): a diagonal covariance.
    pub fn from_independent(estimates: &[TestEstimate]) -> Self {
        let valid: Vec<&TestEstimate> = estimates
            .iter()
            .filter(|e| e.crossover.cm_360 > 0.0 && e.crossover.cm_360.is_finite())
            .collect();
        let n = valid.len();
        let mut cov = vec![vec![0.0; n]; n];
        for (i, e) in valid.iter().enumerate() {
            cov[i][i] = (e.crossover.rel_uncertainty / 1.96).powi(2);
        }
        Self {
            kinds: valid.iter().map(|e| e.kind).collect(),
            ln_cm: valid.iter().map(|e| e.crossover.cm_360.ln()).collect(),
            cov,
            offsets: None,
        }
    }

    pub fn estimates(&self) -> Vec<TestEstimate> {
        (0..self.kinds.len())
            .map(|i| TestEstimate {
                kind: self.kinds[i],
                crossover: Crossover {
                    cm_360: self.ln_cm[i].exp(),
                    rel_uncertainty: 1.96 * self.cov[i][i].max(0.0).sqrt(),
                },
            })
            .collect()
    }

    fn sigma(&self, i: usize) -> f64 {
        self.cov[i][i].max(0.0).sqrt().max(MIN_SIGMA)
    }

    /// Weighted mean in log space with `w_i = k_i / σ_i²`; its uncertainty uses the full
    /// covariance. `None` when no test with a weight above zero has an estimate.
    pub fn combine(&self, weights: &WeightSet) -> Option<Combined> {
        let raw: Vec<f64> = (0..self.kinds.len())
            .map(|i| weights.get(self.kinds[i]) / self.sigma(i).powi(2))
            .collect();
        let total: f64 = raw.iter().sum();
        if !(total > 0.0) {
            return None;
        }
        let w: Vec<f64> = raw.iter().map(|r| r / total).collect();
        let mean: f64 = w.iter().zip(&self.ln_cm).map(|(a, b)| a * b).sum();
        let mut var = 0.0;
        for i in 0..w.len() {
            for j in 0..w.len() {
                var += w[i] * w[j] * self.cov[i][j];
            }
        }
        Some(Combined {
            cm_360: mean.exp(),
            rel_uncertainty: 1.96 * var.max(0.0).sqrt(),
        })
    }

    /// Pairs of counted tests that disagree by more than `CONFLICT_Z` σ of their difference, in
    /// `TestKind::ALL` order. Compares the joint fit's offsets when there are any, otherwise
    /// the estimates in cm.
    pub fn conflicts(&self, weights: &WeightSet) -> Vec<(TestKind, TestKind)> {
        let (vals, cov) = match &self.offsets {
            Some((v, c)) => (v, c),
            None => (&self.ln_cm, &self.cov),
        };
        let mut idx: Vec<usize> = (0..self.kinds.len())
            .filter(|&i| weights.get(self.kinds[i]) > 0.0)
            .collect();
        idx.sort_by_key(|&i| self.kinds[i].index());
        let mut out = Vec::new();
        for (n, &a) in idx.iter().enumerate() {
            for &b in &idx[n + 1..] {
                let gap = (vals[a] - vals[b]).abs();
                let var = cov[a][a] + cov[b][b] - 2.0 * cov[a][b];
                let noise = var.max(0.0).sqrt().max(MIN_SIGMA);
                if gap > CONFLICT_Z * noise {
                    out.push((self.kinds[a], self.kinds[b]));
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    /// The result with neutral weights. Always shown.
    pub neutral: Combined,
    /// The result with the game preset's weights, kept within one σ of `neutral`.
    pub preset: Option<Combined>,
    /// Counted tests that disagree (neutral weights decide which tests count).
    pub conflicts: Vec<(TestKind, TestKind)>,
}

impl Recommendation {
    /// The value to use: the preset's when there is one, otherwise the neutral one.
    pub fn chosen(&self) -> Combined {
        self.preset.unwrap_or(self.neutral)
    }
}

/// Neutral result, the preset's result (if a preset is given and it counts any test), and
/// the conflicts. A preset only leans the result between tests that disagree: it may move at
/// most one standard deviation of the neutral result away from it.
pub fn recommend(set: &EstimateSet, preset: Option<&WeightSet>) -> Option<Recommendation> {
    recommend_with(set, &WeightSet::neutral(), preset)
}

/// `recommend` on `base` weights instead of the neutral ones; a test `base` leaves out stays
/// out of the preset's result too. Conflicts still compare every counted test.
pub fn recommend_with(set: &EstimateSet, base: &WeightSet, preset: Option<&WeightSet>) -> Option<Recommendation> {
    let neutral_weights = WeightSet::neutral();
    let neutral = set.combine(base)?;
    let left_out = |w: &WeightSet| {
        TestKind::ALL.iter().filter(|&&k| base.get(k) == 0.0).fold(*w, |w, &k| w.without(k))
    };
    let preset = preset.and_then(|w| set.combine(&left_out(w))).map(|p| {
        let sigma = neutral.rel_uncertainty / 1.96;
        let ln_n = neutral.cm_360.ln();
        Combined {
            cm_360: p.cm_360.ln().clamp(ln_n - sigma, ln_n + sigma).exp(),
            rel_uncertainty: p.rel_uncertainty,
        }
    });
    Some(Recommendation {
        neutral,
        preset,
        conflicts: set.conflicts(&neutral_weights),
    })
}
