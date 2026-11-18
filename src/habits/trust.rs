//! How far a calibration's habits can be trusted: whether the player was still warming up, and
//! which left-out tests the habits explain.

use serde::{Deserialize, Serialize};

use crate::data::calibration::{CalibrationResult, RoundSignals, RunDiagnostics};
use crate::habits::analysis::{ranked_calibration_habits, Habit, HabitFinding, CAL_MAX_FINDINGS};
use crate::habits::answers::{relates_to, verdict_for, AnswerCheck};
use crate::habits::record::AimLog;
use crate::sensitivity::combiner::{Signal, TestKind};

/// A test left out of the result whose lean a named habit explains.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeftOutNote {
    pub test: String,
    pub habit: Habit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct HabitReport {
    pub findings: Vec<HabitFinding>,
    pub no_answer: bool,
    pub warming_up: bool,
    pub left_out: Vec<LeftOutNote>,
    #[serde(default)]
    pub answer_checks: Vec<AnswerCheck>,
}

const MIN_POINTS: usize = 6;
const FALL_OVER_RUN: f64 = -0.15;

/// Two-sided 95% t quantile for `dof` degrees of freedom.
fn t95(dof: usize) -> f64 {
    const T: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    match dof {
        0 => f64::INFINITY,
        1..=30 => T[dof - 1],
        _ => 1.96,
    }
}

/// Solve `a x = b` (4x4) by Gaussian elimination with partial pivoting; `None` when singular.
fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> Option<[f64; 4]> {
    let scale = a.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()));
    if !(scale > 0.0) {
        return None;
    }
    for c in 0..4 {
        let p = (c..4).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-10 * scale {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..4 {
            let f = a[r][c] / a[c][c];
            for k in c..4 {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; 4];
    for r in (0..4).rev() {
        let s: f64 = (r + 1..4).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Does this test's cost fall round over round beyond what the sensitivity searched explains?
/// Points are `(round index, ln tested cm, normalised mean cost)`.
fn test_falls(points: &[(f64, f64, f64)]) -> bool {
    let n = points.len();
    if n < MIN_POINTS {
        return false;
    }
    let mean_x = points.iter().map(|p| p.1).sum::<f64>() / n as f64;
    let rows: Vec<[f64; 4]> = points
        .iter()
        .map(|p| {
            let x = p.1 - mean_x;
            [1.0, x, x * x, p.0]
        })
        .collect();
    let mut xtx = [[0.0; 4]; 4];
    let mut xty = [0.0; 4];
    for (row, p) in rows.iter().zip(points) {
        for i in 0..4 {
            xty[i] += row[i] * p.2;
            for j in 0..4 {
                xtx[i][j] += row[i] * row[j];
            }
        }
    }
    let Some(beta) = solve4(xtx, xty) else { return false };
    let Some(inv_col) = solve4(xtx, [0.0, 0.0, 0.0, 1.0]) else { return false };
    let rss: f64 = rows
        .iter()
        .zip(points)
        .map(|(row, p)| {
            let fit: f64 = (0..4).map(|i| row[i] * beta[i]).sum();
            (p.2 - fit).powi(2)
        })
        .sum();
    let dof = n - 4;
    let se = (rss / dof as f64 * inv_col[3]).max(0.0).sqrt();
    let d = beta[3];
    let span = points[n - 1].0 - points[0].0;
    d * span < FALL_OVER_RUN && d + t95(dof) * se < 0.0
}

/// The player was still getting better during the run: in at least two tests, cost falls from
/// round to round after allowing for the sensitivity being searched (a U in ln cm).
pub fn still_warming_up(rounds: &[RoundSignals]) -> bool {
    let mut falling = 0;
    for label in [TestKind::Flick.label(), TestKind::Micro.label(), TestKind::Tracking.label()] {
        let costs = |r: &RoundSignals| -> Vec<f64> {
            r.pairs
                .iter()
                .filter(|p| p.test == label)
                .map(|p| (p.faster_cost + p.slower_cost) / 2.0)
                .filter(|c| c.is_finite())
                .collect()
        };
        let all: Vec<f64> = rounds.iter().flat_map(|r| costs(r)).collect();
        if all.is_empty() {
            continue;
        }
        let overall = all.iter().sum::<f64>() / all.len() as f64;
        if !(overall > 0.0) {
            continue;
        }
        let points: Vec<(f64, f64, f64)> = rounds
            .iter()
            .enumerate()
            .filter(|(_, r)| r.tested_cm_360 > 0.0)
            .filter_map(|(i, r)| {
                let c = costs(r);
                (!c.is_empty()).then(|| {
                    (i as f64, r.tested_cm_360.ln(), c.iter().sum::<f64>() / c.len() as f64 / overall)
                })
            })
            .collect();
        if test_falls(&points) {
            falling += 1;
        }
    }
    falling >= 2
}

/// For each test left out of the result, the first named habit that leans the same way the
/// test's own estimate did (beyond the sensitivities tested).
pub fn left_out_notes(diag: &RunDiagnostics, findings: &[HabitFinding]) -> Vec<LeftOutNote> {
    let tested = diag.rounds.iter().map(|r| r.tested_cm_360).filter(|c| c.is_finite());
    let max = tested.clone().fold(f64::NEG_INFINITY, f64::max);
    let min = tested.fold(f64::INFINITY, f64::min);
    let mut notes = Vec::new();
    for test in &diag.left_out {
        let Some(est) = diag.estimates.iter().find(|e| &e.test == test) else { continue };
        let lean = if est.cm_360 > max {
            Signal::TooFast
        } else if est.cm_360 < min {
            Signal::TooSlow
        } else {
            continue;
        };
        if let Some(f) =
            findings.iter().find(|f| f.habit.test_label() == test && f.habit.lean() == Some(lean))
        {
            notes.push(LeftOutNote { test: test.clone(), habit: f.habit });
        }
    }
    notes
}

/// The habits of a calibration run, built from the run's own (unpooled) result.
pub fn habit_report(log: &AimLog, result: &CalibrationResult) -> HabitReport {
    let warming_up = result.diagnostics.as_ref().map_or(false, |d| still_warming_up(&d.rounds));
    if result.no_answer {
        return HabitReport {
            findings: Vec::new(),
            no_answer: true,
            warming_up,
            left_out: Vec::new(),
            answer_checks: Vec::new(),
        };
    }
    // Notes and answer checks look at every habit found, not just the few that fit on screen.
    let mut ranked = ranked_calibration_habits(log, result.cm_360);
    let left_out =
        result.diagnostics.as_ref().map(|d| left_out_notes(d, &ranked)).unwrap_or_default();
    let problems: &[_] = result.setup.as_ref().map_or(&[], |s| s.problems.as_slice());
    let habits: Vec<Habit> = ranked.iter().map(|f| f.habit).collect();
    let answer_checks: Vec<AnswerCheck> = problems
        .iter()
        .filter_map(|&p| verdict_for(p, &habits).map(|verdict| AnswerCheck { problem: p, verdict }))
        .collect();
    // Habits that match or oppose an answer come first (stable), then the cut to what shows.
    ranked.sort_by_key(|f| !problems.iter().any(|&p| relates_to(p, f.habit)));
    ranked.truncate(CAL_MAX_FINDINGS);
    HabitReport { findings: ranked, no_answer: false, warming_up, left_out, answer_checks }
}
