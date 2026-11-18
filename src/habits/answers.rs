//! What the player said about their aim, set against what the run measured.

use serde::{Deserialize, Serialize};

use crate::data::setup::AimProblem;
use crate::habits::analysis::Habit;

/// A habit (or family of habits) that an answer can be checked against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HabitMatch {
    FlickPastAny,
    FlickShortAny,
    Exact(Habit),
}

impl HabitMatch {
    pub fn is(self, habit: Habit) -> bool {
        match self {
            HabitMatch::FlickPastAny => matches!(habit, Habit::FlickPast(_)),
            HabitMatch::FlickShortAny => matches!(habit, Habit::FlickShort(_)),
            HabitMatch::Exact(h) => h == habit,
        }
    }
}

/// Habits that say the player's answer is right.
pub fn matching(p: AimProblem) -> &'static [HabitMatch] {
    match p {
        AimProblem::Overshoot => &[HabitMatch::FlickPastAny, HabitMatch::Exact(Habit::MicroOvershoot)],
        AimProblem::ShakySmall => {
            &[HabitMatch::Exact(Habit::TrackingShakes), HabitMatch::Exact(Habit::MicroOvershoot)]
        }
        AimProblem::CantTurnFast | AimProblem::RunOutOfPad => &[],
    }
}

/// Habits that say the opposite of the player's answer.
pub fn opposite(p: AimProblem) -> &'static [HabitMatch] {
    match p {
        AimProblem::Overshoot => &[HabitMatch::FlickShortAny, HabitMatch::Exact(Habit::MicroCreep)],
        _ => &[],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AnswerVerdict {
    Agrees,
    Opposite,
    NotSeen,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnswerCheck {
    pub problem: AimProblem,
    pub verdict: AnswerVerdict,
}

fn any(list: &[HabitMatch], habit: Habit) -> bool {
    list.iter().any(|m| m.is(habit))
}

/// Does this habit match or oppose the answer?
pub fn relates_to(p: AimProblem, habit: Habit) -> bool {
    any(matching(p), habit) || any(opposite(p), habit)
}

/// The verdict of one answer over every habit found; `None` for a problem no habit can check.
pub fn verdict_for(p: AimProblem, habits: &[Habit]) -> Option<AnswerVerdict> {
    if matching(p).is_empty() {
        return None;
    }
    Some(if habits.iter().any(|&h| any(matching(p), h)) {
        AnswerVerdict::Agrees
    } else if habits.iter().any(|&h| any(opposite(p), h)) {
        AnswerVerdict::Opposite
    } else {
        AnswerVerdict::NotSeen
    })
}

/// Whether a shown habit agrees with or opposes one of the answers.
pub fn badge(habit: Habit, checks: &[AnswerCheck]) -> Option<AnswerVerdict> {
    checks.iter().find_map(|c| {
        if any(matching(c.problem), habit) {
            Some(AnswerVerdict::Agrees)
        } else if any(opposite(c.problem), habit) {
            Some(AnswerVerdict::Opposite)
        } else {
            None
        }
    })
}

/// How the result sits against what the player plays now; the value is the signed % difference
/// of result vs own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DirectionVerdict {
    Agrees(f64),
    Opposite(f64),
    Close,
}

const CLOSE_PCT: f64 = 5.0;

pub fn direction_checks(
    problems: &[AimProblem],
    own_cm: Option<f64>,
    result_cm: f64,
) -> Vec<(AimProblem, DirectionVerdict)> {
    let Some(own) = own_cm.filter(|o| o.is_finite() && *o > 0.0) else { return Vec::new() };
    let delta = (result_cm - own) / own * 100.0;
    problems
        .iter()
        .map(|&p| {
            let v = if delta.abs() <= CLOSE_PCT {
                DirectionVerdict::Close
            } else if (delta > 0.0) == (p.lean() > 0) {
                DirectionVerdict::Agrees(delta)
            } else {
                DirectionVerdict::Opposite(delta)
            };
            (p, v)
        })
        .collect()
}

fn subject(p: AimProblem) -> &'static str {
    match p {
        AimProblem::Overshoot => "you overshoot",
        AimProblem::CantTurnFast => "you can't turn fast enough",
        AimProblem::ShakySmall => "small adjustments are shaky",
        AimProblem::RunOutOfPad => "you run out of mousepad",
    }
}

pub fn direction_sentence(p: AimProblem, v: DirectionVerdict) -> String {
    let said = format!("You said {}", subject(p));
    let how = |d: f64| {
        format!("{}% {}", d.abs().round() as i64, if d > 0.0 { "slower" } else { "faster" })
    };
    match v {
        DirectionVerdict::Agrees(d) => {
            format!("{said} \u{2014} the test agrees: your result is {} than what you play.", how(d))
        }
        DirectionVerdict::Opposite(d) => {
            format!("{said} \u{2014} the test didn't: your result is {} than what you play.", how(d))
        }
        DirectionVerdict::Close => {
            format!("{said} \u{2014} the test didn't: your result is close to what you play now.")
        }
    }
}
