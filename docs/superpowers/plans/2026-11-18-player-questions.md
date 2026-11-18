# Player Questions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ask four click-only questions before the first calibration, use the answers to cap and seed the
search, and compare the answers with what the test measured on the result page.

**Architecture:** Answers live on `PlayerProfile` as a `PlayerSetup`. A pure `SearchPlan` turns profile +
answers into a range and a start point. `BlockScheduler` gains a start hint that steps from it instead of
bisecting. Each run stores the plan and the answers it used. The habit report gains answer checks. The UI
adds a four-step overlay, built like first-run setup, with SVG pictures loaded the same way as game logos.

**Tech Stack:** Rust, egui 0.31 + egui_extras (svg), serde, existing integration tests in `tests/`. Art
tooling: Python + `vtracer` + Pillow, dev-time only.

**Spec:** `docs/superpowers/specs/2026-11-18-player-questions-design.md`

**Working tree note:** `main` has unrelated uncommitted edits. Work in a worktree from `HEAD`, and never
stage files a task doesn't name.

## Global Constraints

- Every new persisted field is `#[serde(default)]` (or a default fn): profiles and results saved before this change must load.
- UI copy is English and verbatim from the spec's "The questions" section; accent colour is `ACCENT` (#FFD23F).
- Search min stays 15 cm/360. Pad caps: Small 30, Medium 52, Large 80, Not sure 80 (one `const` table).
- Q1 lean: ×1.12 per net "slower" answer, ×0.89 per net "faster" answer. Start clamped to `[min×1.10, max×0.90]`.
- Q1 applies only while the player's sensitivity is within ±5% of `answered_at_cm`.
- Base cm when sensitivity unknown, Q3 first then Q2: Fingers 28, Wrist 32, Fingertip 32, Claw 40, Palm 45, Arm 45, Shoulder 55.
- Start-hint step: ×1.25 / ÷1.25 for the first 2 blocks (`BISECTION_BLOCKS`).
- No new runtime crates. SVGs ≤ 300 KB each, and contain no `<script`, `href`, `<image`, `foreignObject`, `onload`.
- Warm-up is unchanged; no pivot detector.

## Review Focus

1. A profile JSON saved before this change (no `setup`, no `sens_known`) loads, and `sens_known()` follows the migration rule. → Task 1
2. Own sensitivity above the pad cap (own 45, Small pad): the start clamps to 27, and a result at the cap shows the pad note and low confidence. → Tasks 2, 4, 8
3. Pooling runs searched with different caps clamps to the widest range, not to today's constant. → Task 4
4. Esc or Cancel in the middle of the questions starts no run and saves no answers. → Task 6
5. The player applies a result 14% away from where they answered Q1: the next Start calibration asks Q1 only, and the old lean is not applied meanwhile. → Tasks 1, 2, 7

---

### Task 1: Setup answers on the profile

**Files:**
- Create: `src/data/setup.rs`
- Modify: `src/data/mod.rs`, `src/data/profile.rs`
- Test: `tests/player_setup_tests.rs`

**Interfaces:**
- Produces:
  - `enum AimProblem { Overshoot, CantTurnFast, ShakySmall, RunOutOfPad }`
    with `fn lean(self) -> i8` (+1 slower: Overshoot, ShakySmall; −1 faster: CantTurnFast, RunOutOfPad)
  - `enum Grip { Palm, Claw, Fingertip }`, `enum Pivot { Wrist, Fingers, Arm, Shoulder }`, `enum PadSize { Small, Medium, Large }`
  - `struct PlayerSetup { problems: Vec<AimProblem>, grip: Option<Grip>, pivot: Option<Pivot>, pad: Option<PadSize>, answered_at_cm: f64, answered_at: DateTime<Utc> }`
    - empty `problems` / `None` mean "Not sure"
    - `fn not_sure(at_cm: f64, at: DateTime<Utc>) -> Self` is what Skip all saves
    - `fn q1_current(&self, own_cm: f64) -> bool`: `(own_cm / answered_at_cm − 1).abs() <= 0.05`
  - `enum QuestionsNeeded { All, Q1Only }`
  - On `PlayerProfile`:
    - fields `#[serde(default)] setup: Option<PlayerSetup>` and `#[serde(default)] sens_known: Option<bool>`
    - `fn sens_known(&self) -> bool`: the stored value, else `current_cm_360 != 40.0 || last_calibration.is_some()`
    - `fn questions_needed(&self) -> Option<QuestionsNeeded>`: `All` when `setup` is None; `Q1Only` when the setup has problems and `!q1_current(current_cm_360)`; otherwise None
