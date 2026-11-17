# Aim Habits Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record per-target aim features in degrees during calibration and practice, turn them into named habits (what to practise) plus trust notes for a calibration result, and show habits and their progress on My sensitivity and in History.

**Architecture:** A new `habits` module owns everything: record types (`record.rs`), stroke maths in degrees (`strokes.rs`), a live `AimRecorder` that `main.rs` feeds every frame (`recorder.rs`), pure analysis (`analysis.rs`), calibration trust notes (`trust.rs`) and practice trends (`trend.rs`). Tasks expose the target being aimed at through one new `Task::aim_target()` method; storage adds `#[serde(default)]` fields; the UI reads a `HabitReport` stored on the calibration result plus the history.

**Tech Stack:** Rust 1.80+, glam, serde/serde_json, chrono, egui (+ egui_phosphor), existing `cargo test` integration tests under `tests/` importing `aim_trainer::…`.

**Spec:** `docs/superpowers/specs/2026-11-18-aim-habits-design.md` (read it first; this plan argues from it).

## Global Constraints

- Every stored angle is in degrees of view rotation, never mouse counts. Same hand movement at 400 and 3200 DPI → same records.
- No change to PSA, the joint fit, pooling, `confidence`, or `cm_360` of any result.
- No raw mouse samples on disk.
- Every new persisted field is `#[serde(default)]`; files saved before this load unchanged.
- No NaN or infinity ever reaches a persisted struct (serde_json writes NaN as `null`, which then fails to load and `load_history` silently starts an empty history: data loss on the next save).
- Thresholds (exact): flick stop short `median first_landing < 0.85`; flick go past `> 1.10`; flick corrections `mean strokes > 2.5`; micro overshoot `share crossed_center > 0.35`; micro creep `share (strokes ≥ 3 && !crossed_center) > 0.35`; tracking trails `median lag_ms > 40`; tracking runs ahead `median lag_ms < -40`; tracking shakes `median cross_rms_deg / radius_deg > 0.5`; misses lean `|mean along| or |mean across| (÷ radius_deg) > 0.4` with ≥ 10 misses.
- Minimum data: practice ≥ 12 records per measure, per-band flick measures ≥ 8 in that band; calibration ≥ 16 counted records spanning ≥ 3 blocks.
- At most three habits shown. Habits come from the latest run only, never pooled.
- Trend counts only practice sessions with `|ln(tested_cm / result_cm)| ≤ ln 1.12`.
- Practice Flick: 30 targets, 10 per distance band, shuffled with `banded_order`.
- UI copy (exact): section "What to practise"; empty "No habit stood out in this run."; card line "Still there at your best sensitivity → practise, don't change it."; "Since calibrating: {first} → {last}"; "Practise at this sensitivity to track progress."; button "Practise: {scenario name}"; warm-up note "Your aim was still improving during this run. Warm up longer before calibrating."; left-out note "{Test} was left out of the result: {phrase} at every speed tested. That's a habit to practise, not a sensitivity problem."; no-answer "No habits are named for this run: it found no sensitivity to measure them at."; history section "Habits this session".

## Review Focus

