//! A multi-test calibration run: a warm-up at the player's own sensitivity, then blocks that
//! each test one sensitivity with a short segment of every available test. The game loop plays
//! `segment()` with the task from `make_task()`, feeds the signals it judges into `record`, and
//! calls `finish_segment` when the task completes.

use chrono::{DateTime, Utc};
use rand::rngs::StdRng;
use rand::SeedableRng;

use super::tasks::flick::{banded_order, FlickBand, FlickTask};
use super::tasks::micro_adjust::MicroAdjustTask;
use super::tasks::tracking::TrackingTask;
use super::tasks::Task;
use crate::data::{CalibrationResult, CalibrationRound, EstimateRecord, PairRecord, RoundSignals, RunDiagnostics};
use crate::sensitivity::paired::{PairResult, Side};
use crate::data::setup::PlayerSetup;
use crate::sensitivity::pooling::{is_answer, RunRecord};
use crate::sensitivity::search_plan::SearchPlan;
use crate::sensitivity::blocks::{block_plan_for, warmup_plan_for, Amount, BlockScheduler, Segment};
use crate::sensitivity::combiner::{Signal, TestKind};
use crate::sensitivity::psa::PsaRoundConclusion;

/// Target sizes, the same as the practice scenarios.
const FLICK_RADIUS: f32 = 0.35;
const MICRO_RADIUS: f32 = 0.16;
const TRACKING_RADIUS: f32 = 0.35;

/// What finishing a segment led to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentEnd {
    /// Another segment at the same sensitivity.
    Next,
    /// A new block at a new sensitivity: after the warm-up (`None`) or a block (its conclusion).
    SensitivityChanged(Option<PsaRoundConclusion>),
    /// The last block is done.
    Completed,
}

pub struct CalibrationRun {
    scheduler: BlockScheduler,
    plan: Vec<Segment>,
    index: usize,
    warmup: bool,
    rng: StdRng,
    seed: u64,
    /// Signals of the segment in progress; they count once it finishes, so a segment that is
    /// paused and replayed never counts twice.
    pending: Vec<(TestKind, Option<FlickBand>, Signal)>,
    /// Pairs of the segment in progress, counted like `pending`.
    pending_pairs: Vec<(TestKind, PairResult)>,
    /// Counted pairs with the 0-based block they were played in.
    pairs: Vec<(usize, TestKind, PairResult)>,
    search_plan: SearchPlan,
    setup: Option<PlayerSetup>,
    own_cm: Option<f64>,
}

impl CalibrationRun {
    /// Tests the run schedules. Recoil and crosshair placement join once they exist.
    pub const AVAILABLE: [TestKind; 3] = [TestKind::Flick, TestKind::Micro, TestKind::Tracking];
    pub const BLOCKS: usize = 7;

    pub fn new(seed: u64) -> Self {
        Self::with_plan(seed, SearchPlan::default(), None, None)
    }

    /// A run that searches `plan`'s range, keeping the player's answers for the result.
    pub fn with_plan(seed: u64, plan: SearchPlan, setup: Option<PlayerSetup>, own_cm: Option<f64>) -> Self {
        Self {
            scheduler: BlockScheduler::with_start(plan.min_cm, plan.max_cm, plan.start_cm, Self::BLOCKS),
            plan: warmup_plan_for(&Self::AVAILABLE),
            index: 0,
            warmup: true,
            rng: StdRng::seed_from_u64(seed),
            seed,
            pending: Vec::new(),
            pending_pairs: Vec::new(),
            pairs: Vec::new(),
            search_plan: plan,
            setup,
            own_cm,
        }
    }

    pub fn plan(&self) -> SearchPlan {
        self.search_plan
    }

    /// Blocks this run plays.
    pub fn blocks(&self) -> usize {
        self.scheduler.max_blocks
    }

    fn next_block_plan(&mut self) -> Vec<Segment> {
        block_plan_for(&Self::AVAILABLE, &mut self.rng)
    }