- All enums derive `Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize`; `PlayerSetup` derives `Clone, Debug, PartialEq, Serialize, Deserialize`.

- [ ] **Step 1: Write failing tests** `old_profile_json_has_no_setup_and_migrates_sens_known`
  - JSON without the new fields; `current_cm_360` = 40 → `sens_known()` is false; = 38 → true
  - `setup.is_none()`; `questions_needed() == Some(All)`

  Also:
  - `setup_round_trips`
  - `skip_all_is_never_asked_again`: `setup = Some(PlayerSetup::not_sure(40.0, now))` → `questions_needed() == None`
  - `q1_reasked_after_sensitivity_moves`: problems `[Overshoot]` at 40; `current_cm_360` 42 → None; 42.1 → `Some(Q1Only)`
  - `explicit_sens_known_wins`: `sens_known = Some(false)` with cm 38 → false
- [ ] **Step 2:** `cargo test --test player_setup_tests` → FAIL (unresolved import `aim_trainer::data::setup`).
- [ ] **Step 3:** Implement the types and methods above; `PlayerProfile::new` sets `setup: None, sens_known: None`.
- [ ] **Step 4:** `cargo test --test player_setup_tests` → PASS; `cargo test` → all existing tests PASS.
- [ ] **Step 5:** Commit `feat: player setup answers on the profile`.

### Task 2: Search plan from profile and answers

**Files:**
- Create: `src/sensitivity/search_plan.rs`
- Modify: `src/sensitivity/mod.rs`
- Test: `tests/search_plan_tests.rs`

**Interfaces:**
- Consumes: Task 1 types, `PlayerProfile::sens_known()`, `PlayerSetup::q1_current`.
- Produces:
  - `pub const PAD_MAX_CM: [(Option<PadSize>, f64); 4]` holding Small 30, Medium 52, Large 80, None 80
  - `pub fn pad_max_cm(pad: Option<PadSize>) -> f64`
  - `#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)] pub struct SearchPlan { pub min_cm: f64, pub max_cm: f64, pub start_cm: Option<f64> }`
  - `impl Default for SearchPlan`: 15, 80, None
  - `pub fn SearchPlan::new(setup: Option<&PlayerSetup>, own_cm: Option<f64>) -> SearchPlan`
    - `own_cm` is `Some` only when the sensitivity is known
    - base = own_cm, else the Q3 → Q2 table, else None (→ `start_cm: None`)
    - lean `1.12^net` for net > 0, `0.89^(−net)` for net < 0, applied only when `own_cm` is None or `q1_current(own_cm)`
    - clamp to `[min×1.10, max×0.90]`
  - `pub fn SearchPlan::for_profile(p: &PlayerProfile) -> SearchPlan` = `new(p.setup.as_ref(), p.sens_known().then_some(p.current_cm_360))`

- [ ] **Step 1: Write failing tests.** Floats compared within 1e-9.
  - `no_answers_unknown_sens_is_todays_search`: `new(None, None) == SearchPlan::default()`
  - `own_sens_is_the_start_even_on_skip_all`: `new(Some(&not_sure(40)), Some(40.0)).start_cm == Some(40.0)` and `max_cm == 80.0`
  - `overshoot_leans_slower`: problems `[Overshoot]`, own 40 → 44.8
  - `opposite_answers_cancel`: `[Overshoot, CantTurnFast]` → 40.0
  - `two_slower_answers_compound`: `[Overshoot, ShakySmall]` → 50.176
  - `stale_q1_is_ignored`: answered at 40, own 46 → start 46.0
  - `small_pad_caps_and_clamps`: pad Small, own 45 → `max_cm == 30.0`, `start_cm == Some(27.0)`
  - `unknown_sens_uses_pivot_then_grip`:
    - pivot Arm, grip Fingertip → 45
    - pivot None, grip Fingertip → 32
    - both None → `start_cm == None`
  - `start_stays_off_the_low_edge`: own 15 → 16.5
- [ ] **Step 2:** `cargo test --test search_plan_tests` → FAIL (module missing).
- [ ] **Step 3:** Implement as specified.
- [ ] **Step 4:** `cargo test --test search_plan_tests` → PASS.
- [ ] **Step 5:** Commit `feat: search plan from mousepad, sensitivity and answers`.

