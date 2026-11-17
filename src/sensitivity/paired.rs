//! Judging a test by comparing two sensitivities against each other.
//!
//! The first micro-adjustment and tracking judges used fixed thresholds, and real players
//! landed on one side of them at every sensitivity: everyone trails a moving target by their
//! reaction delay, and everyone's first move toward a small target falls a little short. A
//! pair played at a slightly faster and a slightly slower sensitivity carries those habits on
//! both sides, so they cancel; what is left is which side the player does better on.

use serde::{Deserialize, Serialize};

use super::combiner::Signal;
use crate::game::tasks::flick::FlickBand;

/// The faster side plays at `cm / STEP`, the slower at `cm × STEP`.
pub const STEP: f64 = 1.12;
/// Costs closer than this share of their mean are a tie.
pub const TIE_SHARE: f64 = 0.03;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Faster,
    Slower,
}

impl Side {
    /// Multiplier on the tested cm/360.
    pub fn factor(self) -> f32 {
        match self {
            Side::Faster => (1.0 / STEP) as f32,
            Side::Slower => STEP as f32,
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Faster => Side::Slower,
            Side::Slower => Side::Faster,
        }
    }
}

/// `A B B A` repeated, `A` = `first`: warming up or tiring within a segment hits both sides alike.
pub fn abba(first: Side, len: usize) -> Vec<Side> {
    let pattern = [first, first.other(), first.other(), first];
    (0..len).map(|i| pattern[i % 4]).collect()
}

/// Lower cost is better. The slower side doing better says the tested sensitivity is too fast.
pub fn judge(faster_cost: f64, slower_cost: f64) -> Signal {
    let tie = TIE_SHARE * (faster_cost + slower_cost) * 0.5;
    if faster_cost - slower_cost > tie {
        Signal::TooFast
    } else if slower_cost - faster_cost > tie {
        Signal::TooSlow
    } else {
        Signal::Direct
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairResult {
    pub faster_cost: f64,
    pub slower_cost: f64,
    pub signal: Signal,
    /// Flick distance band the pair was played at (flick only).
    #[serde(default)]
    pub band: Option<FlickBand>,
}

/// Slots `(0, 1)`, `(2, 3)`, … of an ordered list of sides form pairs. A pair finishes once
/// both its slots have a cost; a slot without one (`None`) drops its pair.
#[derive(Clone, Debug)]
pub struct PairBook {
    order: Vec<Side>,
    costs: Vec<Option<Option<f64>>>,
    reported: Vec<bool>,
    bands: Vec<Option<FlickBand>>,
}

impl PairBook {
    pub fn new(order: Vec<Side>) -> Self {
        let n = order.len();
        Self { order, costs: vec![None; n], reported: vec![false; n / 2], bands: vec![None; n / 2] }
    }

    /// Tag pair `i` with `bands[i]`; its result carries the band.
    pub fn with_bands(mut self, bands: Vec<FlickBand>) -> Self {
        for (slot, b) in self.bands.iter_mut().zip(bands) {
            *slot = Some(b);
        }
        self
    }

    pub fn side(&self, slot: usize) -> Option<Side> {
        self.order.get(slot).copied()
    }

    pub fn record(&mut self, slot: usize, cost: Option<f64>) {
        if let Some(c) = self.costs.get_mut(slot) {
            *c = Some(cost);
        }
    }

    pub fn take_finished(&mut self) -> Vec<PairResult> {
        let mut out = Vec::new();
        for p in 0..self.reported.len() {
            let (a, b) = (2 * p, 2 * p + 1);
            let (Some(ca), Some(cb)) = (self.costs[a], self.costs[b]) else { continue };
            if self.reported[p] {
                continue;
            }
            self.reported[p] = true;
            let (Some(ca), Some(cb)) = (ca, cb) else { continue };
            let (faster_cost, slower_cost) = if self.order[a] == Side::Faster { (ca, cb) } else { (cb, ca) };
            out.push(PairResult {
                faster_cost,
                slower_cost,
                signal: judge(faster_cost, slower_cost),
                band: self.bands[p],
            });
        }
        out
    }
}
