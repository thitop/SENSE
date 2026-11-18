//! Multi-test calibration search. Each block tests one sensitivity with a short run of every
//! scored test, so all tests are measured at the same sensitivities and their estimates can be
//! combined.

use rand::seq::SliceRandom;
use rand::Rng;

use super::combiner::{recommend, recommend_with, EstimateSet, Recommendation, Signal, TestEstimate, TestKind, WeightSet};
use super::joint_fit::{fit_groups, fit_joint};
use super::psa::{Crossover, PsaRoundConclusion};
use crate::game::tasks::flick::FlickBand;

/// Short and far flicks count as preferring different sensitivities beyond this many σ.
/// One planned comparison, so the usual two-sided 5% level.
const DISTANCE_CONFLICT_Z: f64 = 1.96;

/// Per-band flick estimates from one joint fit over the bands.
#[derive(Clone, Debug, PartialEq)]
pub struct FlickBandFit {
    /// Bands with an estimate, in `FlickBand::ALL` order.
    pub estimates: Vec<(FlickBand, Crossover)>,
    /// Short and far flicks lean differently at the tested sensitivities: one sensitivity
    /// doesn't suit every flick distance for this player.
    pub distance_conflict: bool,
}

/// How long a segment of a block runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    Shots(u32),
    Seconds(f32),
    Sprays(u32),
    Peeks(u32),
}

/// One test run inside a block. Uncounted segments let the player adapt and record nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub kind: TestKind,
    pub amount: Amount,
    pub counted: bool,
}

impl Segment {
    fn counted(kind: TestKind, amount: Amount) -> Self {
        Self { kind, amount, counted: true }
    }
    fn practice(kind: TestKind, amount: Amount) -> Self {
        Self { kind, amount, counted: false }
    }
}

/// Played once at the player's current sensitivity before the first block; records nothing.
pub fn warmup_plan() -> Vec<Segment> {
    vec![
        Segment::practice(TestKind::Flick, Amount::Shots(10)),
        Segment::practice(TestKind::Micro, Amount::Shots(4)),
        Segment::practice(TestKind::Tracking, Amount::Seconds(8.0)),
        Segment::practice(TestKind::Recoil, Amount::Sprays(2)),
    ]
}

/// One block: two uncounted flick shots to get used to the new sensitivity, then every scored
/// test in a random order so none of them always gets the tired end of the block.
pub fn block_plan(rng: &mut impl Rng) -> Vec<Segment> {
    let mut tests = vec![
        // Flick is paired too: 9 pairs, 3 per distance band (`FlickTask::paired`).
        Segment::counted(TestKind::Flick, Amount::Shots(18)),
        // Micro and tracking are played as pairs at a faster and a slower sensitivity
        // (`sensitivity::paired`): 4 pairs of targets, two pairs of 3 s chunks.
        Segment::counted(TestKind::Micro, Amount::Shots(8)),
        Segment::counted(TestKind::Tracking, Amount::Seconds(12.0)),
        Segment::counted(TestKind::Recoil, Amount::Sprays(1)),
        Segment::counted(TestKind::Placement, Amount::Peeks(4)),
    ];
    tests.shuffle(rng);
    let mut plan = vec![Segment::practice(TestKind::Flick, Amount::Shots(2))];
    plan.extend(tests);
    plan
}

/// `warmup_plan` without tests that aren't built yet.
pub fn warmup_plan_for(available: &[TestKind]) -> Vec<Segment> {
    warmup_plan().into_iter().filter(|s| available.contains(&s.kind)).collect()
}

/// `block_plan` without tests that aren't built yet. The adaptation flicks always stay.
pub fn block_plan_for(available: &[TestKind], rng: &mut impl Rng) -> Vec<Segment> {
    block_plan(rng)
        .into_iter()
        .enumerate()
        .filter(|(i, s)| *i == 0 || available.contains(&s.kind))
        .map(|(_, s)| s)
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SignalCounts {
    pub too_fast: usize,
    pub too_slow: usize,
    pub direct: usize,
}

impl SignalCounts {
    fn add(&mut self, s: Signal) {
        match s {
            Signal::TooFast => self.too_fast += 1,
            Signal::TooSlow => self.too_slow += 1,
            Signal::Direct => self.direct += 1,
        }
    }
}

/// What the evidence said after a round.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RoundGuess {
    /// The combined best guess so far and its relative uncertainty.
    Guess { cm_360: f64, rel_uncertainty: f64 },
    /// Only one sensitivity tested so far: nothing to compare yet.
    OneSensitivity,
    /// Two or more sensitivities tested, but the misses all lean one way: no crossover.
    NoCrossover,
}

