//! The calibration replay as text and numbers: what each round tested, what the tests said,
//! what SENSE guessed and why the next round went where it did. Pure; drawing lives elsewhere.

use chrono::{DateTime, Utc};

use crate::data::{CalibrationResult, NextPick, RoundGuess, RoundSignals, RoundTrace};
use crate::sensitivity::blocks::SignalCounts;
use crate::sensitivity::pooling::MAX_ANSWER_UNCERTAINTY;
use crate::sensitivity::psa::PsaRoundConclusion;
use crate::ui::screens::sensitivity::{estimate_text, result_note};

pub const INTRO: &str = "Each round tested one sensitivity, in pairs: one target a little faster (+12%), one a little slower. Whichever side you did better on says which way to go. Where that flips is your number.";
pub const LINK: &str = "How we got this number →";

const LAST_ROUND: &str = "That was the last round → result";
const TOO_EARLY: &str = "Too early to trust a guess";
/// `TestKind::Strafe.label()`: it plays no pairs, so it has no row.
const STRAFE: &str = "Counter-strafing";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Normal,
    Muted,
    Warn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestRow {
    pub test: String,
    pub counts: SignalCounts,
    pub verdict: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoundView {
    pub header: String,
    pub tested_cm_360: f64,
    pub conclusion: PsaRoundConclusion,
    /// The range the round started from; `None` for runs saved without a trace.
    pub range: Option<(f64, f64)>,
    /// (cm, relative uncertainty) of the guess after the round, when it is worth plotting.
    pub guess_point: Option<(f64, f64)>,
    pub rows: Vec<TestRow>,
    pub guess_line: Option<String>,
    pub next_line: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultView {
    pub cm_360: Option<f64>,
    pub rel_uncertainty: Option<f64>,
    pub lines: Vec<(String, Tone)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub rounds: Vec<RoundView>,
    pub result: ResultView,
}

pub fn verdict(c: SignalCounts) -> &'static str {
    if c.too_fast > c.too_slow {
        "Better slower · too fast"
    } else if c.too_slow > c.too_fast {
        "Better faster · too slow"
    } else {
        "Even"
    }
}

/// `None` when the run kept no per-round record (flick-only or saved before diagnostics).
pub fn replay(cal: &CalibrationResult) -> Option<Replay> {
    let d = cal.diagnostics.as_ref()?;
    if d.rounds.is_empty() {
        return None;
    }
    let n = d.rounds.len();
    let rounds = d
        .rounds
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let rows = rows(r);
            let conclusion = cal.rounds.get(i).map(|c| c.conclusion).unwrap_or_else(|| {
                let (fast, slow) = rows
                    .iter()
                    .fold((0, 0), |(f, s), row| (f + row.counts.too_fast, s + row.counts.too_slow));
                conclude(fast, slow)
            });
            let guess_line = r.trace.map(|t| guess_line(t.guess));
            let next_line = match r.trace {
                Some(t) => next_line(&t, conclusion, guess_line.as_deref() == Some(TOO_EARLY)),
                None if i + 1 == n => LAST_ROUND.into(),
                None => match conclusion {
                    PsaRoundConclusion::TooFastNeedSlower => "Overall too fast → next round went slower",
                    PsaRoundConclusion::TooSlowNeedFaster => "Overall too slow → next round went faster",
                    PsaRoundConclusion::BalancedNarrow => "About even → next round narrowed in",
                }
                .into(),
            };
            RoundView {
                header: format!("Round {} of {}", i + 1, n),
                tested_cm_360: r.tested_cm_360,
                conclusion,
                range: r.trace.map(|t| (t.low_cm_360, t.high_cm_360)),
                guess_point: match r.trace.map(|t| t.guess) {
                    Some(RoundGuess::Guess { cm_360, rel_uncertainty }) if trusted(rel_uncertainty) => {
                        Some((cm_360, rel_uncertainty))
                    }
                    _ => None,
                },
                rows,
                guess_line,
                next_line,
            }
        })
        .collect();
    Some(Replay { rounds, result: result_view(cal) })
}

