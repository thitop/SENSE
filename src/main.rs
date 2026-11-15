mod data;
mod engine;
mod game;
mod sensitivity;
mod ui;

use data::{DataManager, MetricPoint, PlayerProfile, SessionHistory, SessionRecord};
use engine::{
    audio::AudioManager, camera::Camera, input::InputManager, renderer::Renderer, timing::Timing,
};
use game::{
    calibration::{
        esc_closes_menu, records_calibration_shot, round_change_message, CalibrationProgress,
        PlayMode, PlaySession,
    },
    hit_detection::Ray,
    tasks::{TaskManager, TaskState, TaskType},
};
use sensitivity::{SensitivityConverter, SensitivityEngine};
use std::sync::Arc;
use ui::{render_ui, theme, OnboardingDraft, Screen, UiAction, UiModel, UiState};
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

struct GameState {
    renderer: Renderer,
    camera: Camera,
    input: InputManager,
    timing: Timing,
    audio_manager: AudioManager,
    task_manager: TaskManager,
    sensitivity_engine: SensitivityEngine,
    data_manager: DataManager,
    ui_state: UiState,
    session: PlaySession,
    /// Cached session history of the active profile; refreshed whenever a session is recorded.
    history: SessionHistory,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    current_cm_360: f64,
    rad_per_count: f32,
    player_dpi: f64,
    current_session_points: Vec<MetricPoint>,
    shot_counter: u32,
    last_countdown_sec: i32,
}

impl GameState {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let renderer = Renderer::new(window.clone()).await;
        let aspect_ratio = size.width as f32 / size.height.max(1) as f32;
        let camera = Camera::new(aspect_ratio);
        let mut input = InputManager::new();
        input.register_raw_input(&window);
        // The app opens on the menu, so the cursor starts free.
        input.set_cursor_locked(&window, false);

        let timing = Timing::new(1000.0); // 1000 Hz physics tick
        let audio_manager = AudioManager::new();

        let data_manager = DataManager::new();
        let active_profile = data_manager.profile_manager.get_active_profile().cloned();
        let player_dpi = active_profile.as_ref().map(|p| p.dpi).unwrap_or(800.0);
        let current_cm_360 = active_profile
            .as_ref()
            .map(|p| p.current_cm_360)
            .unwrap_or(40.0);

        let sensitivity_engine = SensitivityEngine::new(player_dpi);
        let rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(player_dpi, current_cm_360);

        let mut task_manager = TaskManager::new();
        task_manager.set_task(TaskType::Flick);

