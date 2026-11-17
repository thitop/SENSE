# Aim habits: what to practise, and how far to trust a result

## Why

Players want to know where their aim goes wrong, the way Oblivity's analysis section shows it.
The owner wants two things from it:

1. **What to practise.** Name the player's habits (far flicks stop short, tracking trails the
   target, ...) and send them to the scenario that trains it.
2. **A sensitivity result they can trust.** Keep habits apart from sensitivity, so a player
   doesn't change sensitivity to fix something sensitivity can't fix, and say plainly when a
   run's data was too unsteady to lean on.

The paired calibration already cancels habits out of the *result*: a habit shows on the faster
and the slower side alike, so it drops out of the comparison. That is right for the result, and
it is also exactly what the player wants to see. This feature reports the part the fit throws
away.

The owner also wants to see whether practice is working: a habit named after calibration should
be visible going down (or not) across practice sessions.

## What exists today

- `MousePathRecorder` records every shot's mouse path (calibration and practice). Its
  `ShotTrajectoryPattern` works in mouse counts, so the same hand movement classifies
  differently at different DPI (`micro_signal.rs` documents this). Practice discards it.
- `micro_signal::classify_micro` measures micro shots in degrees against the real target
  (strokes, overshoot past the centre, settle time). Nothing calls it any more.
- `TrackingAnalyzer` produces half-second windows with `lag_ms`, `mean_along_deg` and
  `cross_rms_deg`. The tracking task computes them and nothing keeps them.
- Calibration saves only signal counts and pair costs (`RoundSignals`, `PairRecord`).
- Practice `MetricPoint`s carry placeholders: `offset_from_center_m` is always 0.1,
  `target_radius_m` 0.35, `precision_percent` 85 on every hit (`main.rs`, `on_shot`). Only
  `hit`, `reaction_time_ms`, `mouse_distance_px` and `peak_velocity_px_s` are real.
- `FlickSpawn` gives the angle to a flick target but not its direction; `MicroSpawn` gives the
  full offset in degrees. Switching, Speed and Moving targets expose no spawn info.

## Goals

- Record, for every target in calibration and practice, a small set of aim features in degrees
  (never mouse counts), and keep them.
- From a calibration run, name up to three habits, each shown to be present on both the faster
  and the slower side, with a button to the practice scenario that trains it.
- Say when a run's data was unsteady (still warming up) or when a test was left out of the
  result because of a habit, in plain words.
- Show each practice session's habit values, compared with the player's previous sessions of
  the same scenario.
- Show, for each named habit, how it has moved across practice sessions since that calibration.
- Replace the placeholder practice metrics with real values.

## Non-goals

- No change to how the sensitivity is computed: PSA, the joint fit, pooling and confidence stay
  as they are. (Owner's call: option "ก".)
- No per-game test mix, no click-timing test (separate pieces of work).
- No raw mouse samples on disk; features only.
- No live hints during play.

## Anti-goals (failure even if it works)

- Calling a sensitivity effect a habit, or a habit a sensitivity problem.
- Habit values that change when only the DPI changes.
- Nagging: more than three habits at once, or habits named from too few targets.
- Any number on screen that isn't measured.

## Approaches considered

- **A. Per-target features in degrees, analysed in one module (chosen by the owner).** Every
  target stores a handful of numbers; every tracking half-second stores its window. One
  analysis module turns these into habits for calibration (comparing sides) and practice
  (values and trends). Rules can be retuned later and old data re-analysed; direction
  breakdowns are possible; trends compare like with like. Costs: the largest change, history
  files grow by tens of KB per session, old files must still load.
- **B. Store only per-session summaries** ("far flicks short 38%"). Simpler and smaller, but
  summaries computed under old rules can't be compared with new ones, which breaks the trend
  the owner cares about, and only pre-chosen breakdowns exist.
- **C. Reuse the count-based `ShotTrajectoryPattern`.** Cheapest, but DPI-dependent: a DPI
  change would look like a habit change. Dropped.

## Design

### 1. What gets recorded

**Per clicked target** (`TargetRecord`), for Flick, Micro-adjust, Switching, Speed and Moving
targets, in calibration and practice:

| Field | Meaning |
|---|---|
| `offset_deg: [f32; 2]` | Mouse movement in degrees (x right, y down) from the aim at spawn to the target centre. Same convention as `MicroSpawn::offset_deg`. |
| `radius_deg: f32` | Target angular radius at spawn. |
| `first_landing: Option<f32>` | Progress along the target direction at the end of the first stroke, divided by the distance. `< 1` stopped short, `> 1` went past. `None` if the player barely moved. |
| `strokes: u8` | Strokes toward or away from the target (as in `classify_micro`). |
| `crossed_center: bool`, `overshoot_deg: f32` | Went past the centre beyond tolerance and came back; how far. |
| `time_to_hit_ms: Option<f32>` | Spawn to the hit. `None` if it expired. |
| `misses: Vec<[f32; 2]>` | Each missed click's position relative to the target centre in degrees, split into along (toward/away from where the aim came from) and across. Capped at 8. |
| `band: Option<FlickBand>` | Flick distance band: from the spawn for Flick (calibration and practice, both banded); for Switching and Speed, from the angle using the same ranges. |
| `side: Option<Side>`, `block: Option<u8>`, `warmup: bool` | Calibration only: faster or slower side, block index, warm-up. |

The stroke and landing maths generalises `classify_micro` (degree thresholds, windowed speed,
stroke on/off shares) to any distance; thresholds scale with distance the way
`CROSS_TOLERANCE_SHARE` already does. It replaces `ShotTrajectoryPattern` for analysis; the
count-based classifier stays only where calibration still uses it.

Moving targets: `offset_deg` and landing are measured against the target's position at spawn,
so `first_landing` is meaningless there; it is stored `None` and only `time_to_hit_ms`,
`strokes` and `misses` count. (My call.)

**Per tracking window** (`TrackingRecord`): the existing `TrackingWindow` fields
(`mean_along_deg`, `lag_ms`, `cross_rms_deg`, `mean_speed_deg_s`, `coverage`) plus
`horizontal: bool` (target moving mostly horizontally) and, in calibration, `side`, `block`,
`warmup`.

Together these form an `AimLog { targets: Vec<TargetRecord>, tracking: Vec<TrackingRecord> }`.

Tasks need to expose spawn info for every clicked target: `FlickSpawn` gains the offset vector;
Switching, Speed and Moving expose the same. The task, not `main.rs`, knows which target is the
one being aimed at (for Switching, the highlighted one).

### 2. Where it is kept

- Practice: `SessionRecord.aim: Option<AimLog>` (`#[serde(default)]`). Sessions saved before
  this have `None` and show no habits.
- Calibration: the run's `AimLog` is saved with the run in `calibrations\` (same
  `#[serde(default)]` rule). The profile's `CalibrationResult` gets the analysed
  `HabitReport` (below), not the raw log.

