//! Scoped / ADS settings that make aiming near the crosshair feel the same zoomed as unzoomed
//! (0% monitor distance, the most common match). They're relative to hipfire, so they don't
//! depend on the player's cm/360.

use aim_trainer::sensitivity::games::{GameDatabase, GameProfile, ScopedSetting};

fn scoped(id: &str) -> Vec<ScopedSetting> {
    GameDatabase::new().get_by_id(id).unwrap_or_else(|| panic!("{id} listed")).scoped.clone()
}

fn values(id: &str) -> Vec<(String, String)> {
    scoped(id).into_iter().map(|s| (s.scope.unwrap_or_default(), s.value)).collect()
}

#[test]
fn cs2_zoom_ratio_matches_the_awp_first_zoom_at_0_percent() {
    // CS2 scales zoomed sens by zoom FOV / 90 (4:3 horizontal); 0% needs the tan ratio instead.
    let awp_zoom = 40.0_f64;
    let ratio = (awp_zoom / 2.0).to_radians().tan() / 45.0_f64.to_radians().tan() / (awp_zoom / 90.0);
    let s = scoped("cs2");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].setting, "zoom_sensitivity_ratio");
    assert_eq!(s[0].value, format!("{ratio:.6}"));
    assert_eq!(s[0].value, "0.818933");
}

#[test]
fn valorant_lists_each_sniper_zoom() {
    assert_eq!(
        values("valorant"),
        [
            ("Operator, first zoom (2.5×)".to_string(), "0.747".to_string()),
            ("Marshal / Outlaw (3.5×)".to_string(), "0.731".to_string()),
            ("Operator, second zoom (5×)".to_string(), "0.723".to_string()),
        ]
    );
    assert!(scoped("valorant").iter().all(|s| s.setting == "Scoped Sensitivity Multiplier"));
}

#[test]
fn overwatch_lists_widowmaker_ana_and_ashe() {
    assert_eq!(
        values("overwatch2"),
        [
            ("Widowmaker / Ana".to_string(), "37.89%".to_string()),
            ("Ashe".to_string(), "51.47%".to_string()),
        ]
    );
}

#[test]
fn apex_and_cod_use_their_built_in_0_percent_scaling() {
    let apex = scoped("apex");
    assert_eq!(apex.len(), 1);
    assert_eq!((apex[0].setting.as_str(), apex[0].value.as_str()), ("ADS Mouse Sensitivity Multiplier", "1.0"));

    let cod: Vec<(String, String)> = scoped("cod_warzone").into_iter().map(|s| (s.setting, s.value)).collect();
    assert_eq!(
        cod,
        [
            ("ADS Sensitivity Multiplier".to_string(), "1.00".to_string()),
            ("ADS Sens. Multiplier Mode".to_string(), "Relative".to_string()),
            ("Monitor Distance Coefficient".to_string(), "0.00".to_string()),
        ]
    );
}

#[test]
fn games_without_verified_data_list_nothing() {
    for id in ["fortnite", "rainbow6", "wardogs"] {
        assert!(scoped(id).is_empty(), "{id}");
    }
}

#[test]
fn a_game_entry_without_scoped_settings_still_parses() {
    let g: GameProfile = serde_json::from_str(r#"{"id":"x","name":"X","yaw":0.022,"default_fov":90.0}"#).unwrap();
    assert!(g.scoped.is_empty());
}

#[test]
fn per_scope_values_group_under_their_setting() {
    use aim_trainer::ui::widgets::{scoped_groups, ScopedGroup};
    let row = |l: &str, v: &str| (l.to_string(), v.to_string());

    // Valorant: one setting, a value per scope.
    assert_eq!(
        scoped_groups(&scoped("valorant")),
        [ScopedGroup {
            heading: Some("Scoped Sensitivity Multiplier".into()),
            rows: vec![
                row("Operator, first zoom (2.5×)", "0.747"),
                row("Marshal / Outlaw (3.5×)", "0.731"),
                row("Operator, second zoom (5×)", "0.723"),
            ],
        }]
    );
    // CoD: several settings without scopes read as one list of setting = value.
    assert_eq!(
        scoped_groups(&scoped("cod_warzone")),
        [ScopedGroup {
            heading: None,
            rows: vec![
                row("ADS Sensitivity Multiplier", "1.00"),
                row("ADS Sens. Multiplier Mode", "Relative"),
                row("Monitor Distance Coefficient", "0.00"),
            ],
        }]
    );
    assert_eq!(
        scoped_groups(&scoped("cs2")),
        [ScopedGroup { heading: None, rows: vec![row("zoom_sensitivity_ratio", "0.818933")] }]
    );
    assert!(scoped_groups(&[]).is_empty());
}
