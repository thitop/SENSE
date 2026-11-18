pub mod history;
pub mod home;
pub mod onboarding;
pub mod practice;
pub mod questions;
pub mod sensitivity;
pub mod settings;

use egui::{vec2, Align, Layout, RichText, Ui};

use crate::ui::theme::*;

/// Screen title with an optional one-line explanation.
pub fn header(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    ui.label(RichText::new(title).font(display_bold(28.0)).color(TEXT));
    if let Some(sub) = subtitle {
        ui.add_space(S1);
        ui.label(RichText::new(sub).font(body(14.0)).color(TEXT_SECONDARY));
    }
    ui.add_space(S6);
}

/// Lay `add` out in a column of at most `max_width`, centred in the available width.
pub fn centered<R>(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_width();
    let width = avail.min(max_width);
    ui.horizontal_top(|ui| {
        ui.add_space(((avail - width) / 2.0).max(0.0));
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(width);
            add(ui)
        })
        .inner
    })
    .inner
}

/// Muted section heading inside a screen.
pub fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).font(medium(16.0)).color(TEXT));
    ui.add_space(S1);
}