### Task 3: Block search steps from a start hint

**Files:**
- Modify: `src/sensitivity/blocks.rs` (`BlockScheduler::new`, `end_block` around lines 167 and 306–366)
- Test: `tests/multi_test_calibration_tests.rs`

**Interfaces:**
- Produces:
  - `pub fn BlockScheduler::with_start(low_cm: f64, high_cm: f64, start_cm: Option<f64>, max_blocks: usize) -> Self`
    - `new(low, high, n)` becomes `with_start(low, high, None, n)`
    - `current_cm_360` = start clamped to `[low, high]`, or the midpoint when None
  - `const HINT_STEP: f64 = 1.25`
- Behaviour: only while `history.len() < BISECTION_BLOCKS` **and** a hint was given. Bounds update exactly as today for TooFast/TooSlow. Then:
  - TooFast → next = `tested × HINT_STEP`
  - TooSlow → next = `tested / HINT_STEP`
  - Balanced → `low = max(low, tested / HINT_STEP)`, `high = min(high, tested × HINT_STEP)`, and next = `tested`
  - next is clamped to `[low, high]` and `[search_min, search_max]`
  - Without a hint, everything stays byte-for-byte as today.

- [ ] **Step 1: Write failing tests**
  - `hint_is_the_first_block`: `with_start(15.0, 80.0, Some(28.0), 7).current_cm_360 == 28.0`
  - `hint_steps_slower_on_a_too_fast_block`: record 9 `(Flick, TooFast)`, `end_block()` → `current_cm_360 == 35.0`, `low_cm_360 == 28.0`
  - `hint_steps_faster_on_a_too_slow_block`: 9 TooSlow → 22.4
  - `hint_balanced_block_narrows_around_it`: 9 Direct → bounds `(22.4, 35.0)`, `current == 28.0`
  - `no_hint_keeps_bisection`: `with_start(15, 80, None, 7)` after a TooFast block → `current == 63.75` (midpoint of 47.5..80)
  - Simulation `hints_are_no_less_accurate`:
    - add `fn run_blocks_from(rng, player, start: Option<f64>) -> BlockScheduler` next to `run_blocks`
    - for prefs `[20, 30, 40, 55, 70]` × 200 runs (seed 21), compare starts `None` against `pref×1.0`, `pref×1.3`, `pref/1.3` and `pref×2.0` (clamped by the scheduler)
    - assert each hinted median error ≤ the no-hint median + 0.5 percentage points, and ≤ 7.0
    - print all medians
- [ ] **Step 2:** `cargo test --test multi_test_calibration_tests hint` → FAIL (`with_start` missing).
- [ ] **Step 3:** Implement `with_start`, a `start_hint: bool` field, and the branch in `end_block`.
- [ ] **Step 4:** `cargo test --test multi_test_calibration_tests -- --nocapture` → all PASS. Record the printed medians in the commit message. If `hints_are_no_less_accurate` fails, stop and report the numbers: the spec makes this a release gate, so don't tune the constants silently.
- [ ] **Step 5:** Commit `feat: block search steps from a start hint`.

### Task 4: Runs carry their plan and answers; pooling uses each run's range

**Files:**
- Modify: `src/game/calibration_run.rs` (`new`, `build_result`, `run_record`), `src/sensitivity/pooling.rs`, `src/data/calibration.rs`
- Test: `tests/calibration_run_tests.rs`, `tests/pooling_tests.rs`

**Interfaces:**
- Consumes: `SearchPlan` (Task 2), `BlockScheduler::with_start` (Task 3), `PlayerSetup` (Task 1).
- Produces:
  - `pub fn CalibrationRun::with_plan(seed: u64, plan: SearchPlan, setup: Option<PlayerSetup>, own_cm: Option<f64>) -> Self`
    - `new(seed)` = `with_plan(seed, SearchPlan::default(), None, None)`
    - `pub fn plan(&self) -> SearchPlan`
  - `RunRecord` gains `#[serde(default = "default_search")] search: (f64, f64)` (default `SEARCH_CM`)
  - `CalibrationResult` gains, all `#[serde(default)]`:
    - `plan: Option<SearchPlan>`
    - `setup: Option<PlayerSetup>`
    - `own_cm_at_start: Option<f64>`
  - `pub fn CalibrationResult::pad_limited(&self) -> bool`: plan max < 80 and `cm_360 >= max × 0.97`
  - `pool()` clamps and edge-checks against `(min of runs' search.0, max of runs' search.1)`
