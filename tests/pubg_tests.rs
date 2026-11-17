//! PUBG's 1–100 slider is exponential: it saves `LastConvertedSensitivity =
//! 0.02 × 10^((slider − 50) / 50)` to GameUserSettings.ini, and the game turns 2.222° per count
//! per unit of that value.

use aim_trainer::sensitivity::converter::SensitivityConverter;
use aim_trainer::sensitivity::games::{GameDatabase, GameProfile, SensScale};

fn pubg() -> GameProfile {
    GameDatabase::new().get_by_id("pubg").expect("PUBG is in the game list").clone()
}

#[test]
fn pubg_is_listed_by_name() {
    assert_eq!(pubg().name, "PUBG: BATTLEGROUNDS");
}

#[test]
fn slider_maps_to_the_value_the_game_saves() {
    let g = pubg();
    // The default: Sensitivity=50 saves LastConvertedSensitivity=0.02.
    assert!((g.scale.value(50.0) - 0.02).abs() < 1e-12);
    // A pair a player read from their GameUserSettings.ini.
    assert!((g.scale.value(26.272554) - 0.006706).abs() < 5e-6, "{}", g.scale.value(26.272554));
    // Each 50 steps of slider is ×10.
    assert!((g.scale.value(100.0) - 0.2).abs() < 1e-12);
    assert!((g.scale.value(0.0) - 0.002).abs() < 1e-12);
}

#[test]
fn saved_values_match_the_published_800_dpi_table() {
    // findmymouse.com/convert/player-unknown-battlegrounds: (LastConvertedSensitivity, cm/360).
    let table = [(0.034, 15.1), (0.029, 17.7), (0.026, 19.8), (0.021, 24.5), (0.017, 30.3), (0.015, 34.3), (0.013, 39.6), (0.011, 46.8), (0.009, 57.2)];
    let g = pubg();
    for (saved, cm) in table {
        let got = 914.4 / (800.0 * g.yaw * saved);
        assert!((got - cm).abs() < 0.06, "{saved}: {got:.2} cm, table {cm}");
    }
}

#[test]
fn default_slider_is_about_25_7_cm_at_800_dpi() {
    let cm = SensitivityConverter::game_sens_to_cm_360(&pubg(), 800.0, 50.0);
    assert!((cm - 25.72).abs() < 0.01, "{cm}");
}

#[test]
fn conversion_round_trips_through_the_slider() {
    let g = pubg();
    for dpi in [400.0, 800.0, 1600.0] {
        for cm in [20.0, 39.6, 60.0] {
            let slider = SensitivityConverter::cm_360_to_game_sens(&g, dpi, cm);
            let back = SensitivityConverter::game_sens_to_cm_360(&g, dpi, slider);
            assert!((back - cm).abs() < 1e-9, "{dpi} DPI {cm} cm -> {slider} -> {back}");
        }
    }
    // 39.6 cm/360 at 800 DPI: saved 0.013 (the table's row), slider about 40.6.
    let slider = SensitivityConverter::cm_360_to_game_sens(&g, 800.0, 39.6);
    assert!((slider - 40.6).abs() < 0.1, "{slider}");
}

#[test]
fn scope_values_are_general_minus_the_0_percent_offset() {
    // At the same slider value a scope turns slower in proportion to its FOV (base 80, 2x = 40);
    // 0% monitor distance needs the tan ratio instead. On PUBG's exponential slider that gap is
    // a fixed number of steps below General, whatever General is.
    let base = 80.0_f64;
    let offset = |zoom: f64| {
        let fov = base / zoom;
        let needed = (fov / 2.0).to_radians().tan() / (base / 2.0).to_radians().tan();
        50.0 * (needed / (fov / base)).log10()
    };
    let g = pubg();
    let expected: Vec<(String, String)> = [2.0, 3.0, 4.0, 6.0, 8.0, 15.0]
        .iter()
        .map(|&z| (format!("{z}x"), format!("General − {:.1}", -offset(z))))
        .collect();
    let got: Vec<(String, String)> = g.scoped.iter().map(|s| (s.scope.clone().unwrap(), s.value.clone())).collect();
    assert_eq!(got, expected);
    assert_eq!(got[0].1, "General − 3.1");
    assert!(g.scoped.iter().all(|s| s.setting == "Scope sensitivity"));
    // Where the numbers come from, and what they assume, is said next to them.
    let note = g.scoped_note.expect("PUBG's scoped values carry a note");
    assert!(note.contains("TPP") && note.contains("FPP"), "{note}");
}

#[test]
fn games_with_verified_scoped_values_need_no_note() {
    for id in ["valorant", "cs2", "overwatch2", "apex", "cod_warzone"] {
        assert!(GameDatabase::new().get_by_id(id).unwrap().scoped_note.is_none(), "{id}");
    }
}

#[test]
fn other_games_keep_a_linear_slider() {
    for g in GameDatabase::new().list().iter().filter(|g| g.id != "pubg") {
        assert_eq!(g.scale, SensScale::Linear, "{}", g.id);
        assert_eq!(g.scale.value(0.75), 0.75);
    }
    let g: GameProfile = serde_json::from_str(r#"{"id":"x","name":"X","yaw":0.022,"default_fov":90.0}"#).unwrap();
    assert_eq!(g.scale, SensScale::Linear);
}
