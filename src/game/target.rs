use glam::Vec3;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TargetState {
    Active,
    Hit { hit_time: Instant, duration_secs: f32 },
    Despawned,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: u64,
    pub position: Vec3,
    pub base_radius: f32,
    pub spawn_time: Instant,
    pub lifetime_secs: Option<f32>,
    pub velocity: Vec3,
    pub state: TargetState,
}

impl Target {
    pub fn new(id: u64, position: Vec3, radius: f32, lifetime_secs: Option<f32>) -> Self {
        Self {
            id,
            position,
            base_radius: radius,
            spawn_time: Instant::now(),
            lifetime_secs,
            velocity: Vec3::ZERO,
            state: TargetState::Active,
        }
    }

    pub fn with_velocity(mut self, vel: Vec3) -> Self {
        self.velocity = vel;
        self
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.state, TargetState::Active)
    }

    pub fn hit(&mut self) {
        if self.is_alive() {
            self.state = TargetState::Hit {
                hit_time: Instant::now(),
                duration_secs: 0.12, // Quick 120ms pop animation
            };
        }
    }

    pub fn update(&mut self, dt: f32) {
        let now = Instant::now();

        match self.state {
            TargetState::Active => {
                // Move target with velocity
                self.position += self.velocity * dt;

                // Check lifetime timeout
                if let Some(lifetime) = self.lifetime_secs {
                    if now.duration_since(self.spawn_time).as_secs_f32() >= lifetime {
                        self.state = TargetState::Despawned;
                    }
                }
            }
            TargetState::Hit { hit_time, duration_secs } => {
                if now.duration_since(hit_time).as_secs_f32() >= duration_secs {
                    self.state = TargetState::Despawned;
                }
            }
            TargetState::Despawned => {}
        }
    }

    /// Returns the current visual render scale and color alpha (for pop/shrink effect)
    pub fn current_render_radius(&self) -> Option<f32> {
        let now = Instant::now();
        match self.state {
            TargetState::Active => Some(self.base_radius),
            TargetState::Hit { hit_time, duration_secs } => {
                let progress = (now.duration_since(hit_time).as_secs_f32() / duration_secs).clamp(0.0, 1.0);
                // Pop slightly then shrink to zero: (1 + 0.3 * (1 - p)) * (1 - p)
                let scale = (1.0 - progress) * (1.0 + 0.3 * (1.0 - progress));
                Some(self.base_radius * scale.max(0.0))
            }
            TargetState::Despawned => None,
        }
    }
}
