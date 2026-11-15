pub mod graphs;

use graphs::PerformanceGraphs;
use crate::data::DataManager;
use crate::engine::audio::AudioManager;
use crate::game::tasks::{TaskManager, TaskState, TaskType};
use crate::sensitivity::{FinalSensitivityRecommendation, SensitivityEngine};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiScreen {
    PlayingHud,
    MainMenu,
    TaskSelect,
    Results,
    Graphs,
    Settings,
    Profiles,
}

pub struct UiState {
    pub current_screen: UiScreen,
    pub is_overlay_open: bool,

    // Settings input fields
    pub input_dpi_str: String,
    pub selected_game_id: String,

    // Profile input field
    pub new_profile_name: String,
}

impl UiState {
    pub fn new(initial_dpi: f64) -> Self {
        Self {
            current_screen: UiScreen::PlayingHud,
            is_overlay_open: false,
            input_dpi_str: format!("{:.0}", initial_dpi),
            selected_game_id: "valorant".into(),
            new_profile_name: String::new(),
        }
    }

    pub fn toggle_overlay(&mut self) {
        self.is_overlay_open = !self.is_overlay_open;
        if self.is_overlay_open && self.current_screen == UiScreen::PlayingHud {
            self.current_screen = UiScreen::MainMenu;
        }
    }
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
    // 1. In-Game Minimal HUD (Always visible during gameplay)
    if !ui_state.is_overlay_open {
        egui::Area::new(egui::Id::new("hud_area"))
            .fixed_pos(egui::pos2(16.0, 16.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(0, 255, 128),
                        format!("FPS: {}", fps),
                    );
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 215, 0),
                        format!("Task: {}", task_manager.current_task_name()),
                    );
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 200, 255),
                        format!("Sensitivity: {:.1} cm/360°", current_cm_360),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new("[ESC] Menu / Cursor | [1-5] Quick Tasks | [C] Quick Rec")
                            .color(egui::Color32::GRAY)
                            .size(12.0),
                    );
                });

                match task_manager.state {
                    TaskState::Countdown { remaining_secs } => {
                        ui.add_space(8.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 100, 100),
                            format!("⏱️ STARTING IN {:.1}s...", remaining_secs),
                        );
                    }
                    TaskState::Finished => {
                        ui.add_space(8.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(0, 255, 128),
                            "✅ TASK COMPLETED! Press [ESC] for Results & Graphs, or [R] to Retry",
                        );
                    }
                    _ => {}
                }
            });

        return;
    }

    // 2. Full Menu / Overlay System when unlocked
    egui::Window::new("🎯 Aim Trainer — Perfect Sensitivity System")
        .default_width(740.0)
        .default_height(540.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .show(ctx, |ui| {
            // Top Navigation Bar
            ui.horizontal(|ui| {
                if ui.selectable_label(ui_state.current_screen == UiScreen::MainMenu, "🏠 Main Menu").clicked() {
                    ui_state.current_screen = UiScreen::MainMenu;
                }
                if ui.selectable_label(ui_state.current_screen == UiScreen::TaskSelect, "🎯 Tasks").clicked() {
                    ui_state.current_screen = UiScreen::TaskSelect;
                }
                if ui.selectable_label(ui_state.current_screen == UiScreen::Results, "📋 Results & Rec").clicked() {
                    ui_state.current_screen = UiScreen::Results;
                }
                if ui.selectable_label(ui_state.current_screen == UiScreen::Graphs, "📊 Performance Graphs").clicked() {
                    ui_state.current_screen = UiScreen::Graphs;
                }
                if ui.selectable_label(ui_state.current_screen == UiScreen::Profiles, "👤 Profiles").clicked() {
                    ui_state.current_screen = UiScreen::Profiles;
                }
                if ui.selectable_label(ui_state.current_screen == UiScreen::Settings, "⚙️ Settings").clicked() {
                    ui_state.current_screen = UiScreen::Settings;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("▶ Resume Game").clicked() {
                        ui_state.is_overlay_open = false;
                    }
                });
            });

            ui.separator();

            // Main Content Area based on active screen
            egui::ScrollArea::vertical().show(ui, |ui| {
                match ui_state.current_screen {
                    UiScreen::MainMenu => render_main_menu_screen(ui, ui_state, task_manager),
                    UiScreen::TaskSelect => render_task_select_screen(ui, ui_state, task_manager),
                    UiScreen::Results => render_results_screen(ui, sensitivity_engine),
                    UiScreen::Graphs => render_graphs_screen(ui, sensitivity_engine, data_manager),
                    UiScreen::Profiles => render_profiles_screen(ui, ui_state, data_manager, sensitivity_engine),
                    UiScreen::Settings => render_settings_screen(ui, ui_state, sensitivity_engine, data_manager, audio_manager),
                    UiScreen::PlayingHud => {}
                }
            });
        });
}

