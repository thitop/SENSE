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
}

impl AggregatedTaskMetrics {
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

        let avg_offset_m = if hits > 0 {
            points
                .iter()
                .filter(|p| p.hit)
                .map(|p| p.offset_from_center_m)
                .sum::<f32>()
                / hits as f32
        } else {
            0.0
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
        }
    }
}

