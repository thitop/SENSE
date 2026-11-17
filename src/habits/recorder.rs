//! Turns live play into aim records: one `TargetRecord` per target, one `TrackingRecord` per
//! tracking window. Everything is stored in degrees, and every float is finite.

use glam::{Vec2, Vec3};

use crate::game::tasks::micro_adjust::mouse_offset_deg;
use crate::game::tasks::AimTarget;
use crate::habits::record::{AimKind, AimLog, PlayContext, TargetRecord, TrackingRecord};
use crate::habits::strokes::{band_for_angle, stroke_features, DegSample};
use crate::sensitivity::tracking_signal::TrackingWindow;

const MAX_MISSES: usize = 8;

/// Where a shot went relative to the target it was fired at, in metres at the target's depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotOffset {
    /// Distance from the target centre to the closest point of the aim ray.
    pub offset_m: f32,
    pub radius_m: f32,
}

struct Current {
    target: AimTarget,
    start_s: f64,
    offset_deg: Vec2,
    radius_deg: f32,
    band: Option<crate::game::tasks::flick::FlickBand>,
    samples: Vec<DegSample>,
    misses: Vec<[f32; 2]>,
    ctx: PlayContext,
    /// Set once the target is recorded; it stays here so its id is not begun again.
    finished: bool,
    /// Latest centre and eye, for misses against a moving target.
    center: Vec3,
}

#[derive(Default)]
pub struct AimRecorder {
    ctx: PlayContext,
    current: Option<Current>,
    pending: AimLog,
    log: AimLog,
}

