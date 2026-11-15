pub mod graphs;

use egui::{vec2, Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Stroke};
use graphs::PerformanceGraphs;
use crate::data::DataManager;
use crate::engine::audio::AudioManager;
use crate::game::tasks::{TaskManager, TaskState, TaskType};
use crate::sensitivity::SensitivityEngine;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiScreen {
    PlayingHud,
    Dashboard,
    Tasks,
    Calibration,
    Analytics,
    Settings,
}

pub struct UiState {
    pub current_screen: UiScreen,
    pub is_overlay_open: bool,
    pub input_dpi_str: String,
    pub new_profile_name: String,
    pub selected_game_filter: String,
}

impl UiState {
    pub fn new(initial_dpi: f64) -> Self {
        Self {
            current_screen: UiScreen::Dashboard,
            is_overlay_open: true, // Welcome player with dashboard
            input_dpi_str: format!("{:.0}", initial_dpi),
            new_profile_name: String::new(),
            selected_game_filter: "all".into(),
        }
    }

    pub fn toggle_overlay(&mut self) {
        self.is_overlay_open = !self.is_overlay_open;
        if self.is_overlay_open && self.current_screen == UiScreen::PlayingHud {
            self.current_screen = UiScreen::Dashboard;
        }
    }
}

// Clean Modern Esports Theme
mod theme {
    use egui::Color32;

    pub const BG_OVERLAY: Color32 = Color32::from_rgba_premultiplied(10, 12, 16, 240);
    pub const PANEL_BG: Color32 = Color32::from_rgb(18, 21, 28);
    pub const CARD_BG: Color32 = Color32::from_rgb(26, 30, 40);
    pub const CARD_BG_HOVER: Color32 = Color32::from_rgb(34, 40, 54);

    pub const PRIMARY: Color32 = Color32::from_rgb(0, 220, 130);     // Mint Green
    pub const ACCENT_GOLD: Color32 = Color32::from_rgb(255, 204, 0); // Target Yellow
    pub const ACCENT_BLUE: Color32 = Color32::from_rgb(60, 160, 255);
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 244, 250);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(140, 150, 170);
    pub const BORDER_COLOR: Color32 = Color32::from_rgb(42, 48, 64);
}

pub fn render_ui(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    task_manager: &mut TaskManager,
    sensitivity_engine: &mut SensitivityEngine,
    data_manager: &mut DataManager,
    audio_manager: &mut AudioManager,
    fps: u32,
    current_cm_360: f64,
) {
    apply_custom_style(ctx);

    // 1. Sleek Floating In-Game HUD (During Gameplay)
    if !ui_state.is_overlay_open {
        render_ingame_hud(ctx, task_manager, fps, current_cm_360);
        return;
    }

    // 2. Modern Fullscreen App Dashboard
    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(theme::BG_OVERLAY))
        .show(ctx, |ui| {
            let available_rect = ui.available_rect_before_wrap();
            let content_rect = Rect::from_center_size(
                available_rect.center(),
                vec2(
                    (available_rect.width() - 80.0).clamp(800.0, 1080.0),
                    (available_rect.height() - 60.0).clamp(560.0, 700.0),
                ),
            );

            ui.allocate_ui_at_rect(content_rect, |ui| {
                Frame::NONE
                    .fill(theme::PANEL_BG)
                    .corner_radius(CornerRadius::same(14))
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
                    .show(ui, |ui| {
                        ui.set_min_size(content_rect.size());

                        ui.vertical(|ui| {
                            render_top_navbar(ui, ui_state, task_manager, data_manager);

                            ui.add_space(4.0);
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add_space(12.0);
                                    match ui_state.current_screen {
                                        UiScreen::Dashboard => render_dashboard_view(
                                            ui,
                                            ui_state,
                                            task_manager,
                                            sensitivity_engine,
                                            data_manager,
                                        ),
                                        UiScreen::Tasks => {
                                            render_tasks_view(ui, ui_state, task_manager)
                                        }
                                        UiScreen::Calibration => {
                                            render_calibration_view(ui, sensitivity_engine)
                                        }
                                        UiScreen::Analytics => render_analytics_view(
                                            ui,
                                            sensitivity_engine,
                                            data_manager,
                                        ),
                                        UiScreen::Settings => render_settings_view(
                                            ui,
                                            ui_state,
                                            sensitivity_engine,
                                            data_manager,
                                            audio_manager,
                                        ),
                                        UiScreen::PlayingHud => {}
                                    }
                                    ui.add_space(20.0);
                                });
                        });
                    });
            });
        });
}

