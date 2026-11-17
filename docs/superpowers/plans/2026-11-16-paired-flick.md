# Paired Flick Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flick judges "too fast / too slow" by comparing time-to-hit at a slightly faster and a slightly slower sensitivity, like micro, instead of reading the shape of the mouse path.

**Architecture:** `FlickTask::paired` places pairs of equal-angle targets per distance band in A-B-B-A side order, measures a cost per target from summed `dt`, and reports pairs (with their band) through the existing `Task::take_pair_results` / `sens_factor` hooks. `CalibrationRun` routes flick pairs to `record_flick(band, signal)`; `main.rs` stops recording the old shape-based flick signal.

**Tech Stack:** Rust 2021, glam, rand 0.8, serde. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-11-16-paired-flick-design.md` (background: `2026-11-16-paired-micro-tracking-design.md`).

## Global Constraints

- `STEP = 1.12`, A-B-B-A order, first side from the segment seed (`first_side`), `judge`/`PairBook` from `sensitivity::paired`.
- Counted flick: **18 targets per block = 9 pairs, 3 pairs per band**; band order of pairs shuffled from the seed; both targets of a pair at the same angle within the band's range, direction drawn per target; far band keeps the ±15° tilt.
- Cost = seconds from spawn to hit (summed `dt`) + 0.3 s per miss on that target; a target that expires (2 s limit) costs 2.0 + 0.3 × misses + 0.6, and its pair is kept.
- Warm-up flick 10 and the 2 adaptation shots per block stay unpaired at factor 1.0.
- The old shape classifier keeps running for diagnostics and the score graph; it no longer sends flick signals.
- `PairRecord` gains `band: Option<String>` with `#[serde(default)]`.
- Session stays under 10 minutes (~7 expected).
- **Do not commit**; the user commits on request. Docs in Thai.

## Review Focus

- **Expired target inside a pair:** the pair still comes out, with the expired side's cost ≥ 2.6 s (Task 1 test).
- **Factor during the adaptation shots and warm-up flicks:** 1.0 (Task 2 test).
- **Pause mid-pair, resume:** aborted flick pairs aren't counted (covered by `restart_segment` clearing `pending_pairs`; Task 2 test with a flick pair).
- **Band of each pair reaches the scheduler:** band counts in the block history match the pairs (Task 2 test).
- **Old saved results:** `PairRecord` JSON without `band` still loads (Task 2 test).

---

### Task 1: Paired flick task

**Files:**
- Modify: `src/sensitivity/paired.rs` (`PairResult.band`)
- Modify: `src/game/tasks/flick.rs`
- Test: `tests/paired_flick_tests.rs`; update `PairResult` literals in `tests/paired_core_tests.rs`, `tests/calibration_run_tests.rs`

**Interfaces:**
- Produces:
  - `PairResult` gains `#[serde(default)] pub band: Option<FlickBand>`; `PairBook::take_finished` sets `None`.
  - `FlickTask::paired(pairs_per_band: usize, seed: u64, target_radius: f32, first: Side) -> Self` — `6 × pairs_per_band` targets (`pairs_per_band` pairs in each of the 3 bands).
  - `pub const EXPIRED_PENALTY_SECS: f64 = 0.6;` and reuse `micro_adjust::MISS_PENALTY_SECS` (0.3).
  - `FlickTask` implements `sens_factor` (side of the live target's slot; between targets the next slot's) and `take_pair_results` (each result's `band` set to its pair's band).

- [ ] **Step 1: Write the failing tests** in `tests/paired_flick_tests.rs` (drive with `update(dt, ray)` / `on_click(ray)`, shooting the newest render target):
  - `three_pairs_per_band_is_eighteen_targets`: `paired(3, 1, 0.35, Side::Faster)` completes after 18 hits; 9 pair results; 3 per band.
  - `factor_follows_abba`: factor at each spawn = `abba(Faster, 18)` factors.
  - `pairs_share_band_and_angle`: slots 2k and 2k+1 have the same `flick_spawn().band` and `angle_deg` within 0.01°; every band's angles lie in its `range_deg()` (allow the existing elevation clamp: angle ≥ 0.5 × lo).
  - `cost_is_time_plus_misses`: slot 0 hit after 0.5 s with one miss, slot 1 after 0.4 s → `faster_cost ≈ 0.8`, `slower_cost ≈ 0.4`, `TooFast`, `band == Some(first pair's band)`.
  - `expired_target_counts_as_slow`: a task built with `.with_time_limit(0.01)`; let slot 0 expire (sleep 20 ms via `std::thread::sleep`, then `update`), hit slot 1 at once → one result whose expired side's cost ≥ 0.01 + 0.6.
  - `unpaired_and_banded_tasks_report_factor_one`: `FlickTask::banded(...)` and `FlickTask::new(...)` → factor 1.0, no pair results.
