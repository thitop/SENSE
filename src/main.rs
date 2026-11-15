mod engine;

use engine::{camera::Camera, input::InputManager, renderer::Renderer, timing::Timing};
use glam::Vec3;
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
    // Demo target positions (for testing 3D rendering & aim controls)
    targets: Vec<(Vec3, f32)>,
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

        // Lock mouse immediately into FPS aim mode
        input.set_cursor_locked(&window, true);

        let timing = Timing::new(1000.0); // 1000 Hz physics tick

        // Standard sensitivity: 800 DPI, 0.35 Valorant (approx 0.00038 rad per mouse count)
        // cm/360° approx 46.6 cm
        let rad_per_count = 0.07_f32.to_radians() * 0.35;

        // Place a few test targets (Yellow spheres at chest/head level)
        let targets = vec![
            (Vec3::new(0.0, 1.5, -8.0), 0.35),
            (Vec3::new(-3.0, 1.8, -10.0), 0.4),
            (Vec3::new(3.5, 1.2, -9.0), 0.3),
            (Vec3::new(-1.5, 2.2, -7.0), 0.25),
        ];

        Self {
            renderer,
            camera,
            input,
            timing,
            targets,
            rad_per_count,
        }
    }

    fn update(&mut self) {
        let _dt = self.timing.update();

        // 1. Process Raw Input deltas
        let (dx, dy) = self.input.consume_mouse_delta();
        if dx != 0.0 || dy != 0.0 {
            self.camera.rotate(dx, dy, self.rad_per_count);
        }

        // 2. Fixed Timestep ticks
        while self.timing.should_do_fixed_update() {
            // Future physics / continuous tracking calculation
        }

        // 3. Update Camera Uniform to GPU
        self.renderer.update_camera(&self.camera);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.renderer.render(&self.targets)
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
                .with_title("Aim Trainer - High FPS & Low Latency Engine (Core Engine)")
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
                // High-precision Raw mouse motion events delivered directly by OS
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
                    // Escape toggles cursor lock so user can move freely or close window
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
                    // Clicking back into window re-locks the cursor
                    state.toggle_cursor_lock();
                }
            }
            WindowEvent::RedrawRequested => {
                state.update();

                // Update FPS in title bar cleanly
                let fps = state.timing.fps();
                let title = format!("Aim Trainer - FPS: {} | Low Latency Engine (Press ESC to unlock cursor)", fps);
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
