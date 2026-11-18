//! Window mode and exclusive fullscreen: which video mode fullscreen uses, and whether a saved
//! window position still lands on a screen.

use winit::monitor::MonitorHandle;
use winit::window::Fullscreen;

use crate::data::WindowPlacement;

/// One video mode a monitor offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeInfo {
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: u32,
    pub bit_depth: u16,
}

/// The mode fullscreen should use: the monitor's own resolution at its highest refresh rate
/// (then colour depth). Without a mode at that resolution, the largest one. `None` when the
/// monitor lists no modes.
pub fn best_mode(native: (u32, u32), modes: &[ModeInfo]) -> Option<usize> {
    let native_modes = modes.iter().any(|m| (m.width, m.height) == native);
    modes
        .iter()
        .enumerate()
        .filter(|(_, m)| !native_modes || (m.width, m.height) == native)
        .max_by_key(|(_, m)| (m.width as u64 * m.height as u64, m.refresh_millihertz, m.bit_depth))
        .map(|(i, _)| i)
}

/// A monitor's area in physical pixels: top-left corner and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRect {
    fn contains(&self, x: i32, y: i32) -> bool {
        let (x, y) = (x as i64, y as i64);
        let (left, top) = (self.x as i64, self.y as i64);
        x >= left && y >= top && x < left + self.width as i64 && y < top + self.height as i64
    }
}

/// The point that has to be on a screen for the window to be grabbable: the middle of its
/// title bar.
fn grab_point(p: &WindowPlacement) -> (i32, i32) {
    const TITLE_BAR: i32 = 16;
    (p.x.saturating_add((p.width / 2) as i32), p.y.saturating_add(TITLE_BAR))
}

/// Which of `screens` the saved window is on, if any. A window whose screen was unplugged or
/// moved is `None`, and opens where Windows puts it instead.
pub fn screen_of(p: &WindowPlacement, screens: &[ScreenRect]) -> Option<usize> {
    let (x, y) = grab_point(p);
    screens.iter().position(|s| s.contains(x, y))
}

pub fn screen_rect(monitor: &MonitorHandle) -> ScreenRect {
    let pos = monitor.position();
    let size = monitor.size();
    ScreenRect { x: pos.x, y: pos.y, width: size.width, height: size.height }
}

/// Exclusive fullscreen on `monitor` at its best mode; borderless fullscreen on it if it
/// offers no modes.
pub fn fullscreen_on(monitor: MonitorHandle) -> Fullscreen {
    let modes: Vec<_> = monitor.video_modes().collect();
    let info: Vec<ModeInfo> = modes
        .iter()
        .map(|m| ModeInfo {
            width: m.size().width,
            height: m.size().height,
            refresh_millihertz: m.refresh_rate_millihertz(),
            bit_depth: m.bit_depth(),
        })
        .collect();
    let native = monitor.size();
    match best_mode((native.width, native.height), &info) {
        Some(i) => Fullscreen::Exclusive(modes[i].clone()),
        None => Fullscreen::Borderless(Some(monitor)),
    }
}