fn fin(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn fin_ctx(mut c: PlayContext) -> PlayContext {
    c.cm_360 = fin(c.cm_360);
    c
}

fn fin_vec(v: Vec2) -> Vec2 {
    if v.is_finite() {
        v
    } else {
        Vec2::ZERO
    }
}

impl AimRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_context(&mut self, ctx: PlayContext) {
        self.ctx = fin_ctx(ctx);
    }

    pub fn push_mouse(&mut self, now_s: f64, dx: f32, dy: f32, deg_per_count: f32) {
        let Some(cur) = self.current.as_mut().filter(|c| !c.finished) else {
            return;
        };
        let d = Vec2::new(dx, dy) * deg_per_count;
        let t_ms = ((now_s - cur.start_s) * 1000.0) as f32;
        if !d.is_finite() || !t_ms.is_finite() {
            return;
        }
        cur.samples.push(DegSample { t_ms, d });
    }

    pub fn track(&mut self, now_s: f64, eye: Vec3, aim_dir: Vec3, target: Option<AimTarget>) {
        let Some(t) = target else {
            // The task has no target now: whatever was in progress ran out.
            self.finish(None);
            return;
        };
        match self.current.as_mut() {
            Some(cur) if cur.target.id == t.id => {
                if !cur.finished {
                    cur.center = t.center;
                }
                return;
            }
            Some(_) => self.finish(None),
            None => {}
        }
        self.begin(now_s, eye, aim_dir, t);
    }

    fn begin(&mut self, now_s: f64, eye: Vec3, aim_dir: Vec3, t: AimTarget) {
        let to_target = t.center - eye;
        let offset_deg = fin_vec(mouse_offset_deg(aim_dir, to_target));
        let dist = to_target.length();
        let radius_deg = fin((t.radius / dist).atan().to_degrees());
        let band = match t.kind {
            AimKind::Flick => t.band.or_else(|| band_for_angle(offset_deg.length())),
            _ => None,
        };
        self.current = Some(Current {
            target: t,
            start_s: now_s,
            offset_deg,
            radius_deg,
            band,
            samples: Vec::new(),
            misses: Vec::new(),
            ctx: self.ctx,
            finished: false,
            center: t.center,
        });
    }

    pub fn on_click(&mut self, now_s: f64, eye: Vec3, aim_dir: Vec3, hit: bool) -> Option<ShotOffset> {
        let cur = self.current.as_mut().filter(|c| !c.finished)?;
        if !hit {
            if cur.misses.len() < MAX_MISSES {
                // p = aim - centre, in mouse degrees.
                let p = -mouse_offset_deg(aim_dir, cur.center - eye);
                let len = cur.offset_deg.length();
                let u = if len > 1e-4 { cur.offset_deg / len } else { Vec2::X };
                let (along, across) = (p.dot(u), p.dot(Vec2::new(-u.y, u.x)));
                if along.is_finite() && across.is_finite() {
                    cur.misses.push([along, across]);
                }
            }
            return None;
        }
        let shot = {
            let to_c = cur.center - eye;
            let dir = aim_dir.normalize_or_zero();
            let perp = to_c - dir * to_c.dot(dir);
            ShotOffset {
                offset_m: fin(perp.length()),
                radius_m: fin(cur.target.radius),
            }
        };
        let t_ms = ((now_s - cur.start_s) * 1000.0) as f32;
        self.finish(t_ms.is_finite().then_some(t_ms));
        Some(shot)
    }

    /// Records the current target (unless already recorded) and keeps its id as finished.
    fn finish(&mut self, time_to_hit_ms: Option<f32>) {
        let Some(cur) = self.current.as_mut() else {
            return;
        };
        if cur.finished {
            return;
        }
        cur.finished = true;
        let samples = std::mem::take(&mut cur.samples);
        let feats = stroke_features(&samples, cur.offset_deg);
        let (first_landing, strokes, crossed_center, overshoot_deg) = match feats {
            Some(f) => (
                Some(f.first_landing).filter(|v| v.is_finite()),
                f.strokes,
                f.crossed_center,
                fin(f.overshoot_deg),
            ),
            None => (None, 0, false, 0.0),
        };
        let kind = cur.target.kind;
        self.pending.targets.push(TargetRecord {
            kind,
            offset_deg: cur.offset_deg.to_array(),
            radius_deg: cur.radius_deg,
            first_landing: if kind == AimKind::Moving { None } else { first_landing },
            strokes,
            crossed_center,
            overshoot_deg,
            time_to_hit_ms: time_to_hit_ms.filter(|v| v.is_finite()),
            misses: std::mem::take(&mut cur.misses),
            band: cur.band,
            ctx: cur.ctx,
        });
    }

    pub fn push_tracking(&mut self, windows: &[TrackingWindow]) {
        for w in windows {
            self.pending.tracking.push(TrackingRecord {
                mean_along_deg: fin(w.mean_along_deg),
                lag_ms: fin(w.lag_ms),
                cross_rms_deg: fin(w.cross_rms_deg),
                mean_speed_deg_s: fin(w.mean_speed_deg_s),
                coverage: fin(w.coverage),
                radius_deg: fin(w.radius_deg),
                horizontal: w.horizontal,
                ctx: self.ctx,
            });
        }
    }

    /// Pending records join the log. A target still in progress stays in progress.
    pub fn commit(&mut self) {
        let p = std::mem::take(&mut self.pending);
        self.log.targets.extend(p.targets);
        self.log.tracking.extend(p.tracking);
    }

    /// One frame of live play. A target that appears this frame is recorded with
    /// `target_ctx` (the setting after the task's update, which has already moved to the new
    /// target's side); tracking windows that completed this frame use `tracking_ctx` (the
    /// setting before the update, since a window ending at a chunk boundary belongs to the
    /// chunk that just ended).
    pub fn observe_frame(
        &mut self,
        now_s: f64,
        eye: Vec3,
        aim_dir: Vec3,
        target: Option<AimTarget>,
        target_ctx: PlayContext,
        tracking_ctx: PlayContext,
        windows: &[TrackingWindow],
    ) {
        self.set_context(target_ctx);
        self.track(now_s, eye, aim_dir, target);
        self.set_context(tracking_ctx);
        self.push_tracking(windows);
    }

    /// Drops the target in progress (or finished) without touching pending records or the log.
    /// A new calibration segment restarts target ids, so the old one must not linger.
    pub fn forget_current(&mut self) {
        self.current = None;
    }

    pub fn discard_pending(&mut self) {
        self.pending = AimLog::default();
        self.current = None;
    }

    pub fn take_log(&mut self) -> AimLog {
        self.current = None;
        std::mem::take(&mut self.log)
    }

    pub fn reset(&mut self) {
        self.current = None;
        self.pending = AimLog::default();
        self.log = AimLog::default();
    }
}
