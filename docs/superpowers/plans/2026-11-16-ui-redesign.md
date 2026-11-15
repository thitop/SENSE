# UI/UX Redesign (Guided Sensitivity Flow) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the five-tab egui UI with a guided "find your sensitivity" flow (first-run setup → Home → calibration run → result), a sidebar app shell, and a restrained dark theme with real fonts and icons.

**Architecture:** Pure logic (profile fields, calibration progress, labels, input parsing) lives in the lib crate and is unit-tested from `tests/`. The UI becomes a function of a read-only `UiModel` snapshot that returns `Vec<UiAction>`; `main.rs` applies actions (cursor lock, PSA reset, persistence) in one place. PSA and the analyzer record shots only in calibration mode.

**Tech Stack:** Rust 2021, winit 0.30, wgpu 24, egui / egui-wgpu / egui-winit / egui_plot 0.31, egui-phosphor 0.9, serde, chrono.

**Spec:** `docs/superpowers/specs/2026-11-16-ui-redesign-design.md` (approved). Visual reference: `docs/superpowers/specs/2026-11-16-ui-redesign-mockups.html`.

## Global Constraints

- egui stays at **0.31**; icons via **`egui-phosphor = "0.9"`** (Regular). No blur, no soft shadows, no gradients.
- Fonts: **Chakra Petch** 500/600 (display + numbers) and **Inter** 400/500 (UI text), TTFs in `assets/fonts/` with `OFL.txt`, embedded with `include_bytes!`. **Ask the user before downloading font files.**
- Colours only from `ui/theme.rs` tokens (hex values in spec § Colour tokens). No `Color32::from_rgb` elsewhere in `src/ui/`.
- One primary (accent-filled) button per screen.
- Copy: English, sentence case, no "!", no emoji. Eyebrow labels may be uppercase.
- Spacing scale 4/8/12/16/24/32; card padding 16; screen padding 32; radius 8 controls, 10 cards, 12 HUD panels; min button height 36; hover anim ~120 ms.
- Layout must work from 1024×640 up (default window 1280×720).
- Old profile JSON must still load: new fields are `#[serde(default)]`.
- Sensitivity changes only during calibration; practice never changes `current_cm_360`.
- `cargo test` and `cargo build` stay green after every task. Leave the pre-existing uncommitted trailing-newline edits in `README.md` and `data/games.json` alone (don't stage them).

## Review Focus

1. **Old profile files** without `onboarded` / `last_calibration` → load as onboarded, no calibration (Task 1 test).
2. **Bad numeric input** in setup/settings ("", "abc", "0", "60000", "0,35" with comma decimal, " 800 ") → inline error or accepted normalised value, never a panic or a silent save of garbage (Task 2 tests for `parse_dpi` / `parse_game_sens`).
3. **Practice shots or paused time leaking into calibration** → PSA/analyzer only fed while `PlaySession::records_calibration()` is true; resuming starts a fresh shot timer (Task 2 test, Task 8 wiring).
4. **Profile's `primary_game_id` not in `games.json`** (renamed/removed game) → fall back to the first game, no panic, no empty highlighted row (Task 2 test for `primary_game`).
5. **Small window (1024×640)** → every screen fits or scrolls inside the content area; sidebar and primary buttons never clipped (Task 5 headless smoke render at 1024×640, Task 9 manual pass).

---

## File structure

```
assets/fonts/            ChakraPetch-Medium.ttf, ChakraPetch-SemiBold.ttf, Inter-Regular.ttf, Inter-Medium.ttf, OFL.txt
src/data/calibration.rs  CalibrationResult, CalibrationRound (persisted)
src/data/profile.rs      + onboarded, last_calibration
src/game/calibration.rs  PlayMode, PlaySession, CalibrationProgress, ConfidenceLevel, round_change_message, input parsing, primary_game
src/ui/mod.rs            render_ui, UiState, Screen, UiAction, UiModel
src/ui/theme.rs          tokens, install(ctx) (fonts + style), font helpers
src/ui/widgets.rs        buttons, card, stat_card, nav_item, badge, toast, eyebrow
src/ui/hud.rs            in-game overlays
src/ui/screens/{mod,onboarding,home,practice,sensitivity,history,settings}.rs
src/ui/graphs.rs         restyled plots, new signatures
src/main.rs              PlaySession, action handling, history cache
tests/ui_redesign_tests.rs
```

---

### Task 1: Profile data model for setup and calibration results

**Files:**
- Create: `src/data/calibration.rs`
- Modify: `src/data/mod.rs`, `src/data/profile.rs`, `src/sensitivity/psa.rs:15` (derive serde on `PsaRoundConclusion`)
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Produces:
  - `pub struct CalibrationRound { pub tested_cm_360: f64, pub conclusion: PsaRoundConclusion }` (Clone, Debug, Serialize, Deserialize, PartialEq)
  - `pub struct CalibrationResult { pub cm_360: f64, pub confidence: f32, pub psa_cm: Option<f64>, pub stat_cm: Option<f64>, pub rounds: Vec<CalibrationRound>, pub samples: Vec<(f64, f64)>, pub at: DateTime<Utc> }` (Clone, Debug, Serialize, Deserialize)
  - `PlayerProfile.onboarded: bool` with `#[serde(default = "default_onboarded")]` where `fn default_onboarded() -> bool { true }`
  - `PlayerProfile.last_calibration: Option<CalibrationResult>` with `#[serde(default)]`
  - `PlayerProfile::new(..)` sets `onboarded: false`, `last_calibration: None`
  - Re-exported as `aim_trainer::data::{CalibrationResult, CalibrationRound}`

- [ ] **Step 1: Write failing tests** in `tests/ui_redesign_tests.rs`:
  - `old_profile_json_loads_as_onboarded`: deserialize this literal and assert `onboarded == true`, `last_calibration.is_none()`, `dpi == 800.0`:
    ```json
    {"id":"default","name":"Player","dpi":800.0,"primary_game_id":"valorant","current_cm_360":40.0,
     "created_at":"2026-01-01T00:00:00Z","last_played_at":"2026-01-01T00:00:00Z"}
    ```
  - `new_profile_needs_setup`: `PlayerProfile::new("p".into(), "P".into(), 800.0, "valorant".into()).onboarded == false`.
  - `calibration_result_round_trips`: build a `CalibrationResult` with 2 rounds (`TooFastNeedSlower`, `BalancedNarrow`) and 3 samples, serialize to JSON and back on a profile, assert equal `cm_360`, `rounds`, `samples.len() == 3`.
- [ ] **Step 2: Run** `cargo test --test ui_redesign_tests` → FAIL (fields/types missing).
- [ ] **Step 3: Implement** the types and fields above. `PsaRoundConclusion` gains `Serialize, Deserialize`.
- [ ] **Step 4: Run** `cargo test` → all PASS (existing `tests/system_tests.rs` included).
- [ ] **Step 5: Commit** `feat(data): add onboarding flag and persisted calibration result to profiles`

---

### Task 2: Calibration session logic and input helpers

**Files:**
- Create: `src/game/calibration.rs`
- Modify: `src/game/mod.rs` (add `pub mod calibration;`), `src/game/tasks/mod.rs` (add `TaskType::Calibration`), `src/sensitivity/psa.rs` (add accessor), `src/sensitivity/mod.rs` (add two methods)
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Consumes: `CalibrationResult`, `CalibrationRound` (Task 1).
- Produces (all in `aim_trainer::game::calibration` unless noted):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PlayMode { Practice, Calibration }`
  - `#[derive(Clone, Copy, Debug)] pub struct PlaySession { pub mode: PlayMode, pub paused: bool }` with `pub fn records_calibration(&self) -> bool` (true only when `mode == Calibration && !paused`)
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct CalibrationProgress { pub round: usize, pub max_rounds: usize, pub testing_cm_360: f64, pub low_cm_360: f64, pub high_cm_360: f64, pub shot_in_round: usize, pub shots_per_round: usize, pub completed: bool }` with `pub fn from_psa(psa: &PsaCalibrator) -> Self` (`round` clamped to `max_rounds`)
  - `pub fn round_change_message(c: PsaRoundConclusion) -> &'static str` → `"Sensitivity changed: slower"` / `"Sensitivity changed: faster"` / `"Range narrowed"` for `TooFastNeedSlower` / `TooSlowNeedFaster` / `BalancedNarrow`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum ConfidenceLevel { High, Medium, Low }` with `pub fn from_score(s: f32) -> Self` (High ≥ 0.75, Medium ≥ 0.5, else Low) and `pub fn label(&self) -> &'static str` → `"High confidence"`, `"Medium confidence"`, `"Low confidence"`
  - `pub fn delta_text(new_cm: f64, current_cm: f64) -> String` → `"7.4 cm faster than now"` when new < current, `"… slower than now"` when greater, `"Same as now"` when |Δ| < 0.05 (one decimal)
  - `pub fn parse_dpi(s: &str) -> Result<f64, &'static str>`: trims; error `"Enter a number"` if unparsable, `"DPI must be between 50 and 50000"` if outside (50, 50000) exclusive
  - `pub fn parse_game_sens(s: &str) -> Result<f64, &'static str>`: trims, accepts `,` as decimal separator; error `"Enter a number"` / `"Sensitivity must be greater than 0"`
  - `pub fn primary_game<'a>(games: &'a [GameProfile], id: &str) -> Option<&'a GameProfile>`: match by id, else first game, `None` only if `games` is empty
  - `PsaCalibrator::shots_in_round(&self) -> usize` and `pub shots_per_round` made readable via `pub fn shots_per_round(&self) -> usize`
  - `SensitivityEngine::reset_calibration(&mut self)`: replaces `psa` with `PsaCalibrator::new(15.0, 80.0, 7)`, `analyzer` with `StatisticalAnalyzer::new()`, and resets `path_recorder`
  - `SensitivityEngine::build_calibration_result(&self, at: DateTime<Utc>) -> CalibrationResult`: from `compute_final_recommendation()`, `psa.history` → rounds, `analyzer.samples` → `(cm_360, composite_score)`
  - `TaskType::Calibration` → `set_task` builds `FlickTask::new(usize::MAX, 0.35)`; the task never finishes on its own (main ends it when PSA completes)

- [ ] **Step 1: Write failing tests**:
  - `play_session_gates_calibration`: Practice/unpaused → false; Calibration/paused → false; Calibration/unpaused → true.
  - `progress_reflects_psa`: fresh `PsaCalibrator::new(15.0, 80.0, 7)`, record 4 `Direct` shots → progress `round == 1`, `shot_in_round == 4`, `testing_cm_360 == 47.5`, `low == 15.0`, `high == 80.0`, `completed == false`. Record 6 `Overshoot` more (round ends, overshoot 60%) → `round == 2`, `shot_in_round == 0`, `low_cm_360 == 47.5`.
  - `confidence_thresholds`: 0.75 → High, 0.7499 → Medium, 0.5 → Medium, 0.49 → Low, labels as above.
  - `delta_text_cases`: (34.2, 41.6) → `"7.4 cm faster than now"`; (45.0, 40.0) → `"5.0 cm slower than now"`; (40.02, 40.0) → `"Same as now"`.
  - `parse_dpi_cases`: `" 800 "` → Ok(800.0); `""`, `"abc"` → Err("Enter a number"); `"0"`, `"50"`, `"60000"` → Err("DPI must be between 50 and 50000").
  - `parse_game_sens_cases`: `"0,35"` → Ok(0.35); `"0.35"` → Ok(0.35); `"-1"`, `"0"` → Err("Sensitivity must be greater than 0"); `"x"` → Err("Enter a number").
  - `primary_game_falls_back`: games `[valorant, cs2]`; id `"cs2"` → cs2; id `"gone"` → valorant; empty slice → None.
  - `reset_and_build_result`: `SensitivityEngine::new(800.0)`, push 10 `Overshoot` shots into `psa`, `reset_calibration()` → `psa.history.is_empty()`, `analyzer.samples.is_empty()`; then 10 `Overshoot` shots → `build_calibration_result(Utc::now())` has `rounds.len() == 1` with `TooFastNeedSlower`.
- [ ] **Step 2: Run** `cargo test --test ui_redesign_tests` → FAIL.
- [ ] **Step 3: Implement** the interfaces above. `TaskType::Calibration`'s task name is `"Calibration"` (wrap `FlickTask` or override name; builder's choice).
- [ ] **Step 4: Run** `cargo test` → PASS.
- [ ] **Step 5: Commit** `feat(game): add calibration session logic, progress view, and input parsing`