fn conclude(fast: usize, slow: usize) -> PsaRoundConclusion {
    if fast > slow {
        PsaRoundConclusion::TooFastNeedSlower
    } else if slow > fast {
        PsaRoundConclusion::TooSlowNeedFaster
    } else {
        PsaRoundConclusion::BalancedNarrow
    }
}

fn is_row(key: &str) -> bool {
    !key.starts_with("Flick ") && key != STRAFE
}

fn trusted(rel: f64) -> bool {
    rel.is_finite() && rel <= MAX_ANSWER_UNCERTAINTY
}

fn rows(r: &RoundSignals) -> Vec<TestRow> {
    const FIRST: [&str; 3] = ["Flicking", "Micro-adjustment", "Tracking"];
    let row = |test: &str, counts: SignalCounts| TestRow {
        test: test.into(),
        counts,
        verdict: verdict(counts),
    };
    let mut out: Vec<TestRow> = FIRST
        .iter()
        .filter_map(|k| r.signals.get(*k).map(|c| row(k, *c)))
        .collect();
    out.extend(
        r.signals
            .iter()
            .filter(|(k, _)| is_row(k) && !FIRST.contains(&k.as_str()))
            .map(|(k, c)| row(k, *c)),
    );
    out
}

fn guess_line(g: RoundGuess) -> String {
    match g {
        RoundGuess::Guess { cm_360, rel_uncertainty } if trusted(rel_uncertainty) => {
            format!("So far SENSE thinks {:.1} cm ± {:.0}%", cm_360, rel_uncertainty * 100.0)
        }
        RoundGuess::Guess { .. } => TOO_EARLY.into(),
        RoundGuess::OneSensitivity => "One sensitivity tested so far: SENSE needs two to start guessing".into(),
        RoundGuess::NoCrossover => "Not enough to guess yet: every test leaned the same way".into(),
    }
}

fn next_line(t: &RoundTrace, conclusion: PsaRoundConclusion, too_early: bool) -> String {
    match t.next {
        NextPick::Midpoint { cm_360 } => match conclusion {
            PsaRoundConclusion::TooFastNeedSlower => format!(
                "Overall too fast → the next round tries the middle of the slower half ({cm_360:.1} cm)"
            ),
            PsaRoundConclusion::TooSlowNeedFaster => format!(
                "Overall too slow → the next round tries the middle of the faster half ({cm_360:.1} cm)"
            ),
            PsaRoundConclusion::BalancedNarrow => "About even → the range narrows around here".into(),
        },
        NextPick::FitEstimate { cm_360 } => {
            let what = if too_early { "a rough first guess" } else { "the best guess so far" };
            format!("Next round tests {cm_360:.1} cm: {what}.")
        }
        NextPick::FitClamped { guess_cm, cm_360 } => format!(
            "The best guess ({guess_cm:.1} cm) is outside what's been tested, so the next round goes as far as it can: {cm_360:.1} cm."
        ),
        NextPick::FitFallbackMidpoint { cm_360 } => {
            format!("Next round tests the middle of the range ({cm_360:.1} cm).")
        }
        NextPick::Last => LAST_ROUND.into(),
    }
}