/// How the next sensitivity was chosen after a round.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NextPick {
    /// Bisection: the middle of the range.
    Midpoint { cm_360: f64 },
    /// The fit's own estimate, inside the reach.
    FitEstimate { cm_360: f64 },
    /// The fit pointed past the reach (`guess_cm`), so the pick stopped at its edge.
    FitClamped { guess_cm: f64, cm_360: f64 },
    /// The fit had no answer, so the pick is the middle of the range.
    FitFallbackMidpoint { cm_360: f64 },
    /// The search is over: nothing further is tested.
    Last,
}

/// One round as it played out: the range it started from, the guess after it and the next pick.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoundTrace {
    pub low_cm_360: f64,
    pub high_cm_360: f64,
    /// The range the next sensitivity was allowed to be picked within: the fit's reach when the
    /// fit picked it, else the bounds after this round. 0 in traces saved before it was kept.
    #[serde(default)]
    pub pick_low_cm_360: f64,
    #[serde(default)]
    pub pick_high_cm_360: f64,
    pub guess: RoundGuess,
    pub next: NextPick,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockResult {
    pub block_number: usize,
    pub tested_cm_360: f64,
    /// Indexed by `TestKind::index()`.
    pub per_test: [SignalCounts; 6],
    /// Flick counts split by distance, indexed by `FlickBand::index()`. Flicks recorded
    /// without a band count toward `per_test` only.
    pub flick_bands: [SignalCounts; 3],
    pub conclusion: PsaRoundConclusion,
    pub trace: RoundTrace,
}

impl BlockResult {
    pub fn counts(&self, kind: TestKind) -> SignalCounts {
        self.per_test[kind.index()]
    }

    pub fn band_counts(&self, band: FlickBand) -> SignalCounts {
        self.flick_bands[band.index()]
    }
}

#[derive(Clone, Debug)]
pub struct BlockScheduler {
    pub low_cm_360: f64,
    pub high_cm_360: f64,
    pub current_cm_360: f64,
    /// 1-based number of the block in progress.
    pub block_number: usize,
    pub max_blocks: usize,
    pub history: Vec<BlockResult>,
    pub is_completed: bool,
    /// Neutral-weight result, set when the search completes. Presets are applied on top with
    /// `recommendation`; the search itself never depends on them.
    pub recommended_cm_360: Option<f64>,
    /// The combined fit over every test when the search completed, before tests whose
    /// estimate lies outside the tested range were left out (`excluded_tests`).
    pub fit_cm_360: Option<f64>,
    search_min_cm: f64,
    search_max_cm: f64,
    /// The player's expected sensitivity. Kept for later use; simulations showed that starting
    /// the search from it was not more accurate than bisecting from the midpoint, so it is ignored.
    #[allow(dead_code)]
    start_cm: Option<f64>,
    current: [SignalCounts; 6],
    current_bands: [SignalCounts; 3],
}

impl BlockScheduler {
    /// Blocks that bisect the range before the search follows the combined fit.
    const BISECTION_BLOCKS: usize = 2;
    /// Once following the fit, a block only moves a bound to within 3% of the tested value.
    const ADAPTIVE_BOUND_MARGIN: f64 = 0.03;
    /// The fit may pick a test point this far outside the current bounds: a noisy early block
    /// can put a bound on the wrong side of the answer, and only the fit can notice.
    const FIT_REACH: f64 = 0.20;

    pub fn new(low_cm: f64, high_cm: f64, max_blocks: usize) -> Self {
        Self::with_start(low_cm, high_cm, None, max_blocks)
    }

