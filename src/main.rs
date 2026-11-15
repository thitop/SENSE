mod data;
mod engine;
mod game;
mod sensitivity;
mod ui;

use data::{DataManager, MetricPoint, SessionRecord};
use engine::{camera::Camera, input::InputManager, renderer::Renderer, timing::Timing};
use game::{
    hit_detection::Ray,
    tasks::{TaskManager, TaskState, TaskType},
};
use sensitivity::{SensitivityConverter, SensitivityEngine};
use std::sync::Arc;
use ui::{render_ui, UiState};
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
    task_manager: TaskManager,
    sensitivity_engine: SensitivityEngine,
    data_manager: DataManager,
    ui_state: UiState,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    current_cm_360: f64,
    rad_per_count: f32,
    player_dpi: f64,
    current_session_points: Vec<MetricPoint>,
    shot_counter: u32,
}

impl GameState {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let renderer = Renderer::new(window.clone()).await;
        let aspect_ratio = size.width as f32 / size.height.max(1) as f32;
        let camera = Camera::new(aspect_ratio);
        let mut input = InputManager::new();
        input.register_raw_input(&window);

        // Lock mouse into FPS aim mode
        input.set_cursor_locked(&window, true);

        let timing = Timing::new(1000.0); // 1000 Hz physics tick

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
        task_manager.start_task_with_countdown();

        let ui_state = UiState::new(player_dpi);
        let egui_ctx = egui::Context::default();
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
            task_manager,
            sensitivity_engine,
            data_manager,
            ui_state,
            egui_ctx,
            egui_state,
            current_cm_360,
            rad_per_count,
            player_dpi,
            current_session_points: Vec::new(),
            shot_counter: 0,
        }
    }

    fn update(&mut self) {
        let dt = self.timing.update();

        // 1. Process Raw Input deltas & record path (only during gameplay when cursor is locked)
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

        // 2. Handle Shooting / Hit Detection on Left Click (when overlay is not open)
        if self.input.consume_left_click() && self.input.is_locked() && !self.ui_state.is_overlay_open {
            let hit = self.task_manager.on_click(crosshair_ray);
            self.shot_counter += 1;

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

                let round_done = self.sensitivity_engine.psa.record_shot(path.pattern);
                if round_done {
                    self.current_cm_360 = self.sensitivity_engine.psa.current_test_cm_360;
                    self.rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(
                        self.player_dpi,
                        self.current_cm_360,
                    );
                }

                let acc = if hit { 100.0 } else { 0.0 };
                self.sensitivity_engine.analyzer.record_sample(
                    self.current_cm_360,
                    acc,
                    rt_ms as f64,
                    &path,
                );
            }

            self.sensitivity_engine.path_recorder.start_shot();
        }

        let was_running = self.task_manager.state == TaskState::Running;

        // 3. Update Task Manager state & targets
        self.task_manager.update(dt, crosshair_ray);

        // Auto-save history when task finishes
        if was_running && self.task_manager.state == TaskState::Finished {
            if let Some(metrics) = &self.task_manager.last_completed_metrics {
                let session_record = SessionRecord {
                    session_id: format!("sess_{}", chrono::Utc::now().timestamp_millis()),
                    task_type: metrics.task_name.clone(),
                    timestamp: chrono::Utc::now(),
                    duration_secs: metrics.duration_secs,
                    tested_cm_360: self.current_cm_360,
                    tested_dpi: self.player_dpi,
                    metrics: data::AggregatedTaskMetrics::compute(
                        std::mem::take(&mut self.current_session_points),
                        metrics.score,
                        0,
                    ),
                };

                let _ = self.data_manager.record_session(session_record);
            }
        }

        // 4. Sync player DPI if modified from UI settings
        self.player_dpi = self.sensitivity_engine.player_dpi;
        self.rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(self.player_dpi, self.current_cm_360);

        // 5. Update Camera Uniform to GPU
        self.renderer.update_camera(&self.camera);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let targets = self.task_manager.get_render_targets();

        // 1. Prepare egui frame
        let raw_input = self.egui_state.take_egui_input(&self.renderer.window);
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            render_ui(
                ctx,
                &mut self.ui_state,
                &mut self.task_manager,
                &mut self.sensitivity_engine,
                &mut self.data_manager,
                self.timing.fps(),
                self.current_cm_360,
            );
        });

        self.egui_state.handle_platform_output(&self.renderer.window, full_output.platform_output);

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

        // 2. Render 3D Scene + Crosshair + Egui primitives
        let result = self.renderer.render(&targets, &clipped_primitives, &screen_descriptor);

        for id in &full_output.textures_delta.free {
            self.renderer.egui_renderer.free_texture(id);
        }

        result
    }

    fn toggle_menu_overlay(&mut self) {
        self.ui_state.toggle_overlay();
        let should_lock = !self.ui_state.is_overlay_open;
        self.input.set_cursor_locked(&self.renderer.window, should_lock);
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
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

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

        // Allow egui to handle window events first (mouse clicks on UI buttons/plots)
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
                        ..
                    },
                ..
            } => match key {
                KeyCode::Escape => {
                    state.toggle_menu_overlay();
                }
                KeyCode::Digit1 if !state.ui_state.is_overlay_open => {
                    state.task_manager.set_task(TaskType::Flick);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit2 if !state.ui_state.is_overlay_open => {
                    state.task_manager.set_task(TaskType::Tracking);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit3 if !state.ui_state.is_overlay_open => {
                    state.task_manager.set_task(TaskType::Switching);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit4 if !state.ui_state.is_overlay_open => {
                    state.task_manager.set_task(TaskType::MicroAdjust);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit5 if !state.ui_state.is_overlay_open => {
                    state.task_manager.set_task(TaskType::Speed);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::KeyR if !state.ui_state.is_overlay_open => {
                    let cur = state.task_manager.current_type;
                    state.task_manager.set_task(cur);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::KeyC if !state.ui_state.is_overlay_open => {
                    state.ui_state.is_overlay_open = true;
                    state.ui_state.current_screen = ui::UiScreen::Results;
                    state.input.set_cursor_locked(&state.renderer.window, false);
                }
                _ => {}
            },
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: btn_state,
                ..
            } => {
                state.input.set_left_mouse(btn_state == ElementState::Pressed);
                if btn_state == ElementState::Pressed && !state.input.is_locked() && !state.ui_state.is_overlay_open {
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
