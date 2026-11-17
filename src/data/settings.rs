//! App-wide settings kept between launches: the sound settings of the Settings screen. They
//! belong to the computer, not to a profile.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub sound_effects: bool,
    pub effects_volume: f32,
    pub music_volume: f32,
}

impl Default for AppSettings {
    /// What a fresh install plays at (`AudioManager::new`).
    fn default() -> Self {
        Self { sound_effects: true, effects_volume: 0.35, music_volume: 0.5 }
    }
}

impl AppSettings {
    /// The settings in `path`; the defaults when it's missing or unreadable, with volumes
    /// kept within 0..1.
    pub fn load_from(path: &Path) -> Self {
        let s: Self = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            effects_volume: s.effects_volume.clamp(0.0, 1.0),
            music_volume: s.music_volume.clamp(0.0, 1.0),
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