    /// A search that may be given a start hint. The hint is stored but does not change the search.
    pub fn with_start(low_cm: f64, high_cm: f64, start_cm: Option<f64>, max_blocks: usize) -> Self {
        Self {
            low_cm_360: low_cm,
            high_cm_360: high_cm,
            current_cm_360: (low_cm + high_cm) * 0.5,
            block_number: 1,
            max_blocks,
            history: Vec::new(),
            is_completed: false,
            recommended_cm_360: None,
            fit_cm_360: None,
            search_min_cm: low_cm,
            search_max_cm: high_cm,
            start_cm,
            current: [SignalCounts::default(); 6],
            current_bands: [SignalCounts::default(); 3],
        }
    }

    /// Record one sample of the block in progress. Ignored once the search is complete.
    pub fn record(&mut self, kind: TestKind, signal: Signal) {
        if !self.is_completed {
            self.current[kind.index()].add(signal);
        }
    }

    /// Record one flick of the block in progress, with its distance band.
    pub fn record_flick(&mut self, band: FlickBand, signal: Signal) {
        if !self.is_completed {
            self.current[TestKind::Flick.index()].add(signal);
            self.current_bands[band.index()].add(signal);
        }
    }

    /// Per-band flick estimates over every finished block (`None` until the misses allow a fit).
    pub fn flick_bands(&self) -> Option<FlickBandFit> {
        let rows: Vec<(FlickBand, f64, usize, usize)> = self
            .history
            .iter()
            .flat_map(|b| {
                FlickBand::ALL.iter().map(move |&band| {
                    let c = b.band_counts(band);
                    (band, b.tested_cm_360, c.too_fast, c.too_slow)
                })
            })
            .collect();
        let fit = fit_groups(&rows)?;
        let estimates = FlickBand::ALL
            .iter()
            .filter_map(|&band| {
                let i = fit.keys.iter().position(|&k| k == band)?;
                Some((
                    band,
                    Crossover {
                        cm_360: fit.ln_cm[i].exp(),
                        rel_uncertainty: 1.96 * fit.cov[i][i].max(0.0).sqrt(),
                    },
                ))
            })
            .collect();
        Some(FlickBandFit {
            estimates,
            distance_conflict: fit.differ(FlickBand::Short, FlickBand::Far, DISTANCE_CONFLICT_Z),
        })
    }

    /// Counts of the block in progress.
    pub fn current_counts(&self, kind: TestKind) -> SignalCounts {
        self.current[kind.index()]
    }

    /// Every counted test's estimate over every finished block, from one joint fit.
    pub fn estimate_set(&self) -> Option<EstimateSet> {
        let weights = WeightSet::neutral();
        let rows: Vec<(TestKind, f64, usize, usize)> = self
            .history
            .iter()
            .flat_map(|b| {
                TestKind::ALL
                    .iter()
                    .filter(|&&k| weights.get(k) > 0.0)
                    .map(move |&k| {
                        let c = b.counts(k);
                        (k, b.tested_cm_360, c.too_fast, c.too_slow)
                    })
            })
            .collect();
        fit_joint(&rows)
    }

    /// Each counted test's estimate (empty until the misses allow a fit).
    pub fn estimates(&self) -> Vec<TestEstimate> {
        self.estimate_set().map(|s| s.estimates()).unwrap_or_default()
    }

    /// The combined result, with `preset` weights applied on top when given.
    pub fn recommendation(&self, preset: Option<&WeightSet>) -> Option<Recommendation> {
        trusted_recommendation(&self.estimate_set()?, &self.tested(), preset)
    }

    /// See `excluded_tests_in`.
    pub fn excluded_tests(&self) -> Vec<TestKind> {
        self.estimate_set().map(|s| excluded_tests_in(&s, &self.tested())).unwrap_or_default()
    }

    fn tested(&self) -> Vec<f64> {
        self.history.iter().map(|b| b.tested_cm_360).collect()
    }

    /// How much to trust the result, 0..1, on the same scale as `PsaCalibrator::confidence`.
    pub fn confidence(&self) -> f32 {
        let Some(r) = self.recommended_cm_360 else {
            return 0.0;
        };
        let rec = self.recommendation(None);
        confidence_of(rec.as_ref(), r, (self.search_min_cm, self.search_max_cm), !self.excluded_tests().is_empty())
    }

