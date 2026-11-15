use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::calibration::CalibrationResult;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub id: String,
    pub name: String,
    pub dpi: f64,
    pub primary_game_id: String,
    pub current_cm_360: f64,
    pub created_at: DateTime<Utc>,
    pub last_played_at: DateTime<Utc>,
    /// Profiles saved before first-run setup existed load as already set up.
    #[serde(default = "default_onboarded")]
    pub onboarded: bool,
    #[serde(default)]
    pub last_calibration: Option<CalibrationResult>,
}

fn default_onboarded() -> bool {
    true
}

/// File-safe id derived from `name`, suffixed (`-2`, `-3`, …) so it never overwrites an existing profile.
pub fn unique_profile_id(name: &str, existing: &[PlayerProfile]) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let base = if base.is_empty() {
        format!("profile_{}", Utc::now().timestamp_millis())
    } else {
        base
    };
    let taken = |id: &str| existing.iter().any(|p| p.id == id);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("unbounded suffix search")
}

impl PlayerProfile {
    pub fn new(id: String, name: String, dpi: f64, primary_game_id: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            dpi: if dpi > 0.0 { dpi } else { 800.0 },
            primary_game_id,
            current_cm_360: 40.0, // Default 40 cm/360°
            created_at: now,
            last_played_at: now,
            onboarded: false,
            last_calibration: None,
        }
    }
}

pub struct ProfileManager {
    base_dir: PathBuf,
    pub profiles: Vec<PlayerProfile>,
    pub active_profile_id: Option<String>,
}

impl ProfileManager {
    /// Initialize storage at `%APPDATA%/AimTrainer/` on Windows, or fallback `./data/`
    pub fn new() -> Self {
        let base_dir = Self::resolve_storage_directory();
        if let Err(e) = fs::create_dir_all(&base_dir) {
            log::warn!("Could not create directory at {:?}: {:?}", base_dir, e);
        }

        let mut manager = Self {
            base_dir,
            profiles: Vec::new(),
            active_profile_id: None,
        };

        manager.load_all_profiles();

        // Ensure at least one default profile exists
        if manager.profiles.is_empty() {
            let default_profile = PlayerProfile::new(
                "default".into(),
                "Player".into(),
                800.0,
                "valorant".into(),
            );
            let _ = manager.save_profile(&default_profile);
            manager.profiles.push(default_profile);
            manager.active_profile_id = Some("default".into());
        } else if manager.active_profile_id.is_none() {
            manager.active_profile_id = Some(manager.profiles[0].id.clone());
        }

        manager
    }

    fn resolve_storage_directory() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(app_data) = std::env::var("APPDATA") {
                return PathBuf::from(app_data).join("AimTrainer");
            }
        }
        PathBuf::from("data").join("storage")
    }

    pub fn load_all_profiles(&mut self) {
        self.profiles.clear();
        let profiles_dir = self.base_dir.join("profiles");
        if !profiles_dir.exists() {
            return;
        }

        if let Ok(entries) = fs::read_dir(profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(contents) = fs::read_to_string(&path) {
                        if let Ok(profile) = serde_json::from_str::<PlayerProfile>(&contents) {
                            self.profiles.push(profile);
                        }
                    }
                }
            }
        }
    }

    pub fn save_profile(&self, profile: &PlayerProfile) -> std::io::Result<()> {
        let profiles_dir = self.base_dir.join("profiles");
        fs::create_dir_all(&profiles_dir)?;

        let file_path = profiles_dir.join(format!("{}.json", profile.id));
        let json_data = serde_json::to_string_pretty(profile)?;
        fs::write(file_path, json_data)
    }

    pub fn get_active_profile(&self) -> Option<&PlayerProfile> {
        let id = self.active_profile_id.as_ref()?;
        self.profiles.iter().find(|p| &p.id == id)
    }

    pub fn get_active_profile_mut(&mut self) -> Option<&mut PlayerProfile> {
        let id = self.active_profile_id.clone()?;
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn set_active_profile(&mut self, profile_id: &str) -> bool {
        if self.profiles.iter().any(|p| p.id == profile_id) {
            self.active_profile_id = Some(profile_id.to_string());
            true
        } else {
            false
        }
    }

    pub fn create_profile(&mut self, name: &str, dpi: f64, game_id: &str) -> PlayerProfile {
        let id = unique_profile_id(name, &self.profiles);
        let profile = PlayerProfile::new(id, name.to_string(), dpi, game_id.to_string());
        let _ = self.save_profile(&profile);
        self.profiles.push(profile.clone());
        self.active_profile_id = Some(profile.id.clone());
        profile
    }

    pub fn delete_profile(&mut self, profile_id: &str) -> bool {
        if self.profiles.len() <= 1 {
            return false; // Prevent deleting the last profile
        }

        let profiles_dir = self.base_dir.join("profiles");
        let file_path = profiles_dir.join(format!("{}.json", profile_id));
        let _ = fs::remove_file(file_path);

        self.profiles.retain(|p| p.id != profile_id);
        if self.active_profile_id.as_deref() == Some(profile_id) {
            self.active_profile_id = Some(self.profiles[0].id.clone());
        }
        true
    }
}

