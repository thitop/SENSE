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

    if model.task_state == TaskState::Running && !model.session.paused {
        offscreen_arrows(ctx, model.offscreen_targets);
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
        TaskState::Countdown { remaining_secs } => {
            let next = model.calibration.and_then(|p| p.segment_label.zip(p.segment_hint));
            countdown(ctx, remaining_secs, next)
        }
        TaskState::Finished if model.session.mode == PlayMode::Practice => finished_card(ctx, model, actions),
        _ => {}
    }
}

/// Distance of off-screen arrows from the screen edge.
const ARROW_MARGIN: f32 = 44.0;

/// Where on a `size` screen the arrow toward `dir` (x right, y up) sits: on the inset border,
/// along the line from the centre.
pub fn edge_point(size: egui::Vec2, dir: glam::Vec2, margin: f32) -> egui::Pos2 {
    let half = size * 0.5;
    let (hx, hy) = ((half.x - margin).max(1.0), (half.y - margin).max(1.0));
    let d = egui::vec2(dir.x, -dir.y); // egui's y grows downward
    let t = (hx / d.x.abs().max(1e-6)).min(hy / d.y.abs().max(1e-6));
    egui::Pos2::new(half.x, half.y) + d * t
}

/// A triangle at the screen edge for each target outside the view.
fn offscreen_arrows(ctx: &egui::Context, dirs: &[glam::Vec2]) {
    if dirs.is_empty() {
        return;
    }
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("offscreen_arrows")));
    for dir in dirs {
        let tip = rect.min + edge_point(rect.size(), *dir, ARROW_MARGIN).to_vec2();
        let d = egui::vec2(dir.x, -dir.y).normalized();
        let side = egui::vec2(-d.y, d.x);
        let base = tip - d * 26.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, base + side * 9.0, base - side * 9.0],
            ACCENT,
            Stroke::new(1.5_f32, HUD_BG),
        ));
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
                if p.warmup {
                    ui.label(RichText::new("Warm-up").font(display_bold(15.0)).color(TEXT));
                } else {
                    ui.label(
                        RichText::new(format!("Round {}", p.round))
                            .font(display_bold(15.0))
                            .color(TEXT),
                    );
                    ui.label(RichText::new(format!("/ {}", p.max_rounds)).font(display(13.0)).color(TEXT_MUTED));
                }
                segments(ui, p);
                ui.label(RichText::new("Testing").font(body(12.0)).color(TEXT_MUTED));
                ui.label(RichText::new(p.speed_text()).font(display(13.0)).color(TEXT));
                ui.label(
                    RichText::new(format!("range {:.0}–{:.0}", p.low_cm_360, p.high_cm_360))
                        .font(body(12.0))
                        .color(TEXT_MUTED),
                );
                let step = match p.segment_label {
                    Some(label) => format!("{label} · {} / {}", p.shot_in_round, p.shots_per_round),
                    None => format!("Shot {} / {}", p.shot_in_round, p.shots_per_round),
                };
                ui.label(RichText::new(step).font(body(12.0)).color(TEXT_MUTED));
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

/// `next`: the calibration test coming up and what it wants, if any.
fn countdown(ctx: &egui::Context, remaining: f32, next: Option<(&str, &str)>) {
    passive("countdown", Align2::CENTER_CENTER, vec2(0.0, -40.0)).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(countdown_label(remaining))
                    .font(display_bold(96.0))
                    .color(ACCENT),
            );
            match next {
                Some((label, hint)) => {
                    ui.label(RichText::new(label).font(medium(16.0)).color(TEXT));
                    ui.label(RichText::new(hint).font(body(13.0)).color(TEXT_SECONDARY));
                }
                None => {
                    ui.label(RichText::new("Get ready").font(medium(16.0)).color(TEXT));
                }
            }
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
