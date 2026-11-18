//! App-wide settings kept between launches: the sound and display settings of the Settings
//! screen, and where the window was. They belong to the computer, not to a profile.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How SENSE fills the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    /// A normal, resizable window.
    #[default]
    Window,
    /// Exclusive fullscreen at the screen's own resolution.
    Fullscreen,
}

impl DisplayMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Window => Self::Fullscreen,
            Self::Fullscreen => Self::Window,
        }
    }
}

/// Where the window was in window mode, in physical pixels: its outer top-left corner and its
/// inner (drawable) size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl WindowPlacement {
    /// Sizes outside this are a broken file, not a window someone had.
    const SIZE_RANGE: std::ops::RangeInclusive<u32> = 200..=16384;

    fn is_plausible(&self) -> bool {
        Self::SIZE_RANGE.contains(&self.width) && Self::SIZE_RANGE.contains(&self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub sound_effects: bool,
    pub effects_volume: f32,
    pub music_volume: f32,
    pub display_mode: DisplayMode,
    /// The last window-mode placement; `None` until the window has been moved or resized.
    pub window: Option<WindowPlacement>,
}

impl Default for AppSettings {
    /// What a fresh install plays at (`AudioManager::new`), in a window.
    fn default() -> Self {
        Self {
            sound_effects: true,
            effects_volume: 0.35,
            music_volume: 0.5,
            display_mode: DisplayMode::Window,
            window: None,
        }
    }
}

impl AppSettings {
    /// The settings in `path`; the defaults when it's missing or unreadable, with volumes
    /// kept within 0..1 and an implausible window size dropped.
    pub fn load_from(path: &Path) -> Self {
        let s: Self = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            effects_volume: s.effects_volume.clamp(0.0, 1.0),
            music_volume: s.music_volume.clamp(0.0, 1.0),
            window: s.window.filter(WindowPlacement::is_plausible),
            ..s
        }
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)
    }
}