Size: roughly 40 bytes per target and 24 bytes per window in JSON-ish terms; a 60-target
session is a few KB, a calibration run ~25 KB. Acceptable. (Builder may pick a compact
encoding.)

### 3. Real practice metrics

`MetricPoint` gets real values from the target record: `offset_from_center_m` from the hit
position (converted the same way the target radius is), `target_radius_m` from the task,
`precision_percent = 100 × (1 − offset / radius)` clamped to 0–100 on a hit, 0 on a miss.
History screens that already show these start showing true numbers; nothing else changes.

### 4. The habits

Each habit is one number computed from records, with a threshold. Starting thresholds below are
my calls; the builder may tune them against recorded play, but each must be covered by a test.

| Habit | Measure | Named when | Trains in |
|---|---|---|---|
| Flicks stop short (per band) | median `first_landing` | `< 0.85` | Flick |
| Flicks go past (per band) | median `first_landing` | `> 1.10` | Flick |
| Many corrections (flick) | mean `strokes` | `> 2.5` | Flick |
| Small aims overshoot | share of micro targets with `crossed_center` | `> 35%` | Micro-adjust |
| Small aims creep | share of micro targets with `strokes ≥ 3` and no crossing | `> 35%` | Micro-adjust |
| Tracking trails | median `lag_ms` | `> 40 ms` | Tracking |
| Tracking runs ahead | median `lag_ms` | `< −40 ms` | Tracking |
| Tracking shakes | median `cross_rms_deg / radius_deg` | `> 0.5` | Tracking |
| Misses lean one way | mean miss position along/across, flick and micro | `> 0.4 × radius` with ≥ 10 misses | Flick or Micro-adjust, by test |

Direction: for flick habits, also compute the measure per dominant direction (left, right, up,
down). If one direction is clearly worse (its value beyond the threshold and the others not),
the card says "mostly to the left". For tracking, the same split by `horizontal`.

Minimum data: calibration, see section 5; practice, ≥ 12 records per measure in the session,
except per-band flick measures, which need ≥ 8 records in that band; otherwise it is silent.

