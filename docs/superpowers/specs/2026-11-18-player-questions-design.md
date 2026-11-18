# Player questions before calibration — design

Date: 2026-11-18
Status: draft for approval (revised after builder check)
Mockups: `docs/superpowers/mockups/player-questions-v2.html` (layout and copy) and
`docs/superpowers/mockups/player-questions-v5.html` (art direction for questions 2–3; the final art comes from AI images, see Art)

## Why

The owner wants two things from asking players about themselves:

1. **A more accurate or faster result.** The answers steer where the calibration search starts and where
   it is allowed to go.
2. **Practice advice that fits the player.** What the player says is wrong with their aim is compared
   with what the test measured, so they learn whether they read their own aim correctly. A player who
   believes they overshoot when they actually stop short would otherwise practise the wrong thing.

## Goals

- Four questions, answered only by clicking picture or text cards. Nothing to type.
- Every question has a "Not sure" answer. A "Not sure" answer doesn't steer the search; the start
  point still comes from the player's own sensitivity when it is known (see Start point).
- Answers change the search: the mousepad size caps the slowest sensitivity tested, and the other
  answers move where the search starts.
- The results page compares the player's answers with what the test measured.
- The pictures are separate SVG files, so the art can be replaced without touching code.

## Non-goals

- Shortening the run. The run keeps its 7 blocks; a better start puts more of them near the answer.
  "Faster" here means getting there sooner within the run, not fewer blocks. That is a later
  decision, once there is data on how much the answers help.