---

### Task 3: Theme, fonts, and icons

**Files:**
- Create: `assets/fonts/*` (5 files), `src/ui/theme.rs`
- Modify: `Cargo.toml` (add `egui-phosphor = "0.9"`), `src/ui/mod.rs` (add `pub mod theme;`)
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Produces in `aim_trainer::ui::theme`:
  - Colour constants named exactly as the spec table: `BG, SIDEBAR, SURFACE, SURFACE_RAISED, BORDER, BORDER_STRONG, TEXT, TEXT_SECONDARY, TEXT_MUTED, TEXT_FAINT, ACCENT, ON_ACCENT, ACCENT_BORDER, SUCCESS_BG, SUCCESS, WARN_BG, WARN, DANGER` (`Color32`), plus `HUD_BG = BG at alpha ~217` (85 %)
  - Spacing consts `S1=4.0, S2=8.0, S3=12.0, S4=16.0, S6=24.0, S8=32.0`; radius consts `R_CONTROL=8, R_CARD=10, R_HUD=12` (u8)
  - `pub fn install(ctx: &egui::Context)`: fonts + `Style`; called once at startup
  - Font families: `Proportional` = Inter Regular → phosphor → egui defaults; `FontFamily::Name("medium")` = Inter Medium → phosphor; `Name("display")` = Chakra Petch Medium → phosphor; `Name("display_bold")` = Chakra Petch SemiBold → phosphor
  - Helpers `pub fn body(size: f32) -> FontId`, `medium(size)`, `display(size)`, `display_bold(size)`
  - Style: dark visuals, `panel_fill = BG`, `window_fill = SURFACE`, widget fills/strokes from tokens, corner radius `R_CONTROL`, `item_spacing = (S2, S2)`, `button_padding = (S4, S2)`, `interact_size.y = 36`, selection colour `ACCENT` at low alpha, `animation_time = 0.12`

