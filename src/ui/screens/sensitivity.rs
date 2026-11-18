//! "My sensitivity": the calibration result, or an empty state with a manual converter.

use egui::{Align, Layout, RichText, TextEdit};
use egui_phosphor::regular as icon;

use crate::data::CalibrationResult;
use crate::game::calibration::{delta_text, parse_game_sens, primary_game, validate_cm_360, ConfidenceLevel};
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
        let dpi = model.profile.dpi;
        let cm = parse_game_sens(&state.convert_value)
            .and_then(|v| validate_cm_360(SensitivityConverter::game_sens_to_cm_360(from, dpi, v)));
        match cm {
            Err(e) => {
                widgets::field_error(ui, e);
            }
            Ok(cm) => {
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

    if cal.no_answer {
        no_answer(ui, cal, actions);
    } else {
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
                if cal.pad_limited() {
                    ui.add_space(S2);
                    ui.label(RichText::new(crate::ui::habits::PAD_LIMIT).font(body(12.0)).color(WARN));
                }
                if let Some(note) = result_note(false, cal.runs_disagree) {
                ui.add_space(S2);
                ui.label(RichText::new(note).font(body(12.0)).color(WARN));
            }
            if let Some(caption) = pooled_caption(cal.pooled_runs) {
                    ui.add_space(S2);
                    ui.label(RichText::new(caption).font(body(12.0)).color(TEXT_SECONDARY));
                    ui.spacing_mut().interact_size.y = 20.0;
                    if widgets::ghost_button(ui, None, "Use only the latest run").clicked() {
                        actions.push(UiAction::UseLatestRunOnly);
                    }
                }
                // When runs disagree, the note above already says what to do instead.
                // At the pad's limit a re-run can't help: the pad note above says what does.
                if level == ConfidenceLevel::Low && !cal.runs_disagree && !cal.pad_limited() {
                    ui.add_space(S2);
                    ui.label(
                        RichText::new("Your tests didn't agree on this one. Run it again before you use it.")
                            .font(body(12.0))
                            .color(WARN),
                    );
                }
                if let Some(d) = &cal.diagnostics {
                    ui.add_space(S4);
                    per_test(ui, d);
                }
            });

            // Right: per-game values.
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                if let Some(g) = main {
                    let v = sens_in(g);
                    widgets::highlight_card(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            crate::ui::logos::logo(ui, &g.id, 28.0, ACCENT);
                            ui.add_space(S1);
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
                        widgets::scoped_settings(ui, &g.scoped, g.scoped_note.as_deref());
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
                    let card = widgets::GameCard {
                        game_id: &g.id,
                        name: &g.name,
                        sens: sens_in(g),
                        main: false,
                        scoped: &g.scoped,
                        scoped_note: g.scoped_note.as_deref(),
                    };
                    widgets::game_card(ui, &card, 0.0);
                    ui.add_space(S1);
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
        // A pad-limited result is low only because the pad cut it off; running again won't change that.
        let low = ConfidenceLevel::from_score(cal.confidence) == ConfidenceLevel::Low && !cal.pad_limited();
        // Low because runs disagree: another run straight away isn't the obvious next step.
        let rerun_first = low && !cal.runs_disagree;
        ui.horizontal(|ui| {
            if rerun_first && widgets::primary_button(ui, Some(icon::ARROWS_CLOCKWISE), "Run again").clicked() {
                actions.push(UiAction::StartCalibration);
            }
            if (cal.cm_360 - p.current_cm_360).abs() >= 0.05 {
                // An unreliable result is still on offer, just not as the obvious next step.
                let use_it = if low {
                    widgets::secondary_button(ui, Some(icon::CHECK), "Use anyway")
                } else {
                    widgets::primary_button(ui, Some(icon::CHECK), "Use this sensitivity")
                };
                if use_it.clicked() {
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
            if !rerun_first && widgets::secondary_button(ui, Some(icon::ARROWS_CLOCKWISE), "Run again").clicked() {
                actions.push(UiAction::StartCalibration);
            }
        });
    }

    if let Some(report) = &cal.habits {
        ui.add_space(S8);
        crate::ui::habits::practise_section(ui, model, cal, report, actions);
    }

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
            graphs::performance_curve(ui, &cal.samples, curve_marker(cal));
        });
    });
}

