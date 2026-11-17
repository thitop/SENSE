use egui::{Align, Layout, RichText, TextEdit};
use egui_phosphor::regular as icon;

use crate::game::calibration::{parse_dpi, primary_game, ConfidenceLevel};
use crate::ui::screens::{header, section};
use crate::ui::theme::*;
use crate::ui::{widgets, UiAction, UiModel, UiState};

const SAVED_SECS: f64 = 1.5;

fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(body(13.0)).color(TEXT_SECONDARY));
    ui.add_space(S2);
}

pub fn show(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    let p = model.profile;
    let now = ui.input(|i| i.time);
    header(ui, "Settings", None);

    let card = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
        widgets::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
        ui.add_space(S3);
    };

    card(ui, &mut |ui| {
        section(ui, "Mouse");
        caption(ui, "Your mouse sensor DPI. It turns mouse counts into centimetres.");
        let input = state.dpi_input.get_or_insert_with(|| format!("{:.0}", p.dpi));
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(input)
                    .font(body(15.0))
                    .desired_width(140.0)
                    .margin(egui::vec2(S3, S2)),
            );
            if widgets::secondary_button(ui, None, "Save").clicked() {
                match parse_dpi(input) {
                    Ok(v) => {
                        actions.push(UiAction::SaveDpi(v));
                        state.dpi_error = None;
                        state.dpi_saved_at = Some(now);
                    }
                    Err(e) => state.dpi_error = Some(e),
                }
            }
            if state.dpi_saved_at.is_some_and(|t| now - t < SAVED_SECS) {
                ui.label(RichText::new(format!("{} Saved", icon::CHECK)).font(body(13.0)).color(SUCCESS));
            }
        });
        if let Some(e) = state.dpi_error {
            widgets::field_error(ui, e);
        }
    });

    card(ui, &mut |ui| {
        section(ui, "Main game");
        caption(ui, "Results show in this game's units first.");
        let current = primary_game(model.games, &p.primary_game_id);
        egui::ComboBox::from_id_salt("main_game")
            .selected_text(current.map(|g| g.name.as_str()).unwrap_or("Choose a game"))
            .width(280.0)
            .show_ui(ui, |ui| {
                for g in model.games {
                    let selected = current.is_some_and(|c| c.id == g.id);
                    if ui.selectable_label(selected, &g.name).clicked() && !selected {
                        actions.push(UiAction::SetMainGame(g.id.clone()));
                    }
                }
            });
    });

    card(ui, &mut |ui| {
        section(ui, "Audio");
        // The global 36 px interact height is for buttons; it makes slider thumbs huge.
        ui.spacing_mut().interact_size.y = 24.0;
        ui.spacing_mut().slider_width = 240.0;
        let mut enabled = model.audio_enabled;
        let mut volume = model.volume;
        let mut music_volume = model.music_volume;
        let a = ui.checkbox(&mut enabled, "Sound effects");
        let slider = |ui: &mut egui::Ui, on: bool, v: &mut f32, label: &str| {
            ui.horizontal(|ui| {
                let r = ui.add_enabled(on, egui::Slider::new(v, 0.0..=1.0).show_value(false));
                ui.label(
                    RichText::new(format!("{label} {:.0}%", *v * 100.0))
                        .font(body(13.0))
                        .color(TEXT_SECONDARY),
                );
                r
            })
            .inner
        };
        let b = slider(ui, enabled, &mut volume, "Effects");
        ui.add_space(S2);
        let c = slider(ui, true, &mut music_volume, "Music");
        if a.changed() || b.changed() || c.changed() {
            actions.push(UiAction::SetAudio { enabled, volume, music_volume });
        }
    });

    card(ui, &mut |ui| {
        section(ui, "Profiles");
        caption(ui, "Each profile keeps its own DPI, results, and history.");
        let active_id = p.id.as_str();
        for prof in model.profiles {
            let is_active = prof.id == active_id;
            ui.horizontal(|ui| {
                ui.label(RichText::new(&prof.name).font(medium(14.0)).color(TEXT));
                ui.label(
                    RichText::new(format!("{:.0} DPI", prof.dpi))
                        .font(body(12.0))
                        .color(TEXT_MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if is_active {
                        widgets::badge(ui, "Active", ConfidenceLevel::High);
                    } else {
                        if widgets::ghost_button(ui, Some(icon::TRASH), "Delete").clicked() {
                            state.pending_delete = Some(prof.id.clone());
                        }
                        if widgets::secondary_button(ui, None, "Switch").clicked() {
                            actions.push(UiAction::SelectProfile(prof.id.clone()));
                        }
                    }
                });
            });
            if !is_active && state.pending_delete.as_deref() == Some(prof.id.as_str()) {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Delete {}? This can't be undone.", prof.name))
                            .font(body(13.0))
                            .color(TEXT_SECONDARY),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::ghost_button(ui, None, "Cancel").clicked() {
                            state.pending_delete = None;
                        }
                        if widgets::danger_button(ui, "Delete").clicked() {
                            actions.push(UiAction::DeleteProfile(prof.id.clone()));
                            state.pending_delete = None;
                        }
                    });
                });
            }
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut state.new_profile_name)
                    .hint_text("Profile name")
                    .font(body(14.0))
                    .desired_width(200.0)
                    .margin(egui::vec2(S3, S2)),
            );
            if widgets::secondary_button(ui, Some(icon::PLUS), "Add profile").clicked() {
                let name = state.new_profile_name.trim().to_string();
                if !name.is_empty() {
                    actions.push(UiAction::CreateProfile(name));
                    state.new_profile_name.clear();
                }
            }
        });
    });

    card(ui, &mut |ui| {
        section(ui, "Setup");
        caption(ui, "Go through DPI, main game, and current sensitivity again.");
        if widgets::ghost_button(ui, Some(icon::ARROW_COUNTER_CLOCKWISE), "Run setup again").clicked() {
            actions.push(UiAction::RerunSetup);
        }
    });
}
