use aim_trainer::data::setup::{AimProblem, Grip, PadSize, PlayerSetup, Pivot, QuestionsNeeded};
use aim_trainer::data::PlayerProfile;
use chrono::Utc;

const OLD: &str = r#"{"id":"default","name":"Player","dpi":800.0,"primary_game_id":"valorant","current_cm_360":CM,
 "created_at":"2026-01-01T00:00:00Z","last_played_at":"2026-01-01T00:00:00Z"}"#;

fn old_profile(cm: &str) -> PlayerProfile {
    serde_json::from_str(&OLD.replace("CM", cm)).expect("old profile must load")
}

fn profile() -> PlayerProfile {
    PlayerProfile::new("p".into(), "P".into(), 800.0, "valorant".into())
}

fn setup_with(problems: Vec<AimProblem>) -> PlayerSetup {
    PlayerSetup {
        problems,
        grip: Some(Grip::Claw),
        pivot: Some(Pivot::Wrist),
        pad: Some(PadSize::Large),
        answered_at_cm: 40.0,
        answered_at: Utc::now(),
    }
}

#[test]
fn old_profile_json_has_no_setup_and_migrates_sens_known() {
    let p = old_profile("40.0");
    assert!(!p.sens_known());
    assert!(p.setup.is_none());
    assert_eq!(p.questions_needed(), Some(QuestionsNeeded::All));
    assert!(old_profile("38.0").sens_known());
}

#[test]
fn setup_round_trips() {
    let mut p = profile();
    p.setup = Some(setup_with(vec![AimProblem::Overshoot, AimProblem::RunOutOfPad]));
    p.sens_known = Some(true);
    let back: PlayerProfile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back.setup, p.setup);
    assert_eq!(back.sens_known, Some(true));
}

#[test]
fn skip_all_is_never_asked_again() {
    let mut p = profile();
    let s = PlayerSetup::not_sure(40.0, Utc::now());
    assert!(s.problems.is_empty() && s.grip.is_none() && s.pivot.is_none() && s.pad.is_none());
    p.setup = Some(s);
    assert_eq!(p.questions_needed(), None);
    p.current_cm_360 = 90.0;
    assert_eq!(p.questions_needed(), None);
}

#[test]
fn q1_reasked_after_sensitivity_moves() {
    let mut p = profile();
    p.setup = Some(setup_with(vec![AimProblem::Overshoot]));
    p.current_cm_360 = 42.0;
    assert_eq!(p.questions_needed(), None);
    p.current_cm_360 = 42.1;
    assert_eq!(p.questions_needed(), Some(QuestionsNeeded::Q1Only));
}

#[test]
fn explicit_sens_known_wins() {
    let mut p = old_profile("38.0");
    p.sens_known = Some(false);
    assert!(!p.sens_known());
}

#[test]
fn problem_leans() {
    assert_eq!(AimProblem::Overshoot.lean(), 1);
    assert_eq!(AimProblem::ShakySmall.lean(), 1);
    assert_eq!(AimProblem::CantTurnFast.lean(), -1);
    assert_eq!(AimProblem::RunOutOfPad.lean(), -1);
}
