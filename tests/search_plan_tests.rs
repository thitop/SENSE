use aim_trainer::data::setup::{AimProblem, Grip, PadSize, PlayerSetup, Pivot};
use aim_trainer::sensitivity::search_plan::{pad_max_cm, SearchPlan};
use chrono::Utc;

fn close(a: Option<f64>, b: f64) {
    let a = a.expect("start expected");
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn setup(problems: Vec<AimProblem>, at_cm: f64) -> PlayerSetup {
    PlayerSetup { problems, ..PlayerSetup::not_sure(at_cm, Utc::now()) }
}

#[test]
fn no_answers_unknown_sens_is_todays_search() {
    assert_eq!(SearchPlan::new(None, None), SearchPlan::default());
}

#[test]
fn own_sens_is_the_start_even_on_skip_all() {
    let s = PlayerSetup::not_sure(40.0, Utc::now());
    let p = SearchPlan::new(Some(&s), Some(40.0));
    close(p.start_cm, 40.0);
    assert_eq!(p.max_cm, 80.0);
}

#[test]
fn overshoot_leans_slower() {
    let s = setup(vec![AimProblem::Overshoot], 40.0);
    close(SearchPlan::new(Some(&s), Some(40.0)).start_cm, 44.8);
}

#[test]
fn opposite_answers_cancel() {
    let s = setup(vec![AimProblem::Overshoot, AimProblem::CantTurnFast], 40.0);
    close(SearchPlan::new(Some(&s), Some(40.0)).start_cm, 40.0);
}

#[test]
fn two_slower_answers_compound() {
    let s = setup(vec![AimProblem::Overshoot, AimProblem::ShakySmall], 40.0);
    close(SearchPlan::new(Some(&s), Some(40.0)).start_cm, 50.176);
}

#[test]
fn stale_q1_is_ignored() {
    let s = setup(vec![AimProblem::Overshoot], 40.0);
    close(SearchPlan::new(Some(&s), Some(46.0)).start_cm, 46.0);
}

#[test]
fn small_pad_caps_and_clamps() {
    let s = PlayerSetup { pad: Some(PadSize::Small), ..PlayerSetup::not_sure(45.0, Utc::now()) };
    let p = SearchPlan::new(Some(&s), Some(45.0));
    assert_eq!(p.max_cm, 30.0);
    close(p.start_cm, 27.0);
    assert_eq!(pad_max_cm(Some(PadSize::Medium)), 52.0);
    assert_eq!(pad_max_cm(None), 80.0);
}

#[test]
fn unknown_sens_uses_pivot_then_grip() {
    let now = Utc::now();
    let s = PlayerSetup { pivot: Some(Pivot::Arm), grip: Some(Grip::Fingertip), ..PlayerSetup::not_sure(40.0, now) };
    close(SearchPlan::new(Some(&s), None).start_cm, 45.0);
    let s = PlayerSetup { grip: Some(Grip::Fingertip), ..PlayerSetup::not_sure(40.0, now) };
    close(SearchPlan::new(Some(&s), None).start_cm, 32.0);
    let s = PlayerSetup::not_sure(40.0, now);
    assert_eq!(SearchPlan::new(Some(&s), None).start_cm, None);
}

#[test]
fn start_stays_off_the_low_edge() {
    let s = PlayerSetup::not_sure(15.0, Utc::now());
    close(SearchPlan::new(Some(&s), Some(15.0)).start_cm, 16.5);
}