        let history = data_manager.load_history(
            data_manager
                .profile_manager
                .active_profile_id
                .as_deref()
                .unwrap_or("default"),
        );
        let egui_ctx = egui::Context::default();
        theme::install(&egui_ctx);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        Self {
            renderer,
            camera,
            input,
            timing,
            audio_manager,
            task_manager,
            sensitivity_engine,
            data_manager,
            ui_state: UiState::new(),
            session: PlaySession {
                mode: PlayMode::Practice,
                paused: false,
            },
            history,
            egui_ctx,
            egui_state,
            current_cm_360,
            rad_per_count,
            player_dpi,
            current_session_points: Vec::new(),
            shot_counter: 0,
            last_countdown_sec: 4,
        }
    }

    // ---------- small helpers ----------

    fn now(&self) -> f64 {
        self.egui_ctx.input(|i| i.time)
    }

    fn profile_cm_360(&self) -> f64 {
        self.data_manager
            .profile_manager
            .get_active_profile()
            .map(|p| p.current_cm_360)
            .unwrap_or(40.0)
    }

    /// Mutate the active profile and persist it.
    fn update_profile(&mut self, f: impl FnOnce(&mut PlayerProfile)) {
        let pm = &mut self.data_manager.profile_manager;
        if let Some(p) = pm.get_active_profile_mut() {
            f(p);
            let copy = p.clone();
            let _ = pm.save_profile(&copy);
        }
    }

    fn set_cm_360(&mut self, cm: f64) {
        self.current_cm_360 = cm;
        self.rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(self.player_dpi, cm);
    }

    fn set_dpi(&mut self, dpi: f64) {
        self.player_dpi = dpi;
        self.sensitivity_engine.player_dpi = dpi;
        self.set_cm_360(self.current_cm_360);
    }

    fn lock_cursor(&mut self, locked: bool) {
        self.input.set_cursor_locked(&self.renderer.window, locked);
    }

    fn open_menu(&mut self, screen: Screen) {
        self.ui_state.menu_open = true;
        self.ui_state.screen = screen;
        self.lock_cursor(false);
    }

    fn close_menu(&mut self) {
        self.ui_state.menu_open = false;
        self.lock_cursor(true);
    }

    /// (Re)start `task` with a countdown, keeping the shots collected so far.
    fn restart_countdown(&mut self, task: TaskType) {
        self.last_countdown_sec = 4;
        self.task_manager.set_task(task);
        self.task_manager.start_task_with_countdown();
        self.sensitivity_engine.path_recorder.start_shot();
    }

    /// Start a fresh run of `task` in `mode`.
    fn start_run(&mut self, mode: PlayMode, task: TaskType) {
        self.session = PlaySession { mode, paused: false };
        self.current_session_points.clear();
        self.shot_counter = 0;
        self.restart_countdown(task);
        self.close_menu();
    }

    fn reload_history(&mut self) {
        let id = self
            .data_manager
            .profile_manager
            .active_profile_id
            .clone()
            .unwrap_or_else(|| "default".into());
        self.history = self.data_manager.load_history(&id);
    }

    /// Re-read everything that depends on the active profile.
    fn after_profile_switch(&mut self) {
        if let Some(p) = self.data_manager.profile_manager.get_active_profile().cloned() {
            self.set_dpi(p.dpi);
            self.set_cm_360(p.current_cm_360);
        }
        self.reload_history();
        self.ui_state.dpi_input = None;
        self.ui_state.dpi_error = None;
        self.ui_state.selected_session = None;
        self.ui_state.onboarding = None; // re-created next frame if the new profile needs setup
    }

    fn record_session(&mut self, task_type: String, tested_cm_360: f64, score: u64, duration_secs: f32) {
        let record = SessionRecord {
            session_id: format!("sess_{}", chrono::Utc::now().timestamp_millis()),
            task_type,
            timestamp: chrono::Utc::now(),
            duration_secs,
            tested_cm_360,
            tested_dpi: self.player_dpi,
            metrics: data::AggregatedTaskMetrics::compute(
                std::mem::take(&mut self.current_session_points),
                score,
                0,
            ),
        };
        let _ = self.data_manager.record_session(record);
        self.reload_history();
    }

    fn finish_calibration(&mut self) {
        let result = self
            .sensitivity_engine
            .build_calibration_result(chrono::Utc::now());
        let metrics = self.task_manager.current_metrics();
        self.record_session("Calibration".into(), result.cm_360, metrics.score, metrics.duration_secs);
        self.update_profile(|p| p.last_calibration = Some(result));
        self.audio_manager.play_complete();

        self.session = PlaySession {
            mode: PlayMode::Practice,
            paused: false,
        };
        self.task_manager.set_task(TaskType::Flick);
        self.set_cm_360(self.profile_cm_360());
        self.ui_state.show_all_games = false;
        self.open_menu(Screen::Sensitivity);
    }

    fn quit_calibration(&mut self) {
        self.sensitivity_engine.reset_calibration();
        self.session = PlaySession {
            mode: PlayMode::Practice,
            paused: false,
        };
        self.current_session_points.clear();
        self.task_manager.set_task(TaskType::Flick);
        self.set_cm_360(self.profile_cm_360());
        self.open_menu(Screen::Home);
    }

    fn resume(&mut self) {
        if self.session.mode == PlayMode::Calibration {
            self.session.paused = false;
            self.restart_countdown(TaskType::Calibration);
        }
        self.close_menu();
    }

    fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::Navigate(screen) => self.open_menu(screen),
            UiAction::StartCalibration => {
                self.sensitivity_engine.reset_calibration();
                self.set_cm_360(self.sensitivity_engine.psa.current_test_cm_360);
                self.start_run(PlayMode::Calibration, TaskType::Calibration);
            }
            UiAction::QuitCalibration => self.quit_calibration(),
            UiAction::StartPractice(task) => {
                self.set_cm_360(self.profile_cm_360());
                self.start_run(PlayMode::Practice, task);
            }
            UiAction::Retry => {
                if self.session.mode == PlayMode::Practice {
                    let task = self.task_manager.current_type;
                    self.start_run(PlayMode::Practice, task);
                }
            }
            UiAction::Resume => self.resume(),
            UiAction::ApplySensitivity(cm) => {
                self.update_profile(|p| p.current_cm_360 = cm);
                self.set_cm_360(cm);
            }
            UiAction::CompleteOnboarding { dpi, game_id, cm_360 } => {
                self.update_profile(|p| {
                    p.dpi = dpi;
                    p.primary_game_id = game_id;
                    p.current_cm_360 = cm_360;
                    p.onboarded = true;
                });
                self.set_dpi(dpi);
                self.set_cm_360(cm_360);
                self.ui_state.onboarding = None;
                self.ui_state.dpi_input = None;
                self.ui_state.screen = Screen::Home;
            }
            UiAction::RerunSetup => {
                if let Some(p) = self.data_manager.profile_manager.get_active_profile() {
                    self.ui_state.onboarding = Some(OnboardingDraft::from_profile(p));
                }
            }
            UiAction::SaveDpi(dpi) => {
                self.update_profile(|p| p.dpi = dpi);
                self.set_dpi(dpi);
            }
            UiAction::SetMainGame(id) => self.update_profile(|p| p.primary_game_id = id),
            UiAction::SetAudio { enabled, volume } => {
                self.audio_manager.is_enabled = enabled;
                self.audio_manager.volume = volume;
            }
            UiAction::CreateProfile(name) => {
                let game = self
                    .data_manager
                    .profile_manager
                    .get_active_profile()
                    .map(|p| p.primary_game_id.clone())
                    .unwrap_or_else(|| "valorant".into());
                // create_profile makes the new profile active; it starts with first-run setup.
                self.data_manager
                    .profile_manager
                    .create_profile(&name, self.player_dpi, &game);
                self.after_profile_switch();
            }
            UiAction::SelectProfile(id) => {
                if self.data_manager.profile_manager.set_active_profile(&id) {
                    self.after_profile_switch();
                }
            }
            UiAction::DeleteProfile(id) => {
                let was_active =
                    self.data_manager.profile_manager.active_profile_id.as_deref() == Some(id.as_str());
                if self.data_manager.profile_manager.delete_profile(&id) && was_active {
                    self.after_profile_switch();
                }
            }
        }
    }

    // ---------- frame ----------

    fn update(&mut self) {
        let dt = self.timing.update();
        let playing = !self.ui_state.menu_open && !self.session.paused;

        // 1. Raw mouse deltas (only while the cursor is captured)
        if self.input.is_locked() {
            let (dx, dy) = self.input.consume_mouse_delta();
            if dx != 0.0 || dy != 0.0 {
                self.sensitivity_engine.path_recorder.record_sample(dx, dy);
                self.camera.rotate(dx, dy, self.rad_per_count);
            }
        }

        let crosshair_ray = Ray {
            origin: self.camera.position,
            direction: self.camera.forward(),
        };

        // 2. Shooting — only once targets are live; clicks during the countdown are ignored.
        if self.input.consume_left_click()
            && self.input.is_locked()
            && playing
            && self.task_manager.state == TaskState::Running
        {
            self.on_shot(crosshair_ray);
        }

        // Countdown beeps
        if let TaskState::Countdown { remaining_secs } = self.task_manager.state {
            let current_sec = remaining_secs.ceil() as i32;
            if current_sec < self.last_countdown_sec && current_sec > 0 {
                self.audio_manager.play_countdown_beep(false);
                self.last_countdown_sec = current_sec;
            }
        }

        // 3. Targets advance only while actually playing
        if playing {
            let was_running = self.task_manager.state == TaskState::Running;
            let was_counting = matches!(self.task_manager.state, TaskState::Countdown { .. });
            self.task_manager.update(dt, crosshair_ray);

            // The first shot's timer and path start when targets appear, not when the countdown does.
            if was_counting && self.task_manager.state == TaskState::Running {
                self.sensitivity_engine.path_recorder.start_shot();
            }

            if was_running
                && self.task_manager.state == TaskState::Finished
                && self.task_manager.current_type != TaskType::Calibration
            {
                self.audio_manager.play_complete();
                if let Some(m) = self.task_manager.last_completed_metrics.clone() {
                    self.record_session(m.task_name, self.current_cm_360, m.score, m.duration_secs);
                }
                // Free the cursor so the result card's buttons can be clicked.
                self.lock_cursor(false);
            }
        }

        // 4. Camera uniform to GPU
        self.renderer.update_camera(&self.camera);
    }

    fn on_shot(&mut self, ray: Ray) {
        let hit = self.task_manager.on_click(ray);
        self.shot_counter += 1;
        if hit {
            self.audio_manager.play_hit();
        } else {
            self.audio_manager.play_miss();
        }

        if let Some(path) = self.sensitivity_engine.path_recorder.end_shot() {
            let rt_ms = path.end_time.duration_since(path.start_time).as_millis() as f32;
            self.current_session_points.push(MetricPoint {
                shot_number: self.shot_counter,
                hit,
                reaction_time_ms: rt_ms,
                offset_from_center_m: 0.1,
                target_radius_m: 0.35,
                precision_percent: if hit { 85.0 } else { 0.0 },
                mouse_distance_px: path.total_mouse_distance,
                peak_velocity_px_s: path.peak_velocity,
            });

            if records_calibration_shot(self.session, self.task_manager.state) {
                // Attribute the shot to the sensitivity it was taken at, before PSA may move it.
                let acc = if hit { 100.0 } else { 0.0 };
                self.sensitivity_engine
                    .analyzer
                    .record_sample(self.current_cm_360, acc, rt_ms as f64, &path);

                if self.sensitivity_engine.psa.record_shot(path.pattern) {
                    if self.sensitivity_engine.psa.is_completed {
                        self.finish_calibration();
                        return;
                    }
                    self.set_cm_360(self.sensitivity_engine.psa.current_test_cm_360);
                    if let Some(last) = self.sensitivity_engine.psa.history.last() {
                        let now = self.now();
                        self.ui_state.show_toast(round_change_message(last.conclusion), now);
                    }
                }
            }
        }

        self.sensitivity_engine.path_recorder.start_shot();
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let targets = self.task_manager.get_render_targets();

        let raw_input = self.egui_state.take_egui_input(&self.renderer.window);
        let profile = self
            .data_manager
            .profile_manager
            .get_active_profile()
            .cloned()
            .unwrap_or_else(|| {
                PlayerProfile::new("default".into(), "Player".into(), self.player_dpi, "valorant".into())
            });
        let model = UiModel {
            profile: &profile,
            profiles: &self.data_manager.profile_manager.profiles,
            games: self.sensitivity_engine.db.list(),
            history: &self.history,
            session: self.session,
            task_state: self.task_manager.state,
            task_name: self.task_manager.current_task_name(),
            last_metrics: self.task_manager.last_completed_metrics.as_ref(),
            calibration: (self.session.mode == PlayMode::Calibration)
                .then(|| CalibrationProgress::from_psa(&self.sensitivity_engine.psa)),
            current_cm_360: self.current_cm_360,
            audio_enabled: self.audio_manager.is_enabled,
            volume: self.audio_manager.volume,
            fps: self.timing.fps(),
        };
        let ui_state = &mut self.ui_state;
        let mut actions = Vec::new();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            actions = render_ui(ctx, ui_state, &model);
        });
        drop(model);
        for action in actions {
            self.apply_action(action);
        }

        self.egui_state
            .handle_platform_output(&self.renderer.window, full_output.platform_output);

        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        for (id, delta) in &full_output.textures_delta.set {
            self.renderer
                .egui_renderer
                .update_texture(&self.renderer.device, &self.renderer.queue, *id, delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.renderer.size.width, self.renderer.size.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        let result = self
            .renderer
            .render(&targets, &clipped_primitives, &screen_descriptor);

        for id in &full_output.textures_delta.free {
            self.renderer.egui_renderer.free_texture(id);
        }

        result
    }

    // ---------- keyboard ----------

    fn on_escape(&mut self) {
        let in_calibration = self.session.mode == PlayMode::Calibration;
        if in_calibration && !self.ui_state.menu_open {
            if self.session.paused {
                self.resume();
            } else {
                self.session.paused = true;
                self.lock_cursor(false);
            }
            return;
        }
        if self.ui_state.menu_open {
            // Back to a run in progress only; with nothing to play the menu stays open.
            if esc_closes_menu(self.task_manager.state, self.ui_state.onboarding.is_some()) {
                self.close_menu();
            }
        } else if self.task_manager.state == TaskState::Finished {
            self.open_menu(Screen::Practice);
        } else {
            let screen = self.ui_state.screen;
            self.open_menu(screen);
        }
    }

    fn on_key(&mut self, key: KeyCode) {
        let practicing = !self.ui_state.menu_open && self.session.mode == PlayMode::Practice;
        let practice_task = match key {
            KeyCode::Digit1 => Some(TaskType::Flick),
            KeyCode::Digit2 => Some(TaskType::Tracking),
            KeyCode::Digit3 => Some(TaskType::Switching),
            KeyCode::Digit4 => Some(TaskType::MicroAdjust),
            KeyCode::Digit5 => Some(TaskType::Speed),
            _ => None,
        };
        match key {
            KeyCode::Escape => self.on_escape(),
            _ if practicing && practice_task.is_some() => {
                self.apply_action(UiAction::StartPractice(practice_task.unwrap()));
            }
            KeyCode::KeyR if practicing => self.apply_action(UiAction::Retry),
            KeyCode::KeyC if practicing => self.open_menu(Screen::Sensitivity),
            _ => {}
        }
    }
}

