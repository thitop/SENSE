# UI/UX Redesign: Guided Sensitivity Flow

Date: 2026-11-16 · Status: draft for approval

## Intent

The app exists to find a player's ideal mouse sensitivity. The current UI (one
894-line `src/ui/mod.rs`, five top tabs) hides that purpose: on launch it shows
an "optimal sensitivity" before any testing (the 40 cm/360 fallback), it drops
straight into a Flick countdown, the PSA calibrator silently changes the
player's sensitivity during *any* scenario, and emoji icons may render as tofu.

The owner wants the whole UI/UX rebuilt so it looks modern and polished, reads
as deliberately designed rather than "AI slop", and makes people want to try it.

## Goals

- A first-time user knows within seconds what to do, and reaches a usable
  sensitivity number in one guided run (~5 min, 70 shots).
- The look is consistent, restrained, and game-native: one accent colour, real
  fonts, real icons, clear hierarchy, big readable numbers.
- The player is never surprised: sensitivity only changes during calibration,
  and every change is announced.
- Results are actionable: the player's own game is shown first, with a copy
  button.

## Non-goals

- No changes to the 3D renderer, shaders, target physics, or the PSA/statistics
  maths (beyond *when* they run).
- No new scenarios, no online features, no light theme.
- No localisation framework; the UI is English only.

## Anti-goals (failure even if it "works")

- Purple/neon gradients, glow, glassmorphism, emoji icons, ALL-CAPS everywhere,
  several competing accent colours: the "AI slop" look the owner rejected.
- Showing a recommended sensitivity that wasn't measured.
- More than one primary (yellow) button per screen.
- Frame-rate regressions during gameplay from UI work.

## Constraints

- Stack stays: Rust, winit 0.30, wgpu 24, **egui 0.31** (+ egui-wgpu,
  egui-winit, egui_plot 0.31). No blur or soft shadows; flat surfaces only.
- Icons: **`egui-phosphor = "0.9"`** (the release built for egui 0.31; 0.10+
  needs egui 0.32). Regular weight.
- Fonts: **Chakra Petch** (display + numbers, 500/600) and **Inter** (UI text,
  400/500), both SIL OFL. TTF files committed under `assets/fonts/` with their
  `OFL.txt`, embedded via `include_bytes!`.
- Existing profile JSON files in `%APPDATA%/AimTrainer/profiles/` must still
  load: new fields use `#[serde(default)]`.
- `cargo test` must keep passing.
- Window is 1280×720 by default, resizable; layout must work from 1024×640 up.

## Approaches considered

| | Approach | Trade-off |
|---|---|---|
| **A (picked)** | Guided "find your sensitivity" flow: first-run setup → Home with one primary CTA → calibration run with visible progress → result screen. Sidebar nav. Plus the full visual reskin. | Most work; touches `main.rs` so PSA only runs during calibration. Solves both the look and the "what do I do" problem. |
| B | Keep 5 top tabs, reskin only (theme, fonts, icons). | Fast, low risk, but the confusing flow and fake recommendation stay. |
| C | Menu floats over the live, moving 3D scene. | Most atmospheric, but needs renderer work, hurts legibility, and doesn't fix the flow. Could be layered on A later. |

Owner picked **A** after seeing mockups of Home, the calibration HUD, and the
Result screen, and approved that look ("Looks good"). The approved mockups are
saved as `2026-11-16-ui-redesign-mockups.html` next to this file. They set the
look, proportions, and hierarchy; where this document is more specific (sizes,
behaviour, copy), this document wins. Mockup numbers are illustrative and
Tabler icons stand in for Phosphor.

## Visual system

### Colour tokens (`ui/theme.rs`)

