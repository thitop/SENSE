//! One logistic fit over every test's misses: a shared slope with a per-test deviation, and an
//! offset per test, both gently pulled toward the others. Tests lean at different rates (a
//! paired comparison flips gently, flicks steeply); with one slope for all, a test's
//! crossover slides toward the side of the tested range that holds more of its data. A test with few misses then borrows strength from the rest
//! instead of failing its own fit, while a test with plenty of data can still sit apart (a
//! player whose flicks and tracking prefer different sensitivities).

use super::combiner::{EstimateSet, TestKind};

/// Same small penalty on the slope as the single-test fit: keeps it finite when blocks
/// separate perfectly.
const SLOPE_PENALTY: f64 = 0.05;
/// Pull of each test's offset toward the shared one, in logit units (prior sd = 1/√λ = 2).
/// Weak enough that a real ~40% gap between two tests still shows.
const OFFSET_PENALTY: f64 = 0.25;
/// Pull of each test's slope toward the shared one (prior sd = 1 logit per unit of ln cm).
/// Weaker pulls (0.05, 0.3) let noisy tests wander and cost accuracy in simulation (5.3% →
/// 6.4% / 5.8% median error); 1.0 keeps it at ~5.4% and still separates a steep test from a
/// shallow one.
const SLOPE_DEVIATION_PENALTY: f64 = 1.0;

/// Fit P(too fast | miss) = σ(c + d_test + (b + e_test)·(ln cm − mean)) to rows of
/// `(test, cm/360, too_fast, too_slow)`, and return each test's 50% point in ln(cm/360) with
/// their joint covariance. `None` without misses on both sides overall, with fewer than two
/// tested sensitivities, or when misses don't lean less "too fast" as the sensitivity slows.
pub fn fit_joint(rows: &[(TestKind, f64, usize, usize)]) -> Option<EstimateSet> {
    let g = fit_groups(rows)?;
    Some(EstimateSet::new(g.keys, g.ln_cm, g.cov).with_offsets(g.offsets, g.offset_cov))
}

/// The joint fit over any grouping (tests, flick distance bands, ...).
#[derive(Clone, Debug, PartialEq)]
pub struct GroupFit<K> {
    /// Groups in order of first appearance in the rows.
    pub keys: Vec<K>,
    /// Each group's 50% point in ln(cm/360).
    pub ln_cm: Vec<f64>,
    /// Covariance of `ln_cm`.
    pub cov: Vec<Vec<f64>>,
    /// Each group's offset in logit units, and their covariance.
    pub offsets: Vec<f64>,
    pub offset_cov: Vec<Vec<f64>>,
}

impl<K: Copy + PartialEq> GroupFit<K> {
    /// Whether groups `a` and `b` lean differently at the tested sensitivities by more than
    /// `z` standard deviations of the difference. `false` if either group is missing.
    pub fn differ(&self, a: K, b: K, z: f64) -> bool {
        let (Some(i), Some(j)) = (self.keys.iter().position(|&k| k == a), self.keys.iter().position(|&k| k == b))
        else {
            return false;
        };
        let var = self.offset_cov[i][i] + self.offset_cov[j][j] - 2.0 * self.offset_cov[i][j];
        (self.offsets[i] - self.offsets[j]).abs() > z * var.max(0.0).sqrt()
    }
}