- [ ] **Step 1: Ask the user to approve the font downloads**: Chakra Petch Medium/SemiBold and `OFL.txt` from `https://github.com/google/fonts/tree/main/ofl/chakrapetch`, Inter 4.x static Regular/Medium TTFs from the `rsms/inter` GitHub release. Save into `assets/fonts/` (append Inter's licence to `OFL.txt` or keep `OFL-Inter.txt`).
- [ ] **Step 2: Write failing test** `theme_installs_font_families`: `let ctx = egui::Context::default(); theme::install(&ctx);` then run one frame (`ctx.run(Default::default(), |_| {})`) and assert `ctx.fonts(|f| f.families())` contains `Name("display")`, `Name("display_bold")`, `Name("medium")`.
- [ ] **Step 3: Run** `cargo test --test ui_redesign_tests theme` → FAIL.
- [ ] **Step 4: Implement** `theme.rs` per Interfaces; phosphor via `egui_phosphor::Variant::Regular.font_data()` inserted into each family at index 1 (don't call `add_to_fonts`, so each named family gets it).
- [ ] **Step 5: Run** `cargo test` → PASS.
- [ ] **Step 6: Commit** `feat(ui): add design tokens, embedded fonts, and phosphor icons`

---

### Task 4: Widget kit

**Files:**
- Create: `src/ui/widgets.rs`
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Consumes: `theme` (Task 3).
- Produces in `aim_trainer::ui::widgets` (all return `egui::Response` unless noted):
  - `primary_button(ui, icon: Option<&str>, label: &str)`: `ACCENT` fill, `ON_ACCENT` text (`medium(14)`), hover lightens via `animate_bool_with_time(…, 0.12)`
  - `secondary_button(ui, icon, label)`: transparent, `BORDER_STRONG` stroke, hover fill `SURFACE_RAISED`
  - `ghost_button(ui, icon, label)`: text only, `TEXT_SECONDARY` → `TEXT` on hover
  - `card<R>(ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R>`: `SURFACE`, 1px `BORDER`, `R_CARD`, padding `S4`; `highlight_card` same with `ACCENT_BORDER`
  - `stat_card(ui, label: &str, value: &str, unit: Option<&str>, caption: &str)`: label `body(12) TEXT_MUTED`, value `display(22)`, unit muted; when `value == "—"` use `TEXT_FAINT`
  - `nav_item(ui, icon: &str, label: &str, selected: bool)`: full-width row, height 36, selected → `SURFACE_RAISED` fill, icon `ACCENT`, text `TEXT`; else `TEXT_MUTED`
  - `badge(ui, text: &str, level: ConfidenceLevel)`: High → `SUCCESS_BG`/`SUCCESS`, Medium → `WARN_BG`/`WARN`, Low → `SURFACE_RAISED`/`TEXT_SECONDARY`
  - `eyebrow(ui, text: &str)`: uppercase, `body(11)`, `TEXT_MUTED`, letter-spaced (use `RichText::extra_letter_spacing(1.0)`)
  - `field_error(ui, msg: &str)`: `body(12)` in `DANGER`
  - `pub fn toast_alpha(age_secs: f64) -> f32`: 0→1 over first 0.15 s, 1 until 2.5 s, 1→0 over next 0.25 s, 0 after 2.75 s

- [ ] **Step 1: Write failing tests**: `toast_alpha_curve` (0.0 → 0.0, 0.15 → 1.0, 1.0 → 1.0, 2.625 → 0.5 ±0.01, 3.0 → 0.0); `widgets_render_headless`: install theme, one `ctx.run` frame with a `CentralPanel` that calls every widget once; assert no panic.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** widgets per Interfaces.
- [ ] **Step 4: Run** `cargo test` → PASS.
- [ ] **Step 5: Commit** `feat(ui): add themed widget kit`

---

### Task 5: App shell, UiModel, UiAction, and sidebar

**Files:**
- Modify: `src/ui/mod.rs` (rewrite; old screen functions move/disappear in later tasks; keep the crate compiling by stubbing screens)
- Create: `src/ui/screens/mod.rs` + stub files for the six screens
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces in `aim_trainer::ui`:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Screen { Home, Practice, Sensitivity, History, Settings }`
  - `#[derive(Clone, Debug, PartialEq)] pub enum UiAction { Navigate(Screen), StartCalibration, QuitCalibration, StartPractice(TaskType), Retry, Resume, ApplySensitivity(f64), CompleteOnboarding { dpi: f64, game_id: String, cm_360: f64 }, RerunSetup, SaveDpi(f64), SetMainGame(String), SetAudio { enabled: bool, volume: f32 }, CreateProfile(String), SelectProfile(String), DeleteProfile(String) }`
  - `pub struct UiModel<'a> { pub profile: &'a PlayerProfile, pub profiles: &'a [PlayerProfile], pub games: &'a [GameProfile], pub history: &'a SessionHistory, pub session: PlaySession, pub task_state: TaskState, pub task_name: &'static str, pub last_metrics: Option<&'a TaskMetrics>, pub calibration: Option<CalibrationProgress>, pub current_cm_360: f64, pub audio_enabled: bool, pub volume: f32, pub fps: u32 }`
  - `pub struct UiState { pub screen: Screen, pub menu_open: bool, pub onboarding: Option<OnboardingDraft>, pub toast: Option<(String, f64)>, … screen-local fields as needed }` with `pub fn new() -> Self` (screen Home, menu open) and `pub fn show_toast(&mut self, text: impl Into<String>, now: f64)`
  - `pub struct OnboardingDraft { pub step: u8, pub dpi: String, pub game_id: String, pub sens: String }`
  - `pub fn render_ui(ctx: &egui::Context, state: &mut UiState, model: &UiModel) -> Vec<UiAction>`: if `!state.menu_open` → `hud::render` (Task 7); else sidebar + content `ScrollArea`; if `!model.profile.onboarded` and `state.onboarding.is_none()` → create draft from profile; draft present → onboarding overlay instead of the screen
- Shell layout: `SidePanel::left` exact width 200, fill `SIDEBAR`, right border `BORDER`; logo row (`regular::CROSSHAIR` in `ACCENT` + "SENSE" in `display_bold(16)`); nav items Home (`HOUSE`), Practice (`TARGET`), My sensitivity (`SLIDERS_HORIZONTAL`), History (`CHART_LINE`); spacer; Settings (`GEAR`); profile chip "Name · 800 DPI" (clicking → Settings). `CentralPanel` fill `BG`, padding `S8`, content in vertical `ScrollArea`, max content width 960 centred.

- [ ] **Step 1: Write failing test** `shell_renders_each_screen_headless`: build a `UiModel` from in-memory values (one onboarded profile, `GameDatabase::new().list()`, empty `SessionHistory`), for each `Screen` and for screen sizes 1280×720 and 1024×640 (`RawInput { screen_rect: Some(..), .. }`) run one frame; assert no panic and `render_ui` returns an empty `Vec`. Also `onboarding_draft_created_for_new_profile`: profile with `onboarded = false` → after one frame `state.onboarding.is_some()`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** shell per Interfaces; screen stubs render a heading only. Update `main.rs` minimally so it compiles: build `UiModel` each frame, call `theme::install` once in `GameState::new`, ignore returned actions for now (Task 8 wires them). Cache `SessionHistory` on `GameState` (`history: SessionHistory`) instead of reading disk per frame.
- [ ] **Step 4: Run** `cargo test && cargo build` → PASS.
- [ ] **Step 5: Commit** `feat(ui): new app shell with sidebar, UiModel snapshot, and UiAction output`

---

### Task 6: Screens: onboarding, home, practice, sensitivity

**Files:**
- Modify: `src/ui/screens/{onboarding,home,practice,sensitivity}.rs`, `src/ui/graphs.rs`
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–5 (`parse_dpi`, `parse_game_sens`, `primary_game`, `ConfidenceLevel`, `delta_text`, widgets, `UiAction`).
- Produces: `pub fn show(ui: &mut Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>)` in each screen module; `graphs::psa_convergence(ui, rounds: &[CalibrationRound])`, `graphs::performance_curve(ui, samples: &[(f64, f64)], optimal: Option<f64>)`, `graphs::reaction_times(ui, rts: &[f32])` with theme colours (points/line `ACCENT`, optimal marker `SUCCESS`, axes `TEXT_MUTED`), plot height 220, no emoji headings.

Screen content (exact copy where quoted):
- **Onboarding** (centred card, max width 480, step dots 1–3, Back / Continue as secondary / primary):
  1. "What's your mouse DPI?" field + preset chips 400/800/1600/3200; `parse_dpi` error shown under the field via `field_error`, Continue does nothing while invalid.
  2. "Which game do you play most?" list of `model.games` as selectable rows.
  3. "What's your sensitivity in {game}?" field, live caption "= {x:.1} cm/360°" via `SensitivityConverter::game_sens_to_cm_360`; buttons "Skip" (ghost; keeps `profile.current_cm_360`) and "Finish" (primary) → `CompleteOnboarding`.
- **Home**: eyebrow "~5 min"; `display_bold(30)` "Find the sensitivity your hand already prefers"; body "Shoot 70 targets. We watch whether you overshoot or undershoot, then narrow it down round by round."; primary "Start calibration" (`PLAY`) → `StartCalibration`; secondary "Warm up first" → `StartPractice(TaskType::Flick)`; three `stat_card`s: "Your current" (`{cm:.1}` cm/360, caption "{game} {sens:.3}"), "Last result" (`last_calibration` cm or "—", caption date "%b %d" or "Not calibrated yet"), "Sessions" (count with `timestamp` in last 7 days, caption "This week").
- **Practice**: heading "Practice", body "Free play at your current sensitivity. Nothing here changes it."; 5 cards in a wrapping grid (min card width 280): Flick (`LIGHTNING`), Tracking (`WAVE_SINE`), Target switching (`SHUFFLE`), Micro-adjust (`MAGNIFYING_GLASS_PLUS`), Speed (`TIMER`), each with one-line description, meta ("30 targets", "30 seconds", "25 switches", "30 targets", "30 targets"), hotkey hint "Key 1"…"Key 5"; whole card clickable → `StartPractice(..)`.
- **Sensitivity**: if `last_calibration` is `None` → empty state: heading "You haven't calibrated yet", body "One run takes about five minutes.", primary "Start calibration", then a "Convert a sensitivity" card (game picker + value field → list of other games). Else result layout per mockup 3: eyebrow "Your sensitivity", `display_bold(56)` in `ACCENT`, "cm / 360° · {dpi} DPI", `badge(ConfidenceLevel::from_score(..))`, `delta_text(..)`; main game in `highlight_card` with secondary "Copy" (`COPY`) → `ctx.copy_text(format!("{:.3}", v))`, label "Copied" for 1.5 s; next 2 games as rows, "+ N more games" ghost toggles the rest; primary "Use this sensitivity" (`CHECK`) → `ApplySensitivity(cm)` (hidden when |Δ| < 0.05), secondary "Run again" (`ARROWS_CLOCKWISE`) → `StartCalibration`; section "How we got here" with `psa_convergence` and `performance_curve`.

- [ ] **Step 1: Write failing tests**: `sessions_this_week_counts_last_7_days` for a helper `pub fn sessions_this_week(h: &SessionHistory, now: DateTime<Utc>) -> usize` in `screens::home` (sessions at now−1d, now−6d23h, now−8d → 2); extend the headless smoke test to render Sensitivity with `last_calibration = Some(..)` and with `None`, and onboarding at steps 1/2/3.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** the four screens and graph restyle.
- [ ] **Step 4: Run** `cargo test` → PASS.
- [ ] **Step 5: Commit** `feat(ui): onboarding, home, practice, and sensitivity screens`

---

### Task 7: Screens: history and settings; in-game HUD

**Files:**
- Modify: `src/ui/screens/{history,settings}.rs`
- Create: `src/ui/hud.rs`
- Test: `tests/ui_redesign_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–6.
- Produces: `screens::history::show`, `screens::settings::show` (same signature as Task 6); `pub fn hud::render(ctx: &egui::Context, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>)`.

Content:
- **History**: empty state "No sessions yet" / "Finish a calibration or practice run and it shows up here." Else table-like rows (newest first): scenario, date "%b %d %H:%M", score, accuracy "{:.0}%", cm/360 "{:.1}"; clicking a row selects it (`state.selected_session`), selected row `SURFACE_RAISED`; below, `graphs::reaction_times` for the selected (default latest) session.
- **Settings** cards: Mouse (DPI field + "Save" secondary → `SaveDpi`, `field_error` on invalid, "Saved" caption 1.5 s); Main game (combo → `SetMainGame`); Audio (toggle + volume slider → `SetAudio` on change); Profiles (rows: name, DPI, "Active" badge or "Switch" secondary → `SelectProfile`; delete ghost on non-active rows opens inline confirm "Delete {name}? This can't be undone." with "Delete" in `DANGER` → `DeleteProfile`, "Cancel"); new-profile field + "Add profile" → `CreateProfile` (ignored when trimmed name empty); ghost "Run setup again" → `RerunSetup`.
- **HUD** (`menu_open == false`):
  - Practice pill top-centre (`HUD_BG`, `BORDER_STRONG`, `R_HUD`): "{task} · {cm:.1} cm/360 · {fps} FPS (muted) · Esc menu".
  - Calibration panel top-centre per mockup 2: "Round {r} / {max}", segmented bar (`max_rounds` segments of 14×4, done = `ACCENT`, current = `ACCENT` at 45 % alpha, rest `BORDER_STRONG`), "Testing {cm:.1} cm · range {low:.0}–{high:.0}", "Shot {n} / {per}"; bottom-right hint "Esc · pause".
  - Toast bottom-centre while `state.toast` age < 2.75 s, alpha from `toast_alpha`, icon `ARROWS_LEFT_RIGHT` in `ACCENT`.
  - Countdown: `display_bold(96)` digit in `ACCENT` (or "Go"), below "Get ready" `medium(16)`.
  - Paused (calibration, `session.paused`): centred card "Calibration paused", body "Your progress is kept.", primary "Resume" → `Resume`, secondary "Quit calibration" → `QuitCalibration`.
  - Practice finished: centred card "{task} complete", three stats (Score, Accuracy, Avg reaction), primary "Retry" + caption "R" → `Retry`, secondary "Back to menu" + caption "Esc" → `Navigate(Screen::Practice)`.

- [ ] **Step 1: Write failing tests**: extend headless smoke to History (empty and 3 sessions), Settings, and HUD for: practice running, calibration running with a toast, countdown, paused, finished (`last_metrics` Some).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**; delete the old `render_*_view`, `render_top_navbar`, `apply_custom_style`, and old `theme` module from `ui/mod.rs`.
- [ ] **Step 4: Run** `cargo test` → PASS; `grep -rn "from_rgb" src/ui --include=*.rs` shows matches only in `theme.rs`.
- [ ] **Step 5: Commit** `feat(ui): history and settings screens, new in-game HUD`

---

### Task 8: Wire actions and calibration mode into main.rs

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: everything above.
- `GameState` gains `session: PlaySession`, `history: SessionHistory` (from Task 5); `ui_state: UiState` (new type); drops `player_dpi` duplication if convenient.

Behaviour to implement (each bullet is checked in Step 2):
- Startup: `theme::install` once; `TaskManager` starts `NotStarted` (no auto Flick countdown); menu open on Home.
- `apply_action(&mut self, a: UiAction)` handles every variant:
  - `StartCalibration`: `sensitivity_engine.reset_calibration()`, `current_cm_360 = psa.current_test_cm_360`, `session = {Calibration, paused: false}`, `set_task(Calibration)` + countdown, close menu, lock cursor, `last_countdown_sec = 4`.
  - `StartPractice(t)`: `session = {Practice, false}`, `current_cm_360 = profile.current_cm_360`, start task, close menu, lock cursor.
  - `Retry`: restart current task (practice only).
  - `Resume`: calibration → `paused = false`, restart the calibration task with countdown, `path_recorder.start_shot()`; practice → close menu; lock cursor either way.
  - `QuitCalibration`: `reset_calibration()`, `session.mode = Practice`, task `NotStarted`, `current_cm_360 = profile.current_cm_360`, open menu on Home. No session recorded.
  - `ApplySensitivity(cm)`: set `profile.current_cm_360`, save profile, `current_cm_360 = cm`, toast "Sensitivity applied".
  - `CompleteOnboarding{..}`: set dpi/game/cm, `onboarded = true`, save, `sensitivity_engine.player_dpi = dpi`, `ui_state.onboarding = None`, screen Home.
  - `RerunSetup`: `ui_state.onboarding = Some(draft from profile)`.
  - `SaveDpi`, `SetMainGame`, `SetAudio`, `CreateProfile` (new profile `onboarded = false`), `SelectProfile` (also reload `history`, set dpi and `current_cm_360`), `DeleteProfile`, `Navigate`.
- Shooting: PSA `record_shot` and `analyzer.record_sample` run only when `session.records_calibration()`; `MetricPoint` collection runs in both modes. On round done: `current_cm_360 = psa.current_test_cm_360` and `ui_state.show_toast(round_change_message(last round conclusion), now)` where `now = egui_ctx.input(|i| i.time)`.
- Calibration completes when `psa.is_completed` after a shot: build `CalibrationResult`, store in `profile.last_calibration`, save profile, record a `SessionRecord { task_type: "Calibration", tested_cm_360: result.cm_360, .. }`, refresh `history`, play complete sound, restore `current_cm_360 = profile.current_cm_360`, `session.mode = Practice`, task `NotStarted`, open menu on `Screen::Sensitivity`, unlock cursor.
- Practice `Finished` (existing auto-save block) records the session and refreshes `history`; skip it for `TaskType::Calibration`.
- `task_manager.update` is skipped while the menu is open or `session.paused`.
- Esc: calibration running → `paused = true`, unlock cursor (HUD shows pause card, menu stays closed); calibration paused → same as `Resume`; otherwise toggle menu (existing behaviour).
- Keys 1–5 and R ignored during calibration; C opens menu on `Screen::Sensitivity`.
- Mouse click to re-lock the cursor is ignored while `session.paused` (so the pause card is clickable).

- [ ] **Step 1: Implement** the behaviour above.
- [ ] **Step 2: Verify**: `cargo build` clean (no new warnings in `src/ui` or `main.rs`); `cargo test` PASS; run `cargo run` and walk: fresh `%APPDATA%/AimTrainer` (rename the folder first and restore it afterwards) → setup appears → finish → Home → Start calibration → HUD shows Round 1 / 7 and shot counter → Esc shows pause card → Resume counts down → complete 7 rounds or convergence → Sensitivity screen shows result, Copy works (paste into Notepad), Use this sensitivity updates Home → Practice Flick runs at the applied cm/360 and HUD sens never changes → History lists the Calibration and Flick sessions.
- [ ] **Step 3: Commit** `feat: wire guided calibration flow and UI actions into the game loop`

---

### Task 9: Docs and visual pass

**Files:**
- Modify: `README.md` (Controls table: 1–5 and R are practice-only, Esc pauses calibration; new "First run" and "Calibration" sections; remove emoji from headings only if you touch them anyway)

- [ ] **Step 1: Visual pass** with `cargo run` at 1280×720 and resized to 1024×640: compare Home, calibration HUD, and Result with `docs/superpowers/specs/2026-11-16-ui-redesign-mockups.html`; check one primary button per screen, no clipped text, no tofu glyphs, hover states on every clickable element, scroll works on the Sensitivity screen at 1024×640. Fix what's off.
- [ ] **Step 2: Update README** per Files.
- [ ] **Step 3: Run** `cargo test && cargo build --release` → PASS.
- [ ] **Step 4: Commit** `docs: describe guided calibration flow and updated controls`