fn apply_custom_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = vec2(10.0, 8.0);
    style.spacing.button_padding = vec2(14.0, 8.0);

    style.visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(8);

    style.visuals.widgets.inactive.bg_fill = theme::CARD_BG;
    style.visuals.widgets.hovered.bg_fill = theme::CARD_BG_HOVER;
    style.visuals.widgets.active.bg_fill = theme::PRIMARY;

    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, theme::TEXT_PRIMARY);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::BLACK);

    ctx.set_style(style);
}

fn render_ingame_hud(
    ctx: &egui::Context,
    task_manager: &TaskManager,
    fps: u32,
    current_cm_360: f64,
) {
    // Top-Center Minimalist Floating Pill
    egui::Area::new(egui::Id::new("floating_hud"))
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 16.0))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(Color32::from_black_alpha(200))
                .corner_radius(CornerRadius::same(24))
                .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(40)))
                .inner_margin(Margin::symmetric(18, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(theme::PRIMARY, format!("⚡ {} FPS", fps));
                        ui.label(RichText::new("•").color(theme::TEXT_MUTED));
                        ui.colored_label(theme::ACCENT_GOLD, task_manager.current_task_name());
                        ui.label(RichText::new("•").color(theme::TEXT_MUTED));
                        ui.colored_label(theme::ACCENT_BLUE, format!("{:.1} cm/360°", current_cm_360));
                        ui.label(RichText::new("•").color(theme::TEXT_MUTED));
                        ui.label(RichText::new("[ESC] Menu").color(theme::TEXT_MUTED));
                    });
                });
        });

    // Big Center Countdown/Complete Banner
    match task_manager.state {
        TaskState::Countdown { remaining_secs } => {
            egui::Area::new(egui::Id::new("countdown_overlay"))
                .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let num = remaining_secs.ceil() as i32;
                        let text = if num > 0 {
                            format!("{}", num)
                        } else {
                            "GO!".into()
                        };
                        ui.label(
                            RichText::new(text)
                                .size(72.0)
                                .strong()
                                .color(theme::ACCENT_GOLD),
                        );
                        ui.label(
                            RichText::new(format!(
                                "GET READY FOR {}",
                                task_manager.current_task_name().to_uppercase()
                            ))
                            .size(16.0)
                            .color(theme::TEXT_PRIMARY),
                        );
                    });
                });
        }
        TaskState::Finished => {
            egui::Area::new(egui::Id::new("finished_overlay"))
                .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0))
                .show(ctx, |ui| {
                    Frame::NONE
                        .fill(Color32::from_black_alpha(220))
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(Margin::same(24))
                        .stroke(Stroke::new(1.5_f32, theme::PRIMARY))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("SESSION COMPLETED")
                                        .size(24.0)
                                        .strong()
                                        .color(theme::PRIMARY),
                                );
                                if let Some(m) = &task_manager.last_completed_metrics {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "Score: {}  •  Accuracy: {:.1}%  •  Avg RT: {:.0} ms",
                                            m.score, m.accuracy, m.avg_reaction_time_ms
                                        ))
                                        .size(16.0)
                                        .color(theme::TEXT_PRIMARY),
                                    );
                                }
                                ui.add_space(14.0);
                                ui.label(
                                    RichText::new("Press [ESC] for Full Analytics or [R] to Retry")
                                        .size(13.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        });
                });
        }
        _ => {}
    }
}

