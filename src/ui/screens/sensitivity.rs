//! "My sensitivity": the calibration result, or an empty state with a manual converter.

use egui::{Align, Layout, RichText, TextEdit};
use egui_phosphor::regular as icon;

use crate::data::CalibrationResult;
use crate::game::calibration::{delta_text, parse_game_sens, primary_game, ConfidenceLevel};
use crate::sensitivity::converter::SensitivityConverter;
use crate::ui::screens::section;
use crate::ui::theme::*;
use crate::ui::{graphs, widgets, UiAction, UiModel, UiState};

const COPIED_SECS: f64 = 1.5;
const ROWS_COLLAPSED: usize = 2;

pub fn show(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    match &model.profile.last_calibration {
        Some(cal) => result(ui, state, model, cal, actions),
        None => empty(ui, state, model, actions),
    }
}

fn empty(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    widgets::eyebrow(ui, "My sensitivity");
    ui.add_space(S1);
    ui.label(
        RichText::new("You haven't calibrated yet")
            .font(display_bold(28.0))
            .color(TEXT),
    );
    ui.label(
        RichText::new("One run takes about five minutes.")
            .font(body(14.0))
            .color(TEXT_SECONDARY),
    );
    ui.add_space(S4);
    if widgets::primary_button(ui, Some(icon::PLAY), "Start calibration").clicked() {
        actions.push(UiAction::StartCalibration);
    }
    ui.add_space(S8);
    converter(ui, state, model);
}

fn converter(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel) {
    widgets::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        section(ui, "Convert a sensitivity");
        ui.label(
            RichText::new("Enter a value from one game to see it in the others.")
                .font(body(13.0))
                .color(TEXT_SECONDARY),
        );
        ui.add_space(S2);
        let from_id = state
            .convert_game_id
            .clone()
            .unwrap_or_else(|| model.profile.primary_game_id.clone());
        let Some(from) = primary_game(model.games, &from_id) else {
            return;
        };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("convert_from")
                .selected_text(&from.name)
                .width(240.0)
                .show_ui(ui, |ui| {
                    for g in model.games {
                        if ui.selectable_label(g.id == from.id, &g.name).clicked() {
                            state.convert_game_id = Some(g.id.clone());
                        }
                    }
                });
            ui.add(
                TextEdit::singleline(&mut state.convert_value)
                    .hint_text("0.35")
                    .font(body(15.0))
                    .desired_width(120.0),
            );
        });
        if state.convert_value.trim().is_empty() {
            return;
        }
        match parse_game_sens(&state.convert_value) {
            Err(e) => {
                widgets::field_error(ui, e);
            }
            Ok(v) => {
                let dpi = model.profile.dpi;
                let cm = SensitivityConverter::game_sens_to_cm_360(from, dpi, v);
                ui.add_space(S2);
                ui.label(
                    RichText::new(format!("= {cm:.1} cm/360° at {dpi:.0} DPI"))
                        .font(body(13.0))
                        .color(TEXT_MUTED),
                );
                for g in model.games.iter().filter(|g| g.id != from.id) {
                    value_row(ui, &g.name, SensitivityConverter::cm_360_to_game_sens(g, dpi, cm));
                }
            }
        }
    });
}

fn value_row(ui: &mut egui::Ui, name: &str, value: f64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).font(body(13.0)).color(TEXT_SECONDARY));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{value:.3}")).font(display(15.0)).color(TEXT));
        });
    });
}

