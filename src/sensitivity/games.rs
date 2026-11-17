use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    /// Degrees rotated per mouse count per unit of in-game sensitivity.
    pub yaw: f64,
    /// Degrees per count added on top: turn rate = `yaw * sens + yaw_offset`. Zero for games
    /// whose turn rate is proportional to the slider; WARDOGS maps its slider linearly onto a
    /// range that doesn't start at zero.
    #[serde(default)]
    pub yaw_offset: f64,
    pub default_fov: f32,
    /// Scoped / ADS settings for a 0% monitor distance match: aim near the crosshair feels
    /// the same zoomed as unzoomed. They're relative to hipfire, so they hold for any cm/360.
    /// Empty where no trustworthy value is known.
    #[serde(default)]
    pub scoped: Vec<ScopedSetting>,
    /// Shown under `scoped` when its values rest on assumptions worth knowing.
    #[serde(default)]
    pub scoped_note: Option<String>,
    /// How the in-game slider maps to the value `yaw` multiplies.
    #[serde(default)]
    pub scale: SensScale,
}

/// How a game's sensitivity slider maps to the value its turn rate is proportional to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensScale {
    /// The slider value itself.
    #[default]
    Linear,
    /// `mid_value × 10^((slider − mid_slider) / slider_per_decade)`: PUBG's 1–100 slider.
    Exponential { mid_slider: f64, mid_value: f64, slider_per_decade: f64 },
}

impl SensScale {
    /// The value the turn rate is proportional to, at slider position `slider`.
    pub fn value(&self, slider: f64) -> f64 {
        match *self {
            SensScale::Linear => slider,
            SensScale::Exponential { mid_slider, mid_value, slider_per_decade } => {
                mid_value * 10f64.powf((slider - mid_slider) / slider_per_decade)
            }
        }
    }

    /// The slider position giving `value`; `None` when no position can (a non-positive value
    /// on an exponential slider).
    pub fn slider(&self, value: f64) -> Option<f64> {
        match *self {
            SensScale::Linear => Some(value),
            SensScale::Exponential { mid_slider, mid_value, slider_per_decade } => {
                (value > 0.0).then(|| mid_slider + slider_per_decade * (value / mid_value).log10())
            }
        }
    }
}

/// One scoped setting as the game's menu (or console) names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScopedSetting {
    pub setting: String,
    pub value: String,
    /// Which scope or hero it's for, when the value differs between them.
    #[serde(default)]
    pub scope: Option<String>,
}

impl GameProfile {
    /// Degrees per mouse count at in-game sensitivity `sens`.
    pub fn deg_per_count(&self, sens: f64) -> f64 {
        self.yaw * self.scale.value(sens) + self.yaw_offset
    }

    /// The in-game sensitivity that turns `deg_per_count`, or 0 when no setting reaches it.
    pub fn sens_for(&self, deg_per_count: f64) -> f64 {
        if self.yaw <= 0.0 {
            return 0.0;
        }
        let value = (deg_per_count - self.yaw_offset) / self.yaw;
        if value <= 0.0 {
            return 0.0;
        }
        self.scale.slider(value).unwrap_or(0.0)
    }
}

pub struct GameDatabase {
    pub games: Vec<GameProfile>,
}

impl GameDatabase {
    pub fn new() -> Self {
        let json_str = include_str!("../../data/games.json");
        let games: Vec<GameProfile> = serde_json::from_str(json_str).unwrap_or_else(|e| {
            log::error!("Failed to parse games.json: {:?}", e);
            vec![
                GameProfile {
                    id: "valorant".into(),
                    name: "Valorant".into(),
                    yaw: 0.07,
                    yaw_offset: 0.0,
                    default_fov: 103.0,
                    scoped: Vec::new(),
                    scoped_note: None,
                    scale: SensScale::Linear,
                },
                GameProfile {
                    id: "cs2".into(),
                    name: "Counter-Strike 2".into(),
                    yaw: 0.022,
                    yaw_offset: 0.0,
                    default_fov: 106.26,
                    scoped: Vec::new(),
                    scoped_note: None,
                    scale: SensScale::Linear,
                },
                GameProfile {
                    id: "overwatch2".into(),
                    name: "Overwatch 2".into(),
                    yaw: 0.0066,
                    yaw_offset: 0.0,
                    default_fov: 103.0,
                    scoped: Vec::new(),
                    scoped_note: None,
                    scale: SensScale::Linear,
                },
                GameProfile {
                    id: "apex".into(),
                    name: "Apex Legends".into(),
                    yaw: 0.022,
                    yaw_offset: 0.0,
                    default_fov: 110.0,
                    scoped: Vec::new(),
                    scoped_note: None,
                    scale: SensScale::Linear,
                },
            ]
        });

        Self { games }
    }

    pub fn get_by_id(&self, id: &str) -> Option<&GameProfile> {
        self.games.iter().find(|g| g.id == id)
    }

    pub fn list(&self) -> &[GameProfile] {
        &self.games
    }
}

