//! Game logos: white single-colour SVGs in `assets/logos/`, tinted when drawn. Sources and
//! licences are in `assets/logos/SOURCES.md`.

use egui::{Color32, Image, Ui, Vec2};

/// The logo file for a game id from `data/games.json`.
pub fn logo_svg(game_id: &str) -> Option<&'static [u8]> {
    Some(match game_id {
        "valorant" => include_bytes!("../../assets/logos/valorant.svg"),
        "cs2" => include_bytes!("../../assets/logos/cs2.svg"),
        "overwatch2" => include_bytes!("../../assets/logos/overwatch2.svg"),
        "apex" => include_bytes!("../../assets/logos/apex.svg"),
        "fortnite" => include_bytes!("../../assets/logos/fortnite.svg"),
        "rainbow6" => include_bytes!("../../assets/logos/rainbow6.svg"),
        "cod_warzone" => include_bytes!("../../assets/logos/cod_warzone.svg"),
        "wardogs" => include_bytes!("../../assets/logos/wardogs.svg"),
        "pubg" => include_bytes!("../../assets/logos/pubg.svg"),
        _ => return None,
    })
}

/// The URI a game's logo is loaded under; the `.svg` ending picks egui's SVG loader.
pub fn logo_uri(game_id: &str) -> String {
    format!("bytes://logo/{game_id}.svg")
}

/// Widest a logo may be, in multiples of its height: wordmarks like R6's are about 3.4:1.
pub const MAX_ASPECT: f32 = 3.5;

/// Draw a game's logo `height` points tall (narrower if it would be wider than
/// `MAX_ASPECT` × height), tinted `tint`, keeping its proportions. A game without a logo
/// leaves a square gap so names still line up.
pub fn logo(ui: &mut Ui, game_id: &str, height: f32, tint: Color32) {
    match logo_svg(game_id) {
        Some(bytes) => {
            ui.add(
                Image::from_bytes(logo_uri(game_id), bytes)
                    .fit_to_exact_size(Vec2::new(height * MAX_ASPECT, height))
                    .maintain_aspect_ratio(true)
                    .tint(tint),
            );
        }
        None => {
            ui.add_space(height);
        }
    }
}
