use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::metrics::AggregatedTaskMetrics;
use crate::habits::record::AimLog;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub task_type: String,
    pub timestamp: DateTime<Utc>,
    pub duration_secs: f32,
    pub tested_cm_360: f64,
    pub tested_dpi: f64,
    pub metrics: AggregatedTaskMetrics,
    /// What the aim did on each target; absent in sessions saved before aim habits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aim: Option<AimLog>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SessionHistory {
    pub profile_id: String,
    pub sessions: Vec<SessionRecord>,
}

impl SessionHistory {
    pub fn new(profile_id: String) -> Self {
        Self {
            profile_id,
            sessions: Vec::new(),
        }
    }

    pub fn add_session(&mut self, session: SessionRecord) {
        self.sessions.push(session);
    }

    pub fn sessions_for_task(&self, task_type: &str) -> Vec<&SessionRecord> {
        self.sessions
            .iter()
            .filter(|s| s.task_type.eq_ignore_ascii_case(task_type))
            .collect()
    }

    pub fn latest_session(&self) -> Option<&SessionRecord> {
        self.sessions.last()
    }
}