- `apply_pool` keeps this run's `plan` / `setup` / `own_cm_at_start` (the newest run's copy).

- [ ] **Step 1: Write failing tests**
  - `run_uses_plan_range_and_start` (calibration_run_tests): `with_plan(1, SearchPlan{min 15, max 30, start Some(27)}, None, None)`
    - after the warm-up, `test_cm(40.0) == 27.0`
    - `run_record(800.0, now).search == (15.0, 30.0)`
    - `build_result(..).plan == Some(plan)`
  - `old_run_record_json_defaults_search`: JSON without `search` → `(15.0, 80.0)`
  - `pool_clamps_to_widest_range` (pooling_tests): two records with search `(15, 30)` and `(15, 80)` whose rows put the answer near 40 → pooled cm > 30
  - `pool_of_capped_runs_is_capped`: both `(15, 30)` with answer near 40 → cm == 30 and confidence ≤ 0.3
  - `pad_limited_flags_only_capped_results`:
    - max 30, cm 29.5 → true
    - max 80, cm 79 → false
    - plan None → false
- [ ] **Step 2:** `cargo test --test calibration_run_tests --test pooling_tests` → FAIL.
- [ ] **Step 3:** Implement. `CalibrationRun` stores `plan`, `setup` and `own_cm`, and copies them into `build_result`.
- [ ] **Step 4:** Same command → PASS; `cargo test` → all PASS.
- [ ] **Step 5:** Commit `feat: runs keep their search plan and answers`.

### Task 5: Answer checks in the habit report

**Files:**
- Create: `src/habits/answers.rs`
- Modify: `src/habits/mod.rs`, `src/habits/trust.rs` (`habit_report`), `src/habits/analysis.rs` (expose ranked list; no rule change)
- Test: `tests/habit_answers_tests.rs`

**Interfaces:**
- Consumes: `AimProblem` (Task 1), `Habit`, `HabitFinding`, `ranked_calibration_habits`.
- Produces:
  - `pub fn matching(p: AimProblem) -> &'static [HabitMatch]` and `pub fn opposite(p: AimProblem) -> &'static [HabitMatch]`, where `enum HabitMatch { FlickPastAny, FlickShortAny, Exact(Habit) }`
    - Overshoot: matching `[FlickPastAny, Exact(MicroOvershoot)]`, opposite `[FlickShortAny, Exact(MicroCreep)]`
    - ShakySmall: matching `[Exact(TrackingShakes), Exact(MicroOvershoot)]`, no opposite
    - CantTurnFast and RunOutOfPad: none
  - `#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)] pub enum AnswerVerdict { Agrees, Opposite, NotSeen }`
  - `pub struct AnswerCheck { pub problem: AimProblem, pub verdict: AnswerVerdict }`
  - `pub fn badge(habit: Habit, checks: &[AnswerCheck]) -> Option<AnswerVerdict>`
  - `HabitReport` gains `#[serde(default)] answer_checks: Vec<AnswerCheck>`
  - `habit_report(log, result)` reads `result.setup`. For each habit-checkable problem, the verdict over the full ranked list:
    - a matching habit → Agrees
    - else an opposite one → Opposite
    - else NotSeen
  - Findings: the ranked list, stably partitioned so habits that match or oppose an answer come first, then cut to 3.
  - `pub enum DirectionVerdict { Agrees(f64), Opposite(f64), Close }`, where `f64` is the signed % difference of result vs own
  - `pub fn direction_checks(problems: &[AimProblem], own_cm: Option<f64>, result_cm: f64) -> Vec<(AimProblem, DirectionVerdict)>`
    - empty when `own_cm` is None
    - Close when |Δ| ≤ 5%
  - `pub fn direction_sentence(p: AimProblem, v: DirectionVerdict) -> String`, matching the spec's examples:
    - "You said you overshoot — the test agrees: your result is 14% slower than what you play."
    - "You said you overshoot — the test didn't: your result is close to what you play now."
    - "You said you overshoot — the test didn't: your result is 9% faster than what you play."
    - Subject phrases:
      - Overshoot "you overshoot"
      - CantTurnFast "you can't turn fast enough"
      - ShakySmall "small adjustments are shaky"
      - RunOutOfPad "you run out of mousepad"

