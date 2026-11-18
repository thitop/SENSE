# Calibration replay — design

Date: 2026-11-18
Status: draft for approval (revised after builder check)
Mockup: `.superpowers/brainstorm/1259-1794984732/content/replay-layout.html` (layout A was picked)

## Why

The owner wants players to see **why SENSE gave them this number**. Today the result page shows the
number, a "By test" list and two static graphs ("Tested per round", "Score per shot") under "How we got
here". A player who doesn't already know how the search works can't read a reason out of them. The
replay walks through the run one round at a time, as the search saw it, and ends on the number.

This is part A of two. Part B, added later, attaches clips of the player's own shots (crosshair paths)
to each round as evidence. B needs a new frame-by-frame recorder and is out of scope here, but the
design should leave a place for it (see Leaving room for B).

## Goals

- On **My sensitivity**, the "How we got here" section becomes a replay player: a graph that builds up
  round by round, and one card for the current round under it.
- The player can press **Play** (steps through rounds 1 to 7, then Result, about 1.6 s per step),
  **Pause**, step with **◀ ▶**, or jump with the round buttons `1 … 7` and `Result`.
- After a calibration finishes and the app opens My sensitivity, the replay plays once by itself. The
  number is visible at the top the whole time; nothing waits on the replay.
- Every sentence on a round card matches what the search actually did in that round, including how it
  picked the next sensitivity.

## Non-goals

