//! Design tokens, embedded fonts, and the global egui style.
//! Every colour used by the UI comes from here.

use std::sync::Arc;

use egui::{
    epaint::Shadow, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke,
    Visuals,
};

// ---- Colour tokens (spec § Colour tokens) ----
pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0C, 0x0E);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x0F, 0x10, 0x13);
pub const SURFACE: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
pub const SURFACE_RAISED: Color32 = Color32::from_rgb(0x1B, 0x1D, 0x21);
pub const BORDER: Color32 = Color32::from_rgb(0x23, 0x25, 0x29);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x2C, 0x2F, 0x35);
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEE);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xA3, 0xA6, 0xAD);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8A, 0x8D, 0x94);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6B, 0x6E, 0x75);
pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0xD2, 0x3F);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xFF, 0xDD, 0x6B);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x1A, 0x15, 0x00);
pub const ACCENT_BORDER: Color32 = Color32::from_rgb(0x4A, 0x3F, 0x12);
pub const SUCCESS_BG: Color32 = Color32::from_rgb(0x1F, 0x2A, 0x1C);
pub const SUCCESS: Color32 = Color32::from_rgb(0x9B, 0xD6, 0x7A);
pub const WARN_BG: Color32 = Color32::from_rgb(0x2E, 0x26, 0x12);
pub const WARN: Color32 = Color32::from_rgb(0xE8, 0xB7, 0x5A);
pub const DANGER: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
/// `BG` at ~85 % alpha, for panels floating over the 3D scene.
pub const HUD_BG: Color32 = Color32::from_rgba_premultiplied(9, 10, 12, 217);

// ---- Spacing and shape ----
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S6: f32 = 24.0;
pub const S8: f32 = 32.0;
pub const R_CONTROL: u8 = 8;
pub const R_CARD: u8 = 10;
pub const R_HUD: u8 = 12;
pub const CONTROL_HEIGHT: f32 = 36.0;

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.ttf");
const CHAKRA_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/ChakraPetch-Medium.ttf");
const CHAKRA_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/ChakraPetch-SemiBold.ttf");

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
pub fn display_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display_bold".into()))
}

/// Install fonts, the global style and the image loaders (game logos are SVG). Call once at
/// startup.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    ctx.set_style(style());
    egui_extras::install_image_loaders(ctx);
}

fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add("inter", INTER_REGULAR);
    add("inter_medium", INTER_MEDIUM);
    add("chakra", CHAKRA_MEDIUM);
    add("chakra_bold", CHAKRA_SEMIBOLD);
    fonts.font_data.insert(
        "phosphor".into(),
        Arc::new(egui_phosphor::Variant::Regular.font_data()),
    );

    // egui's bundled fonts stay as the last fallback for any glyph ours lack.
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |primary: &str| {
        let mut list = vec![primary.to_string(), "phosphor".to_string()];
        list.extend(fallback.iter().cloned());
        list
    };
    fonts.families.insert(FontFamily::Proportional, family("inter"));
    fonts
        .families
        .insert(FontFamily::Name("medium".into()), family("inter_medium"));
    fonts
        .families
        .insert(FontFamily::Name("display".into()), family("chakra"));
    fonts
        .families
        .insert(FontFamily::Name("display_bold".into()), family("chakra_bold"));
    fonts
}

fn style() -> egui::Style {
    let mut style = egui::Style {
        visuals: Visuals::dark(),
        ..Default::default()
    };
    let v = &mut style.visuals;
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_corner_radius = CornerRadius::same(R_CARD);
    v.menu_corner_radius = CornerRadius::same(R_CONTROL);
    v.extreme_bg_color = SIDEBAR; // text edit background
    v.faint_bg_color = SURFACE;
    v.override_text_color = None;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.25);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);

    let w = &mut v.widgets;
    let radius = CornerRadius::same(R_CONTROL);
    w.noninteractive.bg_fill = SURFACE;
    w.noninteractive.weak_bg_fill = SURFACE;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = SURFACE_RAISED;
    w.inactive.weak_bg_fill = SURFACE_RAISED;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_STRONG);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = BORDER_STRONG;
    w.hovered.weak_bg_fill = BORDER_STRONG;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, TEXT_FAINT);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = BORDER_STRONG;
    w.active.weak_bg_fill = BORDER_STRONG;
    w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open = w.active;

    // Built-in widgets (checkbox, combo box, slider values) pick up our fonts through these.
    use egui::TextStyle;
    style.text_styles = [
        (TextStyle::Small, body(12.0)),
        (TextStyle::Body, body(14.0)),
        (TextStyle::Button, medium(14.0)),
        (TextStyle::Heading, display_bold(22.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(S2, S2);
    style.spacing.button_padding = egui::vec2(S4, S2);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.animation_time = 0.12;
    style
}
