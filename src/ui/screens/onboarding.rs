//! Three-step first-run setup: DPI, main game, current in-game sensitivity.

use egui::{vec2, Align, CornerRadius, Key, Layout, RichText, Sense, TextEdit};
use egui_phosphor::regular as icon;

use crate::game::calibration::{parse_dpi, parse_game_sens, primary_game, validate_cm_360};
use crate::sensitivity::converter::SensitivityConverter;
use crate::ui::screens::centered;
use crate::ui::theme::*;
use crate::ui::{widgets, OnboardingDraft, UiAction, UiModel, UiState};

const DPI_PRESETS: [&str; 4] = ["400", "800", "1600", "3200"];

pub fn show(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    let Some(draft) = state.onboarding.as_mut() else {
        return;
    };
    let cancelled = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| card(ui, draft, model, actions))
        .inner;
    if cancelled {
        state.onboarding = None;
    }
}

/// Returns true when the player cancelled a "Run setup again".
fn card(ui: &mut egui::Ui, draft: &mut OnboardingDraft, model: &UiModel, actions: &mut Vec<UiAction>) -> bool {
    ui.add_space(((ui.available_height() - 520.0) / 2.0).max(S4));
    centered(ui, 560.0, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::CROSSHAIR).font(body(20.0)).color(ACCENT));
            ui.label(RichText::new("SENSE").font(display_bold(17.0)).color(TEXT));
        });
        ui.add_space(S6);
        widgets::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = S2;
            ui.horizontal(|ui| {
                widgets::eyebrow(ui, &format!("Setup · step {} of 3", draft.step));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| step_dots(ui, draft.step));
            });
            ui.add_space(S2);
            match draft.step {
                1 => step_dpi(ui, draft),
                2 => {
                    step_game(ui, draft, model);
                    false
                }
                _ => {
                    step_sens(ui, draft, model, actions);
                    false
                }
            }
        })
        .inner
    })
}

fn title(ui: &mut egui::Ui, text: &str, sub: &str) {
    ui.label(RichText::new(text).font(display_bold(24.0)).color(TEXT));
    ui.label(RichText::new(sub).font(body(14.0)).color(TEXT_SECONDARY));
    ui.add_space(S2);
}

fn step_dots(ui: &mut egui::Ui, step: u8) {
    for i in (1..=3u8).rev() {
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 4.0), Sense::hover());
        let color = if i <= step { ACCENT } else { BORDER_STRONG };
        ui.painter().rect_filled(rect, CornerRadius::same(2), color);
    }
}

/// Single-line numeric field; returns true when Enter was pressed in it.
fn number_field(ui: &mut egui::Ui, value: &mut String, hint: &str) -> bool {
    let resp = ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .font(body(16.0))
            .desired_width(f32::INFINITY)
            .margin(vec2(S3, S2)),
    );
    resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

/// Returns true when the player cancelled a "Run setup again".
fn step_dpi(ui: &mut egui::Ui, draft: &mut OnboardingDraft) -> bool {
    title(
        ui,
        "What's your mouse DPI?",
        "Find it in your mouse software. Most mice ship at 800.",
    );
    let enter = number_field(ui, &mut draft.dpi, "800");
    ui.horizontal(|ui| {
        for preset in DPI_PRESETS {
            if widgets::secondary_button(ui, None, preset).clicked() {
                draft.dpi = preset.to_string();
            }
        }
    });
    let parsed = parse_dpi(&draft.dpi);
    if draft.attempted {
        if let Err(e) = parsed {
            widgets::field_error(ui, e);
        }
    }
    ui.add_space(S4);
    let mut next = enter;
    let mut cancel = false;
    ui.horizontal(|ui| {
        if draft.cancellable {
            cancel = widgets::secondary_button(ui, None, "Cancel").clicked();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            next |= widgets::primary_button(ui, Some(icon::ARROW_RIGHT), "Continue").clicked();
        });
    });
    if next {
        draft.attempted = true;
        if parsed.is_ok() {
            draft.step = 2;
            draft.attempted = false;
        }
    }
    cancel
}

fn step_game(ui: &mut egui::Ui, draft: &mut OnboardingDraft, model: &UiModel) {
    title(
        ui,
        "Which game do you play most?",
        "Your result shows in this game's units first.",
    );
    if primary_game(model.games, &draft.game_id).map(|g| g.id != draft.game_id).unwrap_or(false) {
        draft.game_id = model.games[0].id.clone();
    }
    for pair in model.games.chunks(2) {
        ui.columns(2, |cols| {
            for (col, g) in cols.iter_mut().zip(pair) {
                let selected = g.id == draft.game_id;
                let resp = widgets::clickable_row(col, selected, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&g.name).font(medium(13.0)).color(TEXT)).truncate());
                        if selected {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(icon::CHECK).font(body(16.0)).color(ACCENT));
                            });
                        }
                    });
                });
                if resp.clicked() {
                    draft.game_id = g.id.clone();
                }
            }
        });
    }
    ui.add_space(S4);
    ui.horizontal(|ui| {
        if widgets::secondary_button(ui, None, "Back").clicked() {
            draft.step = 1;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::primary_button(ui, Some(icon::ARROW_RIGHT), "Continue").clicked() {
                draft.step = 3;
            }
        });
    });
}

fn step_sens(ui: &mut egui::Ui, draft: &mut OnboardingDraft, model: &UiModel, actions: &mut Vec<UiAction>) {
    let game = primary_game(model.games, &draft.game_id);
    let game_name = game.map(|g| g.name.as_str()).unwrap_or("your game");
    title(
        ui,
        &format!("What's your sensitivity in {game_name}?"),
        "Use the in-game value. We convert it to cm/360° for every game.",
    );
    let enter = number_field(ui, &mut draft.sens, "0.35");
    let dpi = parse_dpi(&draft.dpi).unwrap_or(model.profile.dpi);
    let parsed = parse_game_sens(&draft.sens);
    let cm = match (parsed, game) {
        (Ok(sens), Some(g)) => Some(SensitivityConverter::game_sens_to_cm_360(g, dpi, sens)),
        _ => None,
    };
    let result = parsed.and_then(|_| validate_cm_360(cm.unwrap_or(f64::NAN)));
    if let Some(cm) = cm.filter(|c| c.is_finite()) {
        ui.label(
            RichText::new(format!("= {cm:.1} cm/360° at {dpi:.0} DPI"))
                .font(body(13.0))
                .color(TEXT_MUTED),
        );
    }
    match result {
        // A parsable but implausible value is shown right away: it's almost always a typo.
        Err(e) if parsed.is_ok() || draft.attempted => {
            widgets::field_error(ui, e);
        }
        _ => {}
    }
    ui.add_space(S4);

    let mut finish = enter;
    let mut skip = false;
    ui.horizontal(|ui| {
        if widgets::secondary_button(ui, None, "Back").clicked() {
            draft.step = 2;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            finish |= widgets::primary_button(ui, Some(icon::CHECK), "Finish").clicked();
            skip = widgets::ghost_button(ui, None, "Skip").clicked();
        });
    });

    let complete = |cm_360: f64, sens_known: bool| UiAction::CompleteOnboarding {
        dpi,
        game_id: draft.game_id.clone(),
        cm_360,
        sens_known,
    };
    if skip {
        // A re-run keeps what the player already told us; only a first-run Skip is "unknown".
        let known = draft.cancellable && model.profile.sens_known();
        actions.push(complete(model.profile.current_cm_360, known));
    } else if finish {
        draft.attempted = true;
        if let Ok(cm) = result {
            actions.push(complete(cm, true));
        }
    }
}
