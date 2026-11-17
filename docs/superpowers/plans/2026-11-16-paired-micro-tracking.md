# Paired Micro & Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Micro-adjustment and tracking judge "too fast / too slow" by comparing play at a slightly faster and a slightly slower sensitivity, instead of fixed thresholds that real players always land on one side of.

**Architecture:** A pure `sensitivity::paired` module owns sides, ABBA order, the pair judgement and pair bookkeeping. `MicroAdjustTask::paired` and `TrackingTask::paired` measure a cost per target / per 3 s chunk and report a sensitivity factor for the live side; the game loop multiplies the block's cm/360 by that factor every tick. Pair results flow through `CalibrationRun` like other signals (buffered per segment) and are kept in the run diagnostics.

**Tech Stack:** Rust 2021, glam, rand 0.8 (`StdRng`), serde. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-11-16-paired-micro-tracking-design.md` (read it first; background in `2026-11-16-multi-test-calibration-design.md`).

## Global Constraints

- `STEP = 1.12`: faster side plays at `cm / 1.12`, slower side at `cm × 1.12`.
- `TIE = 3%` of the pair's mean cost → `Signal::Direct`.
- Micro: 8 targets per block (4 pairs), order `A B B A A B B A`; both targets of a pair at the same angle (1.5–5°), independent direction; cost = seconds from spawn to hit + 0.3 s per miss on that target.
- Tracking: 12 s per block = four 3 s chunks, order `A B B A`, pairs (chunk 1, chunk 2) and (chunk 3, chunk 4); cost = mean |angular error| in degrees; skip 0.4 s after a side switch and 0.35 s after a turn/bounce; a chunk with < 1.5 s counted drops its pair.
- Which side is A: chosen per segment from the seed, so it varies between blocks.
- Warm-up gains 4 uncounted micro targets at the player's own sensitivity (factor 1.0).
- The HUD keeps showing the block's cm/360; the player is never told which side is live.
- Practice and flick segments always run at factor 1.0.
- Old classifiers (`classify_micro`, `TrackingAnalyzer`) keep running for diagnostics but no longer send signals.
- Session stays under 10 minutes.
- **Do not commit.** The user commits only when they ask; plan checkpoints replace commit steps.
- Docs and spec notes are written in Thai, matching the existing spec.

## Review Focus

- **Pause mid-pair, then resume:** the replayed segment must not count the aborted segment's pairs (Task 4 test).
- **Factor leaking:** after a paired segment ends, the next flick segment, practice, and the post-calibration state must run at factor 1.0 (Task 4 and Task 5 tests).
- **Low frame rate:** at 30 FPS (`dt = 1/30`) tracking chunk boundaries and exclusions still hold, and pairs still come out (Task 3 test).
- **Turn-heavy chunk:** a chunk where the target keeps turning has too little counted time and drops its pair, without breaking the next pair (Task 3 test).
- **Warm-up micro:** uncounted micro targets stay at factor 1.0 and produce no pair results (Task 4 test).

---

### Task 1: Paired comparison core

**Files:**
- Create: `src/sensitivity/paired.rs`
- Modify: `src/sensitivity/mod.rs` (add `pub mod paired;`)
- Test: `tests/paired_core_tests.rs`

**Interfaces:**
- Produces:
  - `pub const STEP: f64 = 1.12; pub const TIE_SHARE: f64 = 0.03;`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Side { Faster, Slower }` with `pub fn factor(self) -> f32` (`Faster` → `1/STEP`, `Slower` → `STEP`) and `pub fn other(self) -> Side`.
  - `pub fn abba(first: Side, len: usize) -> Vec<Side>` — `A B B A` repeated, truncated to `len`.
  - `pub fn judge(faster_cost: f64, slower_cost: f64) -> Signal` — slower better by more than `TIE_SHARE × mean` → `TooFast`; faster better by more → `TooSlow`; else `Direct`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)] pub struct PairResult { pub faster_cost: f64, pub slower_cost: f64, pub signal: Signal }` (`Signal` already derives serde).
  - `pub struct PairBook` — `new(order: Vec<Side>)`, `side(&self, slot: usize) -> Option<Side>`, `record(&mut self, slot: usize, cost: Option<f64>)`, `take_finished(&mut self) -> Vec<PairResult>`. Slots `(0,1)`, `(2,3)`, … form pairs; a pair finishes when both slots are recorded; a `None` cost drops the pair; each pair is returned once.

- [ ] **Step 1: Write the failing tests** in `tests/paired_core_tests.rs`:
  - `sides_scale_by_step`: `Side::Faster.factor() ≈ 1/1.12`, `Side::Slower.factor() ≈ 1.12`, `Faster.other() == Slower`.
  - `abba_order`: `abba(Faster, 8) == [F,S,S,F,F,S,S,F]`; `abba(Slower, 4) == [S,F,F,S]`; `abba(Faster, 3).len() == 3`.
  - `judge_directions`: `judge(1.0, 0.8) == TooFast`; `judge(0.8, 1.0) == TooSlow`; `judge(1.0, 0.98) == Direct` (2% < 3%); `judge(1.0, 0.95) == TooFast`.
  - `pair_book_pairs_slots_in_order`: order `abba(Faster, 4)`; record slot 0 cost 1.0, slot 1 cost 2.0 → `take_finished()` gives one `PairResult{faster_cost: 1.0, slower_cost: 2.0, signal: TooSlow}`; a second `take_finished()` is empty.
  - `pair_book_maps_costs_by_side`: order `abba(Slower, 2)`; slot 0 (Slower) 1.0, slot 1 (Faster) 2.0 → `faster_cost == 2.0`, `slower_cost == 1.0`, `TooFast`.
  - `dropped_slot_drops_its_pair_only`: slot 0 `None`, slot 1 `Some`, slots 2 and 3 `Some` → exactly one result (the second pair).
- [ ] **Step 2: Run** `cargo test --test paired_core_tests` — expect compile failure (module missing).
- [ ] **Step 3: Implement `src/sensitivity/paired.rs`** with the interfaces above. Doc comment on the module: why fixed thresholds failed (spec §1) and why a pair cancels habits shared by both sides.
- [ ] **Step 4: Run** `cargo test --test paired_core_tests` — all pass.
- [ ] **Step 5: Checkpoint** — `cargo build --all-targets` clean; no commit.

### Task 2: Paired micro-adjustment task

**Files:**
- Modify: `src/game/tasks/micro_adjust.rs`
- Modify: `src/game/tasks/mod.rs` (trait defaults only)
- Test: `tests/paired_micro_tests.rs`

**Interfaces:**
- Consumes: `paired::{Side, abba, PairBook, PairResult}`.
- Produces:
  - `Task` trait defaults: `fn sens_factor(&self) -> f32 { 1.0 }` and `fn take_pair_results(&mut self) -> Vec<PairResult> { Vec::new() }`.
  - `MicroAdjustTask::paired(pairs: usize, target_radius: f32, seed: u64, first: Side) -> Self` — `2 × pairs` targets.
  - `pub const MISS_PENALTY_SECS: f64 = 0.3;`
  - `MicroAdjustTask` implements `sens_factor` (side of the live target's slot; before the first spawn, slot 0's side) and `take_pair_results`.

- [ ] **Step 1: Write the failing tests** (drive the task with `update(dt, ray)` and `on_click(ray)`; time comes from summed `dt`, not `Instant`):
  - `paired_task_has_twice_the_pairs_in_targets`: `paired(4, 0.16, 1, Faster)` completes after 8 hits.
  - `factor_follows_abba`: record `sens_factor()` at each spawn → `[1/1.12, 1.12, 1.12, 1/1.12, …]` for `first = Faster`.
  - `targets_in_a_pair_sit_at_the_same_angle`: `micro_spawn().offset_deg.length()` of slots 0 and 1 equal within 1e-3°, slots 1 and 2 generally differ (check over seeds 1..20 that at least one differs).
  - `cost_is_time_to_hit_plus_miss_penalty`: slot 0 hit after 0.50 s with one miss first, slot 1 hit after 0.40 s → result `faster_cost ≈ 0.80`, `slower_cost ≈ 0.40`, `TooFast`.
  - `seeded_and_unpaired_tasks_report_factor_one`: `MicroAdjustTask::seeded(6, 0.16, 3)` → `sens_factor() == 1.0` and no pair results after 6 hits.
- [ ] **Step 2: Run** `cargo test --test paired_micro_tests` — fails to compile.
- [ ] **Step 3: Implement.** Draw the pair's angle once on its first slot (`rng.gen_range(MICRO_ANGLE_DEG)`), direction `phi` per target. Track the live target's age from `update`'s `dt` and its misses from `on_click`; on hit, `PairBook::record(slot, Some(age + misses × MISS_PENALTY_SECS))`. Unpaired constructors keep today's behaviour.
- [ ] **Step 4: Run** `cargo test --test paired_micro_tests` and `cargo test --test micro_adjust_tests --test calibration_run_tests` — all pass.
- [ ] **Step 5: Checkpoint** — no commit.

### Task 3: Paired tracking task

**Files:**
- Modify: `src/game/tasks/tracking.rs`
- Test: `tests/paired_tracking_tests.rs`

**Interfaces:**
- Consumes: `paired::{Side, abba, PairBook}`; trait defaults from Task 2.
- Produces:
  - `TrackingTask::paired(target_radius: f32, seed: u64, first: Side) -> Self` — 12 s, chunks of `CHUNK_SECS = 3.0`.
  - `pub const SWITCH_SETTLE_SECS: f32 = 0.4; pub const MIN_COUNTED_SECS: f32 = 1.5;` (turn exclusion reuses `TrackingAnalyzer::SETTLE_S`).
  - `sens_factor` = side of the current chunk; `take_pair_results` from its `PairBook`.

- [ ] **Step 1: Write the failing tests** (a scripted aim: put the crosshair exactly on the target, or offset it by a fixed angle, each tick):
  - `four_chunks_two_pairs`: on-target aim for 12 s at `dt = 0.001` → exactly 2 results; `sens_factor()` sequence per chunk = `abba(first, 4)` factors.
  - `cost_is_mean_angular_error`: aim 1.0° off during Faster chunks and 0.5° off during Slower chunks → both results `TooFast`, costs ≈ 1.0 and 0.5 (±0.05).
  - `settle_after_switch_is_left_out`: aim 5° off only during the first 0.3 s of each chunk, on target otherwise → costs < 0.05.
  - `turn_heavy_chunk_counts_too_little`: on `ChunkCost` directly, 3 s of ticks with `since_turn` reset every 0.3 s and 1° error → `finish() == None`; the same 3 s with no turns → `Some(≈1.0)`.
  - `dropped_chunk_keeps_the_other_pair`: a `PairBook` fed `abba(Faster, 4)` with chunk 1 `None` and chunks 2–4 `Some` → exactly one result.
  - `works_at_30_fps`: same as `four_chunks_two_pairs` with `dt = 1/30` → 2 results.
- [ ] **Step 2: Run** `cargo test --test paired_tracking_tests` — fails to compile.
- [ ] **Step 3: Implement.** The turn exclusion uses `TrackingAnalyzer::SETTLE_S` (already `pub`). Put the per-chunk accumulation in a small pure `pub struct ChunkCost` in `tracking.rs` (`push(dt, error_deg, since_switch, since_turn)`, `finish() -> Option<f64>` returning `None` below `MIN_COUNTED_SECS`) so the turn-heavy case is tested on `ChunkCost` directly. Error per tick = `mouse_offset_deg(crosshair, target_dir).length()`. Chunk index = `floor(total_active_time / CHUNK_SECS)`; on crossing a boundary, `finish` the chunk into the `PairBook` and reset `since_switch`. The existing analyzer keeps running.
- [ ] **Step 4: Run** `cargo test --test paired_tracking_tests --test tracking_signal_tests` — all pass.
- [ ] **Step 5: Checkpoint** — no commit.

### Task 4: Calibration run wiring and diagnostics

**Files:**
- Modify: `src/sensitivity/blocks.rs` (`warmup_plan`, `block_plan`)
- Modify: `src/game/calibration_run.rs`
- Modify: `src/game/tasks/mod.rs` (`TaskManager::sens_factor`, `TaskManager::take_pair_results`; `CalibrationTask` forwards both)
- Modify: `src/data/calibration.rs` (`RoundSignals.pairs`)
- Test: `tests/calibration_run_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `warmup_plan()`: flick 10, **micro 4 (practice)**, tracking 8 s, recoil 2. `block_plan()`: micro `Shots(8)`, tracking `Seconds(12.0)`, others unchanged.
  - `CalibrationRun::make_task()`: counted micro → `MicroAdjustTask::paired(n / 2, MICRO_RADIUS, seed, first)`; counted tracking → `TrackingTask::paired(TRACKING_RADIUS, seed, first)`; `first = if seed & 1 == 0 { Faster } else { Slower }` using the segment seed; uncounted segments keep the unpaired tasks.
  - `CalibrationRun::record_pair(&mut self, kind: TestKind, pair: PairResult)` — like `record`: ignored outside counted segments of `kind`; buffered with the segment, dropped by `restart_segment`, counted (its `signal` into the scheduler, the pair into the run's per-block list) by `finish_segment`.
  - `#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)] pub struct PairRecord { pub test: String, pub faster_cost: f64, pub slower_cost: f64, pub signal: Signal }` in `data/calibration.rs`; `RoundSignals` gains `#[serde(default)] pub pairs: Vec<PairRecord>`; `diagnostics()` fills it.
  - `TaskManager::sens_factor(&self) -> f32` (1.0 unless the task is `Running`) and `TaskManager::take_pair_results(&mut self) -> Vec<PairResult>`.

- [ ] **Step 1: Write the failing tests** in `tests/calibration_run_tests.rs`:
  - Update `starts_with_an_uncounted_warmup_at_the_players_sensitivity` and `each_segment_gets_a_matching_task` for the new plans (tracking now yields 24 analyzer windows in 12 s).
  - `counted_micro_and_tracking_are_paired`: in block 1, micro task reports factors from `{1/1.12, 1.12}` after its first update; tracking likewise; flick reports 1.0.
  - `warmup_micro_runs_at_factor_one`: the warm-up micro task's `sens_factor() == 1.0` and gives no pair results after 4 hits.
  - `pairs_count_when_their_segment_finishes`: `record_pair(Micro, TooFast pair)` → scheduler micro `too_fast` is 0 until `finish_segment`, then 1; diagnostics round 1 has one `PairRecord` with test `"Micro-adjustment"`.
  - `restarting_drops_recorded_pairs`: `record_pair` then `restart_segment` then `finish_segment` → no count, no `PairRecord`.
  - `task_manager_factor_is_one_when_not_running`: `TaskManager` with a paired task before `start_task_now` → 1.0.
- [ ] **Step 2: Run** `cargo test --test calibration_run_tests` — new tests fail.
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run** `cargo test` — all suites pass (fix `ui_redesign_tests`/`ui_preview` literals of `RoundSignals` if any).
- [ ] **Step 5: Checkpoint** — no commit.

### Task 5: Game loop wiring

**Files:**
- Modify: `src/main.rs`
- Test: manual (the loop isn't unit-testable); logic it relies on is covered by Task 4.

**Interfaces:**
- Consumes: `TaskManager::sens_factor`, `TaskManager::take_pair_results`, `CalibrationRun::record_pair`.

- [ ] **Step 1: Apply the factor.** At the top of the tick, before mouse deltas rotate the camera: during a calibration session, `rad_per_count = cm_360_to_rad_per_count(dpi, current_cm_360 × sens_factor())`; otherwise the plain `current_cm_360`. `current_cm_360` stays the block's value (HUD, toasts, results).
- [ ] **Step 2: Route pair results.** After `task_manager.update`, `take_pair_results()` every tick; when `session.records_calibration()`, pass each to `run.record_pair(segment.kind, pair)`.
- [ ] **Step 3: Remove the old signals.** Stop calling `run.record` for tracking windows and for `classify_micro`; keep draining `take_tracking_windows()` so the buffer doesn't grow. Remove now-unused imports and `last_micro_shot` if nothing else uses it.
- [ ] **Step 4: Reset on exit.** `finish_calibration`, `abandon`, and switching to practice end with `set_cm_360(...)` so `rad_per_count` matches factor 1.0.
- [ ] **Step 5: Verify** `cargo build --all-targets` and `cargo test` pass; run `cargo run --example ui_preview` and check `hud-run-tracking.png` still shows the block cm. Checkpoint, no commit.

### Task 6: Human-like simulation and end-to-end checks

**Files:**
- Test: `tests/paired_signal_tests.rs`
- Modify: `docs/superpowers/specs/2026-11-16-paired-micro-tracking-design.md` (status + results), `docs/superpowers/specs/2026-11-16-multi-test-calibration-design.md` (phase table note), `README.md` (Calibration section)

**Interfaces:**
- Consumes: everything above; the delayed tracker from `tests/tracking_signal_tests.rs::simulate` (copy it; test files don't share code).

Simulated players (test helpers, no product code):
- **Tracker:** the existing efference-copy tracker with a `prediction` factor: `predicted = err + v·delay·prediction − believed_sum`. `prediction = 1.0` is the old sim; `0.5` lags like real players. Drive `TrackingTask::paired` with the camera rotating at `cm × task.sens_factor()`.
- **Micro player:** 200 ms reaction, then a primary stroke over 150 ms of `0.9 × distance × pref/cm_actual × (1 + N(0, 0.08))` toward the target, then corrections every 140 ms each covering `0.9 × remaining × pref/cm_actual (+ noise)`, clicking once inside the target; misses add a click when the endpoint is outside. Drive `MicroAdjustTask::paired`.
- **"Best" sensitivity** for a simulated player = the cm/360 with the lowest mean cost, found by brute force over 15–80 cm in 2% steps. This is the spec's `pref` (§5 defines it as the cost minimum); it is not assumed to equal the hand's planning sensitivity, since a 10% undershooter is genuinely best somewhat faster.

- [ ] **Step 1: Write the regression tests (old classifiers reproduce the real-play bias):**
  - `lagging_tracker_breaks_the_old_analyzer`: tracker with `prediction = 0.5`; at cm = pref × {0.7, 1.0, 1.4} the old `TrackingAnalyzer` gives `too_slow ≥ 4 × too_fast` at every one.
  - `undershooting_hand_breaks_the_old_micro_rule`: micro player at the same three cm → `classify_micro` gives `too_slow == 0` or `too_fast ≥ 4 × too_slow` at every one.
- [ ] **Step 2: Write the paired tests:**
  - `paired_tracking_flips_at_the_best_sensitivity`: for `prediction ∈ {1.0, 0.5}`, the logistic crossover (`psa::fit_crossover`) of paired tracking signals over cm = best × {0.7…1.4} lies within 5% of the brute-force best.
  - `paired_micro_flips_at_the_best_sensitivity`: same for the micro player.
  - `paired_signals_beat_one_way_leaning`: at cm = best × 0.7 the paired signal is mostly `TooFast`, at best × 1.4 mostly `TooSlow` (each ≥ 60% of non-Direct).
  - `block_search_with_human_like_players`: run `BlockScheduler` with flick (existing `simulated_signal`), paired micro and paired tracking signals drawn from per-cm signal rates measured once by the two sims; median error vs best ≤ 7% at prefs 25/40/60 and lower than a flick-only `BlockScheduler` on the same players (spec §5: not worse than flick only), and tests are excluded in no more than 10% of runs.
  - `shared_slope_does_not_bias_paired_tests`: in the block search above, the joint-fit estimates for micro and tracking (`excluded_tests` aside) each have median error ≤ 5% vs best. **If this fails**, add a per-test slope multiplier to `joint_fit::fit_groups` (one extra parameter per group, ridge 0.25 toward 1.0) and re-run the whole suite; record the decision in the spec.
- [ ] **Step 3: Run** `cargo test --test paired_signal_tests -- --nocapture` — print each measured number; all pass (iterate on `STEP`/`TIE` only if a test fails, recording the change and why in the spec §6).
- [ ] **Step 4: Run** `cargo test` (every suite) and `cargo build --release`.
- [ ] **Step 5: Docs.** Spec: status → built, the measured sim numbers, "not yet verified in real play". Main spec phase table: a row for this change. README Calibration paragraph: micro and tracking compare a slightly faster and slower sensitivity; warm-up includes micro; ~6 minutes. Checkpoint, no commit.
