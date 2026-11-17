//! App-wide settings (sound) are kept between launches in `settings.json`.

use aim_trainer::data::AppSettings;
use std::path::PathBuf;

/// A fresh file path in its own temp folder.
fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aim_trainer_settings_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("settings.json")
}

#[test]
fn defaults_match_a_fresh_install() {
    let s = AppSettings::default();
    assert!(s.sound_effects);
    assert_eq!(s.effects_volume, 0.35);
    assert_eq!(s.music_volume, 0.5);
}

#[test]
fn saved_settings_come_back() {
    let path = temp_file("roundtrip");
    let s = AppSettings { sound_effects: false, effects_volume: 0.8, music_volume: 0.1 };
    s.save_to(&path).unwrap();
    assert_eq!(AppSettings::load_from(&path), s);
}

#[test]
fn a_missing_or_broken_file_gives_the_defaults() {
    let path = temp_file("missing");
    assert_eq!(AppSettings::load_from(&path), AppSettings::default());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json").unwrap();
    assert_eq!(AppSettings::load_from(&path), AppSettings::default());
}

#[test]
fn missing_fields_take_their_default_and_volumes_stay_in_range() {
    let path = temp_file("partial");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, r#"{"music_volume": 3.0}"#).unwrap();
    let s = AppSettings::load_from(&path);
    assert_eq!(s.music_volume, 1.0);
    assert!(s.sound_effects);
    assert_eq!(s.effects_volume, 0.35);
}
