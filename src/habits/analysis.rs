//! Habit measures: one number per habit computed from aim records, with a threshold that
//! names the habit. Pure analysis; nothing here touches the game or the screen.

use serde::{Deserialize, Serialize};

use crate::game::tasks::flick::FlickBand;
use crate::game::tasks::TaskType;
use crate::habits::record::{AimKind, AimLog, PlayContext, TargetRecord, TrackingRecord};
use crate::sensitivity::combiner::{Signal, TestKind};

/// Practice minimum for a measure (records in the session).
const MIN_RECORDS: usize = 12;
/// Practice minimum for a per-band flick measure.
const MIN_BAND_RECORDS: usize = 8;
/// Practice minimum for the miss-lean measures (misses).
const MIN_MISSES: usize = 10;
/// Records a direction needs before it can be named.
const MIN_DIRECTION_RECORDS: usize = 4;

const SHORT_BELOW: f32 = 0.85;
const PAST_ABOVE: f32 = 1.10;
const CORRECTIONS_ABOVE: f32 = 2.5;
const SHARE_ABOVE: f32 = 0.35;
const TRAILS_ABOVE: f32 = 40.0;
const AHEAD_BELOW: f32 = -40.0;
const SHAKE_ABOVE: f32 = 0.5;
const LEAN_ABOVE: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Measure {
    FlickLanding(FlickBand),
    FlickStrokes,
    MicroOvershoot,
    MicroCreep,
    TrackingLag,
    TrackingShake,
    MissLeanFlick,
    MissLeanMicro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Habit {
    FlickShort(FlickBand),
    FlickPast(FlickBand),
    FlickCorrections,
    MicroOvershoot,
    MicroCreep,
    TrackingTrails,
    TrackingAhead,
    TrackingShakes,
    MissLeanFlick,
    MissLeanMicro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
    SideToSide,
    UpAndDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasureValue {
    pub measure: Measure,
    pub value: f32,
    pub n: usize,
    pub habit: Option<Habit>,
}

fn median(mut v: Vec<f32>) -> Option<(f32, usize)> {
    v.retain(|x| x.is_finite());
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    let m = n / 2;
    Some((if n % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }, n))
}

fn share(hits: usize, total: usize) -> Option<(f32, usize)> {
    (total > 0).then(|| (hits as f32 / total as f32, total))
}

impl Measure {
    pub const ALL: [Measure; 10] = [
        Measure::FlickLanding(FlickBand::Short),
        Measure::FlickLanding(FlickBand::Mid),
        Measure::FlickLanding(FlickBand::Far),
        Measure::FlickStrokes,
        Measure::MicroOvershoot,
        Measure::MicroCreep,
        Measure::TrackingLag,
        Measure::TrackingShake,
        Measure::MissLeanFlick,
        Measure::MissLeanMicro,
    ];

    fn minimum(self) -> usize {
        match self {
            Measure::FlickLanding(_) => MIN_BAND_RECORDS,
            Measure::MissLeanFlick | Measure::MissLeanMicro => MIN_MISSES,
            _ => MIN_RECORDS,
        }
    }

    /// The statistic over the records `keep` accepts, with the count that went into it (misses
    /// for the lean measures). No practice minimum: at least one record (one miss for lean).
    pub fn statistic(
        self,
        log: &AimLog,
        keep: impl Fn(&PlayContext) -> bool,
    ) -> Option<(f32, usize)> {
        let targets = |kind: AimKind| -> Vec<&TargetRecord> {
            log.targets.iter().filter(|r| r.kind == kind && keep(&r.ctx)).collect()
        };
        let tracking: Vec<&TrackingRecord> = log.tracking.iter().filter(|r| keep(&r.ctx)).collect();
        let r = match self {
            Measure::FlickLanding(b) => median(
                targets(AimKind::Flick)
                    .iter()
                    .filter(|r| r.band == Some(b))
                    .filter_map(|r| r.first_landing)
                    .collect(),
            ),
            Measure::FlickStrokes => {
                let t = targets(AimKind::Flick);
                (!t.is_empty()).then(|| {
                    (t.iter().map(|r| r.strokes as f32).sum::<f32>() / t.len() as f32, t.len())
                })
            }
            Measure::MicroOvershoot => {
                let t = targets(AimKind::Micro);
                share(t.iter().filter(|r| r.crossed_center).count(), t.len())
            }
            Measure::MicroCreep => {
                let t = targets(AimKind::Micro);
                share(t.iter().filter(|r| r.strokes >= 3 && !r.crossed_center).count(), t.len())
            }
            Measure::TrackingLag => median(tracking.iter().map(|r| r.lag_ms).collect()),
            Measure::TrackingShake => median(
                tracking
                    .iter()
                    .filter(|r| r.radius_deg > 0.0)
                    .map(|r| r.cross_rms_deg / r.radius_deg)
                    .collect(),
            ),
            Measure::MissLeanFlick => miss_lean(&targets(AimKind::Flick)),
            Measure::MissLeanMicro => miss_lean(&targets(AimKind::Micro)),
        };
        r.filter(|(v, _)| v.is_finite())
    }

    /// `statistic` when enough records went into it for practice.
    pub fn value(self, log: &AimLog, keep: impl Fn(&PlayContext) -> bool) -> Option<(f32, usize)> {
        self.statistic(log, keep).filter(|&(_, n)| n >= self.minimum())
    }

    /// How far the value is from clean: |1 - v| for landing, |v| for lag, v otherwise.
    pub fn badness(self, v: f32) -> f32 {
        match self {
            Measure::FlickLanding(_) => (1.0 - v).abs(),
            Measure::TrackingLag => v.abs(),
            _ => v,
        }
    }

    pub fn habits(self) -> &'static [Habit] {
        use FlickBand::*;
        match self {
            Measure::FlickLanding(Short) => &[Habit::FlickShort(Short), Habit::FlickPast(Short)],
            Measure::FlickLanding(Mid) => &[Habit::FlickShort(Mid), Habit::FlickPast(Mid)],
            Measure::FlickLanding(Far) => &[Habit::FlickShort(Far), Habit::FlickPast(Far)],
            Measure::FlickStrokes => &[Habit::FlickCorrections],
            Measure::MicroOvershoot => &[Habit::MicroOvershoot],
            Measure::MicroCreep => &[Habit::MicroCreep],
            Measure::TrackingLag => &[Habit::TrackingTrails, Habit::TrackingAhead],
            Measure::TrackingShake => &[Habit::TrackingShakes],
            Measure::MissLeanFlick => &[Habit::MissLeanFlick],
            Measure::MissLeanMicro => &[Habit::MissLeanMicro],
        }
    }
}

