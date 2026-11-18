use egui::{Align, Layout, RichText, TextEdit};
use egui_phosphor::regular as icon;

use crate::data::crosshair::{
    CrosshairStyle, DOT_SIZE_RANGE, GAP_RANGE, LENGTH_RANGE, OPACITY_RANGE, PRESET_COLORS, THICKNESS_RANGE,
};
use crate::data::DisplayMode;
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

    card(ui, &mut |ui| crosshair_card(ui, state, p.crosshair, actions));

    card(ui, &mut |ui| {
        section(ui, "Display");
        caption(ui, "How SENSE fills your screen. Fullscreen uses your screen's own resolution at its highest refresh rate.");
        const MODES: [DisplayMode; 2] = [DisplayMode::Window, DisplayMode::Fullscreen];
        let current = MODES.iter().position(|m| *m == model.display_mode).unwrap_or(0);
        let choices = [(icon::APP_WINDOW, "Window"), (icon::CORNERS_OUT, "Fullscreen")];
        if let Some(i) = widgets::segmented(ui, "display_mode", &choices, current) {
            actions.push(UiAction::SetDisplayMode(MODES[i]));
        }
        ui.add_space(S2);
        ui.label(
            RichText::new("F11 or Alt+Enter switches at any time.")
                .font(body(12.0))
                .color(TEXT_MUTED),
        );
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
        ui.add_space(S2);
        caption(ui, "Grip, what moves when you aim, mousepad, and what feels off.");
        if widgets::ghost_button(ui, None, "Your setup").clicked() {
            actions.push(UiAction::OpenSetupQuestions);
        }
    });
}

const PREVIEW_SIZE: f32 = 160.0;
/// Roughly the game's walls, so the preview shows the crosshair against what it's aimed at.
const PREVIEW_WALL: egui::Color32 = egui::Color32::from_rgb(44, 45, 52);
const PREVIEW_GRID: egui::Color32 = egui::Color32::from_rgb(58, 59, 68);

fn rgb([r, g, b]: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

/// The Crosshair card. Changes made while a slider or the colour picker is held stay a draft
/// (shown in the preview) and are saved to the profile on release.
fn crosshair_card(ui: &mut egui::Ui, state: &mut UiState, saved: CrosshairStyle, actions: &mut Vec<UiAction>) {
    section(ui, "Crosshair");
    caption(ui, "How your crosshair looks while you aim. Saved with this profile.");
    let mut style = state.crosshair_draft.unwrap_or(saved);
    ui.horizontal_top(|ui| {
        let controls_w = (ui.available_width() - PREVIEW_SIZE - S6).max(300.0);
        ui.allocate_ui_with_layout(egui::vec2(controls_w, 0.0), Layout::top_down(Align::Min), |ui| {
            crosshair_controls(ui, &mut style);
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            ui.vertical(|ui| {
                crosshair_preview(ui, &style);
                ui.add_space(S1);
                ui.label(RichText::new("Actual size").font(body(12.0)).color(TEXT_MUTED));
            });
        });
    });
    if style == saved {
        state.crosshair_draft = None;
    } else if ui.input(|i| i.pointer.any_down()) {
        state.crosshair_draft = Some(style);
    } else {
        actions.push(UiAction::SetCrosshair(style));
        state.crosshair_draft = None;
    }
}

fn crosshair_controls(ui: &mut egui::Ui, style: &mut CrosshairStyle) {
    // The global 36 px interact height is for buttons; it makes slider thumbs huge.
    ui.spacing_mut().interact_size.y = 24.0;
    ui.spacing_mut().slider_width = 200.0;
    let row_label = |ui: &mut egui::Ui, text: &str| {
        ui.label(RichText::new(text).font(body(13.0)).color(TEXT_SECONDARY));
    };
    let value = |ui: &mut egui::Ui, text: String| {
        ui.label(RichText::new(text).font(body(13.0)).color(TEXT_SECONDARY));
    };
    let px_slider = |ui: &mut egui::Ui, v: &mut u32, range: std::ops::RangeInclusive<u32>, enabled: bool| {
        ui.horizontal(|ui| {
            ui.add_enabled(enabled, egui::Slider::new(v, range).show_value(false).step_by(1.0));
            value(ui, format!("{v} px"));
        });
    };

    egui::Grid::new("crosshair_controls")
        .num_columns(2)
        .spacing(egui::vec2(S4, S2))
        .show(ui, |ui| {
            row_label(ui, "Colour");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S1;
                for (name, c) in PRESET_COLORS {
                    if widgets::swatch(ui, Some(rgb(c)), style.color == c, name).clicked() {
                        style.color = c;
                    }
                }
                let custom = !PRESET_COLORS.iter().any(|(_, c)| *c == style.color);
                let resp = widgets::swatch(ui, custom.then(|| rgb(style.color)), custom, "Custom colour");
                let popup_id = ui.make_persistent_id("crosshair_custom_colour");
                if resp.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                }
                egui::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    let mut c = rgb(style.color);
                    if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
                        style.color = [c.r(), c.g(), c.b()];
                    }
                });
            });
            ui.end_row();

            row_label(ui, "Opacity");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut style.opacity, OPACITY_RANGE).show_value(false));
                value(ui, format!("{:.0}%", style.opacity * 100.0));
            });
            ui.end_row();

            row_label(ui, "Length");
            px_slider(ui, &mut style.length, LENGTH_RANGE, true);
            ui.end_row();

            row_label(ui, "Thickness");
            px_slider(ui, &mut style.thickness, THICKNESS_RANGE, true);
            ui.end_row();

            row_label(ui, "Gap");
            px_slider(ui, &mut style.gap, GAP_RANGE, style.length > 0);
            ui.end_row();

            // Without lines the dot is the whole crosshair, so it stays on.
            let mut dot = style.dot || style.length == 0;
            if ui.add_enabled(style.length > 0, egui::Checkbox::new(&mut dot, "Centre dot")).changed() {
                style.dot = dot;
            }
            px_slider(ui, &mut style.dot_size, DOT_SIZE_RANGE, dot);
            ui.end_row();

            ui.checkbox(&mut style.outline, "Outline");
            value(ui, "A black edge, easier to see on bright targets.".into());
            ui.end_row();
        });

    ui.add_space(S2);
    let is_default = *style == CrosshairStyle::default();
    ui.add_enabled_ui(!is_default, |ui| {
        if widgets::ghost_button(ui, Some(icon::ARROW_COUNTER_CLOCKWISE), "Reset to default").clicked() {
            *style = CrosshairStyle::default();
        }
    });
}

