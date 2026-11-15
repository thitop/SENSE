//! Small themed building blocks shared by every screen.

use egui::{
    vec2, Align2, CornerRadius, CursorIcon, Frame, InnerResponse, Margin, Response, RichText,
    Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType,
};

use super::theme::*;
use crate::game::calibration::ConfidenceLevel;

#[derive(Clone, Copy)]
enum Kind {
    Primary,
    Secondary,
    Ghost,
}

fn button(ui: &mut Ui, icon: Option<&str>, label: &str, kind: Kind) -> Response {
    let text = match icon {
        Some(i) => format!("{i}  {label}"),
        None => label.to_string(),
    };
    // PLACEHOLDER lets `painter.galley` apply the per-kind, hover-animated text colour below.
    let galley = ui
        .painter()
        .layout_no_wrap(text, medium(14.0), egui::Color32::PLACEHOLDER);
    let pad_x = if matches!(kind, Kind::Ghost) { S2 } else { S4 };
    let size = vec2(galley.size().x + pad_x * 2.0, CONTROL_HEIGHT.max(galley.size().y + S2 * 2.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        let pressed = resp.is_pointer_button_down_on();
        let (fill, stroke, text_color) = match kind {
            Kind::Primary => (
                ACCENT.lerp_to_gamma(ACCENT_HOVER, t),
                Stroke::NONE,
                ON_ACCENT,
            ),
            Kind::Secondary => (
                SURFACE_RAISED.gamma_multiply(t),
                Stroke::new(1.0_f32, BORDER_STRONG.lerp_to_gamma(TEXT_FAINT, t)),
                TEXT,
            ),
            Kind::Ghost => (
                SURFACE_RAISED.gamma_multiply(t * 0.8),
                Stroke::NONE,
                TEXT_SECONDARY.lerp_to_gamma(TEXT, t),
            ),
        };
        let rect = if pressed { rect.shrink(0.5) } else { rect };
        ui.painter()
            .rect(rect, CornerRadius::same(R_CONTROL), fill, stroke, StrokeKind::Inside);
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, text_color);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Accent-filled call to action. Use at most one per screen.
pub fn primary_button(ui: &mut Ui, icon: Option<&str>, label: &str) -> Response {
    button(ui, icon, label, Kind::Primary)
}

pub fn secondary_button(ui: &mut Ui, icon: Option<&str>, label: &str) -> Response {
    button(ui, icon, label, Kind::Secondary)
}

pub fn ghost_button(ui: &mut Ui, icon: Option<&str>, label: &str) -> Response {
    button(ui, icon, label, Kind::Ghost)
}

/// Outlined red button for destructive confirmations.
pub fn danger_button(ui: &mut Ui, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), medium(13.0), DANGER);
    let size = vec2(galley.size().x + S3 * 2.0, CONTROL_HEIGHT - 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        ui.painter().rect(
            rect,
            CornerRadius::same(R_CONTROL),
            DANGER.gamma_multiply(0.15 * t),
            Stroke::new(1.0_f32, DANGER.gamma_multiply(0.6 + 0.4 * t)),
            StrokeKind::Inside,
        );
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, DANGER);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn card_frame(border: egui::Color32) -> Frame {
    Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(CornerRadius::same(R_CARD))
        .inner_margin(Margin::same(S4 as i8))
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    card_frame(BORDER).show(ui, add)
}

/// Low card for one line of content, e.g. a game and its value.
pub fn row_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    card_frame(BORDER)
        .inner_margin(Margin::symmetric(S3 as i8, S1 as i8))
        .show(ui, add)
}

/// Card with an accent border, for the player's own game.
pub fn highlight_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    card_frame(ACCENT_BORDER).show(ui, add)
}

/// A card that is itself the click target, with a hover border; `selected` gives it the accent border.
pub fn clickable_card(ui: &mut Ui, selected: bool, add: impl FnOnce(&mut Ui)) -> Response {
    clickable(ui, selected, card_frame(BORDER), add)
}