fn result(
    ui: &mut egui::Ui,
    state: &mut UiState,
    model: &UiModel,
    cal: &CalibrationResult,
    actions: &mut Vec<UiAction>,
) {
    let p = model.profile;
    let now = ui.input(|i| i.time);
    let main = primary_game(model.games, &p.primary_game_id);
    let sens_in = |g| SensitivityConverter::cm_360_to_game_sens(g, p.dpi, cal.cm_360);

    ui.horizontal_top(|ui| {
        // Left: the number.
        ui.allocate_ui_with_layout(egui::vec2(250.0, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(250.0);
            widgets::eyebrow(ui, "Your sensitivity");
            ui.label(
                RichText::new(format!("{:.1}", cal.cm_360))
                    .font(display_bold(60.0))
                    .color(ACCENT),
            );
            ui.label(
                RichText::new(format!("cm / 360° · {:.0} DPI", p.dpi))
                    .font(body(14.0))
                    .color(TEXT_SECONDARY),
            );
            ui.add_space(S3);
            let level = ConfidenceLevel::from_score(cal.confidence);
            widgets::badge(ui, level.label(), level);
            ui.label(
                RichText::new(delta_text(cal.cm_360, p.current_cm_360))
                    .font(body(12.0))
                    .color(TEXT_MUTED),
            );
        });

        // Right: per-game values.
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            if let Some(g) = main {
                let v = sens_in(g);
                widgets::highlight_card(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&g.name).font(medium(14.0)).color(TEXT));
                            ui.label(RichText::new("Your game").font(body(12.0)).color(TEXT_MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let copied = state.copied_at.is_some_and(|t| now - t < COPIED_SECS);
                            let (glyph, label) = if copied {
                                (icon::CHECK, "Copied")
                            } else {
                                (icon::COPY, "Copy")
                            };
                            if widgets::secondary_button(ui, Some(glyph), label).clicked() {
                                ui.ctx().copy_text(format!("{v:.3}"));
                                state.copied_at = Some(now);
                            }
                            ui.add_space(S2);
                            ui.label(RichText::new(format!("{v:.3}")).font(display_bold(26.0)).color(TEXT));
                        });
                    });
                });
            }
            ui.add_space(S1);
            let others: Vec<_> = model
                .games
                .iter()
                .filter(|g| Some(g.id.as_str()) != main.map(|m| m.id.as_str()))
                .collect();
            let shown = if state.show_all_games { others.len() } else { ROWS_COLLAPSED.min(others.len()) };
            for g in &others[..shown] {
                widgets::row_card(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    value_row(ui, &g.name, sens_in(g));
                });
            }
            let hidden = others.len() - shown;
            if hidden > 0 {
                if widgets::ghost_button(ui, None, &format!("+ {hidden} more games")).clicked() {
                    state.show_all_games = true;
                }
            } else if state.show_all_games && others.len() > ROWS_COLLAPSED {
                if widgets::ghost_button(ui, None, "Show fewer").clicked() {
                    state.show_all_games = false;
                }
            }
        });
    });

    ui.add_space(S6);
    ui.horizontal(|ui| {
        if (cal.cm_360 - p.current_cm_360).abs() >= 0.05 {
            if widgets::primary_button(ui, Some(icon::CHECK), "Use this sensitivity").clicked() {
                actions.push(UiAction::ApplySensitivity(cal.cm_360));
            }
        } else {
            ui.label(
                RichText::new(format!("{} This is your current sensitivity", icon::CHECK))
                    .font(body(13.0))
                    .color(SUCCESS),
            );
            ui.add_space(S2);
        }
        if widgets::secondary_button(ui, Some(icon::ARROWS_CLOCKWISE), "Run again").clicked() {
            actions.push(UiAction::StartCalibration);
        }
    });

    ui.add_space(S8);
    section(ui, "How we got here");
    ui.label(
        RichText::new(
            "Each round tested one sensitivity and moved the range toward where your shots landed cleanly.",
        )
        .font(body(13.0))
        .color(TEXT_SECONDARY),
    );
    ui.add_space(S3);
    ui.columns(2, |c| {
        widgets::card(&mut c[0], |ui| {
            ui.label(RichText::new("Tested per round").font(body(12.0)).color(TEXT_MUTED));
            graphs::psa_convergence(ui, &cal.rounds);
        });
        widgets::card(&mut c[1], |ui| {
            ui.label(RichText::new("Score per shot").font(body(12.0)).color(TEXT_MUTED));
            graphs::performance_curve(ui, &cal.samples, cal.stat_cm);
        });
    });
}
