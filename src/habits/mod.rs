//! Aim habits: what the player's aim did on each target, recorded in degrees.

pub mod record;
pub mod recorder;
pub mod strokes;
pub mod analysis;
pub mod trust;
pub mod answers;
pub mod trend;

pub use answers::{AnswerCheck, AnswerVerdict};
pub use trust::{HabitReport, LeftOutNote};
