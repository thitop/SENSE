//! In-game overlays drawn over the 3D scene while the menu is closed.

use egui::{vec2, Align2, CornerRadius, Frame, Id, Margin, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;

use crate::game::calibration::{CalibrationProgress, PlayMode};
use crate::game::tasks::TaskState;
use crate::ui::screens::practice::keycap;
use crate::ui::theme::*;
use crate::ui::{widgets, Screen, UiAction, UiModel, UiState};

const TOAST_SECS: f64 = 2.75;

/// "3", "2", "1", then "Go" once the countdown reaches zero.
pub fn countdown_label(remaining_secs: f32) -> String {
    if remaining_secs <= 0.0 {
        "Go".into()
    } else {
        (remaining_secs.ceil() as i32).to_string()
    }
}

fn hud_frame() -> Frame {
    Frame::NONE
        .fill(HUD_BG)
        .stroke(Stroke::new(1.0_f32, BORDER_STRONG))
        .corner_radius(CornerRadius::same(R_HUD))
        .inner_margin(Margin::symmetric(14, 8))
}

/// A non-interactive overlay that never steals clicks from the game.
fn passive(id: &str, anchor: Align2, offset: egui::Vec2) -> egui::Area {
    egui::Area::new(Id::new(id))
        .anchor(anchor, offset)
        .interactable(false)
        .order(egui::Order::Foreground)
}

pub fn render(ctx: &egui::Context, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    let now = ctx.input(|i| i.time);

    match model.session.mode {
        PlayMode::Practice => practice_pill(ctx, model),
        PlayMode::Calibration => {
            if let Some(p) = model.calibration {
                calibration_panel(ctx, &p);
            }
            passive("pause_hint", Align2::RIGHT_BOTTOM, vec2(-16.0, -16.0)).show(ctx, |ui| {
                ui.label(RichText::new("Esc · pause").font(body(12.0)).color(TEXT_MUTED));
            });
        }
    }

    if let Some((text, t0)) = &state.toast {
        let age = now - t0;
        if age > TOAST_SECS {
            state.toast = None;
        } else {
            let alpha = widgets::toast_alpha(age);
            let text = text.clone();
            passive("toast", Align2::CENTER_BOTTOM, vec2(0.0, -28.0)).show(ctx, |ui| {
                ui.set_opacity(alpha);
                hud_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon::ARROWS_LEFT_RIGHT).font(body(15.0)).color(ACCENT));
                        ui.label(RichText::new(text).font(medium(13.0)).color(TEXT));
                    });
                });
            });
        }
    }

    if model.session.mode == PlayMode::Calibration && model.session.paused {
        paused_card(ctx, actions);
        return;
    }

    match model.task_state {
        TaskState::Countdown { remaining_secs } => countdown(ctx, remaining_secs),
        TaskState::Finished if model.session.mode == PlayMode::Practice => finished_card(ctx, model, actions),
        _ => {}
    }
}

fn practice_pill(ctx: &egui::Context, model: &UiModel) {
    passive("practice_pill", Align2::CENTER_TOP, vec2(0.0, 16.0)).show(ctx, |ui| {
        hud_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                let dot = || RichText::new("·").font(body(13.0)).color(TEXT_FAINT);
                ui.label(RichText::new(model.task_name).font(medium(13.0)).color(TEXT));
                ui.label(dot());
                ui.label(
                    RichText::new(format!("{:.1} cm/360", model.current_cm_360))
                        .font(display(13.0))
                        .color(TEXT),
                );
                ui.label(dot());
                ui.label(RichText::new(format!("{} FPS", model.fps)).font(body(12.0)).color(TEXT_MUTED));
                ui.label(dot());
                ui.label(RichText::new("Esc menu").font(body(12.0)).color(TEXT_MUTED));
            });
        });
    });
}