#[derive(Default)]
struct App {
    state: Option<GameState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("Aim Trainer - Find Your Perfect Sensitivity")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
                .with_min_inner_size(winit::dpi::LogicalSize::new(1024, 640));

            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
            let state = pollster::block_on(GameState::new(window));
            self.state = Some(state);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(state) = self.state.as_mut() {
            if let DeviceEvent::MouseMotion { delta } = event {
                if state.input.is_locked() {
                    state.input.add_mouse_delta(delta.0 as f32, delta.1 as f32);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(s) if s.renderer.window.id() == window_id => s,
            _ => return,
        };

        let response = state.egui_state.on_window_event(&state.renderer.window, &event);
        if response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                state.input.set_cursor_locked(&state.renderer.window, false);
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.renderer.resize(physical_size);
                state.camera.update_aspect_ratio(physical_size.width, physical_size.height);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => state.on_key(key),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: btn_state,
                ..
            } => {
                state.input.set_left_mouse(btn_state == ElementState::Pressed);
                let can_capture = !state.ui_state.menu_open
                    && !state.session.paused
                    && state.task_manager.state != TaskState::Finished;
                if btn_state == ElementState::Pressed && !state.input.is_locked() && can_capture {
                    state.input.set_cursor_locked(&state.renderer.window, true);
                }
            }
            WindowEvent::RedrawRequested => {
                state.update();

                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => state.renderer.resize(state.renderer.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => log::error!("Render error: {:?}", e),
                }

                state.renderer.window.request_redraw();
            }
            _ => (),
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
