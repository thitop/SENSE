//! Notices targets that just appeared, so the game can cue the ones the player can't see.

use glam::Vec3;

/// Remembers last frame's targets. A target counts as new when no target from last frame is
/// near it; moving targets (tracking) travel far less than `MATCH_DISTANCE` per frame.
#[derive(Default)]
pub struct SpawnWatcher {
    previous: Vec<Vec3>,
}

impl SpawnWatcher {
    /// Metres. Spawns sit at least a few degrees apart at ~9 m (≥ 0.2 m); a tracking target
    /// moves under 0.1 m per frame even at 30 FPS.
    pub const MATCH_DISTANCE: f32 = 0.15;

    /// Positions in `targets` that were not there last call.
    pub fn update(&mut self, targets: &[(Vec3, f32)]) -> Vec<Vec3> {
        let new = targets
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| self.previous.iter().all(|q| q.distance(*p) > Self::MATCH_DISTANCE))
            .collect();
        self.previous = targets.iter().map(|(p, _)| *p).collect();
        new
    }

    /// Forget the targets seen, so the next call reports every target as new.
    pub fn clear(&mut self) {
        self.previous.clear();
    }
}
