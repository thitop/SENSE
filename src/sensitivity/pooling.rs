//! Combining recent calibration runs into one result.
//!
//! One run gives a result within about ±13–19% (95%); two runs on the same day landed 12%
//! apart in real play. The runs measure the same thing, so their per-round counts are fitted
//! together, as if they were one longer run.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::blocks::{confidence_of, excluded_tests_in, trusted_recommendation};
use super::combiner::{TestEstimate, TestKind};
use super::joint_fit::fit_joint;

/// Runs older than this aren't pooled: a hand's preference can drift.
pub const POOL_DAYS: i64 = 14;
/// At most this many of the newest runs are pooled.
pub const MAX_POOLED: usize = 5;
/// The sensitivity range a calibration searches, cm/360.
pub const SEARCH_CM: (f64, f64) = (15.0, 80.0);

/// What one finished run measured: per round and test, `(test, cm/360, too fast, too slow)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub at: DateTime<Utc>,
    pub dpi: f64,
    pub rows: Vec<(TestKind, f64, usize, usize)>,
}

/// The newest runs at `dpi` from the last `POOL_DAYS` before `now` that found an answer on
/// their own, at most `MAX_POOLED`, newest first. A run whose misses leaned one way at every
/// sensitivity has no crossover, and in real play such runs broke every pool they joined; so
/// did a run whose answer was wider than the whole search (see `is_answer`).
pub fn runs_to_pool(runs: &[RunRecord], now: DateTime<Utc>, dpi: f64) -> Vec<&RunRecord> {
    let mut picked: Vec<&RunRecord> = runs
        .iter()
        .filter(|r| r.dpi == dpi && r.at <= now && now - r.at <= Duration::days(POOL_DAYS))
        .filter(|r| pool(&[r]).is_some_and(|p| is_answer(p.rel_uncertainty)))
        .collect();
    picked.sort_by(|a, b| b.at.cmp(&a.at));
    picked.truncate(MAX_POOLED);
    picked
}

/// Half-width (~95%, in ln cm/360) beyond which a result is no answer: the search range,
/// 15–80 cm/360, is only ±0.84 around its middle, so a wider interval says nothing about where
/// the answer lies. In real play a ±159% run pooled with three others left them all at 5%.
pub const MAX_ANSWER_UNCERTAINTY: f64 = 0.84;

/// Whether a result this uncertain (`rel_uncertainty`, ~95% half-width) is an answer at all.
pub fn is_answer(rel_uncertainty: f64) -> bool {
    rel_uncertainty <= MAX_ANSWER_UNCERTAINTY
}

/// Chi-square at 99% for 1..=4 degrees of freedom (up to `MAX_POOLED` runs).
const CHI2_99: [f64; 4] = [6.63, 9.21, 11.34, 13.28];

/// Whether `runs` measured the same thing: each run's own result is within what its
/// uncertainty allows of the others' (a chi-square test at 99%). In real play four runs in
/// half an hour went 63 → 52 → 46 → 31 cm/360; fitted together they said almost nothing.
pub fn runs_agree(runs: &[&RunRecord]) -> bool {
    let fits: Vec<(f64, f64)> = runs
        .iter()
        .filter_map(|r| pool(&[r]))
        .map(|p| (p.cm_360.ln(), (p.rel_uncertainty / 1.96).max(1e-6)))
        .collect();
    if fits.len() < 2 {
        return true;
    }
    let weights: Vec<f64> = fits.iter().map(|&(_, sd)| 1.0 / (sd * sd)).collect();
    let mean = fits.iter().zip(&weights).map(|(&(x, _), w)| x * w).sum::<f64>() / weights.iter().sum::<f64>();
    let chi2: f64 = fits.iter().zip(&weights).map(|(&(x, _), w)| w * (x - mean).powi(2)).sum();
    chi2 <= CHI2_99[(fits.len() - 2).min(CHI2_99.len() - 1)]
}

/// The newest of `picked` (newest first, as `runs_to_pool` gives them) that agree with each
/// other: runs are added from the newest back while they all still agree. A hand that is
/// still settling in drifts, and the newest runs say most about it now.
pub fn newest_agreeing<'a>(picked: &[&'a RunRecord]) -> Vec<&'a RunRecord> {
    let mut n = picked.len().min(1);
    while n < picked.len() && runs_agree(&picked[..=n]) {
        n += 1;
    }
    picked[..n].to_vec()
}

/// Drop runs too old to pool, and keep the list short.
pub fn keep_recent(runs: &mut Vec<RunRecord>, now: DateTime<Utc>) {
    runs.retain(|r| now - r.at <= Duration::days(POOL_DAYS));
    runs.sort_by(|a, b| b.at.cmp(&a.at));
    runs.truncate(2 * MAX_POOLED);
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pooled {
    pub cm_360: f64,
    pub rel_uncertainty: f64,
    pub confidence: f32,
    pub estimates: Vec<TestEstimate>,
    pub conflicts: Vec<(TestKind, TestKind)>,
    pub left_out: Vec<TestKind>,
    pub runs: usize,
}

/// One fit over every run's rows, under the same rules as a single run's result.
pub fn pool(runs: &[&RunRecord]) -> Option<Pooled> {
    let rows: Vec<(TestKind, f64, usize, usize)> = runs.iter().flat_map(|r| r.rows.iter().copied()).collect();
    let set = fit_joint(&rows)?;
    let tested: Vec<f64> = rows.iter().map(|r| r.1).collect();
    let rec = trusted_recommendation(&set, &tested, None)?;
    let cm_360 = rec.neutral.cm_360.clamp(SEARCH_CM.0, SEARCH_CM.1);
    let left_out = excluded_tests_in(&set, &tested);
    Some(Pooled {
        cm_360,
        rel_uncertainty: rec.neutral.rel_uncertainty,
        confidence: confidence_of(Some(&rec), cm_360, SEARCH_CM, !left_out.is_empty()),
        estimates: set.estimates(),
        conflicts: rec.conflicts,
        left_out,
        runs: runs.len(),
    })
}