fn render_top_navbar(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    _task_manager: &TaskManager,
    data_manager: &DataManager,
) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("🎯").size(22.0));
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("AIM TRAINER")
                        .size(16.0)
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                );
                ui.label(
                    RichText::new("Perfect Sensitivity System")
                        .size(11.0)
                        .color(theme::PRIMARY),
                );
            });
        });

        ui.add_space(32.0);

        let tabs = [
            (UiScreen::Dashboard, "📊 Dashboard"),
            (UiScreen::Tasks, "🎯 Training Tasks"),
            (UiScreen::Calibration, "✨ Sensitivity & Games"),
            (UiScreen::Analytics, "📈 Performance Graphs"),
            (UiScreen::Settings, "⚙️ Settings"),
        ];

        for (screen, label) in tabs {
            let is_selected = ui_state.current_screen == screen;
            let text_color = if is_selected {
                theme::PRIMARY
            } else {
                theme::TEXT_MUTED
            };

            let btn = egui::Button::new(RichText::new(label).size(13.0).color(text_color))
                .fill(if is_selected {
                    theme::CARD_BG_HOVER
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(if is_selected {
                    Stroke::new(1.0_f32, theme::PRIMARY)
                } else {
                    Stroke::NONE
                });

            if ui.add(btn).clicked() {
                ui_state.current_screen = screen;
            }
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(16.0);

            let play_btn = egui::Button::new(
                RichText::new("▶ RESUME AIMING")
                    .size(13.0)
                    .strong()
                    .color(Color32::BLACK),
            )
            .fill(theme::PRIMARY)
            .corner_radius(CornerRadius::same(20))
            .min_size(vec2(130.0, 34.0));

            if ui.add(play_btn).clicked() {
                ui_state.is_overlay_open = false;
            }

            ui.add_space(16.0);
            if let Some(profile) = data_manager.profile_manager.get_active_profile() {
                ui.label(
                    RichText::new(format!("👤 {}", profile.name))
                        .size(13.0)
                        .color(theme::TEXT_PRIMARY),
                );
            }
        });
    });
}

