use aim_trainer::data::{CalibrationResult, CalibrationRound, PlayerProfile};
use aim_trainer::sensitivity::psa::PsaRoundConclusion;
use chrono::Utc;

// ---------- Task 1: profile data model ----------

#[test]
fn old_profile_json_loads_as_onboarded() {
    let json = r#"{"id":"default","name":"Player","dpi":800.0,"primary_game_id":"valorant","current_cm_360":40.0,
     "created_at":"2026-01-01T00:00:00Z","last_played_at":"2026-01-01T00:00:00Z"}"#;
    let p: PlayerProfile = serde_json::from_str(json).expect("old profile must load");
    assert!(p.onboarded);
    assert!(p.last_calibration.is_none());
    assert_eq!(p.dpi, 800.0);
}

#[test]
fn new_profile_needs_setup() {
    let p = PlayerProfile::new("p".into(), "P".into(), 800.0, "valorant".into());
    assert!(!p.onboarded);
    assert!(p.last_calibration.is_none());
}

#[test]
fn calibration_result_round_trips() {
    let mut p = PlayerProfile::new("p".into(), "P".into(), 800.0, "valorant".into());
    let rounds = vec![
        CalibrationRound {
            tested_cm_360: 47.5,
            conclusion: PsaRoundConclusion::TooFastNeedSlower,
        },
        CalibrationRound {
            tested_cm_360: 63.75,
            conclusion: PsaRoundConclusion::BalancedNarrow,
        },
    ];
    p.last_calibration = Some(CalibrationResult {
        cm_360: 55.2,
        confidence: 0.8,
        psa_cm: Some(56.0),
        stat_cm: Some(54.0),
        rounds: rounds.clone(),
        samples: vec![(47.5, 60.0), (63.75, 55.0), (55.0, 70.0)],
        at: Utc::now(),
    });

    let json = serde_json::to_string(&p).unwrap();
    let back: PlayerProfile = serde_json::from_str(&json).unwrap();
    let cal = back.last_calibration.expect("calibration persisted");
    assert_eq!(cal.cm_360, 55.2);
    assert_eq!(cal.rounds, rounds);
    assert_eq!(cal.samples.len(), 3);
}
