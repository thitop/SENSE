//! The app's UI: a pure function of a read-only [`UiModel`] that returns [`UiAction`]s.
//! `main.rs` applies the actions, so cursor locking, PSA resets and persistence live in one place.

pub mod app_icon;
pub mod graphs;
pub mod hud;
pub mod logos;
pub mod screens;
pub mod theme;
pub mod widgets;

use egui::{vec2, Align, Align2, CornerRadius, CursorIcon, Frame, Layout, Margin, RichText, Sense};
use egui_phosphor::regular as icon;

use crate::data::{PlayerProfile, SessionHistory};
use crate::game::calibration::{CalibrationProgress, PlaySession};
use crate::game::tasks::{TaskMetrics, TaskState, TaskType};
use crate::sensitivity::games::GameProfile;
use theme::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Practice,
    Sensitivity,
    History,
    Settings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    Navigate(Screen),
    StartCalibration,
    QuitCalibration,
    StartPractice(TaskType),
    Retry,
    Resume,
    ApplySensitivity(f64),
    /// Stop pooling: the latest calibration run stands alone from now on.
    UseLatestRunOnly,
    CompleteOnboarding { dpi: f64, game_id: String, cm_360: f64 },
    RerunSetup,
    SaveDpi(f64),
    SetMainGame(String),
    SetAudio { enabled: bool, volume: f32, music_volume: f32 },
    CreateProfile(String),
    SelectProfile(String),
    DeleteProfile(String),
}

/// Everything the UI reads, snapshotted by the game loop each frame.
pub struct UiModel<'a> {
    pub profile: &'a PlayerProfile,
    pub profiles: &'a [PlayerProfile],
    pub games: &'a [GameProfile],
    pub history: &'a SessionHistory,
    pub session: PlaySession,
    pub task_state: TaskState,
    pub task_name: &'static str,
    pub last_metrics: Option<&'a TaskMetrics>,
    pub calibration: Option<CalibrationProgress>,
    pub current_cm_360: f64,
    pub audio_enabled: bool,
    pub volume: f32,
    pub music_volume: f32,
    pub fps: u32,
    /// Screen-space directions (x right, y up) to live targets outside the view.
    pub offscreen_targets: &'a [glam::Vec2],
}

/// First-run setup in progress.
#[derive(Clone, Debug)]
pub struct OnboardingDraft {
    pub step: u8,
    pub dpi: String,
    pub game_id: String,
    pub sens: String,
    /// Set once the player pressed Continue/Finish, so errors don't show while they're still typing.
    pub attempted: bool,
    /// "Run setup again" on a profile that's already set up can be cancelled; first-run setup can't.
    pub cancellable: bool,
}

impl OnboardingDraft {
    pub fn from_profile(p: &PlayerProfile) -> Self {
        Self {
            step: 1,
            dpi: format!("{:.0}", p.dpi),
            game_id: p.primary_game_id.clone(),
            sens: String::new(),
            attempted: false,
            cancellable: p.onboarded,
        }
    }
}