- Asking about mouse shape (dropped: it doesn't help either goal).
- Detecting in the warm-up what the player aims with. Dropped from this round (decided): the mouse
  sensor reports movement in the mouse's own frame, so a hand turning rigidly at the wrist or elbow
  draws an almost straight line whatever the pivot, and no recorded runs keep raw paths to try other
  signals on. A later research phase can start by logging raw flick paths from players who know
  their pivot, then look at signals such as sweep length per stroke.
- Correcting the measured result by grip style. The test measures the player's hand directly; a grip
  average must never overwrite that.

## Anti-goals (a failure even if it works)

- An answer that shuts the true sensitivity out of the search. A wrong guess may cost a block, but it
  must never make the result wrong. Only the mousepad may cut the range, because a sensitivity that
  doesn't fit on the pad is wrong for that player.
- Questions that block a player who just wants to play: "Skip all" is always one click away.

## The questions

The copy is final as written here; the order is fixed. English, like the rest of the app.

**Header on every step:** eyebrow `Before you start · N of 4`, step dots, `Skip all` (ghost button),
`Back` / `Continue →`. The last step's primary button reads `Start calibration →`.

### Q1 — What's going wrong? (text cards, pick any)

Subtitle: *Pick anything that sounds like you. This tells us which direction to move.*

| Answer | Card text | Lean |
|---|---|---|
| `Overshoot` | **I overshoot my target** · Fly past it, then drag back | slower |
| `CantTurnFast` | **I can't turn fast enough** · Someone flicks and I'm still turning | faster |
| `ShakySmall` | **Small adjustments are shaky** · Fine at range, jittery up close | slower |
| `RunOutOfPad` | **I run out of mousepad** · Lifting mid-fight to reset | faster |
| `NotSure` | **Not sure** (full-width card) | none |

Picking `Not sure` clears the others; picking any other answer clears `Not sure`. Two columns, no pictures.

### Q2 — How do you hold it? (picture cards, pick one)

Subtitle: *Look down at your hand. Whichever picture matches.*
`Palm grip · Hand flat, full contact`, `Claw grip · Arched, palm rear only`,
`Fingertip grip · Only the tips touch`, `Not sure`.

### Q3 — What moves when you flick? (picture cards, pick one)

Subtitle: *Best guess is fine.*
`Wrist · Hand turns, forearm still`, `Fingers · Mouse barely moves`, `Arm · Elbow stays on the desk`,
`Shoulder · Whole arm sweeps`, and a full-width card `Not sure`.

### Q4 — How big is your mousepad? (picture cards, pick one)

Subtitle: *This is your hard limit - no sensitivity works if you run out of room.*
`Small mousepad · Mouse only, tight`, `Medium mousepad · Room for full sweep`,
`Large mousepad · Desk mat - keyboard sits on it`, `Not sure`.

### Look

Cards follow the reference mockups: a radio circle top-right, the picture above a bold label and a muted
one-line description. The selected card has an `ACCENT` (#FFD23F) border, a faint accent fill and a
filled radio. The app's yellow accent replaces the teal in the reference.

## When the questions appear

- On **Start calibration**, if the profile has no saved answers, the four steps open first. After the
  last step the calibration starts directly.
- `Skip all` saves every question as "Not sure" and starts the calibration. The player isn't asked again.
- **Settings → Your setup** (a new ghost button next to "Run setup again") reopens the four steps with
  the saved answers selected. Finishing there saves them and returns to Settings; it doesn't start a run.
- First-run setup (DPI, game, sensitivity) is unchanged.

## What the answers do

### Mousepad caps the slowest sensitivity searched

A 180° turn should fit in one sweep of the pad's usable width (pad width minus ~10 cm for the mouse and
a margin), so `cm/360 ≤ 2 × usable width`:

| Pad | Assumed width | Search max |
|---|---|---|
| Small | 25 cm | 30 cm/360 |
| Medium | 36 cm | 52 cm/360 |
| Large (desk mat) | 90 cm | 80 (today's limit) |
| Not sure | — | 80 (today's limit) |

*My call:* the numbers are constants in one place, tuned later. The minimum stays 15. The cap is the
run's real range everywhere the range is used today: the scheduler, the "answer at the edge" confidence
rule, and the clamp in pooling (pooled runs are clamped to the newest run's range, the player's current
mousepad; changed 2026-11-18 from "the widest range among them", which let a pooled result exceed the pad). A result at the pad cap
gets a note on the result page: *Your mousepad is the limit here. A bigger pad could let you play slower.*

### Start point

Today the first block tests the middle of 15–80 (47.5) and two bisection blocks follow. The new start:

1. **Base:** the player's own sensitivity whenever it is known (`sens_known`), whatever the answers,
   including after Skip all (decided). Otherwise a base from Q3, or Q2 when Q3 is "Not sure": Fingers 28, Wrist 32, Fingertip 32, Claw 40, Palm 45,
   Arm 45, Shoulder 55. With no answer at all, today's midpoint.
2. **Lean from Q1:** each "slower" answer ×1.12, each "faster" answer ×0.89. Opposite answers cancel.
3. Clamp into the (pad-capped) range, at least 10% inside each edge, so bisection still has room both ways.

*My call:* the player's own sensitivity is the strongest hint there is, so it wins over grip and pivot.
"I overshoot" means "slower than what I play now", which is the same reading the card's subtitle gives.
Grip and pivot then only set the start for players who skipped their sensitivity. The range is never
narrowed by these answers (anti-goal).

**Outcome (2026-11-18):** three ways of steering the start (x1.25 steps, block 1 only, x1.5 steps) all
lost accuracy in simulation. The start point is stored on the run and shown in History, but the block
search does not use it; the answers only cap the range (mousepad) and drive the result-page comparisons.

First-run setup gains one profile flag, `sens_known`: the sensitivity was entered in setup, or a
calibration result was applied with "Use this sensitivity". Today "Skip" silently keeps the 40 cm default.

**Q1 is tied to the sensitivity it was answered at.** The setup stores `answered_at_cm`. The Q1 lean and
comparison 1 apply only while the player's sensitivity is within ±5% of it. Once it has moved further
(usually because they applied a result), the lean is dropped, and the next Start calibration re-asks Q1
alone, as one step, with the old picks selected. Otherwise the lean would compound run after run.

**Search steps from a start hint (my call, to be checked in simulation).** Plain bisection would throw a
good start away: start 28 with the answer at 30 sends block 2 to (28+80)/2 = 54. When a run has a start
hint, the two opening blocks step from the start by ×1.25 or ÷1.25 toward the side the block pointed to,
instead of jumping to the range midpoint. Bounds still move as today, `FIT_REACH` is unchanged, and a
"balanced" block narrows around the tested point rather than around the range's middle. Runs without a
hint keep today's bisection. Before release, the existing simulation must show that runs with hints
(good, poor and wrong) are no less accurate than today's.

### Results page comparison

Two kinds of comparison, under "What to practise":

1. **Direction answers vs the result.** Overshoot / shaky (slower) and can't-turn / out-of-pad (faster)
   are compared with the result against the player's own sensitivity:
   *You said you overshoot — the test agrees: your result is 14% slower than what you play.* or
   *…the test didn't: your result is close to what you play now.*
2. **Habit badges.** Habit cards whose habit matches an answer get a badge:
   - Overshoot ↔ `FlickPast(any band)`, `MicroOvershoot`; opposite: `FlickShort`, `MicroCreep`
   - Shaky small adjustments ↔ `TrackingShakes`, `MicroOvershoot`
   Badges: **You said this too** (match), **The test saw the opposite** (opposite habit found),
   **Not seen at this sensitivity** (answer given, no matching habit; with a *Practise anyway →* button
   to the matching scenario).

Can't-turn and out-of-pad have no habit to compare with; they appear only in comparison 1.

Comparison 1 is hidden when `sens_known` is false: there is no "what you play" to compare with.

**Which habits are checked (my call).** Matches are looked up in every habit the run found, not just the
three shown. A matching or opposite habit is promoted into the three cards ahead of unrelated ones.
"Not seen at this sensitivity" is used only when the run found no matching habit at all. It is a note
line under the cards, with the *Practise anyway →* button, not a habit card. Grip and pivot add no
explanations of their own.

## Data

- `PlayerProfile` gets `#[serde(default)] setup: Option<PlayerSetup>` and
  `#[serde(default)] sens_known: bool`. Old profiles load with no answers (asked on next calibration).
  *My call:* existing profiles get `sens_known = true` when `current_cm_360 != 40.0` or they have a
  calibration; otherwise `false`.
- `PlayerSetup { problems: Vec<AimProblem>, grip: Option<Grip>, pivot: Option<Pivot>, pad: Option<PadSize>, answered_at_cm: f64, answered_at }`.
  `pivot: Option<Pivot>` with `Pivot` = `Wrist | Fingers | Arm | Shoulder`. `None` / empty mean
  "Not sure"; Skip all saves them all as "Not sure". The warm-up is unchanged.
- Each run's `RunRecord` and `CalibrationResult` keep a copy of the answers they used, plus the range
  searched and the start point. Comparisons on a result, including in
  History, use that copy, never the current answers. A pooled result compares against the newest run's
  copy and is clamped to the newest run's range (2026-11-18; was the widest range among the pooled runs).

## Art

Each picture is its own SVG in `assets/questions/` (`grip_palm.svg`, `grip_claw.svg`, `grip_fingertip.svg`,
`pivot_wrist.svg`, `pivot_fingers.svg`, `pivot_arm.svg`, `pivot_shoulder.svg`, `pad_small.svg`,
`pad_medium.svg`, `pad_large.svg`, `not_sure.svg`), embedded with `include_bytes!` and loaded under a
`bytes://….svg` URI like the game logos in `src/ui/logos.rs`.

Pipeline (decided: AI images, then vectorised):

1. The owner generates the 10 images from the prompts in the appendix, at ≥1024 px. The prompts use
   the app's own colours (decided): background `#0B0C0E`, fills `#1B1D21`–`#2C2F35`, outlines
   `#A3A6AD`–`#ECECEE`, highlights only in the accent `#FFD23F`.
2. Each PNG is vectorised with `vtracer` (colour mode, few colours). Image generators don't hit exact
   hex values, so every fill is then snapped to the nearest colour of that palette, and the background
   is made transparent. A small script in `tools/` does this so it can be rerun.
3. Budget: ≤ 300 KB per SVG; it must render crisply at the card's size (about 170 × 120 pt).

Until the final images exist, the build ships the round-5 mockup drawings as placeholders. They are
already SVG and already distinguish the three grips.

## Approaches considered

| | What | Trade-off | Chosen |
|---|---|---|---|
| Questions on Start calibration | Ask just before the first run that uses them | Same for new and existing players; first-run setup stays 3 steps | **Yes** |
| Questions as setup steps 4–7 | Ask during first-run setup | Setup doubles in length; existing players still need another way in | No |
| Self-report compared with the test | Q1 steers the start and is checked against the result | Answers can't make the result wrong | **Yes** |
| Self-report trusted over the test | Cards for whatever the player says | Can send players to practise the wrong thing | No |
| Warm-up pivot detector now | Card "work it out for me" | Curvature can't see the pivot (sensor turns with the hand); no raw paths to find another signal | No, later research |
| Start from own sensitivity even on Skip all | | Nearly every player gets a closer start; existing players' runs change, so simulation must pass first | **Yes** |
| Start from own sensitivity only when something was answered | | Skip all behaves exactly as today | No |
| Art: AI images vectorised | Closest to the reference look | Heavier files, needs regeneration until consistent | **Yes** |
| Art: hand-written SVG | Light, on-theme | Can't reach the reference's realism (tried, round 5 is the ceiling) | Placeholder only |
| Art: commissioned / stock | Best quality | Cost, wait, licence checks | No |
| Pictures drawn with egui painter | No files | Not SVG, can't swap art without code | No |

## Left to the builder

- Module layout for the questions screen (likely `src/ui/screens/questions.rs` next to `onboarding.rs`)
  and the new `UiAction`s.
- Exact badge and note wording beyond what is written here, following the tone of `src/ui/habits.rs`.
- Tests: answer→start-point and pad→range mapping are pure functions and get unit tests; the
  start-hint search gets a simulation comparison against today's search.

## Appendix — image prompts

Shared style line, appended to every prompt:

> Flat vector-style illustration in a dark UI style. Solid near-black background #0B0C0E. Hands, arms,
> mouse and mousepad filled in dark charcoal greys #1B1D21 and #2C2F35, with thin light-grey outlines
> #A3A6AD and highlights on edges #ECECEE, minimal soft shading. Warm yellow #FFD23F used only for the
> highlighted parts named below, as solid flat yellow, nothing else yellow and no other colours.
> No text, no logos, centred, generous empty margin, 4:3.

1. `grip_palm` — A right hand resting on a gaming mouse, three-quarter view from above and slightly to
   the left, wrist entering from the right. Palm grip: the whole palm and all fingers lie flat on the
   mouse. Highlight in yellow: the parts of the mouse the hand touches (the whole top shell under the palm
   and fingers).
2. `grip_claw` — Same mouse, same view. Claw grip: palm heel on the rear of the mouse, knuckles raised,
   fingers arched, fingertips on the buttons. Highlight: the rear of the mouse under the palm heel and the
   fingertip spots on the buttons.
3. `grip_fingertip` — Same mouse, same view. Fingertip grip: the palm floats above the mouse, only the
   fingertips and the thumb tip touch it. Highlight: only the fingertip spots and the thumb-tip spot.
4. `pivot_wrist` — Top-down view of a right forearm and hand on a desk holding a mouse, elbow at bottom
   left. The forearm is still; the hand pivots at the wrist. Highlight: a small yellow dot at the wrist and a
   faint yellow fan-shaped arc showing the hand's sweep.
5. `pivot_fingers` — Same view. The hand and forearm are still; only the fingers move. Highlight: a yellow dot at
   the knuckles and a very small faint arc at the fingertips.
6. `pivot_arm` — Same view. The elbow rests on the desk and the forearm sweeps. Highlight: a yellow dot at the
   elbow and a wide faint arc.
7. `pivot_shoulder` — Same view, upper arm visible to the shoulder. The whole arm sweeps from the
   shoulder. Highlight: a yellow dot at the shoulder and a very wide faint arc.
8. `pad_small` — Top-down view of a small mousepad with only a mouse on it, empty desk around it. No yellow.
9. `pad_medium` — Same view, a medium mousepad with room around the mouse. No yellow.
10. `pad_large` — Same view, a desk mat with a keyboard and a mouse both on it. No yellow.

Generate all ten in one session, reusing the first image as the style reference, so they match.