- [ ] **Step 2: Run** `cargo test --test paired_flick_tests` — fails to compile.
- [ ] **Step 3: Implement.** Paired construction builds the band order of pairs (`[Short, Mid, Far] × pairs_per_band`, shuffled with the seed) and side order `abba(first, n)`. In `next_position`, draw the angle on the even slot and reuse it on the odd slot; phi drawn per target as today. Track live target age from `update`'s `dt` and misses from `on_click`; on hit record `age + misses × 0.3`; when the live target expires (the existing `expired_count` increment), record `age + misses × 0.3 + EXPIRED_PENALTY_SECS`. The expiry uses the target's `lifetime_secs` as today (wall clock).
- [ ] **Step 4: Run** `cargo test --test paired_flick_tests --test flick_bands_tests --test paired_core_tests` — pass.
- [ ] **Step 5: Checkpoint** — no commit.

### Task 2: Calibration wiring

**Files:**
- Modify: `src/sensitivity/blocks.rs` (`block_plan`: flick `Shots(18)`)
- Modify: `src/game/calibration_run.rs` (`make_task`, `finish_segment`, `diagnostics`)
- Modify: `src/data/calibration.rs` (`PairRecord.band`)
- Modify: `src/main.rs` (stop `run.record(TestKind::Flick, …)` from `path.signal()`; keep `pending_curve`)
- Test: `tests/calibration_run_tests.rs`, `tests/multi_test_calibration_tests.rs` (block plan amounts)

**Interfaces:**
- Consumes: Task 1.
- Produces: counted flick segments return `FlickTask::paired(n / 6, seed, FLICK_RADIUS, first_side(seed))`; `finish_segment` sends a flick pair with `band: Some(b)` to `scheduler.record_flick(b, signal)`, otherwise `record(kind, signal)`; `PairRecord { test, band: Option<String> (band.label()), faster_cost, slower_cost, signal }`.

- [ ] **Step 1: Write the failing tests:**
  - `counted_flick_is_paired`: block-1 counted flick segment: amount `Shots(18)`, task factor ∈ {1/1.12, 1.12} after first update; the adaptation segment's factor is 1.0; warm-up flick factor 1.0.
  - `flick_pairs_count_per_band`: `record_pair(Flick, PairResult{…, band: Some(Far)})` then finish → block history `band_counts(Far)` and `counts(Flick)` each have one; diagnostics `PairRecord.band == Some("far")`.
  - `restarting_drops_flick_pairs`: like the micro version, with a flick pair.
  - `old_pair_records_load`: JSON of a `RoundSignals` with a pair lacking `band` deserializes with `band == None`.
  - Update `block_has_every_scored_test_once_and_no_strafe` to `Shots(18)` and `each_segment_gets_a_matching_task` for paired flick.
- [ ] **Step 2: Run** `cargo test --test calibration_run_tests --test multi_test_calibration_tests` — new tests fail.
- [ ] **Step 3: Implement** the interfaces; in `main.rs` remove only the flick `run.record` call and the now-unused `flick_spawn` if nothing else needs it (the pending score curve still uses `path`).
- [ ] **Step 4: Run** `cargo test` (all suites) and `cargo build --all-targets` — pass, no new warnings in touched files.
- [ ] **Step 5: Checkpoint** — no commit.

### Task 3: Simulation checks and docs

**Files:**
- Test: `tests/paired_signal_tests.rs`
- Modify: spec `2026-11-16-paired-flick-design.md` (status, measured numbers), `2026-11-16-multi-test-calibration-design.md` (phase row), `README.md` (Calibration paragraph)

**Interfaces:**
- Consumes: the micro hand model in `paired_signal_tests.rs` (`micro()`): generalize it to drive any task that exposes `micro_spawn()` **or** `flick_spawn()` + render targets — or add `flick(rng, cm, seed, follow)` beside it reusing the same per-tick hand (reaction 200 ms, 90% cover, 8% + 0.02 cm noise, 100 ms + 100 ms/cm moves, 100 ms look). The target to aim at is the newest render target.

- [ ] **Step 1: Write the tests:**
  - `paired_flick_flips_at_the_best_sensitivity`: per band, brute-force best (unpaired play, same `best_cm` parabola) and paired sweep crossover within 5% of it.
  - `creeping_hand_breaks_the_old_flick_rule` (regression, best effort): a hand that at fast sensitivities stops its primary move at 60% and creeps in with 3 small moves; old `ShotMovementPath::signal()` gives `too_slow ≥ 2 × too_fast` at 0.5× best. If the sim cannot reproduce real play, mark the test `#[ignore]` with a comment saying so and record it in the spec.
  - Extend `block_search_with_human_like_players`: flick signals drawn from measured paired-flick rates (pooled over bands) instead of `flick_signal`; median error ≤ the current all-tests figure + 1 point and below the old flick-only search, at best 25/40/60.
- [ ] **Step 2: Run** `cargo test --release --test paired_signal_tests -- --nocapture` and read every printed number; tune `pairs_per_band` (2–4) or the expiry penalty only if a check fails, recording the change in spec §6.
- [ ] **Step 3: Run** `cargo test` (debug, all suites) and `cargo build --release`.
- [ ] **Step 4: Docs** — spec status → built, measured numbers, "not yet verified in real play"; phase-table row; README: flick also compares a faster and slower side, 18 flick targets per round, ~7 minutes.
- [ ] **Step 5: Checkpoint** — no commit.
