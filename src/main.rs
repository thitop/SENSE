mod engine;
mod game;

use engine::{camera::Camera, input::InputManager, renderer::Renderer, timing::Timing};
use game::{hit_detection::Ray, TargetManager};
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
    target_manager: TargetManager,
    rad_per_count: f32,
    target_spawn_timer: f32,
}

impl GameState {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let renderer = Renderer::new(window.clone()).await;
        let aspect_ratio = size.width as f32 / size.height.max(1) as f32;
        let camera = Camera::new(aspect_ratio);
        let mut input = InputManager::new();
        input.register_raw_input(&window);

        // Lock mouse immediately into FPS aim mode
        input.set_cursor_locked(&window, true);

        let timing = Timing::new(1000.0); // 1000 Hz physics tick

        // Standard sensitivity: 800 DPI, 0.35 Valorant (approx 0.00038 rad per mouse count)
        let rad_per_count = 0.07_f32.to_radians() * 0.35;

        let mut target_manager = TargetManager::new();
        // Spawn 3 initial targets
        for _ in 0..3 {
            target_manager.spawn_random_target(0.35, None);
        }

        Self {
            renderer,
            camera,
            input,
            timing,
            target_manager,
            rad_per_count,
            target_spawn_timer: 0.0,
        }
    }

    fn update(&mut self) {
        let dt = self.timing.update();

        // 1. Process Raw Input deltas
        let (dx, dy) = self.input.consume_mouse_delta();
        if dx != 0.0 || dy != 0.0 {
            self.camera.rotate(dx, dy, self.rad_per_count);
        }

        // 2. Handle Shooting / Hit Detection on Left Click
        if self.input.consume_left_click() && self.input.is_locked() {
            let crosshair_ray = Ray {
                origin: self.camera.position,
                direction: self.camera.forward(),
            };

            let hit = self.target_manager.process_click(crosshair_ray);
            if let Some(h) = hit {
                log::info!("HIT target #{} at dist {:.2}m!", h.target_id, h.distance);
            } else {
                log::info!("MISS!");
            }
        }

        // 3. Update Targets & Spawn new ones to keep 3 active targets
        self.target_manager.update(dt);
        self.target_spawn_timer += dt;
        while self.target_manager.active_target_count() < 3 {
            self.target_manager.spawn_random_target(0.35, None);
        }

        // 4. Update Camera Uniform to GPU
        self.renderer.update_camera(&self.camera);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let targets = self.target_manager.get_render_targets();
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
                .with_title("Aim Trainer - Target System Active")
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
                let acc = state.target_manager.score_tracker.accuracy_percentage();
                let score = state.target_manager.score_tracker.total_score;
                let streak = state.target_manager.score_tracker.streak;
                let avg_ms = state.target_manager.score_tracker.average_reaction_time().as_millis();

                let title = format!(
                    "Aim Trainer | FPS: {} | Score: {} | Acc: {:.1}% | Streak: {} | Avg RT: {}ms (ESC to unlock)",
                    fps, score, acc, streak, avg_ms
                );
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