**Practice Flick spawns by distance (owner's call).** Practice Flick today places 30 targets at
random in a box, which gives about 4 far targets a session, too few for any far-flick measure.
It changes to the calibration's three distance bands (`FlickBand::range_deg`), 10 targets
each, in a shuffled order (as `banded_order` does). Target count, size and scoring stay the
same. This changes how practice Flick feels (far targets come more often); the owner accepted
that so every session can show every band and "Practise: Flick" trains the distance that was
named.

### 5. Habit or sensitivity (calibration)

Blocks are played at very different sensitivities (block 1 at 47.5 cm/360, the bisection blocks
far from the answer), and a block far from the player's sensitivity makes both of its sides
overshoot or stop short together. Comparing the two sides of each block, or pooling all blocks,
would therefore name sensitivity effects as habits. Instead (my call, after the builder check):

- For each measure, take every counted record (warm-up excluded) with the cm/360 it was
  actually played at (block cm × side factor), fit the measure against `ln(cm)` (a straight line
  is enough over the searched range; the builder may use a robust fit), and **read it off at the
  run's result**.
- **Habit:** the value at the result is beyond the threshold. It is what remains at the
  player's own best sensitivity, so changing sensitivity won't fix it. This is the meaning the
  owner agreed to ("the same at both speeds"), made robust to where the blocks landed.
- **Sensitivity lean:** the measure crosses the threshold only away from the result. Not shown.
- Runs with `no_answer` have no result to read at: no habits are named, and the section says why.
- Minimum data: ≥ 16 counted records for the measure across the run, spanning at least 3 blocks.

Ranking: by how far the value at the result is beyond its threshold, as a share of the
threshold. Show at most three.

Each card states that the habit is still there at the player's best sensitivity, so changing
sensitivity won't fix it.

### 6. How far to trust this run

Added to the result as a short note; the number and the confidence level are unchanged. (My
call: the owner chose to leave the computation alone; lowering confidence would also change
when pooling and "stop at Medium" advice kick in.)

- **Still warming up:** later blocks are naturally cheaper because they sit nearer the answer,
  so raw costs can't show warm-up. Per test, in that test's own cost units, fit each pair's mean
  cost against `ln(cm)` (quadratic: cost has a minimum) plus a linear term in block index. The
  note fires when, for at least two of the three tests, the block-index term says costs fell by
  more than 15% from the first to the last counted block and the term is clearly non-zero
  (its 95% interval excludes 0). Note: "Your aim was still improving during this run. Warm up
  longer before calibrating." (My call.)
- **Test left out:** reads the latest run's own `left_out`, kept unpooled in the run's
  `HabitReport` (the profile's `RunDiagnostics.left_out` is overwritten by pooling). For each
  left-out test with a named habit pointing the same way as its lean (trailing ↔ too slow,
  running ahead / going past ↔ too fast, stopping short ↔ too slow), say so: "Tracking was left
  out of the result: you trailed the target at every speed tested. That's a habit to practise,
  not a sensitivity problem." A left-out test with no matching habit gets no note. (My call.)

### 7. Screens

**My sensitivity**, new section after the result and before "How we got here":

```
What to practise
┌─────────────────────────────────────────────┐
│ Far flicks stop short           18% short    │
│ Still there at your best sensitivity →       │
│ practise, don't change it. Mostly to left.   │
│ Since calibrating: 18% → 12%   ▁▃▂▁          │
│                         [ Practise: Flick ]  │
└─────────────────────────────────────────────┘
```

- Up to three cards, worst first. None found: one line, "No habit stood out in this run."
- The trust note (section 6) sits under the cards.
- "Since calibrating" appears once at least one practice session after the run has enough
  records for that habit; the sparkline shows the last 10 such sessions. Only sessions played
  within ±12% of the result's cm/360 count, so a sensitivity change isn't shown as progress;
  if the player is still practising at another sensitivity, the card says "Practise at this
  sensitivity to track progress." (My call.)
- The button starts that practice scenario.
- Habits come from the latest run only, not the pooled runs: habits change with practice, and
  pooling would hide that. (My call.)

**History, a practice session's detail**, new section "Habits this session": each measure that
had enough records, its value, and the mean of the previous five sessions of the same scenario
with an arrow (better / worse / same within 10%). Measures beyond their threshold are marked.

**History, a calibration's detail**: the same cards as My sensitivity for that run.

### 8. Testing

- Synthetic mouse paths (like `paired_micro_tests.rs`) for each habit, in and out of threshold.
- DPI invariance: the same hand movement replayed at 400 and 3200 DPI gives the same records.
- Habit vs lean: a habit on both sides is named; a one-sided lean is not.
- Old `SessionRecord` and calibration files without `aim` still load.
- Real metrics: a hit at a known offset gives the expected `precision_percent`.
- `ui_preview` scenes: My sensitivity with three cards, with none, with the warm-up note, with a
  trend; a practice session detail with habits.

## Owner's calls vs mine

**Owner:** purpose is "what to practise" plus a trustworthy result; option ก (don't change the
computation, explain and warn); analyse both calibration and practice, with progress over time;
approach A (per-target features in degrees); the habit list and the screens sketched above;
practice Flick spawns 10 targets in each of the three distance bands.

**Mine:** exact thresholds and minimum counts; moving targets skip landing; warm-up note does not
lower confidence; habits from the latest run, not pooled; three cards max; direction breakdown
rule; the "previous five sessions" comparison; misses capped at 8 per target.
After the builder check: habits read off a fit at the result instead of comparing block sides;
no habits on `no_answer` runs; the warm-up check controls for sensitivity, per test; the
left-out note uses the run's own unpooled list and needs a habit pointing the same way; the
trend counts only sessions within ±12% of the result.

## Left to the builder

- Storage encoding and whether to version the files.
- How `classify_micro`'s maths is generalised to flick distances, as long as thresholds stay in
  degrees and scale with distance.
- Wording polish within the meaning above.