fn render_main_menu_screen(ui: &mut egui::Ui, ui_state: &mut UiState, task_manager: &mut TaskManager) {
    ui.heading("Welcome to Aim Trainer");
    ui.label("Find your biologically optimal mouse sensitivity and DPI through gameplay analysis.");

    ui.add_space(16.0);

    ui.horizontal(|ui| {
        if ui.button("🎯 Select & Play Task").clicked() {
            ui_state.current_screen = UiScreen::TaskSelect;
        }
        if ui.button("📋 View Sensitivity Recommendation").clicked() {
            ui_state.current_screen = UiScreen::Results;
        }
        if ui.button("📊 View Performance Graphs").clicked() {
            ui_state.current_screen = UiScreen::Graphs;
        }
    });

    ui.add_space(20.0);
    ui.separator();
    ui.heading("Quick Task Launch");

    let tasks = [
        ("1. Flick Shot", TaskType::Flick, "Fast reflex flicks to static yellow targets"),
        ("2. Smooth Tracking", TaskType::Tracking, "Continuous crosshair tracking on moving target"),
        ("3. Target Switching", TaskType::Switching, "Rapid target prioritization and snapping"),
        ("4. Micro-Adjustment", TaskType::MicroAdjust, "Small angle fingertip/wrist precision"),
        ("5. Speed vs Precision", TaskType::Speed, "Escalating tempo speed test"),
    ];

    for (name, t_type, desc) in tasks {
        ui.horizontal(|ui| {
            if ui.button(format!("Play {}", name)).clicked() {
                task_manager.set_task(t_type);
                task_manager.start_task_with_countdown();
                ui_state.is_overlay_open = false;
            }
            ui.label(desc);
        });
    }
}

fn render_task_select_screen(ui: &mut egui::Ui, ui_state: &mut UiState, task_manager: &mut TaskManager) {
    ui.heading("Select Training Scenario");

    let scenarios = [
        (
            TaskType::Flick,
            "Flick Shot",
            "Single yellow target appears randomly. Rapidly flick and click. Measures reaction time, flick accuracy, and trajectory overshoots.",
        ),
        (
            TaskType::Tracking,
            "Smooth Tracking",
            "Single target smoothly travels across the range with direction changes. Maintain crosshair contact. Measures tracking stability and smoothness.",
        ),
        (
            TaskType::Switching,
            "Target Switching",
            "Multiple targets spawn together. One active target is highlighted. Destroy it and instantly snap to the next highlighted target.",
        ),
        (
            TaskType::MicroAdjust,
            "Micro-Adjustment",
            "Small head-sized targets spawn tightly around your crosshair. Perfect for practicing micro-corrections for tactical shooters (Valorant/CS2).",
        ),
        (
            TaskType::Speed,
            "Speed vs Precision",
            "Target lifetime decreases progressively. Forces your nervous system to operate at maximum speed and tests breakdown thresholds.",
        ),
    ];

    for (t_type, title, description) in scenarios {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("▶ Start Task").clicked() {
                        task_manager.set_task(t_type);
                        task_manager.start_task_with_countdown();
                        ui_state.is_overlay_open = false;
                    }
                });
            });
            ui.label(description);
        });
        ui.add_space(8.0);
    }
}

fn render_results_screen(ui: &mut egui::Ui, sensitivity_engine: &SensitivityEngine) {
    let rec: FinalSensitivityRecommendation = sensitivity_engine.compute_final_recommendation();

    ui.heading("🎯 Sensitivity Calibration Output");
    ui.label("Combines PSA Binary Search (60%) and Statistical Curve Fitting (40%).");
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Optimal Universal Sensitivity:").size(18.0));
            ui.label(
                egui::RichText::new(format!("{:.2} cm/360°", rec.optimal_cm_360))
                    .size(22.0)
                    .color(egui::Color32::from_rgb(0, 255, 128))
                    .strong(),
            );
        });

        ui.label(format!("Confidence Score: {:.0}%", rec.confidence_score * 100.0));
        if let Some(psa_val) = rec.psa_result_cm_360 {
            ui.label(format!("• PSA Binary Search Result: {:.2} cm/360°", psa_val));
        }
        if let Some(stat_val) = rec.statistical_peak_cm_360 {
            ui.label(format!("• Statistical Curve Peak: {:.2} cm/360°", stat_val));
        }
    });

    ui.add_space(16.0);
    ui.heading(format!("Game Conversions (at {:.0} DPI)", sensitivity_engine.player_dpi));

    egui::Grid::new("game_conversions_grid")
        .striped(true)
        .min_col_width(200.0)
        .show(ui, |ui| {
            ui.strong("Game Title");
            ui.strong("Recommended In-Game Sensitivity");
            ui.end_row();

            for (game_name, sens_val) in &rec.conversions {
                ui.label(game_name);
                ui.colored_label(
                    egui::Color32::from_rgb(255, 215, 0),
                    format!("{:.4}", sens_val),
                );
                ui.end_row();
            }
        });
}