- [ ] **Step 1: Write failing tests**
  - `answer_promotes_a_fourth_ranked_habit`: a log whose ranked list has 4 habits with `FlickPast(Far)` 4th, and answer Overshoot → `findings[0].habit == FlickPast(Far)`, len 3, check Agrees. Reuse the log builders in `tests/habit_calibration_tests.rs`.
  - `opposite_habit_is_reported`: ranked has `FlickShort(Mid)`, answer Overshoot → Opposite.
  - `not_seen_only_when_nothing_matches`: no matching or opposite habit, answer ShakySmall → NotSeen.
  - `no_setup_means_no_checks_and_old_order`: findings equal to `calibration_habits(log, cm)`.
  - `direction_sentences`:
    - own 40, result 45.6, Overshoot → the "14% slower" sentence
    - result 41 → "close"
    - result 36.4 → "9% faster"
    - `own_cm` None → empty
- [ ] **Step 2:** `cargo test --test habit_answers_tests` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --test habit_answers_tests --test habit_calibration_tests --test habit_storage_tests` → PASS.
- [ ] **Step 5:** Commit `feat: compare player answers with measured habits`.

### Task 6: Question pictures and the questions overlay

**Files:**
- Create: `assets/questions/{grip_palm,grip_claw,grip_fingertip,pivot_wrist,pivot_fingers,pivot_arm,pivot_shoulder,pad_small,pad_medium,pad_large,not_sure}.svg`
- Create: `assets/questions/SOURCES.md`, `src/ui/question_art.rs`, `src/ui/screens/questions.rs`
- Modify: `src/ui/mod.rs`, `src/ui/screens/mod.rs`
- Test: `tests/player_questions_ui_tests.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `pub fn question_svg(id: &str) -> Option<&'static [u8]>`, `pub fn question_uri(id: &str) -> String` (`bytes://question/{id}.svg`), and `pub const QUESTION_ART: [&str; 11]`
  - `pub struct QuestionsDraft { pub step: u8, pub problems: Vec<AimProblem>, pub problems_not_sure: bool, pub grip: Option<Grip>, pub pivot: Option<Pivot>, pub pad: Option<PadSize>, pub q1_only: bool, pub from_settings: bool }`
    - `fn from_setup(setup: Option<&PlayerSetup>, q1_only: bool, from_settings: bool) -> Self`
    - `fn toggle_problem(&mut self, p: AimProblem)`: clears `problems_not_sure`
    - `fn set_problems_not_sure(&mut self)`: clears the problems
    - `fn last_step(&self) -> u8`: 1 if `q1_only`, else 4
    - `fn to_setup(&self, own_cm: f64, now) -> PlayerSetup`
  - `UiState.questions: Option<QuestionsDraft>`
  - New actions:
    - `UiAction::SaveSetup { setup: PlayerSetup, start_after: bool }`
    - `UiAction::OpenSetupQuestions`
    - `UiAction::CancelQuestions`
  - The overlay is drawn in `render_ui` exactly like the onboarding branch.
- Placeholders come from mockups `docs/superpowers/mockups/player-questions-v5.html` (grips, pivots) and `player-questions-v2.html` (pads), flattened to standalone SVGs: expand every `<use>` and drop `clipPath`/`href`. The final art replaces them in Task 9 under the same file names.
- Copy and layout follow the spec's "The questions" and "Look" sections.
  - Header: "Before you start · N of 4", or "Before you start" alone when `q1_only`.
  - Buttons: `Skip all` (ghost), `Back`, then `Continue →`. The last step reads `Start calibration →`, or `Save` when `from_settings`.
  - Skip all and the last-step button emit `SaveSetup`, with `start_after = !from_settings`. `Cancel` (only when `from_settings`) and Esc emit `CancelQuestions`.

