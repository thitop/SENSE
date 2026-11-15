use egui::{vec2, Align, Layout, RichText};

use crate::data::{SessionHistory, SessionRecord};
use crate::ui::screens::{header, section};
use crate::ui::theme::*;
use crate::ui::{graphs, widgets, UiAction, UiModel, UiState};

const MAX_ROWS: usize = 50;
/// Column widths as fractions of the row: scenario, date, score, accuracy, cm/360.
const COLS: [f32; 5] = [0.32, 0.24, 0.14, 0.14, 0.16];

/// Sessions newest first.
pub fn rows(h: &SessionHistory) -> Vec<&SessionRecord> {
    let mut v: Vec<_> = h.sessions.iter().collect();
    v.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    v
}

/// The row to show details for: the chosen one if it exists, else the newest.
pub fn selected_index(len: usize, selected: Option<usize>) -> Option<usize> {
    (len > 0).then(|| selected.filter(|&i| i < len).unwrap_or(0))
}

fn cells(ui: &mut egui::Ui, values: [RichText; 5]) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, v) in values.into_iter().enumerate() {
            let layout = if i == 0 {
                Layout::left_to_right(Align::Center)
            } else {
                Layout::right_to_left(Align::Center)
            };
            ui.allocate_ui_with_layout(vec2(width * COLS[i], 20.0), layout, |ui| {
                ui.set_width(width * COLS[i]);
                ui.add(egui::Label::new(v).truncate());
            });
        }
    });
}

pub fn show(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, _actions: &mut Vec<UiAction>) {
    header(ui, "History", Some("Every finished calibration and practice run."));
    let all = rows(model.history);
    if all.is_empty() {
        widgets::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("No sessions yet").font(medium(15.0)).color(TEXT));
            ui.label(
                RichText::new("Finish a calibration or practice run and it shows up here.")
                    .font(body(13.0))
                    .color(TEXT_SECONDARY),
            );
        });
        return;
    }

    let shown = &all[..all.len().min(MAX_ROWS)];
    let sel = selected_index(shown.len(), state.selected_session);

    ui.horizontal(|ui| {
        ui.add_space(S4);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - S4);
            let muted = |s: &str| RichText::new(s).font(body(12.0)).color(TEXT_MUTED);
            cells(ui, [muted("Scenario"), muted("Date"), muted("Score"), muted("Accuracy"), muted("cm/360")]);
        });
    });
    ui.add_space(S1);
    for (i, s) in shown.iter().enumerate() {
        let resp = widgets::clickable_row(ui, sel == Some(i), |ui| {
            ui.set_min_width(ui.available_width());
            cells(
                ui,
                [
                    RichText::new(&s.task_type).font(medium(13.0)).color(TEXT),
                    RichText::new(s.timestamp.with_timezone(&chrono::Local).format("%b %d %H:%M").to_string())
                        .font(body(13.0))
                        .color(TEXT_SECONDARY),
                    RichText::new(s.metrics.total_score.to_string()).font(display(14.0)).color(TEXT),
                    RichText::new(format!("{:.0}%", s.metrics.accuracy_pct)).font(display(14.0)).color(TEXT),
                    RichText::new(format!("{:.1}", s.tested_cm_360)).font(display(14.0)).color(TEXT),
                ],
            );
        });
        if resp.clicked() {
            state.selected_session = Some(i);
        }
    }
    if all.len() > MAX_ROWS {
        ui.label(
            RichText::new(format!("Showing the latest {MAX_ROWS} of {} sessions", all.len()))
                .font(body(12.0))
                .color(TEXT_MUTED),
        );
    }

    if let Some(s) = sel.map(|i| shown[i]) {
        ui.add_space(S6);
        section(ui, "Reaction time");
        ui.label(
            RichText::new(format!(
                "{} · {}",
                s.task_type,
                s.timestamp.with_timezone(&chrono::Local).format("%b %d %H:%M")
            ))
            .font(body(13.0))
            .color(TEXT_SECONDARY),
        );
        ui.add_space(S2);
        widgets::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let rts: Vec<f32> = s.metrics.points.iter().map(|p| p.reaction_time_ms).collect();
            if rts.is_empty() {
                ui.label(
                    RichText::new("No shots were recorded in this session.")
                        .font(body(13.0))
                        .color(TEXT_MUTED),
                );
            } else {
                graphs::reaction_times(ui, &rts);
            }
        });
    }
}
