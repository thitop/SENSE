//! App-wide settings (sound, display, window placement) are kept between launches in
//! `settings.json`.

use aim_trainer::data::{AppSettings, DisplayMode, WindowPlacement};
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
    assert_eq!(s.display_mode, DisplayMode::Window);
    assert_eq!(s.window, None);
}

#[test]
fn saved_settings_come_back() {
    let path = temp_file("roundtrip");
    let s = AppSettings {
        sound_effects: false,
        effects_volume: 0.8,
        music_volume: 0.1,
        display_mode: DisplayMode::Fullscreen,
        window: Some(WindowPlacement { x: -1900, y: 40, width: 1600, height: 900, maximized: true }),
    };
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

#[test]
fn settings_from_before_display_modes_open_in_a_window() {
    let path = temp_file("old");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, r#"{"sound_effects": false, "effects_volume": 0.2, "music_volume": 0.4}"#).unwrap();
    let s = AppSettings::load_from(&path);
    assert!(!s.sound_effects);
    assert_eq!(s.display_mode, DisplayMode::Window);
    assert_eq!(s.window, None);
}

#[test]
fn display_mode_is_saved_by_name() {
    let path = temp_file("mode_name");
    let s = AppSettings { display_mode: DisplayMode::Fullscreen, ..Default::default() };
    s.save_to(&path).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains(r#""display_mode": "fullscreen""#));
}

#[test]
fn an_implausible_window_size_is_dropped() {
    let path = temp_file("bad_window");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"display_mode": "fullscreen", "window": {"x": 0, "y": 0, "width": 0, "height": 720, "maximized": false}}"#,
    )
    .unwrap();
    let s = AppSettings::load_from(&path);
    assert_eq!(s.window, None);
    assert_eq!(s.display_mode, DisplayMode::Fullscreen);
}

#[test]
fn toggling_flips_between_the_two_modes() {
    assert_eq!(DisplayMode::Window.toggled(), DisplayMode::Fullscreen);
    assert_eq!(DisplayMode::Fullscreen.toggled(), DisplayMode::Window);
}
