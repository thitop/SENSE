//! Progress on a habit since calibrating, and how a session compares with earlier ones.
//! Pure analysis over the saved history.

use chrono::{DateTime, Utc};

use crate::data::history::{SessionHistory, SessionRecord};
use crate::habits::analysis::{session_values, Habit, MeasureValue};

const CALIBRATION: &str = "Calibration";
const MAX_POINTS: usize = 10;
const PREVIOUS_SESSIONS: usize = 5;
const SAME_WITHIN: f32 = 0.10;

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    /// The habit's measure in each session near the calibrated result, oldest to newest.
    pub points: Vec<f32>,
    /// Sessions followed calibration, but all were played away from the result.
    pub practising_elsewhere: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Better,
    Worse,
    Same,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionHabitRow {
    pub value: MeasureValue,
    pub previous_mean: Option<f32>,
    pub change: Option<Change>,
}

/// The habit's measure over practice sessions after `since`, kept to those played within 12% of
/// `result_cm`.
pub fn progress(
    history: &SessionHistory,
    habit: Habit,
    since: DateTime<Utc>,
    result_cm: f64,
) -> Progress {
    let measure = habit.measure();
    let band = 1.12f64.ln();
    let mut after: Vec<&SessionRecord> = history
        .sessions
        .iter()
        .filter(|s| s.task_type != CALIBRATION && s.timestamp > since && s.aim.is_some())
        .collect();
    after.sort_by_key(|s| s.timestamp);

    let mut points = Vec::new();
    let mut elsewhere = false;
    for s in after {
        let Some(value) = s
            .aim
            .as_ref()
            .and_then(|log| measure.value(log, |_| true))
            .map(|(v, _)| v)
            .filter(|v| v.is_finite())
        else {
            continue;
        };
        let near = s.tested_cm_360 > 0.0
            && result_cm > 0.0
            && (s.tested_cm_360 / result_cm).ln().abs() <= band;
        if near {
            points.push(value);
        } else {
            elsewhere = true;
        }
    }
    if points.len() > MAX_POINTS {
        points.drain(..points.len() - MAX_POINTS);
    }
    Progress { practising_elsewhere: points.is_empty() && elsewhere, points }
}

/// Each measure the session has enough data for, against the mean of the same measure in the
/// five most recent earlier sessions of the same scenario.
pub fn session_rows(history: &SessionHistory, session: &SessionRecord) -> Vec<SessionHabitRow> {
    let Some(log) = session.aim.as_ref() else {
        return Vec::new();
    };
    let mut earlier: Vec<&SessionRecord> = history
        .sessions
        .iter()
        .filter(|s| {
            s.task_type == session.task_type && s.timestamp < session.timestamp && s.aim.is_some()
        })
        .collect();
    earlier.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    let earlier_values: Vec<Vec<MeasureValue>> =
        earlier.iter().filter_map(|s| s.aim.as_ref()).map(session_values).collect();

    session_values(log)
        .into_iter()
        .map(|value| {
            let prev: Vec<f32> = earlier_values
                .iter()
                .filter_map(|vals| vals.iter().find(|v| v.measure == value.measure))
                .map(|v| v.value)
                .take(PREVIOUS_SESSIONS)
                .collect();
            let previous_mean = (!prev.is_empty()).then(|| prev.iter().sum::<f32>() / prev.len() as f32);
            let previous_mean = previous_mean.filter(|m| m.is_finite());
            let change = previous_mean.map(|mean| {
                let now = value.measure.badness(value.value);
                let before = value.measure.badness(mean);
                if (now - before).abs() <= SAME_WITHIN * before {
                    Change::Same
                } else if now < before {
                    Change::Better
                } else {
                    Change::Worse
                }
            });
            SessionHabitRow { value, previous_mean, change }
        })
        .collect()
}
