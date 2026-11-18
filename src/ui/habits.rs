//! "What to practise": habit cards and the per-session habit rows.

use chrono::{DateTime, Utc};
use egui::{vec2, Align, Layout, RichText};
use egui_phosphor::regular as icon;

use crate::data::CalibrationResult;
use crate::habits::analysis::{Dir, HabitFinding};
use crate::habits::trend::{progress, Change, Progress, SessionHabitRow};
use crate::habits::{HabitReport, LeftOutNote};
use crate::ui::screens::{practice::scenario_name, section};
use crate::ui::theme::*;
use crate::ui::{graphs, widgets, UiAction, UiModel};

const STILL_THERE: &str = "Still there at your best sensitivity → practise, don't change it.";
const NO_HABIT: &str = "No habit stood out in this run.";
const NO_ANSWER: &str =
    "No habits are named for this run: it found no sensitivity to measure them at.";
const WARMING_UP: &str =
    "Your aim was still improving during this run. Warm up longer before calibrating.";
const ELSEWHERE: &str = "Practise at this sensitivity to track progress.";
const COLUMNS: usize = 3;
const CARD_MIN_HEIGHT: f32 = 214.0;

/// The words on one habit card.
#[derive(Clone, Debug, PartialEq)]
pub struct CardLines {
    pub title: String,
    pub value: String,
    pub why: String,
    pub progress: Option<String>,
    pub button: String,
}

fn direction_sentence(d: Dir) -> &'static str {
    match d {
        Dir::Left => "Mostly to the left.",
        Dir::Right => "Mostly to the right.",
        Dir::Up => "Mostly upward.",
        Dir::Down => "Mostly downward.",
        Dir::SideToSide => "Mostly side to side.",
        Dir::UpAndDown => "Mostly up and down.",
    }
}

/// The number part of a value text: "18% short" -> "18%", "62 ms behind" -> "62".
fn number_part(text: &str) -> &str {
    text.split(' ').next().unwrap_or(text)
}

pub fn card_lines(f: &HabitFinding, p: &Progress) -> CardLines {
    let why = match f.direction {
        Some(d) => format!("{STILL_THERE} {}", direction_sentence(d)),
        None => STILL_THERE.to_string(),
    };
    let progress = match p.points.last() {
        Some(&last) => Some(format!(
            "Since calibrating: {} → {}",
            number_part(&f.habit.value_text(f.value)),
            number_part(&f.habit.value_text(last)),
        )),
        None => p.practising_elsewhere.then(|| ELSEWHERE.to_string()),
    };
    CardLines {
        title: f.habit.title(),
        value: f.habit.value_text(f.value),
        why,
        progress,
        button: format!("Practise: {}", scenario_name(f.habit.practice())),
    }
}

pub fn left_out_sentence(n: &LeftOutNote) -> String {
    format!(
        "{} was left out of the result: {} at every speed tested. That's a habit to practise, not a sensitivity problem.",
        n.test,
        n.habit.phrase()
    )
}

/// "What to practise" on My sensitivity: the habits of the latest calibration.
pub fn practise_section(
    ui: &mut egui::Ui,
    model: &UiModel,
    cal: &CalibrationResult,
    report: &HabitReport,
    actions: &mut Vec<UiAction>,
) {
    cards(ui, model, report, cal.at, cal.cm_360, actions);
}

/// The section for a report, with progress counted from `since` at sessions near `result_cm`.
pub fn cards(
    ui: &mut egui::Ui,
    model: &UiModel,
    report: &HabitReport,
    since: DateTime<Utc>,
    result_cm: f64,
    actions: &mut Vec<UiAction>,
) {
    section(ui, "What to practise");
    if report.no_answer {
        muted(ui, NO_ANSWER);
    } else if report.findings.is_empty() {
        muted(ui, NO_HABIT);
    } else {
        ui.columns(COLUMNS, |cols| {
            for (col, f) in cols.iter_mut().zip(&report.findings) {
                let p = progress(model.history, f.habit, since, result_cm);
                col.with_layout(Layout::top_down(Align::Min), |col| card(col, f, &p, actions));
            }
        });
    }
    // Trust notes sit under the cards.
    let mut notes = Vec::new();
    if report.warming_up {
        notes.push(WARMING_UP.to_string());
    }
    notes.extend(report.left_out.iter().map(left_out_sentence));
    if !notes.is_empty() {
        ui.add_space(S3);
        for n in notes {
            ui.label(RichText::new(n).font(body(12.0)).color(TEXT_SECONDARY));
        }
    }
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(body(13.0)).color(TEXT_SECONDARY));
}

