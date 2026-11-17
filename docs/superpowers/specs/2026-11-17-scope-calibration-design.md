# Scope calibration: measuring each player's scoped sensitivity

## Why

The app exists to find the sensitivity that suits *this* player. Scoped / ADS settings so far
were the generic 0% monitor distance values (the same for everyone). The owner wants every
game's every scope set to what suits them, measured, like hipfire.

## Removed 2026-11-17

The owner removed the scope test. Nothing below is built any more: calibration measures hipfire
only, and each game's scoped / ADS values are the 0% monitor distance standard again. Profiles
that saved a scope result still load; the result is ignored.

## What it does

A separate **Scope test**, started from My sensitivity, measures at three zoom levels
(2×, 4×, 8×) how fast the player likes scoped aim *relative to the 0% match* of their own
hipfire, and turns that into each game's scope settings.

- The result per zoom is a **multiplier `k`** on the 0% baseline: `k = 1` means 0% suits them,
  `k = 1.1` means they do better 10% faster.
- For a scope of magnification `z` in a game, `k(z)` is interpolated between the measured
  levels in `ln z` (held flat beyond 2× and 8×).
- Each game's 0% value (already in `data/games.json`) becomes personal:
  multipliers and percentages ×`k`; PUBG's exponential slider adds `50·log10 k` to its
  "General − x" offset. A setting that applies to every zoom (Apex, CoD's ADS multiplier)
  uses the geometric mean of the measured `k`s. Settings with no zoom info (CoD's mode and
  coefficient) stay as they are.
- Fortnite, R6 and WARDOGS still have no scope model, so they still show no value.

## How it measures (decided)

- Reuses `CalibrationRun` / `BlockScheduler` unchanged in spirit: per zoom, a warm-up then
  6 blocks; each block plays paired **micro** (8 shots) and paired **tracking** (12 s) at a
  faster and slower side (±12%), and the staircase + joint fit find the crossover. No flicks:
  scoped play is corrections and tracking.
- The searched quantity is the scoped cm/360. The 0% baseline at zoom `z` is
  `hip_cm × z`; the search range is baseline ÷1.4 … ×1.4.
- The camera's vertical FOV narrows so the view is exactly `z`× magnified
  (`tan(fov/2) / z`). Micro target spread and size, and tracking speed, size and bounds,
  scale by `1/z`, so targets look and move on screen as in the hipfire test.
- Hipfire baseline: the profile's current cm/360 (what the player uses). DPI is the profile's.
- Runs 2×, 4×, 8× in that order (~9 min in all), pausable like calibration.
- Saved on the profile as `last_scoped` (zoom, k, uncertainty, confidence per level; when;
  hipfire cm and DPI it was measured against).

## Non-goals

- Per-game FOV differences beyond the zoom itself (e.g. PUBG FPP FOV) are not modelled.
- No pooling of scope runs yet.

## Owner's calls vs mine

Owner: separate Scope test, three zoom levels (2×/4×/8×).
Mine: micro + tracking only; 6 blocks per level; ±40% range; geometric mean for
all-zoom settings; interpolation in `ln z`.