/// The crosshair at actual screen-pixel size on a patch of wall, drawn from the same pixel
/// rectangles as the game.
fn crosshair_preview(ui: &mut egui::Ui, style: &CrosshairStyle) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(PREVIEW_SIZE, PREVIEW_SIZE), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, egui::CornerRadius::same(R_CONTROL), PREVIEW_WALL);
    // Grid lines as on the walls, none right behind the crosshair.
    for f in [0.125, 0.375, 0.625, 0.875] {
        let x = rect.left() + rect.width() * f;
        let y = rect.top() + rect.height() * f;
        let stroke = egui::Stroke::new(1.0_f32, PREVIEW_GRID);
        p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], stroke);
        p.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], stroke);
    }
    p.rect_stroke(
        rect,
        egui::CornerRadius::same(R_CONTROL),
        egui::Stroke::new(1.0_f32, BORDER_STRONG),
        egui::StrokeKind::Inside,
    );

    // Whole screen pixels, so 1 px lines stay sharp (a mesh isn't feathered like shapes are).
    let ppp = ui.ctx().pixels_per_point();
    let (cx, cy) = ((rect.center().x * ppp).floor(), (rect.center().y * ppp).floor());
    let alpha = (style.clamped().opacity * 255.0).round() as u8;
    let [r, g, b] = style.color;
    let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, alpha);
    let outline = egui::Color32::from_rgba_unmultiplied(0, 0, 0, alpha);
    let shape = style.shape();
    let mut mesh = egui::Mesh::default();
    let rects = shape.outline.iter().map(|r| (r, outline)).chain(shape.fill.iter().map(|r| (r, fill)));
    for (r, color) in rects {
        let min = egui::pos2((cx + r.x as f32) / ppp, (cy + r.y as f32) / ppp);
        let size = egui::vec2(r.w as f32 / ppp, r.h as f32 / ppp);
        mesh.add_colored_rect(egui::Rect::from_min_size(min, size), color);
    }
    p.add(egui::Shape::mesh(mesh));
}