| Token | Hex | Use |
|---|---|---|
| `BG` | `#0B0C0E` | Window / content background |
| `SIDEBAR` | `#0F1013` | Sidebar |
| `SURFACE` | `#141518` | Cards |
| `SURFACE_RAISED` | `#1B1D21` | Selected nav item, hover |
| `BORDER` | `#232529` | Card hairline |
| `BORDER_STRONG` | `#2C2F35` | Secondary buttons, inputs |
| `TEXT` | `#ECECEE` | Primary text |
| `TEXT_SECONDARY` | `#A3A6AD` | Body copy |
| `TEXT_MUTED` | `#8A8D94` | Labels, captions |
| `TEXT_FAINT` | `#6B6E75` | Empty values ("—") |
| `ACCENT` | `#FFD23F` | Target yellow: the single accent |
| `ON_ACCENT` | `#1A1500` | Text on accent |
| `ACCENT_BORDER` | `#4A3F12` | Highlighted card border (player's game) |
| `SUCCESS_BG` / `SUCCESS` | `#1F2A1C` / `#9BD67A` | Confidence "High" badge |
| `WARN_BG` / `WARN` | `#2E2612` / `#E8B75A` | Confidence "Medium" |
| `DANGER` | `#E5484D` | Destructive actions, validation errors |

All UI code uses these tokens; no raw `Color32::from_rgb` outside `theme.rs`.

### Type scale

| Role | Font | Size |
|---|---|---|
| Hero number (result) | Chakra Petch 600 | 56 |
| Display heading | Chakra Petch 600 | 30 |
| Stat number | Chakra Petch 500 | 22 |
| Section heading | Inter 500 | 16 |
| Body | Inter 400 | 14 |
| Label / caption | Inter 400 | 12 (letter-spaced uppercase only for tiny eyebrow labels) |

Register two egui font families: `Proportional` = Inter (+ Phosphor fallback),
`Name("display")` = Chakra Petch. Numbers use the display family.

### Spacing, shape, motion

- Spacing scale 4 / 8 / 12 / 16 / 24 / 32. Card padding 16, screen padding 32.
- Radius: 8 for controls, 10 for cards, 12 for floating HUD panels.
- Hover and selection transitions via `ctx.animate_bool_with_time`, ~120 ms;
  toasts fade in 150 ms / out 250 ms. Nothing animates during gameplay except
  HUD toasts.
- Buttons: `primary` (accent fill, one per screen), `secondary` (border only),
  `ghost` (text only). Minimum height 36.

## Information architecture

Left sidebar (fixed ~200 px): logo, then **Home · Practice · My sensitivity ·
History**, spacer, **Settings**, then a profile chip ("Player · 800 DPI").
Selected item uses `SURFACE_RAISED` + accent icon. The current top navbar and
"Resume aiming" button go away; Esc still returns to the game if a session is
paused.

### First-run setup (overlay, 3 steps)

Shown when the active profile has `onboarded == false`.

Migration (Claude's call): a profile JSON that loads without the `onboarded`
field counts as already set up (custom serde default returning `true`), so
existing users are not pushed through setup. Profiles created fresh (the
first-launch default profile, or "Add profile" in Settings) are created with
`onboarded = false` and go through setup.

1. Mouse DPI: numeric field, presets 400 / 800 / 1600 / 3200, inline error if
   outside 50–50 000.
2. Main game: list from `GameDatabase`.
3. Current in-game sensitivity: numeric field; live preview of the resulting
   cm/360 via `SensitivityConverter::game_sens_to_cm_360`. "Skip" keeps 40.

Finish saves `dpi`, `primary_game_id`, `current_cm_360`, `onboarded = true` to
the profile and lands on Home. Settings has a "Run setup again" action.

### Home

- Eyebrow "~5 min", display heading "Find the sensitivity your hand already
  prefers", one-line explanation.
- Primary: **Start calibration**. Secondary: **Warm up first** (starts Flick
  practice).
- Three stat cards: *Your current* (cm/360 + main-game value), *Last result*
  (persisted calibration or "Not calibrated yet"), *Sessions this week*.

### Calibration run

- Starting it resets `SensitivityEngine.psa` (fresh `PsaCalibrator::new(15, 80,
  7)`) and `analyzer`, closes the menu, locks the cursor, shows the 3-2-1
  countdown, and runs a Flick-style task that keeps spawning targets until PSA
  reports `is_completed`.
- PSA and analyzer record shots **only** in calibration mode. Practice runs at
  the profile's `current_cm_360` and never changes it.
- HUD (top centre): "Round 3 / 7", a 7-segment progress bar, "Testing 38.4 cm ·
  range 31–46", "Shot 4 / 10". Bottom-right hint "Esc · pause".
- When a round finishes and sensitivity changes, a toast for ~2.5 s:
  "Sensitivity changed: slower" / "faster" / "range narrowed".
- Esc pauses: a centred card with **Resume** and **Quit calibration**
  (discard progress).
- On completion, open the menu on the Result screen.

### Result (inside "My sensitivity")

- Eyebrow "Your sensitivity", hero number in accent (cm/360), "cm / 360° ·
  800 DPI".
- Confidence badge from `confidence_score`: High ≥ 0.75, Medium ≥ 0.5, else
  Low. Delta vs current ("7.4 cm faster than now").
- Conversion list: player's main game first in a highlighted card with a
  **Copy** button (`ctx.copy_text`, button shows "Copied" for 1.5 s); the
  rest follow as rows, collapsed behind "+ N more games".
- Actions: primary **Use this sensitivity** (sets `current_cm_360`, saves the
  profile), secondary **Run again**.
- Each completed calibration is persisted on the profile
  (`last_calibration: Option<CalibrationResult { cm_360, confidence, psa_cm,
  stat_cm, rounds: Vec<CalibrationRound { tested_cm_360, conclusion }>,
  samples: Vec<(f64 cm_360, f64 composite_score)>, at }>`) so Home and this
  screen survive restarts.
- Below the actions, a "How we got here" section shows the two calibration
  graphs (PSA convergence and the performance curve), drawn from
  `last_calibration.rounds` / `samples` (Claude's call: these graphs explain a
  calibration, so they live with its result rather than in History).
- A completed calibration is also saved as a `SessionRecord` with
  `task_type = "Calibration"` and `tested_cm_360` = the recommended cm/360. It
  appears in History and counts toward "Sessions this week". A quit
  calibration is discarded and not recorded (Claude's call).
- With no calibration yet, this screen shows an empty state inviting the user
  to calibrate, plus a manual converter (enter a sens in one game, see the
  others). No fake recommendation.

### Practice

Grid of the five scenarios (Flick, Tracking, Switching, Micro-adjust, Speed):
Phosphor icon, name, one-line description, meta ("30 targets"), hotkey hint
(1–5). Clicking a card starts it. The finished overlay becomes a card with
score / accuracy / avg reaction time and buttons **Retry (R)** and **Back to
menu (Esc)**.

### History

Session list (newest first: scenario, date, score, accuracy, cm/360), and a
reaction-time plot for the selected session (default: latest), restyled with
theme colours. Clicking a row selects it. Empty state when there are no
sessions. The other two graphs moved to the Result screen (see above).

### Settings

Cards for: Mouse (DPI with inline validation and save feedback), Main game,
Audio (toggle + volume), Profiles (list, switch, add, delete with confirm),
"Run setup again".

### In-game HUD

Practice: a slim top-centre pill (scenario name · cm/360 · FPS in muted text ·
"Esc menu"). Calibration HUD as above. Countdown: large Chakra Petch digit in
accent plus "Get ready". All panels use `BG` at ~85 % alpha with
`BORDER_STRONG`.

### Hotkeys (unchanged behaviour)

Esc (menu/pause), 1–5 (practice scenarios, not during calibration), R (retry,
not during calibration), C (open My sensitivity).

## Code structure

```
src/ui/
  mod.rs          render_ui entry, UiState, Screen enum, UiAction enum
  theme.rs        colour tokens, fonts setup, egui Style
  widgets.rs      card, stat_card, primary/secondary/ghost buttons, nav_item,
                  badge, toast, section_header
  hud.rs          practice HUD, calibration HUD, countdown, finished/paused cards
  screens/
    onboarding.rs home.rs practice.rs sensitivity.rs history.rs settings.rs
  graphs.rs       restyled plots
assets/fonts/     ChakraPetch-Medium.ttf, ChakraPetch-SemiBold.ttf,
                  Inter-Regular.ttf, Inter-Medium.ttf, OFL.txt
```

The UI stops mutating game state directly. `render_ui` returns a
`Vec<UiAction>` (e.g. `StartCalibration`, `StartPractice(TaskType)`,
`Resume`, `QuitCalibration`, `ApplySensitivity(f64)`, `SaveDpi(f64)`, ...) and
`main.rs` applies them, handling cursor lock, PSA reset, and persistence in one
place. `GameState` gains a `mode: Mode { Practice, Calibration }`. Fonts are
installed once at startup, not every frame (today `apply_custom_style` runs
every frame).

## Testing

- Unit tests for the non-visual logic: confidence label thresholds, onboarding
  cm/360 conversion, `PlayerProfile` deserialising an old JSON without the new
  fields (loads with `onboarded == true`, `last_calibration == None`), calibration mode gating (PSA ignores practice shots), and
  `UiAction` handling where it can be tested without a window.
- Existing `cargo test` suite stays green.
- Manual visual pass: run the app at 1280×720 and 1024×640, walk first-run →
  calibration → result → practice → history → settings, and compare against
  the approved mockups.

## Decisions made by Claude (owner may override)

- Single accent = target yellow, matching the in-game targets.
- Chakra Petch + Inter; Phosphor Regular icons.
- Sidebar navigation instead of top tabs.
- PSA/analyzer run only during calibration; practice never changes
  sensitivity.
- Calibration results persist on the profile.
- `UiAction` pattern to decouple UI from game state.
- Existing profiles skip first-run setup; new profiles go through it.
- Calibration graphs live on the Result screen, backed by persisted rounds and
  samples; History keeps the session list and reaction-time plot.
- Completed calibrations are recorded as "Calibration" sessions; quit runs are
  not.

## Left to the builder

- Exact widget implementation details, file split within `ui/`, and how the
  calibration task is implemented (new `TaskType::Calibration` or a
  `FlickTask` with unbounded targets).
- Exact copy wording within the tone above (short, sentence case, no "!").
- Which Phosphor glyph maps to each item.

## UI language

English (owner's decision, 2026-11-16). No Thai font fallback needed.
