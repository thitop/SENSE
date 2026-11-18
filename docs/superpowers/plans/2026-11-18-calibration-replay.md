# Calibration Replay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the static "How we got here" graphs on My sensitivity with a round-by-round replay that explains why SENSE gave this number.

**Architecture:** The block scheduler records, per round, the search range, its best guess and which branch picked the next sensitivity (`RoundTrace`). The run saves it in `RunDiagnostics`. A pure view-model module (`src/ui/replay.rs`) turns a `CalibrationResult` into steps with their exact copy. A small playback state machine in `UiState` drives Play / step / autoplay, and `sensitivity.rs` draws it (layout A).

**Tech Stack:** Rust 2021, egui 0.31, egui_plot 0.31, serde.

**Spec:** `docs/superpowers/specs/2026-11-18-calibration-replay-design.md`. Mockup: `.superpowers/brainstorm/1259-1794984732/content/replay-layout.html` (layout A).

## Global Constraints

- Every sentence must match what the scheduler did. Wording comes from recorded data, never from the round number or a re-derived rule.
- Never use "overshoot", "stopped short" or "clean" in replay copy. Tests are judged in pairs (`sensitivity::paired::judge`).
- Copy that the spec fixes is quoted verbatim in Task 3. Numbers: cm `{:.1}`, percent `{:.0}`.
- Older saved results must still load. Every new field is `#[serde(default)]` (and `skip_serializing_if = "Option::is_none"` for options).
- The result number, Copy and "Use this sensitivity" never wait on the replay.
- "How we got here" stays at the bottom of the page, under What to practise (owner's decision).
- The page must fit the 1024-wide preview (`sensitivity-result-small`).
- Step length: `STEP_SECS = 1.6`.

## Review Focus

1. A run whose fit picks a value outside `[low × 0.8, high × 1.2]`: the card must say "goes as far as it can", not "the best guess so far". Test in Task 1 (`clamped_pick_is_recorded_as_clamped`).
2. A result saved before this change (`trace` missing on every round) must render the reduced replay, not panic or show empty footers. Tests in Task 2 (`old_diagnostics_json_loads`) and Task 3 (`legacy_round_uses_conclusion_wording`).
3. A guess with `rel_uncertainty` above `MAX_ANSWER_UNCERTAINTY` (0.84) must show "Too early to trust a guess" and give no graph point. Test in Task 3 (`uncertain_guess_is_not_plotted`).
4. Pressing Play on the Result step restarts from Round 1, and any manual step stops playback. Test in Task 4 (`play_on_result_restarts`, `manual_step_stops_playback`).
5. A new run, or "Use only the latest run", must not leave the replay on a stale step. Test in Task 4 (`new_result_resets_state`).

---

### Task 1: Record each round's trace in the block scheduler

**Files:**
- Modify: `src/sensitivity/blocks.rs` (types near `SignalCounts`; `BlockResult`; `BlockScheduler::end_block`, lines ~306–364)
- Test: `tests/calibration_replay_tests.rs` (new)

**Interfaces:**
- Produces (all `#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]`, `pub`, in `sensitivity::blocks`):
  ```rust
  pub enum RoundGuess { Guess { cm_360: f64, rel_uncertainty: f64 }, OneSensitivity, NoCrossover }
  pub enum NextPick {
      Midpoint { cm_360: f64 },
      FitEstimate { cm_360: f64 },
      FitClamped { guess_cm: f64, cm_360: f64 },
      FitFallbackMidpoint { cm_360: f64 },
      Last,
  }
  pub struct RoundTrace { pub low_cm_360: f64, pub high_cm_360: f64, pub guess: RoundGuess, pub next: NextPick }
  ```
- Produces: `BlockResult::trace: RoundTrace` (new field).

- [ ] **Step 1: Write the failing tests** in `tests/calibration_replay_tests.rs`. Drive a `BlockScheduler::new(15.0, 80.0, 7)` with `record(kind, signal)` and `end_block()`, like `run_blocks` in `tests/multi_test_calibration_tests.rs`:
  - `first_round_bisects_and_cannot_guess`: one block of Flick `TooFast` ×6, `TooSlow` ×1. Then `history[0].trace.low_cm_360 == 15.0` and `high_cm_360 == 80.0` (the range before the round), `guess == RoundGuess::OneSensitivity`, and `next == NextPick::Midpoint { cm_360: s.current_cm_360 }`.
  - `later_rounds_follow_the_fit`: play blocks where the misses cross over (TooFast below ~35 cm, TooSlow above, as in `simulated_signal`). For every block from index 1 on, `next` is `FitEstimate { cm_360 }` with `cm_360 == ` the next block's `tested_cm_360`, or `FitClamped`/`FitFallbackMidpoint` whose `cm_360` equals it. Every `Guess` has a finite `cm_360 > 0`.
  - `clamped_pick_is_recorded_as_clamped`: every test says `TooFast` at every block (the fit runs off toward slow). Find a block with `NextPick::FitClamped { guess_cm, cm_360 }`, and assert `guess_cm > cm_360` and `cm_360 == ` the next block's tested value. If the fit has no answer there, assert `FitFallbackMidpoint` instead. Either way the recorded `cm_360` equals the next tested value.
  - `last_round_says_last`: after 7 blocks, `history[6].trace.next == NextPick::Last`, and `is_completed`.
  - `no_crossover_after_two_one_sided_rounds`: 2 blocks, all `TooFast`. Then `history[1].trace.guess == RoundGuess::NoCrossover` (two sensitivities tested, but no fit).

- [ ] **Step 2: Run to verify they fail**
  Run: `cargo test --test calibration_replay_tests`
  Expected: compile error, `RoundTrace` / `trace` not found.

- [ ] **Step 3: Implement.** Add the types and the `trace` field. In `end_block`:
  - Capture `(low, high)` before the bounds move.
  - After pushing the `BlockResult`, compute the guess as `self.estimate_set().and_then(|s| recommend(&s, None)).map(|r| r.neutral)` → `Guess { cm_360, rel_uncertainty }`. When there's none: `OneSensitivity` if fewer than two distinct `tested_cm_360` values are in `history`, otherwise `NoCrossover`.
  - Set `next` from the branch the existing code takes. In the fit branch, compare the raw `fit_estimate()` with the clamped value: `Some(g)` inside `[lo, hi]` → `FitEstimate`, `Some(g)` outside → `FitClamped { guess_cm: g, .. }`, `None` → `FitFallbackMidpoint`. Completion → `Last`. Bisection → `Midpoint`.
  - Write the trace onto `self.history.last_mut()`.
  - Don't change which sensitivity is picked: existing tests in `tests/multi_test_calibration_tests.rs` must pass unchanged.

- [ ] **Step 4: Run tests**
  Run: `cargo test --test calibration_replay_tests --test multi_test_calibration_tests --test calibration_run_tests`
  Expected: all PASS.

- [ ] **Step 5: Commit**
  `git add src/sensitivity/blocks.rs tests/calibration_replay_tests.rs && git commit -m "feat: block scheduler records range, best guess and next pick per round"`

---

### Task 2: Save the trace and the result's ± with the run

**Files:**
- Modify: `src/data/calibration.rs` (`RoundSignals`, `RunDiagnostics`, `CalibrationResult::apply_pool`)
- Modify: `src/game/calibration_run.rs` (`diagnostics()`, ~line 310)
- Modify: `tests/habit_calibration_tests.rs:170` (the `RoundSignals` literal gains `trace: None`)
- Test: `tests/calibration_replay_tests.rs`

**Interfaces:**
- Consumes: `RoundTrace` (Task 1).
- Produces:
  - `RoundSignals::trace: Option<RoundTrace>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
  - `RunDiagnostics::result_rel_uncertainty: Option<f64>` (same attributes).
  - Re-export `RoundTrace, RoundGuess, NextPick` from `crate::data` next to `RoundSignals`.

- [ ] **Step 1: Write the failing tests**
  - `diagnostics_carry_each_rounds_trace`: run a whole `CalibrationRun::new(5)` the way `only_tests_that_exist_are_scheduled` does, recording signals through `record_pair`/`record` so the fit has misses on both sides. Then `run.diagnostics().rounds.len() == 7`, every `rounds[i].trace == Some(run.scheduler().history[i].trace)`, and `result_rel_uncertainty == run.scheduler().recommendation(None).map(|r| r.neutral.rel_uncertainty)`.
  - `old_diagnostics_json_loads`: a `RunDiagnostics` JSON string with one round and no `trace` / `result_rel_uncertainty` keys deserializes, with `rounds[0].trace == None` and `result_rel_uncertainty == None`.
  - `pooling_sets_the_pooled_uncertainty`: `apply_pool` with a `Pooled { rel_uncertainty: 0.07, .. }` sets `diagnostics.result_rel_uncertainty == Some(0.07)`.

- [ ] **Step 2: Run to verify they fail**
  Run: `cargo test --test calibration_replay_tests`
  Expected: compile error on the new fields.

- [ ] **Step 3: Implement.** Add the fields. `diagnostics()` copies `b.trace` into each round and sets `result_rel_uncertainty` from `s.recommendation(None)`. `apply_pool` sets it from `p.rel_uncertainty`.

- [ ] **Step 4: Run all tests**
  Run: `cargo test`
  Expected: all PASS.

- [ ] **Step 5: Commit**
  `git commit -am "feat: save each round's trace and the result's uncertainty with the run"`, adding the new test file if it isn't tracked yet.

---

### Task 3: Replay view model and copy (pure)

**Files:**
- Create: `src/ui/replay.rs` (add `pub mod replay;` in `src/ui/mod.rs`)
- Test: `tests/calibration_replay_tests.rs`

**Interfaces:**
- Consumes: `CalibrationResult`, `RunDiagnostics`, `RoundTrace`/`RoundGuess`/`NextPick`, `PsaRoundConclusion`, `SignalCounts`, `pooling::MAX_ANSWER_UNCERTAINTY`.
- Produces:
  ```rust
  pub struct Replay { pub rounds: Vec<RoundView>, pub result: ResultView }
  pub struct RoundView {
      pub header: String,            // "Round 3 of 7"
      pub tested_cm_360: f64,
      pub conclusion: PsaRoundConclusion,
      pub range: Option<(f64, f64)>, // from trace; None for legacy
      pub guess_point: Option<(f64, f64)>, // (cm, rel) only when Guess and rel <= MAX_ANSWER_UNCERTAINTY
      pub rows: Vec<TestRow>,
      pub guess_line: Option<String>, // None for legacy
      pub next_line: String,
  }
  pub struct TestRow { pub test: String, pub counts: SignalCounts, pub verdict: &'static str }
  pub struct ResultView {
      pub cm_360: Option<f64>,       // None when no_answer
      pub rel_uncertainty: Option<f64>,
      pub lines: Vec<(String, Tone)>, // in display order
  }
  pub enum Tone { Normal, Muted, Warn }
  pub fn replay(cal: &CalibrationResult) -> Option<Replay> // None when diagnostics is None or has no rounds
  pub fn verdict(c: SignalCounts) -> &'static str
  pub const INTRO: &str;
  pub const LINK: &str;
  ```

Exact copy (verbatim from the spec):
- `INTRO` = `Each round tested one sensitivity, in pairs: one target a little faster (+12%), one a little slower. Whichever side you did better on says which way to go. Where that flips is your number.`
- `LINK` = `How we got this number →`
- `verdict`: `too_fast > too_slow` → `Better slower · too fast`; `too_slow > too_fast` → `Better faster · too slow`; otherwise `Even`.
- Rows: keys of `RoundSignals::signals` that don't start with `"Flick "`. Order: `Flicking`, `Micro-adjustment`, `Tracking`, then any other key alphabetically. Skip `Counter-strafing`.
- `guess_line`:
  - `Guess` with rel ≤ 0.84 → `So far SENSE thinks {cm:.1} cm ± {pct:.0}%`
  - `Guess` with rel > 0.84 → `Too early to trust a guess`
  - `OneSensitivity` → `One sensitivity tested so far: SENSE needs two to start guessing`
  - `NoCrossover` → `Not enough to guess yet: every test leaned the same way`
- `next_line` with a trace:
  - `Midpoint` + `TooFastNeedSlower` → `Overall too fast → the next round tries the middle of the slower half ({cm:.1} cm)`
  - `Midpoint` + `TooSlowNeedFaster` → `Overall too slow → the next round tries the middle of the faster half ({cm:.1} cm)`
  - `Midpoint` + `BalancedNarrow` → `About even → the range narrows around here`
  - `FitEstimate` → `Next round tests {cm:.1} cm: the best guess so far.`, or `Next round tests {cm:.1} cm: a rough first guess.` when this round's guess line was "Too early to trust a guess"
  - `FitClamped` → `The best guess ({guess:.1} cm) is outside what's been tested, so the next round goes as far as it can: {cm:.1} cm.`
  - `FitFallbackMidpoint` → `Next round tests the middle of the range ({cm:.1} cm).`
  - `Last` → `That was the last round → result`
- `next_line` for legacy rounds (no trace): `Overall too fast → next round went slower` / `Overall too slow → next round went faster` / `About even → next round narrowed in`. The last round uses `That was the last round → result`.
- `ResultView.lines`, in order:
  1. `no_answer` → (`No result: every test leaned the same way at every sensitivity tested`, Muted), and no further per-test lines except left-out ones. Otherwise (`Every test flips from "too fast" to "too slow" here. Each test on its own:`, Muted).
  2. One line per `diagnostics.estimates` entry not starting with `"Flick "`: `{test}: {estimate_text}`, reusing `screens::sensitivity::estimate_text`. A test in `left_out` gets (`{test}: Left out: leaned one way at every sensitivity`, Warn).
  3. If `pooled_runs > 1`: `Combined with {pooled_runs - 1} earlier runs → {cm_360:.1} cm` (use `run` when that count is 1).
  4. If `pooled_runs == 1`, `left_out` is non-empty, and the last round's `Guess` differs from `cm_360` by ≥ 0.05: `Leaving out {names joined with " and "} moved it from {guess:.1} to {cm_360:.1} cm.`
  5. If `runs_disagree`: the text of `result_note(false, true)`, Warn.

- [ ] **Step 1: Write the failing tests.** Build `CalibrationResult`s by hand, as `sample_calibration()` in `tests/ui_redesign_tests.rs` does:
  - `verdict_text_follows_pairs`: (6,0,1) → `Better slower · too fast`; (1,0,6) → `Better faster · too slow`; (2,3,2) → `Even`.
  - `rows_skip_flick_bands_and_keep_order`: signals for `Tracking`, `Flick far`, `Flicking`, `Micro-adjustment` → rows `["Flicking", "Micro-adjustment", "Tracking"]`.
  - `next_line_matches_each_pick`: one assertion per `NextPick` variant above, with the exact strings.
  - `uncertain_guess_is_not_plotted`: `Guess { cm_360: 35.0, rel_uncertainty: 4.0 }` → `guess_point == None`, `guess_line == Some("Too early to trust a guess")`, and with `FitEstimate` the next line ends `a rough first guess.`
  - `legacy_round_uses_conclusion_wording`: a round with `trace: None` and `TooFastNeedSlower` → `range == None`, `guess_line == None`, `next_line == "Overall too fast → next round went slower"`.
  - `no_diagnostics_means_no_replay`: `diagnostics: None` → `replay(&cal).is_none()`.
  - `result_lines_pooled_and_left_out`: `pooled_runs = 3` → contains `Combined with 2 earlier runs → 34.2 cm`. With `pooled_runs = 1`, `left_out = ["Tracking"]` and a last guess of 33.0 → contains `Leaving out Tracking moved it from 33.0 to 34.2 cm.`, and the Tracking line is Warn.
  - `no_answer_result_has_no_number`: `no_answer = true` → `result.cm_360 == None`, and the first line is the no-result sentence.

- [ ] **Step 2: Run to verify they fail**
  Run: `cargo test --test calibration_replay_tests`
  Expected: compile error, `aim_trainer::ui::replay` not found.

- [ ] **Step 3: Implement `src/ui/replay.rs`** to the interfaces and copy above. The header is `Round {i+1} of {n}`. The conclusion comes from `cal.rounds[i]` when present, otherwise from the trace's dot rule: more `too_fast` than `too_slow` summed over rows → `TooFastNeedSlower`, and so on.

- [ ] **Step 4: Run tests**
  Run: `cargo test --test calibration_replay_tests`
  Expected: PASS.

- [ ] **Step 5: Commit**
  `git add src/ui/replay.rs src/ui/mod.rs tests/calibration_replay_tests.rs && git commit -m "feat: replay view model with the round and result copy"`

---

### Task 4: Playback state

**Files:**
- Modify: `src/ui/replay.rs` (add the state)
- Modify: `src/ui/mod.rs` (`UiState::replay: ReplayState`, initialised in `UiState::new`)
- Modify: `src/main.rs` (`finish_calibration`, next to `self.ui_state.show_all_games = false;`: `self.ui_state.replay.arm_autoplay();`)
- Test: `tests/calibration_replay_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const STEP_SECS: f64 = 1.6;
  #[derive(Clone, Debug, Default, PartialEq)]
  pub struct ReplayState {
      pub step: usize,          // 0..rounds-1 = rounds, rounds = Result
      pub playing: bool,
      pub last_step_at: f64,
      pub autoplay_armed: bool,
      pub scroll_pending: bool,
      key: Option<(chrono::DateTime<chrono::Utc>, usize)>, // (result.at, pooled_runs)
  }
  impl ReplayState {
      pub fn sync(&mut self, at: DateTime<Utc>, pooled_runs: usize); // resets step/playing when the key changes; keeps autoplay_armed
      pub fn arm_autoplay(&mut self);
      pub fn play(&mut self, now: f64, steps: usize);   // from Result → step 0
      pub fn pause(&mut self);
      pub fn go_to(&mut self, step: usize, steps: usize); // clamps, stops playback
      pub fn prev(&mut self, steps: usize);
      pub fn next(&mut self, steps: usize);
      pub fn tick(&mut self, now: f64, steps: usize);   // advances one step per STEP_SECS; stops on Result
      pub fn on_visible(&mut self, now: f64, steps: usize); // if armed: disarm and play from step 0
      pub fn link_clicked(&mut self, now: f64, steps: usize); // scroll_pending = true; play from step 0
  }
  ```
  Here `steps` = `rounds.len() + 1`.

- [ ] **Step 1: Write the failing tests**
  - `tick_advances_every_step_secs_and_stops_on_result`: play at t=0 with steps=8. Ticks at 1.59 → step 0; 1.6 → 1; …; reaching step 7 sets `playing == false`. Further ticks stay at 7.
  - `play_on_result_restarts`: `go_to(7, 8)`, then `play` → step 0 and playing.
  - `manual_step_stops_playback`: playing, then `next` → not playing, step+1. `prev` at 0 stays 0. `go_to(99, 8)` → 7.
  - `autoplay_waits_until_visible`: `arm_autoplay` → not playing. `on_visible(5.0, 8)` → playing from step 0, armed cleared. A second `on_visible` does nothing.
  - `new_result_resets_state`: `sync(a, 1)`, `go_to(4)`, then `sync(a, 1)` → still 4. `sync(b, 1)` → step 0, not playing. `sync(b, 3)` → reset again.

- [ ] **Step 2: Run to verify they fail**
  Run: `cargo test --test calibration_replay_tests`
  Expected: compile error.

- [ ] **Step 3: Implement** `ReplayState`, add the `UiState` field, and arm it in `finish_calibration`.

- [ ] **Step 4: Run tests**
  Run: `cargo test`
  Expected: PASS.

- [ ] **Step 5: Commit**
  `git commit -am "feat: replay playback state with one-time autoplay after a run"`

---

### Task 5: Draw the replay on My sensitivity

**Files:**
- Modify: `src/ui/replay.rs` (`pub fn show(ui: &mut egui::Ui, state: &mut ReplayState, replay: &Replay)`)
- Modify: `src/ui/screens/sensitivity.rs` (the "How we got here" block at the end of `result()`, ~lines 279–301; the link in the left column after the Low-confidence note, ~line 170, and in `no_answer()`)
- Modify: `src/ui/graphs.rs` (remove `psa_convergence`, and `performance_curve` if nothing else uses it; check with `grep`)
- Modify: `examples/ui_preview.rs` (scenes)
- Modify: `README.md` ("Reading the result": the line about graphs of how the search narrowed)

**Interfaces:**
- Consumes: `replay()`, `Replay`, `ReplayState` and its methods, `INTRO`, `LINK` (Tasks 3–4).

- [ ] **Step 1: Wire the section.** In `result()`:
  - Call `state.replay.sync(cal.at, cal.pooled_runs)`.
  - If `replay(cal)` is `Some`: keep `section(ui, "How we got here")`, then `INTRO` (body 13, TEXT_SECONDARY), then `replay::show`.
  - If `scroll_pending` is set, call `scroll_to_me(Some(Align::Min))` on the section header response and clear the flag.
  - If `ui.is_rect_visible` on the header rect, call `on_visible(now)`.
  - If `replay(cal)` is `None`, keep today's two graphs unchanged.
  - The link is a `widgets::ghost_button(ui, None, LINK)` and calls `link_clicked`. Show it only when `replay(cal)` is `Some`.

- [ ] **Step 2: Draw `replay::show`** to match mockup layout A:
  - **Graph card:** `egui_plot`, y = ln(cm) with tick labels at 15, 20, 30, 40, 60, 80, x = R1…Rn. It shows only rounds ≤ the current step:
    - the range band per round (ACCENT at ~7% alpha; skipped when `range` is None);
    - a green dashed best-guess line with a ±rel band (SUCCESS; only rounds with `guess_point`);
    - an ACCENT line joining tested values;
    - dots red/blue/grey by conclusion, using `DANGER` for too fast, a new `theme::SLOW` (#5AA9E8) for too slow, and TEXT_SECONDARY for even, with the current dot ringed in ACCENT;
    - on Result, a horizontal ACCENT line at `cm_360` labelled `{cm:.1} cm`.
  - **Legend:** `Too fast · Too slow · Tie · Search range · Best guess`.
  - **Controls row:** `primary_button` `▶ Play` / `❚❚ Pause`, `◀`, `▶`, then round buttons `1`…`n` and `Result`. Done steps are filled, the current one has an ACCENT outline.
  - **Card:** the header with the tested value right-aligned in ACCENT. Rows: name (150 px), a 3-part bar (DANGER / BORDER_STRONG / SLOW), then the verdict coloured to match. Footer on SURFACE_RAISED: `guess_line` (if any) above `next_line`. The Result step shows the big number (`display_bold(34.0)`, ACCENT), `cm/360° ± {pct:.0}%` when `rel_uncertainty` is Some, then `lines` by `Tone`.
  - **Repaint:** while `playing`, call `tick(now, steps)` and `ui.ctx().request_repaint_after(...)`. Don't repaint otherwise.
  - **Room for B:** draw the round card in its own function taking `(&RoundView)`, rows and footer as separate blocks, so a clip area can go between them later.

- [ ] **Step 3: Remove the old graphs** from the replay path and delete unused graph functions. `cargo build` must give no warnings about dead code.

- [ ] **Step 4: Preview scenes.** Give the `profile(true)` calibration real `diagnostics` with 7 rounds (signals for Flicking / Micro-adjustment / Tracking, traces covering `Midpoint`, `FitEstimate`, one `FitClamped`, and `Last`) and `result_rel_uncertainty: Some(0.06)`. Add these scenes at (1280, 1600) unless noted, all with `state.replay` set via `go_to`:
  - `sensitivity-replay-round` (step 2)
  - `sensitivity-replay-result` (Result)
  - `sensitivity-replay-pooled` (Result, `pooled_runs = 3`)
  - `sensitivity-replay-no-answer` (Result, `no_answer = true`)
  - `sensitivity-replay-legacy` (step 2, every `trace = None`)
  - `sensitivity-replay-small` (1024 × 1600, step 2)

  Run: `cargo run --example ui_preview`
  Expected: the PNGs are written to `target/ui-preview/`. Open each one and check it against the mockup: nothing clipped at 1024, legacy shows no band and no green line, no-answer shows no result line.

- [ ] **Step 5: README.** In "Reading the result", replace the graphs sentence with: `**How we got here** replays the run round by round: what each test said, where SENSE's best guess was, and why it tried the next sensitivity. It plays once after a run; the link under your number replays it.`

- [ ] **Step 6: Run everything**
  Run: `cargo test` and `cargo build --release`
  Expected: all PASS, no warnings.

- [ ] **Step 7: Commit**
  `git add -A src examples README.md && git commit -m "feat: How we got here replays the run round by round"`