fn card(ui: &mut egui::Ui, f: &HabitFinding, p: &Progress, actions: &mut Vec<UiAction>) {
    let l = card_lines(f, p);
    widgets::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(CARD_MIN_HEIGHT);
        ui.spacing_mut().item_spacing.y = S1;
        ui.label(RichText::new(&l.title).font(medium(14.0)).color(TEXT));
        ui.label(RichText::new(&l.value).font(display(22.0)).color(WARN));
        ui.label(RichText::new(widgets::arrows(&l.why)).font(body(12.0)).color(TEXT_SECONDARY));
        if let Some(line) = &l.progress {
            ui.add_space(S1);
            ui.label(RichText::new(widgets::arrows(line)).font(body(12.0)).color(TEXT_MUTED));
            if p.points.len() >= 2 {
                let mut values = vec![f.value];
                values.extend(&p.points);
                graphs::sparkline(ui, &values);
            }
        }
        ui.add_space(S2);
        if widgets::secondary_button(ui, None, &l.button).clicked() {
            actions.push(UiAction::StartPractice(f.habit.practice()));
        }
    });
}

fn change_arrow(c: Option<Change>) -> (&'static str, egui::Color32) {
    match c {
        Some(Change::Better) => (icon::TREND_UP, SUCCESS),
        Some(Change::Worse) => (icon::TREND_DOWN, WARN),
        Some(Change::Same) => (icon::EQUALS, TEXT_MUTED),
        None => ("", TEXT_MUTED),
    }
}

/// Fractions of the row: measure, value, earlier mean, change.
const ROW_COLS: [f32; 4] = [0.36, 0.26, 0.28, 0.10];

/// "Habits this session" on a practice session's detail: each measure with enough data, next to
/// the mean of earlier sessions.
pub fn session_section(ui: &mut egui::Ui, rows: &[SessionHabitRow]) {
    if rows.is_empty() {
        return;
    }
    ui.add_space(S3);
    section(ui, "Habits this session");
    widgets::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = S2;
        for r in rows {
            let width = ui.available_width();
            let (name, value, prev) = match r.value.habit {
                Some(h) => (
                    h.title(),
                    h.value_text(r.value.value),
                    r.previous_mean.map(|m| h.value_text(m)),
                ),
                None => (
                    r.value.measure.label(),
                    r.value.measure.value_text(r.value.value),
                    r.previous_mean.map(|m| r.value.measure.value_text(m)),
                ),
            };
            let value_color = if r.value.habit.is_some() { WARN } else { TEXT };
            let (arrow, arrow_color) = change_arrow(r.change);
            let prev_text = prev.map_or(String::new(), |p| format!("earlier {p}"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let cell = |ui: &mut egui::Ui, i: usize, layout: Layout, text: RichText| {
                    ui.allocate_ui_with_layout(vec2(width * ROW_COLS[i], 22.0), layout, |ui| {
                        ui.set_width(width * ROW_COLS[i]);
                        ui.add(egui::Label::new(text).truncate());
                    });
                };
                let left = Layout::left_to_right(Align::Center);
                cell(ui, 0, left, RichText::new(name).font(medium(13.0)).color(TEXT));
                cell(ui, 1, left, RichText::new(value).font(display(14.0)).color(value_color));
                cell(ui, 2, left, RichText::new(prev_text).font(body(12.0)).color(TEXT_MUTED));
                let right = Layout::right_to_left(Align::Center);
                cell(ui, 3, right, RichText::new(arrow).font(body(16.0)).color(arrow_color));
            });
        }
    });
}
