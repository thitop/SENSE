mod engine;
mod game;
mod sensitivity;

use engine::{camera::Camera, input::InputManager, renderer::Renderer, timing::Timing};
use game::{
    hit_detection::Ray,
    tasks::{TaskManager, TaskState, TaskType},
};
use sensitivity::{SensitivityConverter, SensitivityEngine};
use std::sync::Arc;
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
    current_cm_360: f64,
    rad_per_count: f32,
    player_dpi: f64,
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

        let player_dpi = 800.0;
        let sensitivity_engine = SensitivityEngine::new(player_dpi);
        let current_cm_360 = sensitivity_engine.psa.current_test_cm_360;
        let rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(player_dpi, current_cm_360);

        let mut task_manager = TaskManager::new();
        task_manager.set_task(TaskType::Flick);
        task_manager.start_task_with_countdown();

        Self {
            renderer,
            camera,
            input,
            timing,
            task_manager,
            sensitivity_engine,
            current_cm_360,
            rad_per_count,
            player_dpi,
        }
    }

    fn update(&mut self) {
        let dt = self.timing.update();

        // 1. Process Raw Input deltas & record path
        let (dx, dy) = self.input.consume_mouse_delta();
        if dx != 0.0 || dy != 0.0 {
            self.sensitivity_engine.path_recorder.record_sample(dx, dy);
            self.camera.rotate(dx, dy, self.rad_per_count);
        }

        let crosshair_ray = Ray {
            origin: self.camera.position,
            direction: self.camera.forward(),
        };

        // 2. Handle Shooting / Hit Detection on Left Click
        if self.input.consume_left_click() && self.input.is_locked() {
            let hit = self.task_manager.on_click(crosshair_ray);

            // Finish shot trajectory analysis
            if let Some(path) = self.sensitivity_engine.path_recorder.end_shot() {
                log::info!("Shot trajectory classified as: {:?}", path.pattern);

                // Feed into PSA Calibrator
                let round_done = self.sensitivity_engine.psa.record_shot(path.pattern);
                if round_done {
                    log::info!(
                        "PSA Round #{} complete! Next test cm/360: {:.1} cm",
                        self.sensitivity_engine.psa.round_number,
                        self.sensitivity_engine.psa.current_test_cm_360
                    );
                    self.current_cm_360 = self.sensitivity_engine.psa.current_test_cm_360;
                    self.rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(
                        self.player_dpi,
                        self.current_cm_360,
                    );
                }

                // Feed into Statistical Analyzer
                let acc = if hit { 100.0 } else { 0.0 };
                let rt_ms = path.end_time.duration_since(path.start_time).as_millis() as f64;
                self.sensitivity_engine.analyzer.record_sample(
                    self.current_cm_360,
                    acc,
                    rt_ms,
                    &path,
                );
            }

            // Immediately prime recorder for the next shot
            self.sensitivity_engine.path_recorder.start_shot();
        }

        // 3. Update Task Manager state & targets
        self.task_manager.update(dt, crosshair_ray);

        // 4. Update Camera Uniform to GPU
        self.renderer.update_camera(&self.camera);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let targets = self.task_manager.get_render_targets();
        self.renderer.render(&targets)
    }

    fn toggle_cursor_lock(&mut self) {
        let new_state = !self.input.is_locked();
        self.input.set_cursor_locked(&self.renderer.window, new_state);
        log::info!("Cursor locked: {}", new_state);
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
                .with_title("Aim Trainer - Sensitivity Engine Running")
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
                state.input.add_mouse_delta(delta.0 as f32, delta.1 as f32);
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
                    state.toggle_cursor_lock();
                }
                KeyCode::Digit1 => {
                    state.task_manager.set_task(TaskType::Flick);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit2 => {
                    state.task_manager.set_task(TaskType::Tracking);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit3 => {
                    state.task_manager.set_task(TaskType::Switching);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit4 => {
                    state.task_manager.set_task(TaskType::MicroAdjust);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit5 => {
                    state.task_manager.set_task(TaskType::Speed);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::KeyR => {
                    let cur = state.task_manager.current_type;
                    state.task_manager.set_task(cur);
                    state.task_manager.start_task_with_countdown();
                }
                // Key C: Compute final sensitivity recommendation and log to console
                KeyCode::KeyC => {
                    let rec = state.sensitivity_engine.compute_final_recommendation();
                    println!("\n=======================================================");
                    println!("🎯 SENSITIVITY CALIBRATION RESULTS 🎯");
                    println!("Optimal cm/360° : {:.2} cm", rec.optimal_cm_360);
                    println!("Confidence      : {:.0}%", rec.confidence_score * 100.0);
                    println!("Game Conversions (at {:.0} DPI):", state.player_dpi);
                    for (game, sens) in &rec.conversions {
                        println!("  • {:<28} : {:.4}", game, sens);
                    }
                    println!("=======================================================\n");
                }
                _ => {}
            },
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: btn_state,
                ..
            } => {
                state.input.set_left_mouse(btn_state == ElementState::Pressed);
                if btn_state == ElementState::Pressed && !state.input.is_locked() {
                    state.toggle_cursor_lock();
                }
            }
            WindowEvent::RedrawRequested => {
                state.update();

                let fps = state.timing.fps();
                let task_name = state.task_manager.current_task_name();
                let cm = state.current_cm_360;
                let round = state.sensitivity_engine.psa.round_number;

                let title = match state.task_manager.state {
                    TaskState::Countdown { remaining_secs } => {
                        format!("Aim Trainer | Task: {} | Starting in {:.1}s... [Current: {:.1} cm/360]", task_name, remaining_secs, cm)
                    }
                    TaskState::Running => {
                        format!(
                            "Aim Trainer | FPS: {} | Task: {} | PSA Round: #{} ({:.1} cm/360) | Press C for Rec",
                            fps, task_name, round, cm
                        )
                    }
                    TaskState::Finished => {
                        format!("Aim Trainer | Task Finished | Press C for Sensitivity Recommendation | Press R to Retry")
                    }
                    TaskState::NotStarted => {
                        format!("Aim Trainer | Ready [Keys 1-5 to switch, C to view recommendation]")
                    }
                };

                state.renderer.window.set_title(&title);

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