fn calibration_panel(ctx: &egui::Context, p: &CalibrationProgress) {
    passive("calibration_panel", Align2::CENTER_TOP, vec2(0.0, 16.0)).show(ctx, |ui| {
        hud_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S3;
                ui.label(
                    RichText::new(format!("Round {}", p.round))
                        .font(display_bold(15.0))
                        .color(TEXT),
                );
                ui.label(RichText::new(format!("/ {}", p.max_rounds)).font(display(13.0)).color(TEXT_MUTED));
                segments(ui, p);
                ui.label(RichText::new("Testing").font(body(12.0)).color(TEXT_MUTED));
                ui.label(
                    RichText::new(format!("{:.1} cm", p.testing_cm_360))
                        .font(display(13.0))
                        .color(TEXT),
                );
                ui.label(
                    RichText::new(format!("range {:.0}–{:.0}", p.low_cm_360, p.high_cm_360))
                        .font(body(12.0))
                        .color(TEXT_MUTED),
                );
                ui.label(
                    RichText::new(format!("Shot {} / {}", p.shot_in_round, p.shots_per_round))
                        .font(body(12.0))
                        .color(TEXT_MUTED),
                );
            });
        });
    });
}

fn segments(ui: &mut egui::Ui, p: &CalibrationProgress) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for r in 1..=p.max_rounds {
            let (rect, _) = ui.allocate_exact_size(vec2(14.0, 4.0), Sense::hover());
            let color = if r < p.round || p.completed {
                ACCENT
            } else if r == p.round {
                ACCENT.gamma_multiply(0.45)
            } else {
                BORDER_STRONG
            };
            ui.painter().rect_filled(rect, CornerRadius::same(2), color);
        }
    });
}

fn countdown(ctx: &egui::Context, remaining: f32) {
    passive("countdown", Align2::CENTER_CENTER, vec2(0.0, -40.0)).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(countdown_label(remaining))
                    .font(display_bold(96.0))
                    .color(ACCENT),
            );
            ui.label(RichText::new("Get ready").font(medium(16.0)).color(TEXT));
        });
    });
}

fn centered_card(ctx: &egui::Context, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(Id::new(id))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -20.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            Frame::NONE
                .fill(SURFACE)
                .stroke(Stroke::new(1.0_f32, BORDER_STRONG))
                .corner_radius(CornerRadius::same(R_HUD))
                .inner_margin(Margin::same(S6 as i8))
                .show(ui, |ui| {
                    ui.set_width(360.0);
                    add(ui);
                });
        });
}

fn paused_card(ctx: &egui::Context, actions: &mut Vec<UiAction>) {
    centered_card(ctx, "paused_card", |ui| {
        ui.label(RichText::new("Calibration paused").font(display_bold(22.0)).color(TEXT));
        ui.label(
            RichText::new("Your progress is kept.")
                .font(body(14.0))
                .color(TEXT_SECONDARY),
        );
        ui.add_space(S4);
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, Some(icon::PLAY), "Resume").clicked() {
                actions.push(UiAction::Resume);
            }
            if widgets::secondary_button(ui, None, "Quit calibration").clicked() {
                actions.push(UiAction::QuitCalibration);
            }
        });
    });
}

fn finished_card(ctx: &egui::Context, model: &UiModel, actions: &mut Vec<UiAction>) {
    centered_card(ctx, "finished_card", |ui| {
        ui.label(
            RichText::new(format!("{} complete", model.task_name))
                .font(display_bold(22.0))
                .color(TEXT),
        );
        ui.add_space(S3);
        if let Some(m) = model.last_metrics {
            ui.columns(3, |c| {
                let stat = |ui: &mut egui::Ui, label: &str, value: String| {
                    ui.label(RichText::new(label).font(body(12.0)).color(TEXT_MUTED));
                    ui.label(RichText::new(value).font(display(20.0)).color(TEXT));
                };
                stat(&mut c[0], "Score", m.score.to_string());
                stat(&mut c[1], "Accuracy", format!("{:.0}%", m.accuracy));
                stat(&mut c[2], "Avg reaction", format!("{:.0} ms", m.avg_reaction_time_ms));
            });
        }
        ui.add_space(S4);
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, Some(icon::ARROWS_CLOCKWISE), "Retry").clicked() {
                actions.push(UiAction::Retry);
            }
            keycap(ui, "R");
            ui.add_space(S3);
            if widgets::secondary_button(ui, None, "Back to menu").clicked() {
                actions.push(UiAction::Navigate(Screen::Practice));
            }
            keycap(ui, "Esc");
        });
    });
}
