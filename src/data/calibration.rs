use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::sensitivity::blocks::{RoundTrace, SignalCounts};
use crate::sensitivity::combiner::Signal;
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
    /// What each test said in each round of a multi-test run, to see which test drove the
    /// result. Missing from flick-only results and results saved before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<RunDiagnostics>,
    /// How many recent runs the numbers above combine (1 = this run alone).
    #[serde(default = "one_run")]
    pub pooled_runs: usize,
    /// No test crossed over: the misses leaned the same way at every sensitivity tested, so
    /// `cm_360` is only where the search stopped, not a measurement.
    #[serde(default)]
    pub no_answer: bool,
    /// Recent runs couldn't be fitted together; the numbers are this run's alone.
    #[serde(default)]
    pub runs_disagree: bool,
    /// Aim habits found in this run's play; absent in older results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habits: Option<crate::habits::HabitReport>,
}

fn one_run() -> usize {
    1
}

impl CalibrationResult {
    /// Take the result, confidence and per-test estimates from `p`, which pools this run with
    /// recent ones. The rounds and shots stay this run's (the graphs show the latest run).
    pub fn apply_pool(&mut self, p: &crate::sensitivity::pooling::Pooled) {
        self.cm_360 = p.cm_360;
        self.psa_cm = Some(p.cm_360);
        self.confidence = p.confidence;
        self.pooled_runs = p.runs;
        self.no_answer = false;
        self.runs_disagree = false;
        if let Some(d) = &mut self.diagnostics {
            let bands: Vec<EstimateRecord> = d.estimates.drain(..).filter(|e| e.test.starts_with("Flick ")).collect();
            d.estimates = p
                .estimates
                .iter()
                .map(|e| EstimateRecord {
                    test: e.kind.label().to_string(),
                    cm_360: e.crossover.cm_360,
                    rel_uncertainty: e.crossover.rel_uncertainty,
                })
                .chain(bands)
                .collect();
            d.conflicts = p.conflicts.iter().map(|(a, b)| (a.label().to_string(), b.label().to_string())).collect();
            d.left_out = p.left_out.iter().map(|k| k.label().to_string()).collect();
            d.result_rel_uncertainty = Some(p.rel_uncertainty);
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RunDiagnostics {
    pub rounds: Vec<RoundSignals>,
    /// Each test's own estimate where its misses allowed one, flick distances included.
    pub estimates: Vec<EstimateRecord>,
    /// Pairs of tests that disagree beyond chance.
    pub conflicts: Vec<(String, String)>,
    /// The combined fit with every test, before biased-looking tests were left out.
    pub fit_cm_360: Option<f64>,
    /// Tests left out of the result: they leaned one way at every tested sensitivity.
    #[serde(default)]
    pub left_out: Vec<String>,
    /// Half-width of the ~95% interval of the result, as a fraction of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_rel_uncertainty: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RoundSignals {
    pub tested_cm_360: f64,
    /// By test ("Flick", "Micro", "Tracking") and flick distance ("Flick far", ...).
    pub signals: BTreeMap<String, SignalCounts>,
    /// Micro and tracking pairs played this round, in order.
    #[serde(default)]
    pub pairs: Vec<PairRecord>,
    /// The range this round started from, the best guess after it and how the next pick was made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<RoundTrace>,
}

/// One pair of a paired test: its cost at the faster and the slower side (lower is better).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PairRecord {
    pub test: String,
    /// Flick distance ("short", "mid", "far"); none for micro and tracking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    pub faster_cost: f64,
    pub slower_cost: f64,
    pub signal: Signal,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EstimateRecord {
    pub test: String,
    pub cm_360: f64,
    /// Half-width of the ~95% interval as a fraction of `cm_360`.
    pub rel_uncertainty: f64,
}
