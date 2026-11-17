//! WARDOGS maps its 0.1–2 sensitivity slider linearly onto 0.007–0.07° per count, so its
//! turn rate isn't proportional to the slider like the other games': it has an offset.

use aim_trainer::sensitivity::converter::SensitivityConverter;
use aim_trainer::sensitivity::games::{GameDatabase, GameProfile};

fn wardogs() -> GameProfile {
    GameDatabase::new().get_by_id("wardogs").expect("WARDOGS is in the game list").clone()
}

#[test]
fn wardogs_is_listed_by_name() {
    assert_eq!(wardogs().name, "WARDOGS");
}

#[test]
fn wardogs_matches_the_published_800_dpi_table() {
    // findmymouse.com/convert/wardogs, measured on the beta build: (slider, cm/360 at 800 DPI).
    let table = [(1.86, 17.5), (1.61, 20.0), (1.27, 25.0), (1.04, 29.9), (0.87, 35.1), (0.75, 40.0), (0.65, 45.3), (0.52, 54.6)];
    let g = wardogs();
    for (sens, cm) in table {
        let got = SensitivityConverter::game_sens_to_cm_360(&g, 800.0, sens);
        // The table's slider values are rounded to 0.01.
        assert!((got - cm).abs() / cm < 0.012, "sens {sens}: {got:.2} cm, table {cm}");
    }
}

#[test]
fn wardogs_slider_ends_land_on_007_and_007_degrees_per_count() {
    let g = wardogs();
    let cm_at = |sens| SensitivityConverter::game_sens_to_cm_360(&g, 800.0, sens);
    assert!((cm_at(0.1) - 914.4 / (800.0 * 0.007)).abs() < 0.01, "{}", cm_at(0.1));
    assert!((cm_at(2.0) - 914.4 / (800.0 * 0.07)).abs() < 0.01, "{}", cm_at(2.0));
}

#[test]
fn wardogs_conversion_round_trips() {
    let g = wardogs();
    for dpi in [400.0, 800.0, 1600.0] {
        for cm in [20.0, 39.6, 60.0] {
            let sens = SensitivityConverter::cm_360_to_game_sens(&g, dpi, cm);
            let back = SensitivityConverter::game_sens_to_cm_360(&g, dpi, sens);
            assert!((back - cm).abs() < 1e-9, "{dpi} DPI {cm} cm -> {sens} -> {back}");
        }
    }
    // 39.6 cm/360 at 800 DPI.
    let s = SensitivityConverter::cm_360_to_game_sens(&g, 800.0, 39.6);
    assert!((s - 0.759).abs() < 0.001, "{s}");
}

#[test]
fn games_without_an_offset_convert_as_before() {
    let db = GameDatabase::new();
    let cs2 = db.get_by_id("cs2").unwrap();
    assert_eq!(cs2.yaw_offset, 0.0);
    let s = SensitivityConverter::cm_360_to_game_sens(cs2, 800.0, 39.6);
    let before = 914.4 / (800.0 * 39.6 * 0.022);
    assert!((s - before).abs() < 1e-12, "{s} vs {before}");
}

#[test]
fn a_game_entry_without_an_offset_still_parses() {
    let g: GameProfile = serde_json::from_str(r#"{"id":"x","name":"X","yaw":0.022,"default_fov":90.0}"#).unwrap();
    assert_eq!(g.yaw_offset, 0.0);
}
