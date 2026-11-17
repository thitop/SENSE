//! Small themed building blocks shared by every screen.

use egui::{
    vec2, Align, Align2, CornerRadius, CursorIcon, Frame, InnerResponse, Layout, Margin, Response, RichText,
    Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType,
};

use super::theme::*;
use crate::game::calibration::ConfidenceLevel;
use crate::sensitivity::games::ScopedSetting;

/// Scoped settings as they're laid out: per-scope values of one setting sit under that
/// setting's name; settings without a scope are listed as setting = value.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopedGroup {
    pub heading: Option<String>,
    /// (label, value)
    pub rows: Vec<(String, String)>,
}

pub fn scoped_groups(settings: &[ScopedSetting]) -> Vec<ScopedGroup> {
    let mut groups: Vec<ScopedGroup> = Vec::new();
    for s in settings {
        let heading = s.scope.as_ref().map(|_| s.setting.clone());
        let row = (s.scope.clone().unwrap_or_else(|| s.setting.clone()), s.value.clone());
        match groups.last_mut() {
            Some(g) if g.heading == heading => g.rows.push(row),
            _ => groups.push(ScopedGroup { heading, rows: vec![row] }),
        }
    }
    groups
}

const KV_ROW_HEIGHT: f32 = 20.0;
/// Game logos on cards, in points.
pub const LOGO_SIZE: f32 = 20.0;

/// Label on the left, value right-aligned, on one compact line.
fn kv_row(ui: &mut Ui, label: &str, value: &str) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(vec2(width, KV_ROW_HEIGHT), Layout::left_to_right(Align::Center), |ui| {
        ui.set_width(width);
        ui.add(egui::Label::new(RichText::new(label).font(body(12.0)).color(TEXT_SECONDARY)).truncate());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).font(display(13.0)).color(TEXT));
        });
    });
}

/// A thin rule across the available width.
pub fn divider(ui: &mut Ui) {
    ui.add_space(S2);
    let x = ui.available_rect_before_wrap().x_range();
    let y = ui.cursor().top();
    ui.painter().hline(x, y, Stroke::new(1.0_f32, BORDER));
    ui.add_space(S2);
}

/// A game's scoped / ADS settings (0% monitor distance) under its hipfire value, with `note`
/// (what the values assume) in small print after them.
pub fn scoped_settings(ui: &mut Ui, settings: &[ScopedSetting], note: Option<&str>) {
    divider(ui);
    // A fixed-height line: `horizontal` would take a full control's height.
    let width = ui.available_width();
    ui.allocate_ui_with_layout(vec2(width, 14.0), Layout::left_to_right(Align::Center), |ui| {
        ui.label(RichText::new("SCOPED / ADS").font(medium(10.0)).color(TEXT_MUTED));
        if !settings.is_empty() {
            ui.label(RichText::new("0% STANDARD").font(medium(10.0)).color(TEXT_FAINT));
        }
    });
    ui.add_space(S1);
    if settings.is_empty() {
        ui.label(RichText::new("No verified value yet").font(body(12.0)).color(TEXT_FAINT));
        return;
    }
    for g in scoped_groups(settings) {
        if let Some(h) = &g.heading {
            ui.label(RichText::new(h).font(body(11.0)).color(TEXT_MUTED));
        }
        for (label, value) in &g.rows {
            kv_row(ui, label, value);
        }
    }
    if let Some(note) = note {
        ui.add_space(S1);
        ui.add(egui::Label::new(RichText::new(note).font(body(11.0)).color(TEXT_FAINT)).wrap());
    }
}

/// What one game card shows.
pub struct GameCard<'a> {
    pub game_id: &'a str,
    pub name: &'a str,
    pub sens: f64,
    /// The player's own game: accent border, tag and logo.
    pub main: bool,
    pub scoped: &'a [ScopedSetting],
    pub scoped_note: Option<&'a str>,
}

/// One game's values: logo and name (with a tag when it's the player's own game), the in-game
/// sensitivity, then its scoped settings. Cards placed side by side pass the tallest card's
/// height as `min_height` so a row lines up; returns this card's own content height.
pub fn game_card(ui: &mut Ui, c: &GameCard, min_height: f32) -> f32 {
    let GameCard { game_id, name, sens, main, scoped, scoped_note } = *c;
    let content = |ui: &mut Ui| {
        ui.set_min_width(ui.available_width());
        let top = ui.cursor().top();
        // Counted from the cursor, so it must come before the content.
        ui.set_min_height(min_height);
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, 28.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_width(width);
            super::logos::logo(ui, game_id, LOGO_SIZE, if main { ACCENT } else { TEXT_SECONDARY });
            ui.add_space(S1);
            ui.add(egui::Label::new(RichText::new(name).font(medium(14.0)).color(TEXT)).truncate());
            if main {
                ui.label(RichText::new("YOUR GAME").font(medium(10.0)).color(ACCENT));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{sens:.3}")).font(display_bold(20.0)).color(TEXT));
            });
        });
        scoped_settings(ui, scoped, scoped_note);
        // The cursor only moves with content, so this is the card's own height.
        ui.cursor().top() - top
    };
    if main {
        highlight_card(ui, content).inner
    } else {
        card(ui, content).inner
    }
}

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