/// max(|mean along|, |mean across|) over all misses, in units of the mean radius of the records
/// the misses belong to. The count is the number of misses.
fn miss_lean(records: &[&TargetRecord]) -> Option<(f32, usize)> {
    let mut n = 0usize;
    let (mut along, mut across, mut radius) = (0.0f32, 0.0f32, 0.0f32);
    let mut owners = 0usize;
    for r in records {
        let m: Vec<&[f32; 2]> =
            r.misses.iter().filter(|m| m[0].is_finite() && m[1].is_finite()).collect();
        if m.is_empty() || !r.radius_deg.is_finite() {
            continue;
        }
        owners += 1;
        radius += r.radius_deg;
        for p in m {
            n += 1;
            along += p[0];
            across += p[1];
        }
    }
    if n == 0 || owners == 0 {
        return None;
    }
    let mean_radius = radius / owners as f32;
    if mean_radius <= 0.0 {
        return None;
    }
    let lean = (along / n as f32).abs().max((across / n as f32).abs());
    Some((lean / mean_radius, n))
}

fn band_title(b: FlickBand) -> &'static str {
    match b {
        FlickBand::Short => "Short",
        FlickBand::Mid => "Mid",
        FlickBand::Far => "Far",
    }
}

impl Measure {
    /// What the measure is called when no habit is named.
    pub fn label(self) -> String {
        match self {
            Measure::FlickLanding(b) => format!("{} flick landing", band_title(b)),
            Measure::FlickStrokes => "Corrections per flick".into(),
            Measure::MicroOvershoot => "Small-aim overshoot".into(),
            Measure::MicroCreep => "Small-aim creep".into(),
            Measure::TrackingLag => "Tracking lag".into(),
            Measure::TrackingShake => "Tracking shake".into(),
            Measure::MissLeanFlick => "Flick miss lean".into(),
            Measure::MissLeanMicro => "Small-aim miss lean".into(),
        }
    }