1. **Degenerate targets** (player doesn't move, target spawns under the crosshair, zero-length offset): records have `first_landing: None` and finite fields; history with such a record saves and reloads intact. → Task 1 and Task 4 tests.
2. **Paused and resumed calibration segment:** the replayed segment's targets count once, not twice. → Task 3 test `discarded_segment_does_not_count`.
3. **Practice abandoned or profile switched mid-run:** no records leak into the next saved session. → Task 3 test `reset_drops_everything`, wired in Task 4.
4. **Sessions too short for any measure** (a 5-target Speed run): no habit, no panic, no division by zero. → Task 5 test `too_few_records_is_silent`.
5. **Old files:** a history and a profile JSON written before this change load with `aim: None` / `habits: None` and every old value intact. → Task 4 test `old_history_loads`.

---

### Task 1: Record types and stroke features in degrees

**Files:**
- Create: `src/habits/mod.rs`, `src/habits/record.rs`, `src/habits/strokes.rs`
- Modify: `src/lib.rs` (add `pub mod habits;`), `src/sensitivity/micro_signal.rs` (share stroke splitting)
- Test: `tests/habit_strokes_tests.rs`

**Interfaces:**
- Produces (`habits::record`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub enum AimKind { Flick, Micro, Moving }   // Switching and Speed record as Flick
  #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
  pub struct PlayContext { pub cm_360: f32, pub side: Option<Side>, pub block: Option<u8>, pub warmup: bool }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct TargetRecord {
      pub kind: AimKind, pub offset_deg: [f32; 2], pub radius_deg: f32,
      pub first_landing: Option<f32>, pub strokes: u8, pub crossed_center: bool, pub overshoot_deg: f32,
      pub time_to_hit_ms: Option<f32>, pub misses: Vec<[f32; 2]>, // [along, across], deg, max 8
      pub band: Option<FlickBand>, pub ctx: PlayContext,
  }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct TrackingRecord {
      pub mean_along_deg: f32, pub lag_ms: f32, pub cross_rms_deg: f32, pub mean_speed_deg_s: f32,
      pub coverage: f32, pub radius_deg: f32, pub horizontal: bool, pub ctx: PlayContext,
  }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
  pub struct AimLog { pub targets: Vec<TargetRecord>, pub tracking: Vec<TrackingRecord> }
  ```
  `PlayContext.block == None` means practice. `ctx.cm_360` is the cm/360 actually played (block cm × side factor).
- Produces (`habits::strokes`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct DegSample { pub t_ms: f32, pub d: Vec2 }          // degrees moved this sample, y down; ms since the target appeared
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct StrokeFeatures { pub first_landing: f32, pub strokes: u8, pub crossed_center: bool, pub overshoot_deg: f32 }
  pub fn stroke_features(samples: &[DegSample], target_offset_deg: Vec2) -> Option<StrokeFeatures>;
  pub fn band_for_angle(angle_deg: f32) -> Option<FlickBand>;  // using FlickBand::range_deg; None between bands
  ```
  `first_landing` = net progress along the target direction at the end of the first stroke ÷ distance. `strokes`, `crossed_center` and `overshoot_deg` follow `classify_micro` exactly (same constants, tolerance `max(0.15°, 0.10 × distance)`). `None` when distance < 1e-4° or path < 0.1°.

- [ ] **Step 1: Write the failing tests** in `tests/habit_strokes_tests.rs`. Build samples with a helper `stroke(from_ms, to_ms, total: Vec2, n) -> Vec<DegSample>` (evenly spaced, bell-shaped speed) and gaps of 60 ms between strokes.

```rust
#[test] fn single_stroke_stopping_at_70_percent_lands_short() {
    let f = stroke_features(&stroke(0., 120., Vec2::new(7.0, 0.), 24), Vec2::new(10.0, 0.)).unwrap();
    assert!((f.first_landing - 0.7).abs() < 0.03); assert_eq!(f.strokes, 1); assert!(!f.crossed_center);
}
#[test] fn going_past_then_back_crosses_center() {
    // 12° then -2° toward a 10° target
    let f = stroke_features(&[stroke(0.,120.,Vec2::new(12.,0.),24), stroke(180.,240.,Vec2::new(-2.,0.),12)].concat(), Vec2::new(10.,0.)).unwrap();
    assert!(f.first_landing > 1.15); assert!(f.crossed_center); assert!((f.overshoot_deg - 2.0).abs() < 0.2); assert_eq!(f.strokes, 2);
}
#[test] fn three_short_strokes_count_three() { /* 6°, 2.5°, 1.5° toward 10° */ assert_eq!(f.strokes, 3); assert!(!f.crossed_center); }
#[test] fn no_movement_is_none() { assert!(stroke_features(&[], Vec2::new(10.,0.)).is_none()); }
#[test] fn zero_distance_target_is_none() { assert!(stroke_features(&stroke(0.,100.,Vec2::new(3.,0.),10), Vec2::ZERO).is_none()); }
#[test] fn bands_follow_flick_ranges() {
    for b in FlickBand::ALL { let (lo, hi) = b.range_deg(); assert_eq!(band_for_angle((lo+hi)/2.0), Some(b)); }
}
#[test] fn target_record_round_trips_json() { /* serde_json to_string → from_str equals */ }
```

- [ ] **Step 2: Run** `cargo test --test habit_strokes_tests` — Expected: FAIL (module `habits` not found).

- [ ] **Step 3: Implement.** Move the windowed-speed stroke splitting out of `classify_micro` into a `pub(crate) fn split_strokes(samples_deg: &[DegSample], u: Vec2) -> Vec<f32>` (each stroke's net progress along `u`) in `micro_signal.rs`, have `classify_micro` convert its `MouseSample`s to `DegSample`s and call it, and build `stroke_features` on the same helper. `classify_micro`'s behaviour must not change.

- [ ] **Step 4: Run** `cargo test --test habit_strokes_tests --test paired_micro_tests --test paired_signal_tests` — Expected: all PASS.

- [ ] **Step 5: Commit** `git add src/habits src/lib.rs src/sensitivity/micro_signal.rs tests/habit_strokes_tests.rs && git commit -m "feat: aim record types and stroke features in degrees"`

---

### Task 2: Tasks expose the aimed target; practice Flick by distance; tracking windows carry direction

**Files:**
- Modify: `src/game/tasks/mod.rs` (trait method, `TaskManager::current_aim_target`, practice Flick construction), `flick.rs`, `micro_adjust.rs`, `switching.rs`, `speed.rs`, `moving.rs`, `tracking.rs`, `src/sensitivity/tracking_signal.rs`
- Test: `tests/aim_target_tests.rs`, `tests/flick_bands_tests.rs` (add), `tests/tracking_signal_tests.rs` (add)

**Interfaces:**
- Produces (`game::tasks`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct AimTarget { pub id: u64, pub center: Vec3, pub radius: f32, pub kind: AimKind, pub band: Option<FlickBand> }
  // on trait Task, default None:
  fn aim_target(&self) -> Option<AimTarget>;
  // on TaskManager (None unless Running):
  pub fn current_aim_target(&self) -> Option<AimTarget>;
  ```
  Flick (incl. `CalibrationTask`): live target, `band` from its `FlickSpawn`. Micro: live target, kind `Micro`. Switching: the highlighted target, kind `Flick`, band `None`. Speed: the live target, kind `Flick`. Moving: live target, kind `Moving`. Tracking: `None`.
- Produces (`TrackingWindow`): new pub fields `radius_deg: f32` and `horizontal: bool` (mean target velocity's |x| ≥ |y| over the window's counted samples).
- Practice Flick: `TaskType::Flick` builds `FlickTask::banded(banded_order(10, &mut rng), seed, 0.35)` with a time-seeded `StdRng`; 30 targets total.

- [ ] **Step 1: Write failing tests.**

```rust
// aim_target_tests.rs
#[test] fn flick_reports_live_target_with_band() { /* FlickTask::banded(vec![FlickBand::Far], 7, 0.35); start; update(0.016, level()); let a = t.aim_target().unwrap(); assert_eq!(a.kind, AimKind::Flick); assert_eq!(a.band, Some(FlickBand::Far)); */ }
#[test] fn aim_target_id_changes_after_hit() { /* shoot the target's centre; update; new id != old id */ }
#[test] fn switching_reports_highlighted_target() { /* the reported id is the one whose hit returns true from on_click */ }
#[test] fn micro_and_moving_report_their_kind() { /* Micro → AimKind::Micro, Moving → AimKind::Moving */ }
#[test] fn tracking_reports_none() {}
// flick_bands_tests.rs
#[test] fn practice_flick_spawns_ten_per_band() {
    let mut m = TaskManager::new(); m.set_task(TaskType::Flick); m.start_task_now();
    // shoot each target's centre 30 times, counting current_aim_target().band
    assert_eq!(counts, [10, 10, 10]);
}
// tracking_signal_tests.rs
#[test] fn horizontal_motion_marks_window_horizontal() { /* push 0.5 s with target_vel (20,0) → horizontal == true; (0,20) → false; radius_deg passed to new() is copied */ }
```

- [ ] **Step 2: Run** `cargo test --test aim_target_tests --test flick_bands_tests --test tracking_signal_tests` — Expected: FAIL (`aim_target` not found).

- [ ] **Step 3: Implement** the trait method per task, `TrackingWindow` fields, and the practice Flick change. Practice Flick's description string stays.

- [ ] **Step 4: Run** `cargo test` — Expected: all PASS (existing flick, micro, tracking, render-loop tests included).

- [ ] **Step 5: Commit** `git commit -am "feat: tasks report the aimed target; practice flick spawns by distance"` (add the new test file first).

---

### Task 3: AimRecorder

**Files:**
- Create: `src/habits/recorder.rs`
- Test: `tests/aim_recorder_tests.rs`

**Interfaces:**
- Consumes: `AimTarget` (Task 2), `stroke_features`, `band_for_angle`, record types (Task 1), `TrackingWindow` (Task 2), `micro_adjust::mouse_offset_deg`.
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct ShotOffset { pub offset_m: f32, pub radius_m: f32 }   // hit point's distance from the target centre, at the target's depth
  pub struct AimRecorder { /* … */ }
  impl AimRecorder {
      pub fn new() -> Self;
      pub fn set_context(&mut self, ctx: PlayContext);                   // read when a target begins / a window is pushed
      pub fn push_mouse(&mut self, now_s: f64, dx: f32, dy: f32, deg_per_count: f32);
      pub fn track(&mut self, now_s: f64, eye: Vec3, aim_dir: Vec3, target: Option<AimTarget>);
      pub fn on_click(&mut self, now_s: f64, eye: Vec3, aim_dir: Vec3, hit: bool) -> Option<ShotOffset>;
      pub fn push_tracking(&mut self, windows: &[TrackingWindow]);
      pub fn commit(&mut self);              // pending records → log
      pub fn discard_pending(&mut self);     // drop pending records and the target in progress
      pub fn take_log(&mut self) -> AimLog;  // committed log, then empty; the target in progress is dropped
      pub fn reset(&mut self);               // everything empty
  }
  ```
- Behaviour:
  - `track`: when `target.id` differs from the current one, finish the current one as expired (`time_to_hit_ms: None`) unless it already finished, then begin the new one: `offset_deg = mouse_offset_deg(aim_dir, center - eye)`, `radius_deg = atan(radius / |center - eye|)` in degrees, band = target's band or `band_for_angle(|offset|)` for `AimKind::Flick`, `None` otherwise; samples start empty, clock starts at `now_s`, context captured now.
  - `push_mouse`: converts counts to degrees at once (`d = (dx, dy) × deg_per_count`) and appends to the current target's samples.
  - `on_click` miss: appends `[along, across]` (degrees, relative to the target centre, along = the unit of `offset_deg`) if fewer than 8; returns `None`. Hit: finishes the target with `time_to_hit_ms`, returns `ShotOffset`. No current target: `None`.
  - Finishing computes `stroke_features`; `None` → `first_landing: None, strokes: 0, crossed_center: false, overshoot_deg: 0.0`. `AimKind::Moving` always stores `first_landing: None`. Every float is checked finite; non-finite → that field `None`/0.
  - A target id that already finished is never begun again.

- [ ] **Step 1: Write failing tests.**

```rust
#[test] fn same_hand_movement_same_record_at_any_dpi() {
    // 800 counts @400 DPI (deg_per_count d) vs 6400 counts @3200 DPI (d/8): equal TargetRecord
}
#[test] fn hit_records_time_and_landing() { /* begin at t=0, push 7° along +x toward 10°, hit at t=0.4 → time_to_hit_ms≈400, first_landing≈0.7 */ }
#[test] fn expired_target_has_no_time() { /* track A, then track B → A recorded with time_to_hit_ms None */ }
#[test] fn misses_capped_at_eight() { /* 11 misses → 8 entries */ }
#[test] fn miss_position_is_along_and_across() {
    // p = aim − centre in mouse degrees (y down); u = unit of offset_deg; along = p·u (− short, + past); across = p·(−u.y, u.x)
    // target 10° right of the spawn aim, click aimed 9° right and 1° up → misses[0] ≈ [-1.0, -1.0]
}
#[test] fn discarded_segment_does_not_count() { /* record 3, commit; record 2, discard_pending; record 2, commit; take_log().targets.len() == 5 */ }
#[test] fn reset_drops_everything() { /* record, commit, reset → take_log() is empty */ }
#[test] fn context_is_captured_at_begin() { /* set_context(cm 30, Faster), begin, set_context(cm 40), hit → ctx.cm_360 == 30 */ }
#[test] fn degenerate_target_is_finite_and_round_trips() { /* target exactly at aim, no movement, hit → first_landing None; serde_json round trip equal */ }
#[test] fn moving_target_has_no_landing() {}
#[test] fn tracking_windows_take_context() { /* push_tracking copies fields + ctx */ }
```

- [ ] **Step 2: Run** `cargo test --test aim_recorder_tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `AimRecorder` per the behaviour above.
- [ ] **Step 4: Run** `cargo test --test aim_recorder_tests` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat: AimRecorder turns live play into aim records"`

---

### Task 4: Wire the recorder into the game; store logs; real practice metrics

**Files:**
- Modify: `src/main.rs`, `src/data/history.rs`, `src/data/metrics.rs` (doc comments only if needed)
- Test: `tests/habit_storage_tests.rs`

**Interfaces:**
- Consumes: `AimRecorder` (Task 3), `TaskManager::current_aim_target` (Task 2).
- Produces: `SessionRecord.aim: Option<AimLog>` and `SessionRecord.habits: Option<HabitReport>` (both `#[serde(default, skip_serializing_if = "Option::is_none")]`; `HabitReport` comes from Task 6 — add the field in Task 6, not here).
- `main.rs` wiring (`App` gets `aim: AimRecorder`):
  - Raw mouse deltas: `self.aim.push_mouse(now, dx, dy, self.rad_per_count.to_degrees())` next to `path_recorder.record_sample`.
  - Every frame while playing, after `task_manager.update`: `set_context(PlayContext { cm_360: (current_cm_360 × factor) as f32, side, block, warmup })` where `side` = `Faster` if factor < 1, `Slower` if > 1, else `None`, and `block`/`warmup` as below. Then `self.aim.track(now, eye, forward, self.task_manager.current_aim_target())`, then `self.aim.push_tracking(&self.task_manager.take_tracking_windows())` (replacing the bare drain).
  - Tracking windows (0.5 s) line up with paired tracking chunks (3.0 s, both from t = 0), so a window never straddles a side change; take the context at push time.
  - `on_shot`: `let off = self.aim.on_click(now, eye, forward, hit)` **before** `path_recorder.end_shot()`; `MetricPoint` uses `off` for `offset_from_center_m`, `target_radius_m`, `precision_percent = if hit {100 × (1 − offset/radius) clamped 0..=100} else {0}`; with no `off` (tracking clicks), keep the shot but use `0.0, 0.0, 0.0`.
  - Practice finish (`record_session` for non-calibration): `aim: Some(self.aim.take_log())`.
  - Calibration: `on_segment_finished` → `self.aim.commit()`; resume's `run.restart_segment()` → `self.aim.discard_pending()`; `begin_segment` does not reset; `finish_calibration` → `take_log()` (used in Task 6; for now store it on the "Calibration" `SessionRecord`); `quit_calibration`, `start_run`, `end_paused_practice`, `after_profile_switch` → `reset()`.
  - Practice: `start_run` resets; `record_session` for practice calls `self.aim.commit()` then `take_log()` (practice never discards; only calibration uses `discard_pending`).
  - Calibration context: `block: Some(run.block_number() as u8)` and `warmup: false` only in counted segments after the warm-up; the warm-up and every uncounted segment record `block: None, warmup: true`. Practice records `block: None, warmup: false`.
  - Storage choice (spec left encoding to the builder): the calibration's `AimLog` lives on its "Calibration" `SessionRecord` in `history\`, next to practice logs, so the History detail can redraw it; the `calibrations\` archive gets the analysed `HabitReport` through `CalibrationResult.habits` (Task 6).

- [ ] **Step 1: Write failing tests** in `tests/habit_storage_tests.rs`.

```rust
#[test] fn old_history_loads() {
    // a SessionHistory JSON literal in the pre-change shape (no "aim"), incl. MetricPoints
    let h: SessionHistory = serde_json::from_str(OLD).unwrap();
    assert!(h.sessions[0].aim.is_none()); assert_eq!(h.sessions[0].metrics.points.len(), 2);
}
#[test] fn session_with_aim_round_trips() { /* SessionRecord with an AimLog of one TargetRecord + one TrackingRecord */ }
#[test] fn session_without_aim_omits_field() { /* to_string doesn't contain "\"aim\"" */ }
```

- [ ] **Step 2: Run** `cargo test --test habit_storage_tests` — Expected: FAIL (no field `aim`).
- [ ] **Step 3: Implement** the `SessionRecord` field and the `main.rs` wiring above.
- [ ] **Step 4: Run** `cargo test` — Expected: all PASS. Then `cargo run`, play a practice Flick session and a few calibration segments, close, and confirm `%APPDATA%\AimTrainer\history\*_history.json` has `aim.targets` with finite numbers and `precision_percent` varying between hits.
- [ ] **Step 5: Commit** `git commit -m "feat: record aim habits during play; real precision in practice history"`

---

### Task 5: Habit measures and practice session values

**Files:**
- Create: `src/habits/analysis.rs`
- Test: `tests/habit_analysis_tests.rs`

**Interfaces:**
- Consumes: `AimLog`, `TargetRecord`, `TrackingRecord`, `AimKind` (Task 1).
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub enum Measure { FlickLanding(FlickBand), FlickStrokes, MicroOvershoot, MicroCreep, TrackingLag, TrackingShake, MissLeanFlick, MissLeanMicro }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub enum Habit { FlickShort(FlickBand), FlickPast(FlickBand), FlickCorrections, MicroOvershoot, MicroCreep, TrackingTrails, TrackingAhead, TrackingShakes, MissLeanFlick, MissLeanMicro }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub enum Dir { Left, Right, Up, Down, SideToSide, UpAndDown }
  #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
  pub struct MeasureValue { pub measure: Measure, pub value: f32, pub n: usize, pub habit: Option<Habit> }

  impl Measure {
      pub const ALL: [Measure; 10];                       // 3 landing bands + 7 others
      pub fn value(self, log: &AimLog, keep: impl Fn(&PlayContext) -> bool) -> Option<(f32, usize)>; // None below practice minimum
      pub fn badness(self, v: f32) -> f32;                 // |1−v| landing, |v| lag, v otherwise
      pub fn habits(self) -> &'static [Habit];             // e.g. FlickLanding(b) → [FlickShort(b), FlickPast(b)]
  }
  impl Habit {
      pub fn measure(self) -> Measure;
      pub fn severity(self, v: f32) -> Option<f32>;        // (v beyond threshold) ÷ |threshold|; None if not beyond
      pub fn title(self) -> String;                        // "Far flicks stop short", "Tracking trails the target", …
      pub fn value_text(self, v: f32) -> String;           // "18% short", "62 ms behind", "41% overshoot", "2.8 moves per target", "0.6× target size", "0.5× target size off"
      pub fn practice(self) -> TaskType;                   // Flick*/MissLeanFlick → Flick; Micro*/MissLeanMicro → MicroAdjust; Tracking* → Tracking
      pub fn lean(self) -> Option<Signal>;                 // Short/Creep/Trails → TooSlow; Past/Overshoot/Ahead → TooFast; others None
      pub fn test_label(self) -> &'static str;             // "Flick" | "Micro" | "Tracking" (matches TestKind::label)
      pub fn phrase(self) -> &'static str;                 // for the left-out note: "you trailed the target", "your flicks stopped short", …
  }
  pub fn session_values(log: &AimLog) -> Vec<MeasureValue>;             // practice: every measure with enough data, habit set when beyond threshold
  pub fn worst_direction(habit: Habit, log: &AimLog, keep: impl Fn(&PlayContext) -> bool) -> Option<Dir>;
  ```
- Measure definitions are the spec's table; flick measures use `AimKind::Flick` records only, micro measures `AimKind::Micro`, `MissLean*` the misses of that kind (value = the larger of |mean along| and |mean across| ÷ mean `radius_deg`, needs ≥ 10 misses), records with `first_landing: None` are skipped by landing measures.
- `worst_direction`: split by the dominant axis of `offset_deg` (x > 0 Right, x < 0 Left, y > 0 Down, y < 0 Up); tracking by `horizontal`. A direction qualifies with ≥ 4 records; returns it when it alone is beyond the habit's threshold.

- [ ] **Step 1: Write failing tests** (build logs with a helper `flick(band, landing, strokes)`, `micro(crossed, strokes)`, `window(lag_ms, cross_rms, radius)`):

```rust
#[test] fn far_flicks_short_are_flagged() { /* 10 far at landing 0.78 → MeasureValue{FlickLanding(Far), value≈0.78, habit: Some(FlickShort(Far))} */ }
#[test] fn landing_inside_band_is_not_flagged() { /* 10 at 0.95 → habit None */ }
#[test] fn per_band_minimum_is_eight() { /* 7 far → no FlickLanding(Far) value; 8 → value */ }
#[test] fn too_few_records_is_silent() { /* 5 flicks, 3 micro, 4 windows → session_values empty */ }
#[test] fn micro_overshoot_share() { /* 12 micro, 5 crossed → 0.4167, habit MicroOvershoot */ }
#[test] fn creep_needs_three_strokes_without_crossing() {}
#[test] fn tracking_lag_both_ways() { /* median 55 → TrackingTrails; median −50 → TrackingAhead; 30 → None */ }
#[test] fn shake_is_relative_to_radius() { /* cross_rms 0.6, radius 1.0 → 0.6 flagged */ }
#[test] fn miss_lean_needs_ten_misses() {}
#[test] fn severity_is_share_beyond_threshold() { assert!((Habit::FlickShort(FlickBand::Far).severity(0.68).unwrap() - 0.2).abs() < 1e-3); }
#[test] fn direction_named_only_when_alone() { /* left flicks 0.7, others 0.95 → Some(Left); all 0.7 → None */ }
#[test] fn value_texts() { assert_eq!(Habit::FlickShort(FlickBand::Far).value_text(0.82), "18% short"); assert_eq!(Habit::TrackingTrails.value_text(62.4), "62 ms behind"); }
```

- [ ] **Step 2: Run** `cargo test --test habit_analysis_tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `analysis.rs`.
- [ ] **Step 4: Run** `cargo test --test habit_analysis_tests` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat: habit measures and practice session values"`

---

### Task 6: Calibration habits, trust notes, HabitReport on the result

**Files:**
- Create: `src/habits/trust.rs`
- Modify: `src/habits/analysis.rs`, `src/data/calibration.rs`, `src/data/history.rs`, `src/main.rs` (`finish_calibration`)
- Test: `tests/habit_calibration_tests.rs`

**Interfaces:**
- Consumes: Task 5 API; `RunDiagnostics`, `RoundSignals`, `PairRecord`, `EstimateRecord` (existing).
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct HabitFinding { pub habit: Habit, pub value: f32, pub direction: Option<Dir> }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct LeftOutNote { pub test: String, pub habit: Habit }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
  pub struct HabitReport { pub findings: Vec<HabitFinding>, pub no_answer: bool, pub warming_up: bool, pub left_out: Vec<LeftOutNote> }

  // analysis.rs
  pub fn calibration_habits(log: &AimLog, result_cm: f64) -> Vec<HabitFinding>;   // ranked, ≤ 3
  // trust.rs
  pub fn still_warming_up(rounds: &[RoundSignals]) -> bool;
  pub fn left_out_notes(diag: &RunDiagnostics, findings: &[HabitFinding]) -> Vec<LeftOutNote>;
  pub fn habit_report(log: &AimLog, result: &CalibrationResult) -> HabitReport;  // uses result.cm_360, no_answer, diagnostics
  ```
  `CalibrationResult.habits: Option<HabitReport>` and `SessionRecord.habits: Option<HabitReport>`, both `#[serde(default, skip_serializing_if = "Option::is_none")]`. `apply_pool` must not touch `habits`.
- `calibration_habits` algorithm (spec §5): counted records only (`block.is_some() && !warmup`). For each measure, group records by `ctx.cm_360` (rounded to 0.01); each group with ≥ 2 records gives (ln cm, the measure's statistic over the group, weight n). Needs ≥ 16 records over ≥ 3 distinct blocks. Weighted least squares line; value at `ln(result_cm)`; a habit is named when its `severity` there is `Some`. Rank by severity, keep 3. `direction` from `worst_direction` over the same counted records.
- `still_warming_up` algorithm (spec §6): per test label (`"Flick"` for every flick band, `"Micro"`, `"Tracking"`), one point per counted round with pairs: `(i, ln tested_cm, mean of (faster_cost+slower_cost)/2 over its pairs ÷ the test's overall mean)`. Fit `y = a + b·ln + c·ln² + d·i` by least squares; needs ≥ 6 points. Fires when ≥ 2 tests have `d × (i_last − i_first) < −0.15` and `d`'s 95% interval (t with n−4 dof) excludes 0.
- `left_out_notes`: for each `diag.left_out` test, its lean = `TooFast` if its estimate's cm > max tested cm, `TooSlow` if < min; a note for the first finding with `test_label == test` and `lean == Some(that)`.
- `habit_report`: `no_answer` → empty findings, `no_answer: true`, still computes `warming_up`; otherwise all three parts. Must use the run's own diagnostics: in `finish_calibration`, build the report from `result` **before** `apply_pool`, set `result.habits`, and put the same report plus the log on the "Calibration" `SessionRecord`.

- [ ] **Step 1: Write failing tests.**

```rust
#[test] fn habit_at_result_is_named() {
    // 7 blocks at 20..60 cm, each side ±12%, far flick landing 0.78 everywhere → FlickShort(Far) named
}
#[test] fn sensitivity_lean_is_not_a_habit() {
    // landing = 1.0 + 0.6·ln(cm/35): fine at 35, short far below it; result 35 → no FlickShort
}
#[test] fn no_answer_names_nothing() { /* habit_report with result.no_answer → findings empty, no_answer true */ }
#[test] fn needs_three_blocks() { /* 20 records in 2 blocks → nothing */ }
#[test] fn at_most_three_ranked() { /* 5 habits → 3, descending severity */ }
#[test] fn warm_up_not_confused_with_search() {
    // costs follow a U in ln cm only (no block trend), blocks visited far→near → false
}
#[test] fn warm_up_detected() { /* same U plus 25% fall from first to last block in all tests → true */ }
#[test] fn left_out_note_needs_matching_lean() {
    // Tracking left out with estimate above tested range (TooFast lean) + TrackingTrails (TooSlow) → no note;
    // + TrackingAhead → note
}
#[test] fn report_uses_unpooled_left_out() { /* build report, then apply_pool with different left_out → report unchanged */ }
#[test] fn old_profile_loads_without_habits() { /* CalibrationResult JSON without "habits" → habits None */ }
```

- [ ] **Step 2: Run** `cargo test --test habit_calibration_tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `calibration_habits`, `trust.rs`, the fields, and the `finish_calibration` change.
- [ ] **Step 4: Run** `cargo test` — Expected: all PASS (pooling and calibration tests unchanged).
- [ ] **Step 5: Commit** `git commit -m "feat: calibration habits and trust notes on the result"`

---

### Task 7: Progress since calibrating and session comparison

**Files:**
- Create: `src/habits/trend.rs`
- Test: `tests/habit_trend_tests.rs`

**Interfaces:**
- Consumes: `SessionHistory`, `SessionRecord.aim` (Task 4), `Measure::value`, `Measure::badness`, `session_values` (Task 5), `HabitFinding` (Task 6).
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq)]
  pub struct Progress { pub points: Vec<f32>, pub practising_elsewhere: bool }   // points: oldest → newest, ≤ 10
  pub fn progress(history: &SessionHistory, habit: Habit, since: DateTime<Utc>, result_cm: f64) -> Progress;
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Change { Better, Worse, Same }
  #[derive(Clone, Debug, PartialEq)]
  pub struct SessionHabitRow { pub value: MeasureValue, pub previous_mean: Option<f32>, pub change: Option<Change> }
  pub fn session_rows(history: &SessionHistory, session: &SessionRecord) -> Vec<SessionHabitRow>;
  ```
- `progress`: practice sessions (`task_type != "Calibration"`) after `since` with `aim`, `|ln(tested_cm_360 / result_cm)| ≤ ln 1.12`, and a value for `habit.measure()`; last 10. `practising_elsewhere` = no such session but at least one after `since` outside the band with a value.
- `session_rows`: `session_values(session.aim)`; `previous_mean` over the five previous sessions with the same `task_type` that have that measure; `change` by `badness`: Same when within 10% of the previous badness.

- [ ] **Step 1: Write failing tests.**

```rust
#[test] fn progress_counts_sessions_after_calibration_near_result() { /* 3 sessions within ±12%, 1 at +30%, 1 before since → 3 points in time order */ }
#[test] fn practising_elsewhere_when_only_far_sessions() {}
#[test] fn progress_keeps_last_ten() {}
#[test] fn rows_compare_with_previous_five_same_scenario() { /* 6 earlier Flick sessions + 1 Tracking; mean over the 5 most recent Flick */ }
#[test] fn change_uses_badness() { /* landing 0.80 → 0.90: Better; lag −30 → −60: Worse; 0.50 → 0.52 share: Same */ }
#[test] fn sessions_without_aim_are_skipped() {}
```

- [ ] **Step 2: Run** `cargo test --test habit_trend_tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `trend.rs`.
- [ ] **Step 4: Run** `cargo test --test habit_trend_tests` — Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "feat: habit progress since calibrating and per-session comparison"`

---

### Task 8: Screens

**Files:**
- Create: `src/ui/habits.rs`
- Modify: `src/ui/mod.rs` (`mod habits;`), `src/ui/screens/sensitivity.rs`, `src/ui/screens/history.rs`, `src/ui/graphs.rs` (sparkline), `examples/ui_preview.rs`
- Test: `tests/ui_habits_tests.rs`

**Interfaces:**
- Consumes: `HabitReport`, `HabitFinding`, `Habit` texts (Tasks 5–6), `progress`, `session_rows` (Task 7), `UiAction::StartPractice(TaskType)` (existing).
- Produces:
  ```rust
  // ui::habits — pure text helpers (tested) and drawing
  pub fn card_lines(f: &HabitFinding, p: &Progress) -> CardLines;   // title, value, "Still there…" + direction sentence, since-line or elsewhere-line, button label
  pub struct CardLines { pub title: String, pub value: String, pub why: String, pub progress: Option<String>, pub button: String }
  pub fn left_out_sentence(n: &LeftOutNote) -> String;
  pub fn practise_section(ui: &mut egui::Ui, model: &UiModel, cal: &CalibrationResult, report: &HabitReport, actions: &mut Vec<UiAction>);
  pub fn session_section(ui: &mut egui::Ui, rows: &[SessionHabitRow]);
  // ui::graphs
  pub fn sparkline(ui: &mut egui::Ui, values: &[f32]);
  ```
- Placement: My sensitivity `result()` — after the buttons, before "How we got here", only when `cal.habits` is `Some`. History `detail()` — practice: "Habits this session" after the stat cards (rows with value, previous mean, ↑/↓/= arrow, beyond-threshold rows marked with the warning colour); calibration session with `habits`: the same cards as My sensitivity (progress from `s.timestamp`, result `s.tested_cm_360`). Sessions without `aim` show nothing new.
- Direction sentences: "Mostly to the left." / right / "Mostly upward." / "Mostly downward." / "Mostly side to side." / "Mostly up and down."
- Scenario names for buttons: the practice screen's existing names.
- "Since calibrating: {first} → {last}" uses `value_text`'s number part with the finding value as first and the newest point as last; only when `points` is non-empty.

- [ ] **Step 1: Write failing tests** (text helpers only):

```rust
#[test] fn card_reads_like_the_spec() {
    let l = card_lines(&finding(Habit::FlickShort(FlickBand::Far), 0.82, Some(Dir::Left)), &Progress { points: vec![0.85, 0.88], practising_elsewhere: false });
    assert_eq!(l.title, "Far flicks stop short"); assert_eq!(l.value, "18% short");
    assert_eq!(l.why, "Still there at your best sensitivity → practise, don't change it. Mostly to the left.");
    assert_eq!(l.progress.as_deref(), Some("Since calibrating: 18% → 12%")); assert_eq!(l.button, "Practise: Flick");
}
#[test] fn elsewhere_line() { /* practising_elsewhere → Some("Practise at this sensitivity to track progress.") */ }
#[test] fn no_progress_line_without_sessions() {}
#[test] fn left_out_sentence_text() { assert_eq!(left_out_sentence(&note("Tracking", Habit::TrackingTrails)), "Tracking was left out of the result: you trailed the target at every speed tested. That's a habit to practise, not a sensitivity problem."); }
```

- [ ] **Step 2: Run** `cargo test --test ui_habits_tests` — Expected: FAIL.
- [ ] **Step 3: Implement** helpers, sections, sparkline, and the placements. Add preview scenes `sensitivity-habits` (three cards, one with progress, one practising elsewhere), `sensitivity-habits-none`, `sensitivity-habits-warmup` (warm-up note + a left-out note), `history-habits` (practice detail with rows), built from fixture data in `ui_preview.rs`.
- [ ] **Step 4: Run** `cargo test` — Expected: all PASS. Run `cargo run --example ui_preview` and look at the four new PNGs in `target/ui-preview/`: cards don't overflow at the small size, text is legible in the existing theme.
- [ ] **Step 5: Commit** `git commit -m "feat: What to practise on My sensitivity and habits in History"`

---

### Task 9: Whole-feature check

- [ ] **Step 1:** `cargo test` — all PASS; `cargo build --release` — succeeds.
- [ ] **Step 2:** `cargo run`: play one full calibration and two practice Flick sessions at the result. My sensitivity shows "What to practise" (cards or the empty line); History shows the calibration's cards and each Flick session's "Habits this session"; the history JSON reloads after restarting the app.
- [ ] **Step 3:** Update `README.md` "Reading the result" and "Practice" with two short paragraphs: what "What to practise" means (habits still there at your best sensitivity) and that practice Flick now spreads targets over three distances.
- [ ] **Step 4: Commit** `git commit -m "docs: README covers What to practise"`