/// `fit_joint` for any group key.
pub fn fit_groups<K: Copy + PartialEq>(rows: &[(K, f64, usize, usize)]) -> Option<GroupFit<K>> {
    let mut kinds: Vec<K> = Vec::new();
    let data: Vec<(usize, f64, f64, f64)> = rows
        .iter()
        .filter(|r| r.1 > 0.0 && r.1.is_finite() && r.2 + r.3 > 0)
        .map(|&(k, cm, f, s)| {
            let j = kinds.iter().position(|&x| x == k).unwrap_or_else(|| {
                kinds.push(k);
                kinds.len() - 1
            });
            (j, cm.ln(), f as f64, (f + s) as f64)
        })
        .collect();
    let overs: f64 = data.iter().map(|d| d.2).sum();
    let misses: f64 = data.iter().map(|d| d.3).sum();
    let distinct_x = {
        let mut xs: Vec<f64> = data.iter().map(|d| d.1).collect();
        xs.sort_by(|a, b| a.total_cmp(b));
        xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        xs.len()
    };
    if distinct_x < 2 || overs == 0.0 || overs == misses {
        return None;
    }
    let mean_x = data.iter().map(|d| d.1 * d.3).sum::<f64>() / misses;
    let m = kinds.len();
    // θ = [c, b, d_0 .. d_{m-1}, e_0 .. e_{m-1}]
    let p = 2 * m + 2;
    let e_at = |j: usize| 2 + m + j;
    let mut theta = vec![0.0f64; p];
    let mut info = vec![vec![0.0f64; p]; p];

    for _ in 0..100 {
        let mut grad = vec![0.0f64; p];
        for row in info.iter_mut() {
            row.iter_mut().for_each(|v| *v = 0.0);
        }
        info[0][0] = 1e-9;
        info[1][1] = SLOPE_PENALTY;
        grad[1] = -SLOPE_PENALTY * theta[1];
        for j in 0..m {
            info[2 + j][2 + j] = OFFSET_PENALTY;
            grad[2 + j] = -OFFSET_PENALTY * theta[2 + j];
            info[e_at(j)][e_at(j)] = SLOPE_DEVIATION_PENALTY;
            grad[e_at(j)] = -SLOPE_DEVIATION_PENALTY * theta[e_at(j)];
        }
        for &(j, x, y, n) in &data {
            let xc = x - mean_x;
            let eta = theta[0] + (theta[1] + theta[e_at(j)]) * xc + theta[2 + j];
            let pr = 1.0 / (1.0 + (-eta).exp());
            let r = y - n * pr;
            let w = n * pr * (1.0 - pr);
            let z = [(0usize, 1.0), (1, xc), (2 + j, 1.0), (e_at(j), xc)];
            for &(a, za) in &z {
                grad[a] += r * za;
                for &(b, zb) in &z {
                    info[a][b] += w * za * zb;
                }
            }
        }
        let step = solve(&info, &grad)?;
        let mut done = true;
        for (t, s) in theta.iter_mut().zip(&step) {
            *t += s;
            done &= s.abs() < 1e-9;
        }
        if done {
            break;
        }
    }
    let (c, b) = (theta[0], theta[1]);
    // Faster (smaller cm) must mean more "too fast".
    if !(b < 0.0) || theta.iter().any(|t| !t.is_finite()) {
        return None;
    }

    let cov = invert(&info)?;
    // A test whose own slope barely falls with the sensitivity has no crossover to speak of:
    // dividing by a slope near zero sends it to absurd values (thousands of cm, or infinity,
    // which can't be saved). Such a test, or one whose crossover lands more than 8× away from
    // the tested sensitivities, sits at the centre with a variance that gives it no weight.
    // (4× also caught honest flick-distance bands with few misses.)
    const NO_INFORMATION: f64 = 1e6;
    const MIN_SLOPE: f64 = 0.25;
    let max_reach = 8f64.ln();
    let slope = |j: usize| b + theta[e_at(j)];
    let informative = |j: usize| {
        let s = slope(j);
        s < -MIN_SLOPE && ((c + theta[2 + j]) / s).abs() <= max_reach
    };
    // x*_j = mean − (c + d_j)/s_j with s_j = b + e_j; Jacobian rows over θ.
    let jac: Vec<Vec<f64>> = (0..m)
        .map(|j| {
            let mut g = vec![0.0; p];
            if informative(j) {
                let s = slope(j);
                g[0] = -1.0 / s;
                g[2 + j] = -1.0 / s;
                g[1] = (c + theta[2 + j]) / (s * s);
                g[e_at(j)] = (c + theta[2 + j]) / (s * s);
            }
            g
        })
        .collect();
    let ln_cm: Vec<f64> = (0..m)
        .map(|j| if informative(j) { mean_x - (c + theta[2 + j]) / slope(j) } else { mean_x })
        .collect();
    let est_cov: Vec<Vec<f64>> = (0..m)
        .map(|a| {
            (0..m)
                .map(|bb| match (informative(a), a == bb) {
                    (false, true) => NO_INFORMATION,
                    (false, false) => 0.0,
                    _ if !informative(bb) => 0.0,
                    _ => quad(&jac[a], &cov, &jac[bb]),
                })
                .collect()
        })
        .collect();
    let offsets: Vec<f64> = (0..m).map(|j| theta[2 + j]).collect();
    let offset_cov: Vec<Vec<f64>> = (0..m)
        .map(|a| (0..m).map(|bb| cov[2 + a][2 + bb]).collect())
        .collect();
    Some(GroupFit { keys: kinds, ln_cm, cov: est_cov, offsets, offset_cov })
}

/// uᵀ M v
fn quad(u: &[f64], m: &[Vec<f64>], v: &[f64]) -> f64 {
    u.iter()
        .enumerate()
        .map(|(i, ui)| ui * m[i].iter().zip(v).map(|(mij, vj)| mij * vj).sum::<f64>())
        .sum()
}

/// Solve A x = b by Gaussian elimination with partial pivoting.
fn solve(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a.iter().zip(b).map(|(row, &bi)| {
        let mut r = row.clone();
        r.push(bi);
        r
    }).collect();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        for row in 0..n {
            if row != col {
                let f = m[row][col] / m[col][col];
                if f != 0.0 {
                    for k in col..=n {
                        m[row][k] -= f * m[col][k];
                    }
                }
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

fn invert(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let cols: Option<Vec<Vec<f64>>> = (0..n)
        .map(|j| {
            let e: Vec<f64> = (0..n).map(|i| if i == j { 1.0 } else { 0.0 }).collect();
            solve(a, &e)
        })
        .collect();
    let cols = cols?;
    Some((0..n).map(|i| (0..n).map(|j| cols[j][i]).collect()).collect())
}