fn render_dashboard_view(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    task_manager: &mut TaskManager,
    sensitivity_engine: &SensitivityEngine,
    _data_manager: &DataManager,
) {
    ui.add_space(8.0);
    let rec = sensitivity_engine.compute_final_recommendation();

    // 1. Hero Recommendation Banner (Card)
    Frame::NONE
        .fill(theme::CARD_BG)
        .corner_radius(CornerRadius::same(12))
        .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("CALCULATED OPTIMAL SENSITIVITY")
                            .size(12.0)
                            .color(theme::PRIMARY)
                            .strong(),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{:.1}", rec.optimal_cm_360))
                                .size(38.0)
                                .strong()
                                .color(theme::ACCENT_GOLD),
                        );
                        ui.label(RichText::new("cm / 360°").size(18.0).color(theme::TEXT_MUTED));
                    });
                    ui.label(
                        RichText::new(format!(
                            "Confidence Score: {:.0}%  •  Hardware DPI: {:.0}",
                            rec.confidence_score * 100.0,
                            sensitivity_engine.player_dpi
                        ))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                    );
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("🔍 View All Game Conversions").clicked() {
                        ui_state.current_screen = UiScreen::Calibration;
                    }
                });
            });
        });

    ui.add_space(16.0);

    // 2. Quick Game Conversions Preview Cards
    ui.label(
        RichText::new("TOP GAME CONVERSIONS")
            .size(12.0)
            .strong()
            .color(theme::TEXT_MUTED),
    );
    ui.horizontal(|ui| {
        let popular = [
            "Valorant",
            "Counter-Strike 2 / CS:GO",
            "Overwatch 2",
            "Apex Legends / Source Engine",
        ];
        for game_name in popular {
            if let Some((_, sens)) = rec.conversions.iter().find(|(name, _)| name == game_name) {
                let short_name = match game_name {
                    "Counter-Strike 2 / CS:GO" => "CS2",
                    "Apex Legends / Source Engine" => "Apex",
                    other => other,
                };
                Frame::NONE
                    .fill(theme::CARD_BG)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(12))
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
                    .show(ui, |ui| {
                        ui.set_width(170.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(short_name)
                                    .size(13.0)
                                    .strong()
                                    .color(theme::TEXT_PRIMARY),
                            );
                            ui.label(
                                RichText::new(format!("{:.3}", sens))
                                    .size(20.0)
                                    .strong()
                                    .color(theme::PRIMARY),
                            );
                        });
                    });
            }
        }
    });

    ui.add_space(20.0);

    // 3. One-Click Scenario Launch Grid
    ui.label(
        RichText::new("SELECT A TRAINING SCENARIO")
            .size(12.0)
            .strong()
            .color(theme::TEXT_MUTED),
    );
    ui.horizontal_wrapped(|ui| {
        let quick_tasks = [
            (
                TaskType::Flick,
                "⚡ Flick Shot",
                "Fast reflex flicks to static yellow spheres",
            ),
            (
                TaskType::Tracking,
                "🌊 Smooth Tracking",
                "Maintain continuous lock on moving target",
            ),
            (
                TaskType::Switching,
                "🔀 Target Switching",
                "Prioritize highlighted target and snap",
            ),
            (
                TaskType::MicroAdjust,
                "🔬 Micro-Adjust",
                "Head-sized precision corrections",
            ),
            (
                TaskType::Speed,
                "⏱️ Speed Test",
                "High tempo reaction test",
            ),
        ];

        for (t_type, title, desc) in quick_tasks {
            Frame::NONE
                .fill(theme::CARD_BG)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(14))
                .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
                .show(ui, |ui| {
                    ui.set_width(175.0);
                    ui.set_height(90.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(title)
                                .size(14.0)
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.label(RichText::new(desc).size(11.0).color(theme::TEXT_MUTED));
                        ui.add_space(4.0);
                        if ui.button("Play Now ▶").clicked() {
                            task_manager.set_task(t_type);
                            task_manager.start_task_with_countdown();
                            ui_state.is_overlay_open = false;
                        }
                    });
                });
        }
    });
}

fn render_tasks_view(ui: &mut egui::Ui, ui_state: &mut UiState, task_manager: &mut TaskManager) {
    ui.heading("Aim Training Scenarios");
    ui.label(
        RichText::new(
            "Each scenario exercises a distinct motor skill and trains your neuro-muscular adaptation.",
        )
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(12.0);

    let scenarios = [
        (
            TaskType::Flick,
            "⚡ Flick Shot",
            "Targets appear at random wide coordinates. Test initial flick velocity, precision landing, and deceleration control.",
            "30 Targets  •  2.0s Limit per target",
        ),
        (
            TaskType::Tracking,
            "🌊 Smooth Tracking",
            "A smooth oscillating yellow sphere travels across the room. Keep crosshair centered to measure continuous smoothness.",
            "30 Seconds Duration  •  Sine & Linear Movement",
        ),
        (
            TaskType::Switching,
            "🔀 Target Switching",
            "Multiple spheres are active simultaneously with one highlighted. Eliminate the target and rapidly snap to the next.",
            "25 Switches  •  Multi-target prioritization",
        ),
        (
            TaskType::MicroAdjust,
            "🔬 Micro-Adjustment",
            "Small head-sized targets spawn in tight angular proximity (1.5° - 5.0°) around your crosshair for tactical shooter precision.",
            "30 Targets  •  Fingertip & wrist precision",
        ),
        (
            TaskType::Speed,
            "⏱️ Speed vs Precision",
            "Escalating challenge with progressively shrinking lifetime windows. Discovers the speed ceiling where your accuracy degrades.",
            "30 Targets  •  Progressive tempo",
        ),
    ];

    for (t_type, title, desc, meta) in scenarios {
        Frame::NONE
            .fill(theme::CARD_BG)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(16))
            .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(title)
                                .size(16.0)
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.label(RichText::new(desc).size(13.0).color(theme::TEXT_PRIMARY));
                        ui.add_space(4.0);
                        ui.label(RichText::new(meta).size(11.0).color(theme::PRIMARY));
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new("START SCENARIO ▶")
                                .size(13.0)
                                .strong()
                                .color(Color32::BLACK),
                        )
                        .fill(theme::PRIMARY)
                        .min_size(vec2(140.0, 36.0));

                        if ui.add(btn).clicked() {
                            task_manager.set_task(t_type);
                            task_manager.start_task_with_countdown();
                            ui_state.is_overlay_open = false;
                        }
                    });
                });
            });
        ui.add_space(10.0);
    }
}