    /// Where the evidence so far puts the answer (neutral weights).
    /// Steers the search: every test counts, so a test pointing outside the tested range can
    /// send the next block there to check.
    fn fit_estimate(&self) -> Option<f64> {
        self.estimate_set().and_then(|s| recommend(&s, None)).map(|r| r.neutral.cm_360)
    }

    /// The combined best guess over the finished blocks, or why there is none yet.
    fn round_guess(&self) -> RoundGuess {
        let guess = self.estimate_set().and_then(|s| recommend(&s, None)).map(|r| r.neutral);
        if let Some(c) = guess {
            return RoundGuess::Guess { cm_360: c.cm_360, rel_uncertainty: c.rel_uncertainty };
        }
        let first = self.history[0].tested_cm_360;
        if self.history.iter().all(|b| b.tested_cm_360 == first) {
            RoundGuess::OneSensitivity
        } else {
            RoundGuess::NoCrossover
        }
    }

    /// Weighted vote of the block's misses: every judged miss counts once, times its test's
    /// weight. (Normalising per test let a single tracking pair outvote nine flicks in real
    /// play; that normalisation dated from tracking giving 16 windows per block.)
    fn block_vote(&self) -> f64 {
        let weights = WeightSet::neutral();
        TestKind::ALL
            .iter()
            .map(|&k| {
                let c = self.current[k.index()];
                weights.get(k) * (c.too_fast as f64 - c.too_slow as f64)
            })
            .sum()
    }

    /// Finish the block in progress and choose the next sensitivity (or complete the search).
    pub fn end_block(&mut self) -> PsaRoundConclusion {
        if self.is_completed {
            return self.history.last().map_or(PsaRoundConclusion::BalancedNarrow, |b| b.conclusion);
        }
        let adaptive = self.history.len() >= Self::BISECTION_BLOCKS;
        let tested = self.current_cm_360;
        let vote = self.block_vote();
        let (range_low, range_high) = (self.low_cm_360, self.high_cm_360);

        let conclusion = if vote > 0.0 {
            self.low_cm_360 = if adaptive {
                self.low_cm_360.max(tested * (1.0 - Self::ADAPTIVE_BOUND_MARGIN)).min(self.high_cm_360)
            } else {
                tested
            };
            PsaRoundConclusion::TooFastNeedSlower
        } else if vote < 0.0 {
            self.high_cm_360 = if adaptive {
                self.high_cm_360.min(tested * (1.0 + Self::ADAPTIVE_BOUND_MARGIN)).max(self.low_cm_360)
            } else {
                tested
            };
            PsaRoundConclusion::TooSlowNeedFaster
        } else {
            if !adaptive {
                let quarter = (self.high_cm_360 - self.low_cm_360) * 0.25;
                self.low_cm_360 += quarter;
                self.high_cm_360 -= quarter;
            }
            PsaRoundConclusion::BalancedNarrow
        };

        self.history.push(BlockResult {
            block_number: self.block_number,
            tested_cm_360: tested,
            per_test: self.current,
            flick_bands: self.current_bands,
            conclusion,
            // Filled in below, once the guess and the next pick are known.
            trace: RoundTrace {
                low_cm_360: range_low,
                high_cm_360: range_high,
                pick_low_cm_360: 0.0,
                pick_high_cm_360: 0.0,
                guess: RoundGuess::OneSensitivity,
                next: NextPick::Last,
            },
        });
        let guess = self.round_guess();
        self.current = [SignalCounts::default(); 6];
        self.current_bands = [SignalCounts::default(); 3];
        self.block_number += 1;

        // Every block runs: the session has a fixed length, and stopping when the bounds meet
        // would let one noisy block end the search.
        let midpoint = (self.low_cm_360 + self.high_cm_360) * 0.5;
        let bounds = (self.low_cm_360, self.high_cm_360);
        let (next, pick_range) = if self.block_number > self.max_blocks {
            self.is_completed = true;
            self.fit_cm_360 = self.fit_estimate();
            let estimate = self.recommendation(None).map(|r| r.neutral.cm_360).unwrap_or(midpoint);
            self.recommended_cm_360 = Some(estimate.clamp(self.search_min_cm, self.search_max_cm));
            (NextPick::Last, bounds)
        } else if self.history.len() >= Self::BISECTION_BLOCKS {
            let lo = (self.low_cm_360 * (1.0 - Self::FIT_REACH)).max(self.search_min_cm);
            let hi = (self.high_cm_360 * (1.0 + Self::FIT_REACH)).min(self.search_max_cm);
            let fit = self.fit_estimate();
            self.current_cm_360 = fit.unwrap_or(midpoint).clamp(lo, hi);
            let cm_360 = self.current_cm_360;
            let next = match fit {
                Some(g) if (lo..=hi).contains(&g) => NextPick::FitEstimate { cm_360 },
                Some(g) => NextPick::FitClamped { guess_cm: g, cm_360 },
                None => NextPick::FitFallbackMidpoint { cm_360 },
            };
            (next, (lo, hi))
        } else {
            self.current_cm_360 = midpoint;
            (NextPick::Midpoint { cm_360: midpoint }, bounds)
        };
        if let Some(last) = self.history.last_mut() {
            last.trace.guess = guess;
            last.trace.next = next;
            (last.trace.pick_low_cm_360, last.trace.pick_high_cm_360) = pick_range;
        }
        conclusion
    }
}