/// Where the score graph draws its line: the result itself. The per-shot score fit
/// (`stat_cm`) isn't the answer, and when it finds no peak it falls back to the single best
/// shot, which in real play put the line 8 cm away from the result shown above it.
pub fn curve_marker(cal: &CalibrationResult) -> Option<f64> {
    (!cal.no_answer).then_some(cal.cm_360)
}

/// What to tell the player when the numbers aren't a plain measurement.
pub fn result_note(no_answer: bool, runs_disagree: bool) -> Option<&'static str> {
    if no_answer {
        Some(
            "No clear answer this time. Your misses didn't point clearly to faster or slower \
             at the sensitivities tried. Warm up first, then run it again.",
        )
    } else if runs_disagree {
        Some(
            "Your recent runs disagree: they found quite different sensitivities, so this is \
             the latest run alone. Your aim may still be settling in. Running it again right \
             away won't fix that: warm up first, or calibrate again another day.",
        )
    } else {
        None
    }
}

/// The result area of a run that found no answer: no number to copy or use.
fn no_answer(ui: &mut egui::Ui, cal: &CalibrationResult, actions: &mut Vec<UiAction>) {
    ui.scope(|ui| {
        ui.set_max_width(520.0);
        widgets::eyebrow(ui, "Your sensitivity");
        ui.label(RichText::new("No result").font(display_bold(40.0)).color(TEXT_MUTED));
        ui.add_space(S2);
        if let Some(note) = result_note(true, false) {
            ui.label(RichText::new(note).font(body(14.0)).color(TEXT_SECONDARY));
        }
        if let Some(d) = &cal.diagnostics {
            ui.add_space(S4);
            per_test(ui, d);
        }
    });
    ui.add_space(S6);
    if widgets::primary_button(ui, Some(icon::ARROWS_CLOCKWISE), "Run again").clicked() {
        actions.push(UiAction::StartCalibration);
    }
}

/// Under the result when it combines several recent runs.
pub fn pooled_caption(runs: usize) -> Option<String> {
    (runs > 1).then(|| format!("Combined from your last {runs} runs"))
}

/// One test's own estimate as the "By test" row shows it. A test whose misses showed no trend
/// across the tested sensitivities gets no weight in the fit and an uncertainty in the
/// hundreds of thousands of percent; a number would only mislead.
pub fn estimate_text(e: &crate::data::EstimateRecord, left_out: bool) -> String {
    const NO_TREND: f64 = 1.0;
    if left_out {
        "left out".into()
    } else if !(e.rel_uncertainty <= NO_TREND) || !e.cm_360.is_finite() {
        "no trend".into()
    } else {
        format!("{:.1} ±{:.0}%", e.cm_360, e.rel_uncertainty * 100.0)
    }
}

/// Where each test alone would land, and the tests left out of the result.
fn per_test(ui: &mut egui::Ui, d: &crate::data::RunDiagnostics) {
    // The global 36 px interact height is for buttons; these are plain text rows.
    ui.spacing_mut().interact_size.y = 18.0;
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(RichText::new("By test").font(body(12.0)).color(TEXT_MUTED));
    for e in d.estimates.iter().filter(|e| !e.test.starts_with("Flick ")) {
        let left_out = d.left_out.contains(&e.test);
        ui.horizontal(|ui| {
            ui.label(RichText::new(&e.test).font(body(12.0)).color(TEXT_SECONDARY));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let text = estimate_text(e, left_out);
                ui.label(RichText::new(text).font(body(12.0)).color(if left_out { WARN } else { TEXT }));
            });
        });
    }
}