- [ ] **Step 1: Write failing tests**
  - `every_question_picture_exists_and_is_safe`: each id in `QUESTION_ART` has bytes, starts with `<svg`, is ≤ 300 KB, and contains none of `<script`, `href`, `<image`, `foreignObject`, `onload`.
  - `not_sure_and_problems_are_exclusive`: toggle Overshoot, then `set_problems_not_sure` → problems empty; toggle ShakySmall → `problems_not_sure == false`.
  - `skip_all_saves_not_sure_and_starts`: render the step-1 frame with a simulated click on Skip all (use `headless_frame` and `egui::Event` in the `ui_redesign_tests.rs` pattern, or call the screen fn with a test hook `questions::skip_all(&draft, own_cm, now) -> UiAction`) → `SaveSetup { setup: PlayerSetup::not_sure(..), start_after: true }`.
  - `cancel_saves_nothing`: `questions::cancel() == UiAction::CancelQuestions`, and no `SaveSetup` is produced for a draft that was only navigated.
  - `q1_only_has_one_step`: `from_setup(Some(&s), true, false).last_step() == 1`, with s's problems preselected.
  - `every_step_renders`: steps 1–4 render headless without panicking.
- [ ] **Step 2:** `cargo test --test player_questions_ui_tests` → FAIL.
- [ ] **Step 3:** Implement the art module, the placeholder SVGs, `SOURCES.md` (placeholders: hand-drawn for SENSE; final art: AI-generated from the spec prompts, vectorised by `tools/vectorize_questions.py`), the draft, and the screen.
- [ ] **Step 4:** Same command → PASS.
- [ ] **Step 5:** Commit `feat: player questions screen with SVG pictures`.

### Task 7: Wire questions into Start calibration, setup, Settings and Apply

**Files:**
- Modify: `src/main.rs` (`apply_action` around 426–480, profile switch near 308, Esc near 834)
- Modify: `src/ui/screens/onboarding.rs` (`CompleteOnboarding`), `src/ui/screens/settings.rs`, `src/ui/mod.rs` (`CompleteOnboarding { .., sens_known: bool }`)
- Modify: `README.md` ("First run" / "Finding your sensitivity")
- Test: `tests/player_setup_tests.rs`, `tests/ui_redesign_tests.rs`

**Interfaces:**
- Consumes: everything above.
- Behaviour in `apply_action`:
  - `StartCalibration`:
    - if `questions_needed()` is Some → `ui_state.questions = Some(QuestionsDraft::from_setup(p.setup.as_ref(), needed == Q1Only, false))` and return
    - else build the run with `CalibrationRun::with_plan(seed, SearchPlan::for_profile(p), p.setup.clone(), p.sens_known().then_some(p.current_cm_360))`
  - `SaveSetup { setup, start_after }` → save on the profile, clear `questions`, and if `start_after` run the `StartCalibration` path.
  - `CancelQuestions` → clear `questions`.
  - `OpenSetupQuestions` → `from_setup(.., false, true)`.
  - `ApplySensitivity` → also `sens_known = Some(true)`.
  - `CompleteOnboarding`: Finish sends `sens_known: true`, Skip sends `false`; stored as `Some(_)`.
  - A profile switch clears `questions` the way it clears `onboarding`.
  - Esc with `questions` open → `CancelQuestions`.
- `finish_calibration` needs no change beyond Task 4/5: `habit_report` reads `result.setup`.
- Settings "Setup" card: add a ghost button `Your setup` that emits `OpenSetupQuestions`, with the caption "Grip, what moves when you aim, mousepad, and what feels off."
- README: one paragraph under "Finding your sensitivity" on the four questions, Skip all, and Settings › Your setup.

- [ ] **Step 1: Write failing tests**
  - `onboarding_skip_marks_sens_unknown` (ui_redesign_tests): the step-3 Skip action → `CompleteOnboarding { sens_known: false, .. }`; Finish → `true`.
  - `settings_offers_your_setup`: the headless Settings frame contains the text "Your setup".
- [ ] **Step 2:** `cargo test --test ui_redesign_tests` → FAIL.
- [ ] **Step 3:** Implement the wiring.
- [ ] **Step 4:** `cargo test` → all PASS. Then `cargo run --release`, and on a fresh profile check each of these:
  - Start calibration shows the questions
  - Skip all starts the run
  - a second Start calibration goes straight in
  - Settings › Your setup › Save returns to Settings
  - Use this sensitivity on a result >5% away, then Start calibration, shows Q1 only
- [ ] **Step 5:** Commit `feat: ask setup questions before the first calibration`.

### Task 8: Result page shows the comparison

**Files:**
- Modify: `src/ui/habits.rs` (`cards`, `card`), `src/ui/screens/sensitivity.rs` (result card), `examples/ui_preview.rs`
- Test: `tests/ui_habits_tests.rs`

