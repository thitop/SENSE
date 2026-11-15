use std::sync::atomic::{AtomicBool, Ordering};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetWindowRect};

static RAW_INPUT_REGISTERED: AtomicBool = AtomicBool::new(false);

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

    /// Register Windows Raw Input device for the mouse to bypass pointer precision/accel
    pub fn register_raw_input(&mut self, window: &Window) {
        #[cfg(target_os = "windows")]
        {
            if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
                return;
            }

            let hwnd = match window.window_handle() {
                Ok(handle) => match handle.as_raw() {
                    RawWindowHandle::Win32(w) => HWND(w.hwnd.get() as _),
                    _ => return,
                },
                Err(_) => return,
            };

            // Usage Page 1 (Generic Desktop), Usage 2 (Mouse)
            let device = RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x02,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            };

            unsafe {
                if RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32).is_ok() {
                    log::info!("Registered Windows Raw Input mouse device successfully");
                    RAW_INPUT_REGISTERED.store(true, Ordering::SeqCst);
                } else {
                    log::warn!("Failed to register Windows Raw Input mouse device");
                }
            }
        }
    }

    /// Lock mouse cursor to the center of window and hide it for true FPS aim control
    pub fn set_cursor_locked(&mut self, window: &Window, locked: bool) {
        self.is_cursor_locked = locked;
        window.set_cursor_visible(!locked);

        #[cfg(target_os = "windows")]
        {
            if locked {
                // Trap cursor inside window rectangle
                if let Ok(handle) = window.window_handle() {
                    if let RawWindowHandle::Win32(w) = handle.as_raw() {
                        let hwnd = HWND(w.hwnd.get() as _);
                        unsafe {
                            let mut rect = RECT::default();
                            if GetWindowRect(hwnd, &mut rect).is_ok() {
                                let _ = ClipCursor(Some(&rect));
                            }
                        }
                    }
                }
            } else {
                unsafe {
                    let _ = ClipCursor(None);
                }
            }
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