/// UI-only state that survives between frames.
pub struct UiState {
    pub screen: Screen,
    pub menu_open: bool,
    pub onboarding: Option<OnboardingDraft>,
    /// (message, egui time it appeared)
    pub toast: Option<(String, f64)>,
    // Settings
    pub dpi_input: Option<String>,
    pub dpi_error: Option<&'static str>,
    pub dpi_saved_at: Option<f64>,
    pub new_profile_name: String,
    pub pending_delete: Option<String>,
    // Sensitivity screen
    pub copied_at: Option<f64>,
    pub show_all_games: bool,
    pub convert_game_id: Option<String>,
    pub convert_value: String,
    // History
    /// `session_id` of the session whose detail page is open; `None` shows the list.
    pub open_session: Option<String>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            screen: Screen::Home,
            menu_open: true,
            onboarding: None,
            toast: None,
            dpi_input: None,
            dpi_error: None,
            dpi_saved_at: None,
            new_profile_name: String::new(),
            pending_delete: None,
            copied_at: None,
            show_all_games: false,
            convert_game_id: None,
            convert_value: String::new(),
            open_session: None,
        }
    }

    pub fn show_toast(&mut self, text: impl Into<String>, now: f64) {
        self.toast = Some((text.into(), now));
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_ui(ctx: &egui::Context, state: &mut UiState, model: &UiModel) -> Vec<UiAction> {
    let mut actions = Vec::new();

    if !model.profile.onboarded && state.onboarding.is_none() {
        state.onboarding = Some(OnboardingDraft::from_profile(model.profile));
    }

    if !state.menu_open {
        hud::render(ctx, state, model, &mut actions);
        return actions;
    }

    if state.onboarding.is_some() {
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(BG).inner_margin(Margin::same(S8 as i8)))
            .show(ctx, |ui| screens::onboarding::show(ui, state, model, &mut actions));
        return actions;
    }

    egui::SidePanel::left("sidebar")
        .exact_width(200.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(SIDEBAR)
                .inner_margin(Margin::symmetric(S3 as i8, S4 as i8)),
        )
        .show(ctx, |ui| sidebar(ui, state, model));

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(BG).inner_margin(Margin::same(S8 as i8)))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let avail = ui.available_width();
                    let width = avail.min(960.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(((avail - width) / 2.0).max(0.0));
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                let show = match state.screen {
                                    Screen::Home => screens::home::show,
                                    Screen::Practice => screens::practice::show,
                                    Screen::Sensitivity => screens::sensitivity::show,
                                    Screen::History => screens::history::show,
                                    Screen::Settings => screens::settings::show,
                                };
                                show(ui, state, model, &mut actions);
                            },
                        );
                    });
                });
        });

    actions
}

fn sidebar(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel) {
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.horizontal(|ui| {
        ui.add_space(S2);
        ui.label(RichText::new(icon::CROSSHAIR).font(body(20.0)).color(ACCENT));
        ui.label(RichText::new("SENSE").font(display_bold(17.0)).color(TEXT));
    });
    ui.add_space(S6);

    let mut nav = |ui: &mut egui::Ui, glyph: &str, label: &str, screen: Screen| {
        if widgets::nav_item(ui, glyph, label, state.screen == screen).clicked() {
            state.screen = screen;
            // History in the sidebar always lands on the list, even from a detail page.
            state.open_session = None;
        }
    };
    nav(ui, icon::HOUSE, "Home", Screen::Home);
    nav(ui, icon::TARGET, "Practice", Screen::Practice);
    nav(ui, icon::SLIDERS_HORIZONTAL, "My sensitivity", Screen::Sensitivity);
    nav(ui, icon::CHART_LINE, "History", Screen::History);

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if profile_chip(ui, model.profile).clicked() {
            state.screen = Screen::Settings;
        }
        ui.add_space(S1);
        if widgets::nav_item(ui, icon::GEAR, "Settings", state.screen == Screen::Settings).clicked() {
            state.screen = Screen::Settings;
        }
    });
}

fn profile_chip(ui: &mut egui::Ui, profile: &PlayerProfile) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(R_CONTROL),
            SURFACE_RAISED.gamma_multiply(t * 0.7),
        );
        let center = rect.left_center() + vec2(S3 + 12.0, 0.0);
        ui.painter().circle_filled(center, 12.0, SURFACE_RAISED);
        let initial = profile
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        ui.painter()
            .text(center, Align2::CENTER_CENTER, initial, medium(11.0), TEXT);
        let text_pos = center + vec2(20.0, 0.0);
        ui.painter().text(
            text_pos + vec2(0.0, -7.0),
            Align2::LEFT_CENTER,
            &profile.name,
            medium(12.0),
            TEXT,
        );
        ui.painter().text(
            text_pos + vec2(0.0, 8.0),
            Align2::LEFT_CENTER,
            format!("{:.0} DPI", profile.dpi),
            body(11.0),
            TEXT_MUTED,
        );
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