**Interfaces:**
- Consumes: `badge`, `direction_checks`, `direction_sentence`, `AnswerVerdict` (Task 5), `CalibrationResult::pad_limited` (Task 4).
- Produces:
  - `pub fn badge_text(v: AnswerVerdict) -> &'static str`:
    - Agrees → "You said this too"
    - Opposite → "The test saw the opposite"
    - NotSeen → "Not seen at this sensitivity"
  - `pub fn not_seen_line(p: AimProblem) -> String`: "You said <subject phrase from Task 5>: not seen at this sensitivity."
  - `pub const PAD_LIMIT: &str = "Your mousepad is the limit here. A bigger pad could let you play slower."`
- Layout:
  - badges render as a pill after the card title (`SUCCESS` on `SUCCESS_BG` for Agrees, `WARN` on `WARN_BG` otherwise)
  - direction sentences and NotSeen lines go in the notes area under the cards, each NotSeen line followed by a `Practise anyway →` secondary button to the matching scenario (`Habit::practice()` of the problem's first `Exact` match, or the Flick scenario for `FlickPastAny`)
  - `PAD_LIMIT` shows under the result value when `pad_limited()`
  - `cards()` draws badges and NotSeen lines from `report.answer_checks` alone, so History (`history.rs:271`, which has no result) shows them too. Direction sentences need the result, so only `practise_section()` (My sensitivity) adds them.

- [ ] **Step 1: Write failing tests**
  - `badge_texts`
  - `not_seen_line_text` for ShakySmall
  - `notes_include_direction_sentences`: build a `CalibrationResult` with setup `[Overshoot]`, `own_cm_at_start` Some(40) and cm 45.6, render the section headless, and find the "14% slower" sentence in the output (use the text-collection helper in `ui_habits_tests.rs`)
  - `pad_note_only_when_limited`
- [ ] **Step 2:** `cargo test --test ui_habits_tests` → FAIL.
- [ ] **Step 3:** Implement. Add preview shots `sensitivity-answers` (one card of each badge, two direction sentences) and `questions-1`..`questions-4` to `ui_preview.rs`.
- [ ] **Step 4:** `cargo test` → PASS; `cargo run --example ui_preview` → open `target/ui-preview/sensitivity-answers.png` and `questions-*.png`, and check the badges, notes and pictures by eye.
- [ ] **Step 5:** Commit `feat: result page compares answers with the test`.

### Task 9: Art pipeline for the final pictures

**Files:**
- Create: `tools/vectorize_questions.py`, `tools/requirements-art.txt` (`vtracer`, `Pillow`)
- Modify: `assets/questions/SOURCES.md`

**Interfaces:**
- `python tools/vectorize_questions.py <in_dir> <out_dir>` reads `<id>.png` or `<id>.webp` for each id in the spec's art list (sources live in `assets/questions/originals/`) and writes `<out_dir>/<id>.svg`:
  - make background pixels transparent (within 12 of `#0B0C0E` per channel)
  - vectorise with vtracer colour mode, `filter_speckle=8`, `color_precision=5`
  - snap every fill to the nearest of the palette `PALETTE = ["#1B1D21", "#2C2F35", "#A3A6AD", "#ECECEE", "#FFD23F"]` (RGB distance; fills with R > 180, G > 140, B < 120 always snap to `#FFD23F`)
  - round coordinates to 1 decimal
  - fail if the output is > 300 KB

- [ ] **Step 1:** Write `tools/selftest_vectorize.py`: draw a 512×384 PNG with Pillow (background `#0B0C0E`, a `#2A2C30` rectangle, an off-yellow `#F5C842` circle), run the script on it, and assert that every fill in the output is in `PALETTE`, `#FFD23F` and `#2C2F35` are present, nothing is left of the background colour, and the file is ≤ 300 KB.
- [ ] **Step 2:** `python tools/selftest_vectorize.py` → FAIL (script missing).
- [ ] **Step 3:** Implement the script.
- [ ] **Step 4:** `pip install -r tools/requirements-art.txt` then `python tools/selftest_vectorize.py` → PASS.
- [ ] **Step 5:** Document in `SOURCES.md`: generate with the spec's prompts → run the script → copy into `assets/questions/` → `cargo test --test player_questions_ui_tests`. Commit `chore: vectorise tool for question art`. Replacing the placeholders waits on the owner's generated PNGs.