/// How far outside the tested sensitivities a test's own estimate may lie and still count.
const EXTRAPOLATION_MARGIN: f64 = 0.10;

/// Tests left out of a result: their whole ~95% interval lies more than
/// `EXTRAPOLATION_MARGIN` outside the `tested` sensitivities while the other tests' combined
/// estimate lies inside. Such a test leaned the same way at every tested sensitivity, so its
/// "crossover" is an extrapolation; leaning one way regardless of sensitivity is what a
/// biased test looks like. When every test points outside, the answer is out there and
/// nothing is left out.
pub fn excluded_tests_in(set: &EstimateSet, tested: &[f64]) -> Vec<TestKind> {
    let lo = tested.iter().copied().fold(f64::INFINITY, f64::min) * (1.0 - EXTRAPOLATION_MARGIN);
    let hi = tested.iter().copied().fold(0.0, f64::max) * (1.0 + EXTRAPOLATION_MARGIN);
    let outside: Vec<TestKind> = set
        .estimates()
        .into_iter()
        .filter(|e| {
            let (cm, u) = (e.crossover.cm_360, e.crossover.rel_uncertainty);
            cm * (1.0 - u) > hi || cm * (1.0 + u) < lo
        })
        .map(|e| e.kind)
        .collect();
    if outside.is_empty() {
        return outside;
    }
    let rest = outside.iter().fold(WeightSet::neutral(), |w, &k| w.without(k));
    match set.combine(&rest) {
        Some(c) if (lo..=hi).contains(&c.cm_360) => outside,
        _ => Vec::new(),
    }
}

/// The result with `excluded_tests_in` left out (or every test, if that leaves none).
pub fn trusted_recommendation(set: &EstimateSet, tested: &[f64], preset: Option<&WeightSet>) -> Option<Recommendation> {
    let base = excluded_tests_in(set, tested).into_iter().fold(WeightSet::neutral(), |w, k| w.without(k));
    recommend_with(set, &base, preset).or_else(|| recommend(set, preset))
}

/// Trust in a result `cm`, 0..1: from its uncertainty, capped low when it sits at the edge of
/// the `search` range or a test was left out.
pub fn confidence_of(rec: Option<&Recommendation>, cm: f64, search: (f64, f64), any_excluded: bool) -> f32 {
    const UNCERTAINTY_FOR_ZERO: f64 = 0.50;
    let mut conf = match rec.map(|r| r.neutral) {
        Some(c) => (1.0 - c.rel_uncertainty / UNCERTAINTY_FOR_ZERO).clamp(0.05, 0.95),
        None => 0.3,
    };
    let at_edge = cm <= search.0 * 1.03 || cm >= search.1 * 0.97;
    if at_edge || any_excluded {
        conf = conf.min(0.3);
    }
    conf as f32
}