fn render_graphs_screen(
    ui: &mut egui::Ui,
    sensitivity_engine: &SensitivityEngine,
    data_manager: &DataManager,
) {
    ui.heading("📊 Performance Visualization");

    let samples: Vec<(f64, f64)> = sensitivity_engine
        .analyzer
        .samples
        .iter()
        .map(|s| (s.cm_360, s.composite_score))
        .collect();
    let opt_cm = sensitivity_engine.analyzer.find_optimal_cm_360();
    PerformanceGraphs::render_accuracy_curve(ui, &samples, opt_cm);

    ui.separator();

    let psa_rounds: Vec<f64> = sensitivity_engine
        .psa
        .history
        .iter()
        .map(|r| r.tested_cm_360)
        .collect();
    PerformanceGraphs::render_psa_convergence(ui, &psa_rounds);

    ui.separator();

    if let Some(profile_id) = &data_manager.profile_manager.active_profile_id {
        let history = data_manager.load_history(profile_id);
        if let Some(latest) = history.latest_session() {
            let rts: Vec<f32> = latest.metrics.points.iter().map(|p| p.reaction_time_ms).collect();
            PerformanceGraphs::render_reaction_time_plot(ui, &rts);
        }
    }
}

fn render_profiles_screen(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    data_manager: &mut DataManager,
    sensitivity_engine: &mut SensitivityEngine,
) {
    ui.heading("👤 Player Profile Management");

    let current_active = data_manager.profile_manager.active_profile_id.clone().unwrap_or_default();
    ui.label(format!("Active Profile: {}", current_active));

    ui.add_space(8.0);
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("New Profile Name:");
        ui.text_edit_singleline(&mut ui_state.new_profile_name);
        if ui.button("➕ Create Profile").clicked() && !ui_state.new_profile_name.trim().is_empty() {
            let created = data_manager.profile_manager.create_profile(
                ui_state.new_profile_name.trim(),
                sensitivity_engine.player_dpi,
                "valorant",
            );
            ui_state.new_profile_name.clear();
            log::info!("Created profile: {}", created.name);
        }
    });

    ui.add_space(16.0);
    ui.heading("Saved Profiles");

    let profiles = data_manager.profile_manager.profiles.clone();
    for p in profiles {
        ui.horizontal(|ui| {
            let is_active = Some(&p.id) == data_manager.profile_manager.active_profile_id.as_ref();
            if is_active {
                ui.colored_label(egui::Color32::from_rgb(0, 255, 128), "● [ACTIVE]");
            }

            ui.label(format!("{} (DPI: {:.0}, Sens: {:.1} cm/360°)", p.name, p.dpi, p.current_cm_360));

            if !is_active && ui.button("Select").clicked() {
                data_manager.profile_manager.set_active_profile(&p.id);
                sensitivity_engine.player_dpi = p.dpi;
            }

            if !is_active && ui.button("🗑 Delete").clicked() {
                data_manager.profile_manager.delete_profile(&p.id);
            }
        });
    }
}

fn render_settings_screen(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    sensitivity_engine: &mut SensitivityEngine,
    data_manager: &mut DataManager,
    audio_manager: &mut AudioManager,
) {
    ui.heading("⚙️ Settings & Audio Configuration");

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label("Mouse Hardware DPI:");
            ui.text_edit_singleline(&mut ui_state.input_dpi_str);
            if ui.button("Apply DPI").clicked() {
                if let Ok(val) = ui_state.input_dpi_str.trim().parse::<f64>() {
                    if val > 50.0 && val < 50000.0 {
                        sensitivity_engine.player_dpi = val;
                        if let Some(p) = data_manager.profile_manager.get_active_profile_mut() {
                            p.dpi = val;
                            let clone_p = p.clone();
                            let _ = data_manager.profile_manager.save_profile(&clone_p);
                        }
                        log::info!("DPI updated to: {:.0}", val);
                    }
                }
            }
        });
        ui.label(egui::RichText::new("Enter your mouse native DPI (e.g., 400, 800, 1600). Essential for true cm/360° calculation.").size(12.0).color(egui::Color32::GRAY));
    });

    ui.add_space(16.0);
    ui.group(|ui| {
        ui.heading("🔊 Sound Settings");
        ui.checkbox(&mut audio_manager.is_enabled, "Enable Sound Effects (Hit / Miss / Countdown)");
        ui.add(egui::Slider::new(&mut audio_manager.volume, 0.0..=1.0).text("Sound Volume"));
    });

    ui.add_space(16.0);
    ui.group(|ui| {
        ui.label("Display & Rendering:");
        ui.label("• Backend: Vulkan / DirectX 12 (wgpu high-performance)");
        ui.label("• Presentation: AutoNoVsync (lowest input-to-display latency)");
        ui.label("• Windows Raw Input: WM_INPUT Enabled (0% OS mouse acceleration)");
    });
}