    /// A value of this measure in words, whether or not it is beyond a habit's threshold.
    pub fn value_text(self, v: f32) -> String {
        let r = |x: f32| x.round() as i64;
        match self {
            Measure::FlickLanding(_) => format!("{}% of the way", r(v * 100.0)),
            Measure::FlickStrokes => format!("{v:.1} moves per target"),
            Measure::MicroOvershoot => format!("{}% overshoot", r(v * 100.0)),
            Measure::MicroCreep => format!("{}% creep", r(v * 100.0)),
            Measure::TrackingLag if v < 0.0 => format!("{} ms ahead", r(-v)),
            Measure::TrackingLag => format!("{} ms behind", r(v)),
            Measure::TrackingShake => format!("{v:.1}\u{d7} target size"),
            Measure::MissLeanFlick | Measure::MissLeanMicro => {
                format!("{v:.1}\u{d7} target size off")
            }
        }
    }
}

impl Habit {
    pub fn measure(self) -> Measure {
        match self {
            Habit::FlickShort(b) | Habit::FlickPast(b) => Measure::FlickLanding(b),
            Habit::FlickCorrections => Measure::FlickStrokes,
            Habit::MicroOvershoot => Measure::MicroOvershoot,
            Habit::MicroCreep => Measure::MicroCreep,
            Habit::TrackingTrails | Habit::TrackingAhead => Measure::TrackingLag,
            Habit::TrackingShakes => Measure::TrackingShake,
            Habit::MissLeanFlick => Measure::MissLeanFlick,
            Habit::MissLeanMicro => Measure::MissLeanMicro,
        }
    }

    /// Share of the threshold the value is beyond, or `None` when it is not beyond it.
    pub fn severity(self, v: f32) -> Option<f32> {
        if !v.is_finite() {
            return None;
        }
        let s = match self {
            Habit::FlickShort(_) => (SHORT_BELOW - v) / SHORT_BELOW,
            Habit::FlickPast(_) => (v - PAST_ABOVE) / PAST_ABOVE,
            Habit::FlickCorrections => (v - CORRECTIONS_ABOVE) / CORRECTIONS_ABOVE,
            Habit::MicroOvershoot | Habit::MicroCreep => (v - SHARE_ABOVE) / SHARE_ABOVE,
            Habit::TrackingTrails => (v - TRAILS_ABOVE) / TRAILS_ABOVE,
            Habit::TrackingAhead => (AHEAD_BELOW - v) / AHEAD_BELOW.abs(),
            Habit::TrackingShakes => (v - SHAKE_ABOVE) / SHAKE_ABOVE,
            Habit::MissLeanFlick | Habit::MissLeanMicro => (v - LEAN_ABOVE) / LEAN_ABOVE,
        };
        (s > 0.0).then_some(s)
    }

    pub fn title(self) -> String {
        match self {
            Habit::FlickShort(b) => format!("{} flicks stop short", band_title(b)),
            Habit::FlickPast(b) => format!("{} flicks go past", band_title(b)),
            Habit::FlickCorrections => "Flicks need many corrections".into(),
            Habit::MicroOvershoot => "Small aims overshoot".into(),
            Habit::MicroCreep => "Small aims creep up on the target".into(),
            Habit::TrackingTrails => "Tracking trails the target".into(),
            Habit::TrackingAhead => "Tracking runs ahead of the target".into(),
            Habit::TrackingShakes => "Tracking shakes across the target".into(),
            Habit::MissLeanFlick => "Flick misses lean one way".into(),
            Habit::MissLeanMicro => "Small-aim misses lean one way".into(),
        }
    }