    /// The seed the run's targets come from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn scheduler(&self) -> &BlockScheduler {
        &self.scheduler
    }

    pub fn in_warmup(&self) -> bool {
        self.warmup
    }

    pub fn is_completed(&self) -> bool {
        self.scheduler.is_completed
    }

    /// 1-based block in progress (1 during the warm-up), at most `BLOCKS`.
    pub fn block_number(&self) -> usize {
        self.scheduler.block_number.min(self.blocks())
    }

    /// The segment to play, `None` once the run is complete.
    pub fn segment(&self) -> Option<Segment> {
        if self.is_completed() {
            None
        } else {
            self.plan.get(self.index).copied()
        }
    }

    /// 0-based position of the segment in the warm-up or block.
    pub fn segment_index(&self) -> usize {
        self.index
    }

    pub fn segments_in_phase(&self) -> usize {
        self.plan.len()
    }

    /// Sensitivity to play the current segment at. The warm-up uses the player's own.
    pub fn test_cm(&self, player_cm: f64) -> f64 {
        if self.warmup {
            player_cm
        } else {
            self.scheduler.current_cm_360
        }
    }

    /// The task for the current segment. Its targets depend only on the run's seed and the
    /// segment's place, so a replayed segment shows the same targets.
    pub fn make_task(&self) -> Box<dyn Task> {
        let Some(s) = self.segment() else {
            return Box::new(FlickTask::new(0, FLICK_RADIUS));
        };
        let seed = self.task_seed();
        match (s.kind, s.amount) {
            (TestKind::Flick, Amount::Shots(n)) if !s.counted && self.warmup => {
                Box::new(FlickTask::new(n as usize, FLICK_RADIUS))
            }
            (TestKind::Flick, Amount::Shots(n)) if s.counted => {
                Box::new(FlickTask::paired(n as usize / 6, seed, FLICK_RADIUS, first_side(seed)))
            }
            (TestKind::Flick, Amount::Shots(n)) => {
                let n = n as usize;
                let order = if n % 3 == 0 {
                    banded_order(n / 3, &mut StdRng::seed_from_u64(seed))
                } else {
                    vec![FlickBand::Mid; n]
                };
                Box::new(FlickTask::banded(order, seed, FLICK_RADIUS))
            }
            // Which side leads varies with the segment's seed.
            (TestKind::Micro, Amount::Shots(n)) if s.counted => {
                Box::new(MicroAdjustTask::paired(n as usize / 2, MICRO_RADIUS, seed, first_side(seed)))
            }
            (TestKind::Tracking, Amount::Seconds(_)) if s.counted => {
                Box::new(TrackingTask::paired(TRACKING_RADIUS, seed, first_side(seed)))
            }
            (TestKind::Micro, Amount::Shots(n)) => Box::new(MicroAdjustTask::seeded(n as usize, MICRO_RADIUS, seed)),
            (TestKind::Tracking, Amount::Seconds(secs)) => Box::new(TrackingTask::seeded(secs, TRACKING_RADIUS, seed)),
            // Not built yet; `AVAILABLE` keeps them out of the plan.
            _ => Box::new(FlickTask::new(0, FLICK_RADIUS)),
        }
    }

    /// Seed of the current segment's targets: depends only on the run's seed and the
    /// segment's place, so a replayed segment shows the same targets.
    fn task_seed(&self) -> u64 {
        let block = if self.warmup { 0 } else { self.scheduler.block_number as u64 };
        self.seed ^ (block << 32) ^ (self.index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// One judged sample of the current segment. Ignored outside counted segments and for a
    /// test other than the segment's.
    pub fn record(&mut self, kind: TestKind, band: Option<FlickBand>, signal: Signal) {
        match self.segment() {
            Some(s) if s.counted && s.kind == kind => self.pending.push((kind, band, signal)),
            _ => {}
        }
    }

    /// One finished pair of a paired test; counted like `record` once the segment finishes.
    pub fn record_pair(&mut self, kind: TestKind, pair: PairResult) {
        match self.segment() {
            Some(s) if s.counted && s.kind == kind => self.pending_pairs.push((kind, pair)),
            _ => {}
        }
    }

    /// Drop what the current segment recorded so it can be played again from the start.
    pub fn restart_segment(&mut self) {
        self.pending.clear();
        self.pending_pairs.clear();
    }

    pub fn finish_segment(&mut self) -> SegmentEnd {
        if self.is_completed() {
            return SegmentEnd::Completed;
        }
        for (kind, band, signal) in self.pending.drain(..) {
            match (kind, band) {
                (TestKind::Flick, Some(b)) => self.scheduler.record_flick(b, signal),
                _ => self.scheduler.record(kind, signal),
            }
        }
        let block = self.scheduler.history.len();
        for (kind, pair) in self.pending_pairs.drain(..) {
            match (kind, pair.band) {
                (TestKind::Flick, Some(b)) => self.scheduler.record_flick(b, pair.signal),
                _ => self.scheduler.record(kind, pair.signal),
            }
            self.pairs.push((block, kind, pair));
        }
        self.index += 1;
        if self.index < self.plan.len() {
            return SegmentEnd::Next;
        }
        self.index = 0;
        if self.warmup {
            self.warmup = false;
            self.plan = self.next_block_plan();
            return SegmentEnd::SensitivityChanged(None);
        }
        let conclusion = self.scheduler.end_block();
        if self.scheduler.is_completed {
            return SegmentEnd::Completed;
        }
        self.plan = self.next_block_plan();
        SegmentEnd::SensitivityChanged(Some(conclusion))
    }

    /// The result to save on the profile. `samples` and `stat_cm` are the per-shot score curve
    /// for the result graph (flick shots only).
    pub fn build_result(&self, samples: Vec<(f64, f64)>, stat_cm: Option<f64>, at: DateTime<Utc>) -> CalibrationResult {
        let s = &self.scheduler;
        let cm = s
            .recommended_cm_360
            .or_else(|| s.recommendation(None).map(|r| r.neutral.cm_360))
            .unwrap_or(s.current_cm_360);
        CalibrationResult {
            cm_360: cm,
            confidence: s.confidence(),
            psa_cm: Some(cm),
            stat_cm,
            rounds: s
                .history
                .iter()
                .map(|b| CalibrationRound { tested_cm_360: b.tested_cm_360, conclusion: b.conclusion })
                .collect(),
            samples,
            at,
            diagnostics: Some(self.diagnostics()),
            pooled_runs: 1,
            no_answer: s.recommendation(None).map_or(true, |r| !is_answer(r.neutral.rel_uncertainty)),
            runs_disagree: false,
            habits: None,
            plan: Some(self.search_plan),
            setup: self.setup.clone(),
            own_cm_at_start: self.own_cm,
        }
    }

    /// What this run measured, kept on the profile so later runs can be pooled with it.
    pub fn run_record(&self, dpi: f64, at: DateTime<Utc>) -> RunRecord {
        let rows = self
            .scheduler
            .history
            .iter()
            .flat_map(|b| {
                TestKind::ALL.into_iter().filter_map(move |k| {
                    let c = b.counts(k);
                    (c.too_fast + c.too_slow + c.direct > 0).then_some((k, b.tested_cm_360, c.too_fast, c.too_slow))
                })
            })
            .collect();
        RunRecord { at, dpi, rows, search: (self.search_plan.min_cm, self.search_plan.max_cm) }
    }

    /// What each test said per block and where each one alone would land.
    pub fn diagnostics(&self) -> RunDiagnostics {
        let s = &self.scheduler;
        let rounds = s
            .history
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let mut signals = std::collections::BTreeMap::new();
                for kind in TestKind::ALL {
                    let c = b.counts(kind);
                    if c.too_fast + c.too_slow + c.direct > 0 {
                        signals.insert(kind.label().to_string(), c);
                    }
                }
                for band in FlickBand::ALL {
                    let c = b.band_counts(band);
                    if c.too_fast + c.too_slow + c.direct > 0 {
                        signals.insert(format!("Flick {}", band.label()), c);
                    }
                }
                let pairs = self
                    .pairs
                    .iter()
                    .filter(|(block, _, _)| *block == i)
                    .map(|(_, kind, p)| PairRecord {
                        test: kind.label().to_string(),
                        band: p.band.map(|b| b.label().to_string()),
                        faster_cost: p.faster_cost,
                        slower_cost: p.slower_cost,
                        signal: p.signal,
                    })
                    .collect();
                RoundSignals { tested_cm_360: b.tested_cm_360, signals, pairs, trace: Some(b.trace) }
            })
            .collect();
        let record = |test: String, c: crate::sensitivity::psa::Crossover| EstimateRecord {
            test,
            cm_360: c.cm_360,
            rel_uncertainty: c.rel_uncertainty,
        };
        let mut estimates: Vec<EstimateRecord> =
            s.estimates().into_iter().map(|e| record(e.kind.label().to_string(), e.crossover)).collect();
        if let Some(bands) = s.flick_bands() {
            estimates.extend(bands.estimates.into_iter().map(|(b, c)| record(format!("Flick {}", b.label()), c)));
        }
        let conflicts = s
            .recommendation(None)
            .map(|r| r.conflicts.iter().map(|(a, b)| (a.label().to_string(), b.label().to_string())).collect())
            .unwrap_or_default();
        let result_rel_uncertainty = s.recommendation(None).map(|r| r.neutral.rel_uncertainty);
        let left_out = s.excluded_tests().iter().map(|k| k.label().to_string()).collect();
        RunDiagnostics { rounds, estimates, conflicts, fit_cm_360: s.fit_cm_360, left_out, result_rel_uncertainty }
    }
}

/// The side a paired segment starts on.
fn first_side(seed: u64) -> Side {
    if seed & 1 == 0 {
        Side::Faster
    } else {
        Side::Slower
    }
}