fn result_view(cal: &CalibrationResult) -> ResultView {
    let d = cal.diagnostics.as_ref();
    let mut lines: Vec<(String, Tone)> = Vec::new();
    lines.push(if cal.no_answer {
        ("No result: every test leaned the same way at every sensitivity tested".into(), Tone::Muted)
    } else {
        (
            "Every test flips from \"too fast\" to \"too slow\" here. Each test on its own:".into(),
            Tone::Muted,
        )
    });
    if let Some(d) = d {
        for e in d.estimates.iter().filter(|e| !e.test.starts_with("Flick ")) {
            if d.left_out.contains(&e.test) {
                lines.push((format!("{}: Left out: leaned one way at every sensitivity", e.test), Tone::Warn));
            } else if !cal.no_answer {
                lines.push((format!("{}: {}", e.test, estimate_text(e, false)), Tone::Normal));
            }
        }
    }
    if !cal.no_answer {
        if cal.pooled_runs > 1 {
            let earlier = cal.pooled_runs - 1;
            let noun = if earlier == 1 { "run" } else { "runs" };
            lines.push((format!("Combined with {earlier} earlier {noun} → {:.1} cm", cal.cm_360), Tone::Normal));
        } else if let Some(d) = d.filter(|d| !d.left_out.is_empty()) {
            let last_guess = d.rounds.last().and_then(|r| r.trace).and_then(|t| match t.guess {
                RoundGuess::Guess { cm_360, .. } => Some(cm_360),
                _ => None,
            });
            if let Some(g) = last_guess.filter(|g| (g - cal.cm_360).abs() >= 0.05) {
                lines.push((
                    format!("Leaving out {} moved it from {:.1} to {:.1} cm.", d.left_out.join(" and "), g, cal.cm_360),
                    Tone::Normal,
                ));
            }
        }
    }
    if cal.runs_disagree {
        if let Some(note) = result_note(false, true) {
            lines.push((note.into(), Tone::Warn));
        }
    }
    ResultView {
        cm_360: (!cal.no_answer).then_some(cal.cm_360),
        rel_uncertainty: d.and_then(|d| d.result_rel_uncertainty),
        lines,
    }
}

/// Seconds each round (and the result) stays on screen while the replay plays.
pub const STEP_SECS: f64 = 1.6;

/// Playback position of the replay. Pure: the drawing code feeds it the clock.
/// `step` runs `0..rounds-1` for the rounds and `rounds` for the result; `steps` below is `rounds + 1`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplayState {
    pub step: usize,
    pub playing: bool,
    pub last_step_at: f64,
    /// Set when a run finishes; the first time the replay is on screen it starts playing.
    pub autoplay_armed: bool,
    /// The "How we got this number" link asks the page to scroll to the replay.
    pub scroll_pending: bool,
    /// (result.at, pooled_runs) of the result being replayed.
    key: Option<(DateTime<Utc>, usize)>,
}

impl ReplayState {
    /// Call every frame with the shown result's identity; a different result starts over.
    pub fn sync(&mut self, at: DateTime<Utc>, pooled_runs: usize) {
        let key = Some((at, pooled_runs));
        if self.key != key {
            if self.key.is_some() {
                self.step = 0;
                self.playing = false;
            }
            self.key = key;
        }
    }

    pub fn arm_autoplay(&mut self) {
        self.autoplay_armed = true;
    }

    pub fn play(&mut self, now: f64, steps: usize) {
        if self.step + 1 >= steps {
            self.step = 0;
        }
        self.playing = true;
        self.last_step_at = now;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn go_to(&mut self, step: usize, steps: usize) {
        self.step = step.min(steps.saturating_sub(1));
        self.playing = false;
    }

    pub fn prev(&mut self, steps: usize) {
        self.go_to(self.step.saturating_sub(1), steps);
    }

    pub fn next(&mut self, steps: usize) {
        self.go_to(self.step + 1, steps);
    }

    pub fn tick(&mut self, now: f64, steps: usize) {
        if !self.playing {
            return;
        }
        // Slack for clocks that add up to a hair under STEP_SECS.
        if now - self.last_step_at >= STEP_SECS - 1e-9 {
            self.step = (self.step + 1).min(steps.saturating_sub(1));
            self.last_step_at = now;
            if self.step + 1 >= steps {
                self.playing = false;
            }
        }
    }

    pub fn on_visible(&mut self, now: f64, steps: usize) {
        if self.autoplay_armed {
            self.autoplay_armed = false;
            self.step = 0;
            self.play(now, steps);
        }
    }

    /// Call on every frame the page isn't drawn: an autoplay the player never scrolled to is
    /// dropped, so opening the page later starts paused.
    pub fn disarm(&mut self) {
        self.autoplay_armed = false;
    }

    pub fn link_clicked(&mut self, now: f64, steps: usize) {
        self.autoplay_armed = false;
        self.scroll_pending = true;
        self.step = 0;
        self.play(now, steps);
    }
}
