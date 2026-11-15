use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use egui_phosphor::regular as icon;

use crate::game::tasks::TaskType;
use crate::ui::screens::header;
use crate::ui::theme::*;
use crate::ui::{widgets, UiAction, UiModel, UiState};

struct Scenario {
    task: TaskType,
    icon: &'static str,
    name: &'static str,
    desc: &'static str,
    meta: &'static str,
    key: &'static str,
}

const SCENARIOS: [Scenario; 5] = [
    Scenario {
        task: TaskType::Flick,
        icon: icon::LIGHTNING,
        name: "Flick",
        desc: "Snap to targets that appear anywhere.",
        meta: "30 targets",
        key: "1",
    },
    Scenario {
        task: TaskType::Tracking,
        icon: icon::WAVE_SINE,
        name: "Tracking",
        desc: "Keep your crosshair on a moving target.",
        meta: "30 seconds",
        key: "2",
    },
    Scenario {
        task: TaskType::Switching,
        icon: icon::SHUFFLE,
        name: "Target switching",
        desc: "Clear the highlighted target, then the next.",
        meta: "25 switches",
        key: "3",
    },
    Scenario {
        task: TaskType::MicroAdjust,
        icon: icon::MAGNIFYING_GLASS_PLUS,
        name: "Micro-adjust",
        desc: "Small corrections on head-sized targets.",
        meta: "30 targets",
        key: "4",
    },
    Scenario {
        task: TaskType::Speed,
        icon: icon::TIMER,
        name: "Speed",
        desc: "Targets vanish faster the longer you last.",
        meta: "30 targets",
        key: "5",
    },
];

const MIN_CARD_WIDTH: f32 = 280.0;

pub fn show(ui: &mut egui::Ui, _state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    header(
        ui,
        "Practice",
        Some(&format!(
            "Free play at your current {:.1} cm/360. Nothing here changes it.",
            model.profile.current_cm_360
        )),
    );

    let gap = ui.spacing().item_spacing.x;
    let cols = (((ui.available_width() + gap) / (MIN_CARD_WIDTH + gap)).floor() as usize).max(1);
    for row in SCENARIOS.chunks(cols) {
        ui.columns(cols, |c| {
            for (i, sc) in row.iter().enumerate() {
                let resp = widgets::clickable_card(&mut c[i], false, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(118.0);
                    ui.spacing_mut().item_spacing.y = S1;
                    ui.label(RichText::new(sc.icon).font(body(22.0)).color(ACCENT));
                    ui.add_space(S1);
                    ui.label(RichText::new(sc.name).font(medium(15.0)).color(TEXT));
                    ui.label(RichText::new(sc.desc).font(body(13.0)).color(TEXT_SECONDARY));
                    ui.add_space(S2);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(sc.meta).font(body(12.0)).color(TEXT_MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| keycap(ui, sc.key));
                    });
                });
                if resp.clicked() {
                    actions.push(UiAction::StartPractice(sc.task));
                }
            }
        });
        ui.add_space(S2);
    }
}

/// A small bordered key hint, e.g. "1".
pub fn keycap(ui: &mut egui::Ui, key: &str) {
    Frame::NONE
        .stroke(Stroke::new(1.0_f32, BORDER_STRONG))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(key).font(medium(11.0)).color(TEXT_MUTED));
        });
}
