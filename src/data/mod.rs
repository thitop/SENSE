pub mod calibration;
pub mod history;
pub mod metrics;
pub mod profile;

use std::fs;
use std::path::PathBuf;

pub use calibration::{CalibrationResult, CalibrationRound};
pub use history::{SessionHistory, SessionRecord};
pub use metrics::{AggregatedTaskMetrics, MetricPoint};
pub use profile::{PlayerProfile, ProfileManager};

pub struct DataManager {
    base_dir: PathBuf,
    pub profile_manager: ProfileManager,
}

impl DataManager {
    pub fn new() -> Self {
        let profile_manager = ProfileManager::new();
        let base_dir = Self::resolve_storage_directory();

        Self {
            base_dir,
            profile_manager,
        }
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

    /// Load history for a given profile
    pub fn load_history(&self, profile_id: &str) -> SessionHistory {
        let history_file = self
            .base_dir
            .join("history")
            .join(format!("{}_history.json", profile_id));

        if history_file.exists() {
            if let Ok(contents) = fs::read_to_string(&history_file) {
                if let Ok(history) = serde_json::from_str::<SessionHistory>(&contents) {
                    return history;
                }
            }
        }

        SessionHistory::new(profile_id.to_string())
    }

    /// Save session history for a given profile
    pub fn save_history(&self, history: &SessionHistory) -> std::io::Result<()> {
        let history_dir = self.base_dir.join("history");
        fs::create_dir_all(&history_dir)?;

        let history_file = history_dir.join(format!("{}_history.json", history.profile_id));
        let json_data = serde_json::to_string_pretty(history)?;
        fs::write(history_file, json_data)
    }

    /// Automatically record a completed task session into profile history
    pub fn record_session(&mut self, session: SessionRecord) -> std::io::Result<()> {
        let profile_id = self
            .profile_manager
            .active_profile_id
            .clone()
            .unwrap_or_else(|| "default".into());

        let mut history = self.load_history(&profile_id);
        history.add_session(session);
        self.save_history(&history)?;

        // Update profile last_played_at
        if let Some(profile) = self.profile_manager.get_active_profile_mut() {
            profile.last_played_at = chrono::Utc::now();
            let p_clone = profile.clone();
            let _ = self.profile_manager.save_profile(&p_clone);
        }

        Ok(())
    }
}