fn render_calibration_view(ui: &mut egui::Ui, sensitivity_engine: &SensitivityEngine) {
    let rec = sensitivity_engine.compute_final_recommendation();

    ui.heading("Sensitivity Recommendation & Conversions");
    ui.label(
        RichText::new(
            "Your empirical results converted directly to your native game sensitivities.",
        )
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(12.0);

    Frame::NONE
        .fill(theme::CARD_BG)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("UNIVERSAL SENSITIVITY")
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(format!("{:.2} cm/360°", rec.optimal_cm_360))
                            .size(30.0)
                            .strong()
                            .color(theme::ACCENT_GOLD),
                    );
                });
                ui.add_space(40.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("METHODOLOGY BREAKDOWN")
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(format!(
                            "• PSA Binary Search: {:.1} cm/360°",
                            rec.psa_result_cm_360.unwrap_or(rec.optimal_cm_360)
                        ))
                        .size(13.0)
                        .color(theme::TEXT_PRIMARY),
                    );
                    ui.label(
                        RichText::new(format!(
                            "• Statistical Curve Peak: {:.1} cm/360°",
                            rec.statistical_peak_cm_360.unwrap_or(rec.optimal_cm_360)
                        ))
                        .size(13.0)
                        .color(theme::TEXT_PRIMARY),
                    );
                });
            });
        });

    ui.add_space(16.0);
    ui.label(
        RichText::new("IN-GAME SENSITIVITY TABLE")
            .size(12.0)
            .strong()
            .color(theme::TEXT_MUTED),
    );

    Frame::NONE
        .fill(theme::CARD_BG)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
        .show(ui, |ui| {
            egui::Grid::new("conversions_table")
                .striped(true)
                .min_col_width(260.0)
                .spacing(vec2(20.0, 12.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("GAME TITLE")
                            .size(12.0)
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new("RECOMMENDED VALUE")
                            .size(12.0)
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.end_row();

                    for (game, sens) in &rec.conversions {
                        ui.label(RichText::new(game).size(14.0).color(theme::TEXT_PRIMARY));
                        ui.label(
                            RichText::new(format!("{:.4}", sens))
                                .size(16.0)
                                .strong()
                                .color(theme::PRIMARY),
                        );
                        ui.end_row();
                    }
                });
        });
}

fn render_analytics_view(
    ui: &mut egui::Ui,
    sensitivity_engine: &SensitivityEngine,
    data_manager: &DataManager,
) {
    ui.heading("Performance Analytics & Graphs");
    ui.label(
        RichText::new("Interactive visualizations powered by egui_plot.").color(theme::TEXT_MUTED),
    );
    ui.add_space(12.0);

    let samples: Vec<(f64, f64)> = sensitivity_engine
        .analyzer
        .samples
        .iter()
        .map(|s| (s.cm_360, s.composite_score))
        .collect();
    let opt_cm = sensitivity_engine.analyzer.find_optimal_cm_360();
    PerformanceGraphs::render_accuracy_curve(ui, &samples, opt_cm);

    ui.add_space(16.0);

    let psa_rounds: Vec<f64> = sensitivity_engine
        .psa
        .history
        .iter()
        .map(|r| r.tested_cm_360)
        .collect();
    PerformanceGraphs::render_psa_convergence(ui, &psa_rounds);

    ui.add_space(16.0);

    if let Some(profile_id) = &data_manager.profile_manager.active_profile_id {
        let history = data_manager.load_history(profile_id);
        if let Some(latest) = history.latest_session() {
            let rts: Vec<f32> = latest
                .metrics
                .points
                .iter()
                .map(|p| p.reaction_time_ms)
                .collect();
            PerformanceGraphs::render_reaction_time_plot(ui, &rts);
        }
    }
}

