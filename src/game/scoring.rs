use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct ShotRecord {
    pub target_id: u64,
    pub hit: bool,
    pub reaction_time: Duration,
    pub distance_from_center: f32, // Precision metric
    pub target_radius: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ScoreTracker {
    pub total_shots: u32,
    pub hits: u32,
    pub misses: u32,
    pub streak: u32,
    pub max_streak: u32,
    pub total_score: u64,
    pub shot_history: Vec<ShotRecord>,
}

impl ScoreTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn record_hit(
        &mut self,
        target_id: u64,
        reaction_time: Duration,
        offset_from_center: f32,
        target_radius: f32,
    ) {
        self.total_shots += 1;
        self.hits += 1;
        self.streak += 1;
        if self.streak > self.max_streak {
            self.max_streak = self.streak;
        }

        // Precision factor: 1.0 at dead center down to 0.5 at the sphere edge
        let precision_ratio = (1.0 - (offset_from_center / target_radius.max(0.01))).clamp(0.0, 1.0);
        let precision_multiplier = 0.5 + 0.5 * precision_ratio;

        // Speed factor: quicker reactions gain bonus points
        let reaction_secs = reaction_time.as_secs_f32();
        let speed_bonus = (1.5 - reaction_secs).clamp(0.2, 2.0);

        let base_pts = 100.0;
        let points = (base_pts * precision_multiplier * speed_bonus) as u64;
        self.total_score += points;

        self.shot_history.push(ShotRecord {
            target_id,
            hit: true,
            reaction_time,
            distance_from_center: offset_from_center,
            target_radius,
        });
    }

    pub fn record_miss(&mut self) {
        self.total_shots += 1;
        self.misses += 1;
        self.streak = 0;

        self.shot_history.push(ShotRecord {
            target_id: 0,
            hit: false,
            reaction_time: Duration::ZERO,
            distance_from_center: 0.0,
            target_radius: 0.0,
        });
    }

    pub fn accuracy_percentage(&self) -> f32 {
        if self.total_shots == 0 {
            0.0
        } else {
            (self.hits as f32 / self.total_shots as f32) * 100.0
        }
    }

    pub fn average_reaction_time(&self) -> Duration {
        let hit_records: Vec<_> = self.shot_history.iter().filter(|s| s.hit).collect();
        if hit_records.is_empty() {
            Duration::ZERO
        } else {
            let total_dur: Duration = hit_records.iter().map(|s| s.reaction_time).sum();
            total_dur / hit_records.len() as u32
        }
    }
}

