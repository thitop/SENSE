use egui::{Align, CornerRadius, Layout, RichText, Stroke};
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

const SCENARIOS: [Scenario; 6] = [
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
        desc: "Keep your crosshair on a moving target. No shooting: points build while you stay on it.",
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
    Scenario {
        task: TaskType::MovingTargets,
        icon: icon::CROSSHAIR,
        name: "Moving targets",
        desc: "Hit the moving target, then the next one.",
        meta: "30 targets",
        key: "6",
    },
];

const MIN_CARD_WIDTH: f32 = 280.0;

/// The practice screen's name for a scenario.
pub fn scenario_name(task: TaskType) -> &'static str {
    SCENARIOS.iter().find(|s| s.task == task).map_or("Practice", |s| s.name)
}

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
    // Every card is as tall as the tallest: the icon, name and description of the wordiest card
    // set the height (measured on the frame before), and the count and key sit at the bottom.
    let id = ui.id().with("practice_card_top");
    let tallest_top: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
    let mut measured = 0.0_f32;
    for row in SCENARIOS.chunks(cols) {
        ui.columns(cols, |c| {
            for (i, sc) in row.iter().enumerate() {
                let resp = widgets::clickable_card(&mut c[i], false, |ui| {
                    ui.set_min_width(ui.available_width());
                    // Columns lay out justified; card text reads left-aligned like the rest of the app.
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = S1;
                        let top = ui.cursor().top();
                        ui.label(RichText::new(sc.icon).font(body(22.0)).color(ACCENT));
                        ui.add_space(S1);
                        ui.label(RichText::new(sc.name).font(medium(15.0)).color(TEXT));
                        ui.label(RichText::new(sc.desc).font(body(13.0)).color(TEXT_SECONDARY));
                        let used = ui.cursor().top() - top;
                        measured = measured.max(used);
                        ui.add_space((tallest_top - used).max(0.0) + S2);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(sc.meta).font(body(12.0)).color(TEXT_MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| keycap(ui, sc.key));
                        });
                    });
                });
                if resp.clicked() {
                    actions.push(UiAction::StartPractice(sc.task));
                }
            }
        });
        ui.add_space(S2);
    }
    if (measured - tallest_top).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(id, measured));
        ui.ctx().request_repaint();
    }
}

/// A small bordered key hint, e.g. "1". Painted at its own size so rows can't stretch it.
pub fn keycap(ui: &mut egui::Ui, key: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(key.to_string(), medium(11.0), TEXT_MUTED);
    let size = egui::vec2((galley.size().x + 12.0).max(20.0), 20.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, BORDER_STRONG),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, TEXT_MUTED);
}