fn render_settings_view(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    sensitivity_engine: &mut SensitivityEngine,
    data_manager: &mut DataManager,
    audio_manager: &mut AudioManager,
) {
    ui.heading("Settings & Hardware Calibration");
    ui.add_space(12.0);

    // 1. Mouse Hardware DPI Configuration Card
    Frame::NONE
        .fill(theme::CARD_BG)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
        .show(ui, |ui| {
            ui.label(
                RichText::new("MOUSE HARDWARE DPI")
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            ui.label(
                RichText::new(
                    "Your mouse physical sensor DPI (e.g. 400, 800, 1600). Crucial for authentic cm/360° calculation.",
                )
                .size(12.0)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut ui_state.input_dpi_str).desired_width(120.0));
                if ui.button("Save DPI").clicked() {
                    if let Ok(val) = ui_state.input_dpi_str.trim().parse::<f64>() {
                        if val > 50.0 && val < 50000.0 {
                            sensitivity_engine.player_dpi = val;
                            if let Some(p) = data_manager.profile_manager.get_active_profile_mut() {
                                p.dpi = val;
                                let clone_p = p.clone();
                                let _ = data_manager.profile_manager.save_profile(&clone_p);
                            }
                        }
                    }
                }
            });
        });

    ui.add_space(14.0);

    // 2. Audio Settings Card
    Frame::NONE
        .fill(theme::CARD_BG)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
        .show(ui, |ui| {
            ui.label(
                RichText::new("PROCEDURAL AUDIO")
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            ui.checkbox(
                &mut audio_manager.is_enabled,
                "Enable In-Game Audio Feedback (Hit / Miss / Countdown)",
            );
            ui.add_space(6.0);
            ui.add(egui::Slider::new(&mut audio_manager.volume, 0.0..=1.0).text("Master Volume"));
        });

    ui.add_space(14.0);

    // 3. Profiles Management Card
    Frame::NONE
        .fill(theme::CARD_BG)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, theme::BORDER_COLOR))
        .show(ui, |ui| {
            ui.label(
                RichText::new("PLAYER PROFILES")
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut ui_state.new_profile_name)
                        .hint_text("New Profile Name")
                        .desired_width(160.0),
                );
                if ui.button("➕ Add Profile").clicked()
                    && !ui_state.new_profile_name.trim().is_empty()
                {
                    let _ = data_manager.profile_manager.create_profile(
                        ui_state.new_profile_name.trim(),
                        sensitivity_engine.player_dpi,
                        "valorant",
                    );
                    ui_state.new_profile_name.clear();
                }
            });

            ui.add_space(10.0);
            let profiles = data_manager.profile_manager.profiles.clone();
            for p in profiles {
                ui.horizontal(|ui| {
                    let is_active =
                        Some(&p.id) == data_manager.profile_manager.active_profile_id.as_ref();
                    if is_active {
                        ui.colored_label(theme::PRIMARY, "● [ACTIVE]");
                    }
                    ui.label(
                        RichText::new(format!("{} (DPI: {:.0})", p.name, p.dpi))
                            .color(theme::TEXT_PRIMARY),
                    );

                    if !is_active && ui.button("Select").clicked() {
                        data_manager.profile_manager.set_active_profile(&p.id);
                        sensitivity_engine.player_dpi = p.dpi;
                    }
                    if !is_active && ui.button("🗑 Delete").clicked() {
                        data_manager.profile_manager.delete_profile(&p.id);
                    }
                });
            }
        });
}