    /// The value in the habit's own words ("18% short"). A value on the other side of clean
    /// for this habit never prints as a negative number: it reads in the measure's neutral
    /// words instead ("105% of the way", "10 ms ahead").
    pub fn value_text(self, v: f32) -> String {
        let r = |x: f32| x.round() as i64;
        let (n, text): (i64, String) = match self {
            Habit::FlickShort(_) => (r((1.0 - v) * 100.0), format!("{}% short", r((1.0 - v) * 100.0))),
            Habit::FlickPast(_) => (r((v - 1.0) * 100.0), format!("{}% past", r((v - 1.0) * 100.0))),
            Habit::TrackingTrails => (r(v), format!("{} ms behind", r(v))),
            Habit::TrackingAhead => (r(-v), format!("{} ms ahead", r(-v))),
            _ => return self.measure().value_text(v),
        };
        if n < 0 {
            self.measure().value_text(v)
        } else {
            text
        }
    }

    pub fn practice(self) -> TaskType {
        match self {
            Habit::FlickShort(_)
            | Habit::FlickPast(_)
            | Habit::FlickCorrections
            | Habit::MissLeanFlick => TaskType::Flick,
            Habit::MicroOvershoot | Habit::MicroCreep | Habit::MissLeanMicro => {
                TaskType::MicroAdjust
            }
            Habit::TrackingTrails | Habit::TrackingAhead | Habit::TrackingShakes => {
                TaskType::Tracking
            }
        }
    }

    /// Which way the sensitivity should move to help this habit, if it says.
    pub fn lean(self) -> Option<Signal> {
        match self {
            Habit::FlickShort(_) | Habit::MicroCreep | Habit::TrackingTrails => {
                Some(Signal::TooSlow)
            }
            Habit::FlickPast(_) | Habit::MicroOvershoot | Habit::TrackingAhead => {
                Some(Signal::TooFast)
            }
            _ => None,
        }
    }

    pub fn test_label(self) -> &'static str {
        match self.practice() {
            TaskType::Flick => TestKind::Flick.label(),
            TaskType::MicroAdjust => TestKind::Micro.label(),
            _ => TestKind::Tracking.label(),
        }
    }

    pub fn phrase(self) -> &'static str {
        match self {
            Habit::FlickShort(_) => "your flicks stopped short",
            Habit::FlickPast(_) => "your flicks went past",
            Habit::MicroOvershoot => "your small aims overshot",
            Habit::MicroCreep => "your small aims crept up on the target",
            Habit::TrackingTrails => "you trailed the target",
            Habit::TrackingAhead => "you ran ahead of the target",
            _ => "",
        }
    }
}

/// Practice: every measure with enough data in the session, with the habit it names, if any.
pub fn session_values(log: &AimLog) -> Vec<MeasureValue> {
    Measure::ALL
        .iter()
        .filter_map(|&m| {
            let (value, n) = m.value(log, |_| true)?;
            let habit = m.habits().iter().copied().find(|h| h.severity(value).is_some());
            Some(MeasureValue { measure: m, value, n, habit })
        })
        .collect()
}

fn dominant_dir(offset: [f32; 2]) -> Dir {
    if offset[0].abs() >= offset[1].abs() {
        if offset[0] >= 0.0 {
            Dir::Right
        } else {
            Dir::Left
        }
    } else if offset[1] >= 0.0 {
        Dir::Down
    } else {
        Dir::Up
    }
}

/// The one direction where the habit shows while the others are clean, if there is one.
/// A direction needs at least 4 records to count either way.
pub fn worst_direction(
    habit: Habit,
    log: &AimLog,
    keep: impl Fn(&PlayContext) -> bool,
) -> Option<Dir> {
    let measure = habit.measure();
    let mut beyond = Vec::new();
    let mut clean = 0usize;
    let mut judge = |dir: Dir, sub: AimLog| {
        if let Some((v, n)) = measure.statistic(&sub, |_| true) {
            if n >= MIN_DIRECTION_RECORDS {
                if habit.severity(v).is_some() {
                    beyond.push(dir);
                } else {
                    clean += 1;
                }
            }
        }
    };
    match measure {
        Measure::MissLeanFlick | Measure::MissLeanMicro => return None,
        Measure::TrackingLag | Measure::TrackingShake => {
            for (dir, horizontal) in [(Dir::SideToSide, true), (Dir::UpAndDown, false)] {
                let tracking = log
                    .tracking
                    .iter()
                    .filter(|r| r.horizontal == horizontal && keep(&r.ctx))
                    .cloned()
                    .collect();
                judge(dir, AimLog { targets: Vec::new(), tracking });
            }
        }
        _ => {
            for dir in [Dir::Left, Dir::Right, Dir::Up, Dir::Down] {
                let targets = log
                    .targets
                    .iter()
                    .filter(|r| dominant_dir(r.offset_deg) == dir && keep(&r.ctx))
                    .cloned()
                    .collect();
                judge(dir, AimLog { targets, tracking: Vec::new() });
            }
        }
    }
    (beyond.len() == 1 && clean >= 1).then(|| beyond[0])
}

