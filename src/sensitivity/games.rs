use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    pub yaw: f64, // Degrees rotated per mouse count at sens = 1.0
    pub default_fov: f32,
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
                    default_fov: 103.0,
                },
                GameProfile {
                    id: "cs2".into(),
                    name: "Counter-Strike 2".into(),
                    yaw: 0.022,
                    default_fov: 106.26,
                },
                GameProfile {
                    id: "overwatch2".into(),
                    name: "Overwatch 2".into(),
                    yaw: 0.0066,
                    default_fov: 103.0,
                },
                GameProfile {
                    id: "apex".into(),
                    name: "Apex Legends".into(),
                    yaw: 0.022,
                    default_fov: 110.0,
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