/// One-line clickable row (history entries, pickers) with compact padding.
pub fn clickable_row(ui: &mut Ui, selected: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let frame = card_frame(BORDER).inner_margin(Margin::symmetric(S3 as i8, S1 as i8));
    clickable(ui, selected, frame, add)
}

fn clickable(ui: &mut Ui, selected: bool, frame: Frame, add: impl FnOnce(&mut Ui)) -> Response {
    let mut prepared = frame.begin(ui);
    add(&mut prepared.content_ui);
    let resp = prepared.allocate_space(ui).interact(Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    prepared.frame.fill = SURFACE.lerp_to_gamma(SURFACE_RAISED, t * 0.6);
    prepared.frame.stroke.color = if selected {
        ACCENT_BORDER.lerp_to_gamma(ACCENT, 0.35)
    } else {
        BORDER.lerp_to_gamma(TEXT_FAINT, t)
    };
    prepared.paint(ui);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn stat_card(ui: &mut Ui, label: &str, value: &str, unit: Option<&str>, caption: &str) -> Response {
    card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = S1;
        ui.label(RichText::new(label).font(body(12.0)).color(TEXT_MUTED));
        ui.horizontal(|ui| {
            let color = if value == "—" { TEXT_FAINT } else { TEXT };
            ui.label(RichText::new(value).font(display(22.0)).color(color));
            if let Some(unit) = unit {
                ui.label(RichText::new(unit).font(body(12.0)).color(TEXT_MUTED));
            }
        });
        ui.label(RichText::new(caption).font(body(12.0)).color(TEXT_MUTED));
    })
    .response
}

pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> Response {
    let size = vec2(ui.available_width(), CONTROL_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || selected, 0.12);
        let fill = SURFACE_RAISED.gamma_multiply(if selected { 1.0 } else { t * 0.7 });
        ui.painter().rect_filled(rect, CornerRadius::same(R_CONTROL), fill);
        let (icon_color, text_color) = if selected {
            (ACCENT, TEXT)
        } else {
            let c = TEXT_MUTED.lerp_to_gamma(TEXT, t * 0.6);
            (c, c)
        };
        let left = rect.left_center() + vec2(S3, 0.0);
        ui.painter()
            .text(left, Align2::LEFT_CENTER, icon, body(17.0), icon_color);
        ui.painter().text(
            left + vec2(26.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            if selected { medium(13.0) } else { body(13.0) },
            text_color,
        );
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn badge(ui: &mut Ui, text: &str, level: ConfidenceLevel) -> Response {
    let (fill, color) = match level {
        ConfidenceLevel::High => (SUCCESS_BG, SUCCESS),
        ConfidenceLevel::Medium => (WARN_BG, WARN),
        ConfidenceLevel::Low => (SURFACE_RAISED, TEXT_SECONDARY),
    };
    Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(S2 as i8, 3))
        .show(ui, |ui| ui.label(RichText::new(text).font(body(12.0)).color(color)))
        .response
}

/// Tiny uppercase label above a heading or number.
pub fn eyebrow(ui: &mut Ui, text: &str) -> Response {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(body(11.0))
            .color(TEXT_MUTED)
            .extra_letter_spacing(1.0),
    )
}

pub fn field_error(ui: &mut Ui, msg: &str) -> Response {
    ui.label(RichText::new(msg).font(body(12.0)).color(DANGER))
}

/// Opacity of a toast `age_secs` after it appeared: fade in 0.15 s, hold to 2.5 s, fade out 0.25 s.
pub fn toast_alpha(age_secs: f64) -> f32 {
    let a = if age_secs <= 0.0 {
        0.0
    } else if age_secs < 0.15 {
        age_secs / 0.15
    } else if age_secs < 2.5 {
        1.0
    } else if age_secs < 2.75 {
        1.0 - (age_secs - 2.5) / 0.25
    } else {
        0.0
    };
    a as f32
}
