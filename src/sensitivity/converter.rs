use super::games::GameProfile;

/// cm/360° is the universal unit for mouse sensitivity.
/// Formula (`yaw_offset` is 0 and `scale` linear for most games, see `GameProfile`):
///   counts_per_360 = 360.0 / (scale(sens) * yaw + yaw_offset)
///   inches_per_360 = counts_per_360 / DPI
///   cm_per_360 = inches_per_360 * 2.54 = (360 * 2.54) / (DPI * sens * yaw)
///              = 914.4 / (DPI * sens * yaw)
pub struct SensitivityConverter;

impl SensitivityConverter {
    /// Convert In-Game sensitivity and DPI into universal cm/360°
    pub fn game_sens_to_cm_360(game: &GameProfile, dpi: f64, sens: f64) -> f64 {
        if dpi <= 0.0 || sens <= 0.0 || game.yaw <= 0.0 {
            return 0.0;
        }
        (360.0 * 2.54) / (dpi * game.deg_per_count(sens))
    }

    /// Convert universal cm/360° back into game-specific in-game sensitivity value
    pub fn cm_360_to_game_sens(game: &GameProfile, dpi: f64, cm_360: f64) -> f64 {
        if dpi <= 0.0 || cm_360 <= 0.0 || game.yaw <= 0.0 {
            return 0.0;
        }
        game.sens_for((360.0 * 2.54) / (dpi * cm_360))
    }

    /// Convert cm/360° and DPI into camera rotation angle (radians per mouse count)
    pub fn cm_360_to_rad_per_count(dpi: f64, cm_360: f64) -> f32 {
        if dpi <= 0.0 || cm_360 <= 0.0 {
            return 0.0003;
        }
        // counts for 360 deg = (cm_360 / 2.54) * dpi
        let counts_per_360 = (cm_360 / 2.54) * dpi;
        let deg_per_count = 360.0 / counts_per_360;
        deg_per_count.to_radians() as f32
    }

    /// Direct cross-game sensitivity conversion (e.g. Valorant -> CS2)
    pub fn convert_game_to_game(
        from_game: &GameProfile,
        to_game: &GameProfile,
        dpi: f64,
        sens: f64,
    ) -> f64 {
        let cm_360 = Self::game_sens_to_cm_360(from_game, dpi, sens);
        Self::cm_360_to_game_sens(to_game, dpi, cm_360)
    }
}