- Part B (the player's own shots). No raw path recording in this change.
- Replays of older runs from History. Only the latest result on My sensitivity (`last_calibration`).
- Changing how the calibration chooses sensitivities or computes the result.
- A full-screen replay before the number is revealed (option 2, rejected: makes the player wait for the
  number they came for).

## Anti-goals

- A replay that tells a different story from the math. If the card says "too fast → slower" but the
  next round went faster, players stop trusting the number. Every explanation is derived from what the
  scheduler recorded, never re-guessed with simpler logic.
- Blocking or delaying the result, the copy button or "Use this sensitivity".
- Breaking results saved before this change. They must still open; the replay shows what their data
  allows.

## Approaches considered

1. **Replay player inside My sensitivity** — picked. Uses data already saved per run, rewatchable,
   doesn't block the number, and B can attach clips to each round card later.
2. **Full-screen replay before the result** — rejected: delays the number, needs Skip, and still needs a
   way to reopen it.
3. **Static round-by-round list** — rejected: barely more than the graphs that already fail to explain.

Within 1, layout **A** was picked over B (graph full width on top, one card for the current round below
it, rather than graph left + list of rounds right).

## What the player sees (layout A)

```
How we got here
Each round tested one sensitivity, in pairs: one target a little faster (+12%), one a little
slower. Whichever side you did better on says which way to go. Where that flips is your number.

┌ graph ───────────────────────────────────────────────────────────────┐
│ y: cm/360 (log scale)   x: R1 … R7                                   │
│ · tested sensitivity per round (centre value only, not the ±12%     │
│   sides), dot red = too fast, blue = too slow, grey = even; current  │
│   round ringed in accent; line joins the dots                        │
│ · search range per round, faint accent band                          │
│ · best guess so far, green dashed line with a ± band                 │
│ · on Result: horizontal accent line at the result, labelled          │
│ legend: Too fast · Too slow · Tie · Search range · Best guess        │
└──────────────────────────────────────────────────────────────────────┘
[▶ Play] [◀] [▶]  (1)(2)(3)(4)(5)(6)(7)(Result)

┌ Round 3 of 7                                         36.0 cm/360° ┐
│ Flicking          [red|grey|blue]  Even                            │
│ Micro-adjustment  [bar]            Better slower · too fast        │
│ Tracking          [bar]            Better faster · too slow        │
│ ─────────────────────────────────────────────────────────────────  │
│ So far SENSE thinks 35.1 cm ± 14%                                  │
│ Next round tests 33.0 cm: the best guess so far.                   │
└────────────────────────────────────────────────────────────────────┘
```

Graph steps only reveal rounds up to the current one; later rounds are blank.

### Round card

- Header: `Round N of M` and the tested cm/360.
- One row per test that recorded something that round, in the order Flicking, Micro-adjustment,
  Tracking. Today only these three are played (`CalibrationRun::make_task`; Recoil and Placement
  aren't built, and Counter-strafing is diagnostic only). Any other test that does record counts in
  the future gets a row too, after these, with its `TestKind::label()`.
- Every counted test is judged in pairs (`sensitivity::paired::judge`): a pair is "too fast" when the
  player did better on the slower side, "too slow" when better on the faster side, and a tie
  (`Signal::Direct`) otherwise. The copy must say this, never "overshoot"/"stopped short"/"clean",
  which describe a judging method the run no longer uses. (Decided from the code, my call on wording.)
- The bar splits that test's pairs into too fast / tie / too slow. The verdict reads
  `Better slower · too fast`, `Better faster · too slow` or `Even`. A row may show the count, e.g.
  `6 of 9 pairs`, builder's call.
- Flick distance bands ("Flick short/mid/far") are not shown as rows in this version (builder's call
  whether to add a small breakdown under Flicking; keep it out if it crowds the card).
- Footer, two lines:
  1. The best guess after this round: `So far SENSE thinks X cm ± Y%`. The guess is
     `fit_estimate()` (all tests, no exclusions), because that is what steers the search and what
     line 2 refers to (my call). When there's no usable guess, the line says why (recorded per
     round, my call):
     - only one sensitivity tested so far (always true after round 1): `One sensitivity tested so far:
       SENSE needs two to start guessing`;
     - every test leaned the same way: `Not enough to guess yet: every test leaned the same way`;
     - a guess exists but its uncertainty is above `MAX_ANSWER_UNCERTAINTY` (0.84, the same limit the
       page uses for "no trend"): `Too early to trust a guess`. No green point is drawn for that
       round, so the log-scale band never blows up.
  2. Why the next round tests what it tests, matching the scheduler (`BlockScheduler::end_block`):
     - Rounds in the bisection phase (the first 2): `Overall too fast → the next round tries the middle
       of the slower half (X cm)`, the mirror for too slow, or `About even → the range narrows around
       here` for a tie.
     - Later rounds (whenever `end_block` took the fit branch, which already happens after round 2):
       `Next round tests X cm: the best guess so far.` When line 1 said the guess was too uncertain:
       `Next round tests X cm: a rough first guess.`
     - The fit pick was clamped to the reach `[low × 0.8, high × 1.2]` (within the overall search
       limits): `The best guess (A cm) is outside what's been tested, so the next round goes as far
       as it can: X cm.`
     - The fit had no answer and the scheduler fell back to the midpoint:
       `Next round tests the middle of the range (X cm).`
     - The wording follows the branch `end_block` actually took (recorded), not the round number.
     - Last round: `That was the last round → result`.

### Result step

- The number and its ± (the combined estimate's `rel_uncertainty`, saved as
  `RunDiagnostics::result_rel_uncertainty`; no ± when it's missing), `cm/360°`.
- One line: `Every test flips from "too fast" to "too slow" here. Each test on its own:`
- If the last round's best guess (all tests) differs from the result because tests were left out
  (`trusted_recommendation` drops them), one line explains the jump:
  `Leaving out Tracking moved it from A to B cm.`
- Each test's own estimate from `diagnostics.estimates` (main tests only, not flick bands), `X cm ± Y%`.
  Tests in `left_out` show in WARN: `Left out: leaned one way at every sensitivity`.
- If `pooled_runs > 1`: `Combined with N earlier runs → X cm`. The graph and rounds are this run's; the
  number and per-test estimates are pooled, and the step must say so rather than imply this run alone
  produced the number.
- If `no_answer`: the Result step shows `No result: every test leaned the same way at every sensitivity
  tested`, no result line on the graph.
- If `runs_disagree`: same note the page already shows (`result_note`).

## Data

What's already saved per run (`RunDiagnostics.rounds: Vec<RoundSignals>`): tested cm and per-test
counts per round. What's missing, and must be recorded at run time so the cards tell the truth:

| New, per round | Source | Use |
|---|---|---|
| search range before the round (`low`, `high`) | `BlockScheduler` bounds when the block starts | range band (shows `low`/`high`; a clamped pick may put the dot outside it, which the card explains) |
| best guess after the round (`cm`, `rel_uncertainty`), or why there is none (`OneSensitivity`, `AllOneWay`) | `fit_estimate()` / the joint fit after `end_block` | green line, footer line 1 |
| how the next sensitivity was chosen: `Midpoint`, `FitEstimate`, `FitClamped { guess_cm }`, `FitFallbackMidpoint`, `Last` | which branch `end_block` took | footer line 2 |
| the block's overall verdict | existing `conclusion` (`CalibrationRound`) | dot colour, bisection wording |

Add these as optional fields with `#[serde(default)]` on `RoundSignals` (or a sibling list on
`RunDiagnostics`, builder's call) so older saved results still load.

**Older results** (missing the new fields): show rounds and verdict bars; leave out the range band and
best-guess line; footer line 1 omitted; footer line 2 uses only the stored `conclusion`
(`Overall too fast → next round went slower` etc.) without naming the reason. Results without
`diagnostics` at all (flick-only, very old) keep today's two graphs instead of the replay.

## Behaviour details

- Placement (decided by the owner): "How we got here" stays where it is, at the bottom of the page under
  What to practise. Under the number (left column, after the confidence notes) a link
  `How we got this number →` scrolls the page to the section and starts playback from Round 1.
- Autoplay: once per finished run, armed when My sensitivity opens straight from `finish_calibration`,
  and started only when the section first comes on screen (e.g. `ui.is_rect_visible` on the section's
  rect), so it doesn't finish unseen below the fold. Opening the page later starts paused on Round 1
  (my call: rewatching is opt-in; the page shouldn't animate every visit).
- Any click on ◀ ▶ or a round button stops playback. Play from Result restarts at Round 1.
- Playback state lives in `UiState` (current step, playing, time of last step) and resets when a new
  calibration finishes or the result changes (e.g. "Use only the latest run").
- Playback uses egui time and requests repaints only while playing.
- The "Score per shot" graph goes away with "Tested per round" (my call: it plots the old composite
  score, which no longer drives the result, so it contradicts the replay's story). The builder should
  check nothing else depends on `samples` being shown before removing the graph; the data stays saved.
- Window widths: must fit the existing small preview size (`sensitivity-result-small`, 1024 wide).

## Leaving room for B

Part B will add, per round card, one or two short clips of the player's own shots that show that
round's verdict. Keep the round card a function of `(round data, step)` so a clip area can be added
below the rows later. Nothing else is needed now.

## Testing

- Unit tests that the recorded per-round fields match the scheduler's actual choices (e.g. a scripted
  run where round 3 falls back to midpoint says `FitFallbackMidpoint`), in the style of
  `tests/calibration_run_tests.rs`.
- The text helpers (verdict, footer lines, result step lines) are pure functions with tests, as
  `estimate_text` / `result_note` are today.
- An old result JSON without the new fields deserializes and renders the reduced replay.
- New `examples/ui_preview.rs` scenes: replay on a middle round, on Result, pooled, no answer, old
  result without the new fields, small width.

## Left to the builder

- Exact struct and field names, and whether the per-round data sits on `RoundSignals` or beside it.
- Graph drawing: `egui_plot` like the other graphs, or painted directly, as long as it matches the
  mockup.
- Step duration (start at 1.6 s) and whether the step change animates.
