use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricPoint {
    pub shot_number: u32,
    pub hit: bool,
    pub reaction_time_ms: f32,
    pub offset_from_center_m: f32,
    pub target_radius_m: f32,
    pub precision_percent: f32,
    pub mouse_distance_px: f32,
    pub peak_velocity_px_s: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AggregatedTaskMetrics {
    pub total_shots: u32,
    pub hits: u32,
    pub misses: u32,
    pub accuracy_pct: f32,
    pub avg_reaction_time_ms: f32,
    pub median_reaction_time_ms: f32,
    pub avg_offset_m: f32,
    pub total_score: u64,
    pub max_streak: u32,
    pub points: Vec<MetricPoint>,
    /// Tracking sessions: scored by time on target, not by shots. Missing from other
    /// scenarios and from tracking sessions saved before clicks stopped counting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<TrackingStats>,
}

/// How long the crosshair stayed on a tracking target.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TrackingStats {
    /// Share of the run spent on the target, 0–100.
    pub on_target_pct: f32,
    pub time_on_target_secs: f32,
    /// Longest unbroken stretch on the target.
    pub longest_on_target_secs: f32,
}

impl TrackingStats {
    /// Points for a tracking run: 100 per percent of the time on target.
    pub fn score(&self) -> u64 {
        (self.on_target_pct.max(0.0) * 100.0) as u64
    }
}

impl AggregatedTaskMetrics {
    /// Whether any hit has a measured offset from the target's centre.
    pub fn has_offsets(&self) -> bool {
        self.points.iter().any(|p| p.hit && p.target_radius_m > 0.0)
    }

    pub fn compute(points: Vec<MetricPoint>, total_score: u64, max_streak: u32) -> Self {
        let total_shots = points.len() as u32;
        let hits = points.iter().filter(|p| p.hit).count() as u32;
        let misses = total_shots.saturating_sub(hits);
        let accuracy_pct = if total_shots > 0 {
            (hits as f32 / total_shots as f32) * 100.0
        } else {
            0.0
        };

        let mut hit_rts: Vec<f32> = points
            .iter()
            .filter(|p| p.hit)
            .map(|p| p.reaction_time_ms)
            .collect();

        let avg_reaction_time_ms = if !hit_rts.is_empty() {
            hit_rts.iter().sum::<f32>() / hit_rts.len() as f32
        } else {
            0.0
        };

        let median_reaction_time_ms = if !hit_rts.is_empty() {
            hit_rts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            hit_rts[hit_rts.len() / 2]
        } else {
            0.0
        };

        // Hits with no measured offset (tracking clicks) carry radius 0; leave them out.
        let offsets: Vec<f32> = points
            .iter()
            .filter(|p| p.hit && p.target_radius_m > 0.0)
            .map(|p| p.offset_from_center_m)
            .collect();
        let avg_offset_m = if offsets.is_empty() {
            0.0
        } else {
            offsets.iter().sum::<f32>() / offsets.len() as f32
        };

        Self {
            total_shots,
            hits,
            misses,
            accuracy_pct,
            avg_reaction_time_ms,
            median_reaction_time_ms,
            avg_offset_m,
            total_score,
            max_streak,
            points,
            tracking: None,
        }
    }

    /// A tracking session: no shots, accuracy is the share of time on target.
    pub fn tracking(stats: TrackingStats) -> Self {
        Self {
            accuracy_pct: stats.on_target_pct,
            total_score: stats.score(),
            tracking: Some(stats),
            ..Self::default()
        }
    }
}

