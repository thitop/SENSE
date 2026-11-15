mod engine;
mod game;

use engine::{camera::Camera, input::InputManager, renderer::Renderer, timing::Timing};
use game::{
    hit_detection::Ray,
    tasks::{TaskManager, TaskState, TaskType},
};
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
    rad_per_count: f32,
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

        // Standard sensitivity: 800 DPI, 0.35 Valorant (approx 0.00038 rad per mouse count)
        let rad_per_count = 0.07_f32.to_radians() * 0.35;

        let mut task_manager = TaskManager::new();
        task_manager.set_task(TaskType::Flick);
        task_manager.start_task_with_countdown();

        Self {
            renderer,
            camera,
            input,
            timing,
            task_manager,
            rad_per_count,
        }
    }

    fn update(&mut self) {
        let dt = self.timing.update();

        // 1. Process Raw Input deltas
        let (dx, dy) = self.input.consume_mouse_delta();
        if dx != 0.0 || dy != 0.0 {
            self.camera.rotate(dx, dy, self.rad_per_count);
        }

        let crosshair_ray = Ray {
            origin: self.camera.position,
            direction: self.camera.forward(),
        };

        // 2. Handle Shooting / Hit Detection on Left Click
        if self.input.consume_left_click() && self.input.is_locked() {
            let hit = self.task_manager.on_click(crosshair_ray);
            if hit {
                log::info!("Task Target Hit!");
            }
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
                .with_title("Aim Trainer - 5 Task Scenarios Ready")
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
                // Quick keybind shortcuts to test all 5 tasks (1..=5) & restart (R)
                KeyCode::Digit1 => {
                    log::info!("Switching to Flick Task");
                    state.task_manager.set_task(TaskType::Flick);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit2 => {
                    log::info!("Switching to Tracking Task");
                    state.task_manager.set_task(TaskType::Tracking);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit3 => {
                    log::info!("Switching to Switching Task");
                    state.task_manager.set_task(TaskType::Switching);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit4 => {
                    log::info!("Switching to MicroAdjust Task");
                    state.task_manager.set_task(TaskType::MicroAdjust);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::Digit5 => {
                    log::info!("Switching to Speed Task");
                    state.task_manager.set_task(TaskType::Speed);
                    state.task_manager.start_task_with_countdown();
                }
                KeyCode::KeyR => {
                    log::info!("Restarting current task");
                    let cur = state.task_manager.current_type;
                    state.task_manager.set_task(cur);
                    state.task_manager.start_task_with_countdown();
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

                let title = match state.task_manager.state {
                    TaskState::Countdown { remaining_secs } => {
                        format!("Aim Trainer | Task: {} | Starting in {:.1}s... [1-5 Switch Task, R Restart]", task_name, remaining_secs)
                    }
                    TaskState::Running => {
                        format!("Aim Trainer | FPS: {} | Task: {} | RUNNING [Keys 1-5 to switch, ESC to unlock]", fps, task_name)
                    }
                    TaskState::Finished => {
                        if let Some(metrics) = &state.task_manager.last_completed_metrics {
                            format!(
                                "Aim Trainer | {} COMPLETED! Score: {} | Acc: {:.1}% | Avg RT: {:.0}ms | Press R to Retry",
                                metrics.task_name, metrics.score, metrics.accuracy, metrics.avg_reaction_time_ms
                            )
                        } else {
                            format!("Aim Trainer | {} FINISHED | Press R to Retry", task_name)
                        }
                    }
                    TaskState::NotStarted => {
                        format!("Aim Trainer | Task: {} | Ready (Press 1-5 or R)", task_name)
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
