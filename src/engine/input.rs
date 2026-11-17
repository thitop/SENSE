//! Mouse input state. Movement arrives as winit `DeviceEvent::MouseMotion`, which on Windows
//! is raw `WM_INPUT` (no pointer acceleration). Don't call `RegisterRawInputDevices` for the
//! mouse here: it replaces winit's registration and motion events stop arriving.

use winit::dpi::PhysicalPosition;
use winit::window::{CursorGrabMode, Window};

pub struct InputManager {
    is_cursor_locked: bool,
    pending_mouse_dx: f32,
    pending_mouse_dy: f32,
    left_mouse_pressed: bool,
    left_mouse_clicked: bool,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            is_cursor_locked: false,
            pending_mouse_dx: 0.0,
            pending_mouse_dy: 0.0,
            left_mouse_pressed: false,
            left_mouse_clicked: false,
        }
    }

    /// Lock the cursor to the centre of the window and hide it for FPS aiming.
    /// While locked, the cursor can't drift over the title bar or out of the window, so every
    /// click lands in the game. winit re-applies the lock after resizes and focus changes.
    /// The UI must not show the cursor again while locked (see `GameState::render`).
    pub fn set_cursor_locked(&mut self, window: &Window, locked: bool) {
        self.is_cursor_locked = locked;
        if locked {
            // Locked pins the cursor where it is (on Windows), so park it in the centre first.
            // An existing lock would keep the cursor from moving, so release it before.
            // Where Locked isn't supported, confine it to the window instead.
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            let size = window.inner_size();
            let _ = window.set_cursor_position(PhysicalPosition::new(size.width / 2, size.height / 2));
            if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                let _ = window.set_cursor_grab(CursorGrabMode::Confined);
            }
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        }
        window.set_cursor_visible(!locked);
    }

    /// Move a locked cursor back to the window centre, e.g. after the window was resized or
    /// moved, or got focus back.
    pub fn recenter_if_locked(&mut self, window: &Window) {
        if self.is_cursor_locked {
            self.set_cursor_locked(window, true);
        }
    }

    pub fn is_locked(&self) -> bool {
        self.is_cursor_locked
    }

    /// Add mouse delta from Raw Input or DeviceEvent::MouseMotion
    pub fn add_mouse_delta(&mut self, dx: f32, dy: f32) {
        if self.is_cursor_locked {
            self.pending_mouse_dx += dx;
            self.pending_mouse_dy += dy;
        }
    }

    pub fn set_left_mouse(&mut self, pressed: bool) {
        if pressed && !self.left_mouse_pressed {
            self.left_mouse_clicked = true;
        }
        self.left_mouse_pressed = pressed;
    }

    /// Take and reset the accumulated mouse movement deltas for this frame
    pub fn consume_mouse_delta(&mut self) -> (f32, f32) {
        let delta = (self.pending_mouse_dx, self.pending_mouse_dy);
        self.pending_mouse_dx = 0.0;
        self.pending_mouse_dy = 0.0;
        delta
    }

    /// Check if a mouse click happened this frame and reset click state
    pub fn consume_left_click(&mut self) -> bool {
        let clicked = self.left_mouse_clicked;
        self.left_mouse_clicked = false;
        clicked
    }
}
