use chrono::{DateTime, Duration, Utc};
use egui::RichText;
use egui_phosphor::regular as icon;

use crate::data::SessionHistory;
use crate::game::calibration::primary_game;
use crate::game::tasks::TaskType;
use crate::sensitivity::converter::SensitivityConverter;
use crate::ui::theme::*;
use crate::ui::{widgets, UiAction, UiModel, UiState};

/// Sessions recorded in the 7 days up to `now`.
pub fn sessions_this_week(h: &SessionHistory, now: DateTime<Utc>) -> usize {
    h.sessions
        .iter()
        .filter(|s| s.timestamp <= now && now - s.timestamp <= Duration::days(7))
        .count()
}

pub fn show(ui: &mut egui::Ui, _state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    let p = model.profile;
    ui.add_space(S6);
    widgets::eyebrow(ui, "~5 min");
    ui.add_space(S1);
    ui.scope(|ui| {
        ui.set_max_width(620.0);
        ui.label(
            RichText::new("Find the sensitivity your hand already prefers")
                .font(display_bold(34.0))
                .color(TEXT),
        );
        ui.add_space(S2);
        ui.label(
            RichText::new(
                "Shoot 70 targets. We watch whether you overshoot or undershoot, \
                 then narrow it down round by round.",
            )
            .font(body(15.0))
            .color(TEXT_SECONDARY),
        );
    });
    ui.add_space(S6);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, Some(icon::PLAY), "Start calibration").clicked() {
            actions.push(UiAction::StartCalibration);
        }
        if widgets::secondary_button(ui, None, "Warm up first").clicked() {
            actions.push(UiAction::StartPractice(TaskType::Flick));
        }
    });

    ui.add_space(S8 * 1.5);
    let game = primary_game(model.games, &p.primary_game_id);
    let current_caption = match game {
        Some(g) => format!(
            "{} {:.3}",
            g.name,
            SensitivityConverter::cm_360_to_game_sens(g, p.dpi, p.current_cm_360)
        ),
        None => format!("{:.0} DPI", p.dpi),
    };
    let (last_value, last_caption) = match &p.last_calibration {
        Some(c) => (format!("{:.1}", c.cm_360), format!("Calibrated {}", c.at.format("%b %d"))),
        None => ("—".to_string(), "Not calibrated yet".to_string()),
    };
    let week = sessions_this_week(model.history, Utc::now());

    ui.columns(3, |cols| {
        widgets::stat_card(
            &mut cols[0],
            "Your current",
            &format!("{:.1}", p.current_cm_360),
            Some("cm/360"),
            &current_caption,
        );
        let unit = p.last_calibration.as_ref().map(|_| "cm/360");
        widgets::stat_card(&mut cols[1], "Last result", &last_value, unit, &last_caption);
        widgets::stat_card(&mut cols[2], "Sessions", &week.to_string(), None, "This week");
    });
}