/// A habit named from a calibration run, with the value it has at the run's result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HabitFinding {
    pub habit: Habit,
    pub value: f32,
    pub direction: Option<Dir>,
}

/// Fewest counted records, and distinct blocks, a calibration needs before habits are named.
const CAL_MIN_RECORDS: usize = 16;
const CAL_MIN_BLOCKS: usize = 3;
const CAL_MAX_FINDINGS: usize = 3;

fn counted(c: &PlayContext) -> bool {
    c.block.is_some() && !c.warmup
}

fn cm_key(cm: f32) -> i64 {
    (cm as f64 * 100.0).round() as i64
}

/// The habits a calibration run shows at its result: each measure is fitted against ln(cm)
/// over the sensitivities played, and read at `result_cm`, so a habit that only appears away
/// from the result (a lean that belongs to the sensitivity) is not named.
pub fn calibration_habits(log: &AimLog, result_cm: f64) -> Vec<HabitFinding> {
    use std::collections::BTreeSet;
    let mut blocks = BTreeSet::new();
    let mut cms = BTreeSet::new();
    for c in log.targets.iter().map(|r| &r.ctx).chain(log.tracking.iter().map(|r| &r.ctx)) {
        if counted(c) {
            if let Some(b) = c.block {
                blocks.insert(b);
            }
            if c.cm_360.is_finite() && c.cm_360 > 0.0 {
                cms.insert(cm_key(c.cm_360));
            }
        }
    }
    if blocks.len() < CAL_MIN_BLOCKS || !(result_cm > 0.0) {
        return Vec::new();
    }
    let at = result_cm.ln();
    let mut found: Vec<(f32, HabitFinding)> = Vec::new();
    for measure in Measure::ALL {
        let mut pts: Vec<(f64, f64, f64)> = Vec::new(); // (ln cm, statistic, weight)
        for &key in &cms {
            let stat = measure.statistic(log, |c| counted(c) && cm_key(c.cm_360) == key);
            if let Some((v, n)) = stat {
                if n >= 2 {
                    pts.push(((key as f64 / 100.0).ln(), v as f64, n as f64));
                }
            }
        }
        let enough = match measure {
            Measure::MissLeanFlick | Measure::MissLeanMicro => 10,
            _ => CAL_MIN_RECORDS,
        };
        let total: usize = pts.iter().map(|p| p.2 as usize).sum();
        let measure_blocks = blocks
            .iter()
            .filter(|&&b| measure.statistic(log, |c| counted(c) && c.block == Some(b)).is_some())
            .count();
        if pts.is_empty() || total < enough || measure_blocks < CAL_MIN_BLOCKS {
            continue;
        }
        let w: f64 = pts.iter().map(|p| p.2).sum();
        let mx = pts.iter().map(|p| p.0 * p.2).sum::<f64>() / w;
        let my = pts.iter().map(|p| p.1 * p.2).sum::<f64>() / w;
        let sxx: f64 = pts.iter().map(|p| p.2 * (p.0 - mx) * (p.0 - mx)).sum();
        let sxy: f64 = pts.iter().map(|p| p.2 * (p.0 - mx) * (p.1 - my)).sum();
        let value = if sxx > 1e-9 { my + sxy / sxx * (at - mx) } else { my } as f32;
        if !value.is_finite() {
            continue;
        }
        for &habit in measure.habits() {
            if let Some(sev) = habit.severity(value) {
                let direction = worst_direction(habit, log, counted);
                found.push((sev, HabitFinding { habit, value, direction }));
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().take(CAL_MAX_FINDINGS).map(|(_, f)| f).collect()
}
