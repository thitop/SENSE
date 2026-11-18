use aim_trainer::data::setup::{AimProblem, Grip, PadSize, PlayerSetup, Pivot};
use aim_trainer::ui::question_art::{question_svg, QUESTION_ART};
use aim_trainer::ui::screens::questions::{self, QuestionsDraft};
use aim_trainer::ui::{theme, UiAction};
use chrono::Utc;

fn headless_frame(size: egui::Vec2, ui_fn: impl FnMut(&egui::Context)) -> egui::Context {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };
    let _ = ctx.run(input(), |_| {});
    let _ = ctx.run(input(), ui_fn);
    ctx
}

fn saved() -> PlayerSetup {
    PlayerSetup {
        problems: vec![AimProblem::Overshoot, AimProblem::RunOutOfPad],
        grip: Some(Grip::Claw),
        pivot: Some(Pivot::Wrist),
        pad: Some(PadSize::Medium),
        answered_at_cm: 40.0,
        answered_at: Utc::now(),
    }
}

#[test]
fn every_question_picture_exists_and_is_safe() {
    assert_eq!(QUESTION_ART.len(), 11);
    for id in QUESTION_ART {
        let bytes = question_svg(id).unwrap_or_else(|| panic!("missing picture {id}"));
        assert!(bytes.starts_with(b"<svg"), "{id} must start with <svg");
        assert!(bytes.len() <= 150 * 1024, "{id} too big");
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        for bad in ["<script", "href", "<image", "foreignObject", "onload"] {
            assert!(!text.contains(bad), "{id} contains {bad}");
        }
    }
    assert!(question_svg("nope").is_none());
}

#[test]
fn not_sure_and_problems_are_exclusive() {
    let mut d = QuestionsDraft::from_setup(None, false, false);
    d.toggle_problem(AimProblem::Overshoot);
    assert_eq!(d.problems, vec![AimProblem::Overshoot]);
    d.set_problems_not_sure();
    assert!(d.problems.is_empty());
    assert!(d.problems_not_sure);
    d.toggle_problem(AimProblem::ShakySmall);
    assert!(!d.problems_not_sure);
    assert_eq!(d.problems, vec![AimProblem::ShakySmall]);
    d.toggle_problem(AimProblem::ShakySmall);
    assert!(d.problems.is_empty());
}

#[test]
fn skip_all_saves_not_sure_and_starts() {
    let now = Utc::now();
    let draft = QuestionsDraft::from_setup(None, false, false);
    assert_eq!(
        questions::skip_all(&draft, 41.0, now),
        UiAction::SaveSetup { setup: PlayerSetup::not_sure(41.0, now), start_after: true }
    );
    let from_settings = QuestionsDraft::from_setup(Some(&saved()), false, true);
    assert_eq!(
        questions::skip_all(&from_settings, 41.0, now),
        UiAction::SaveSetup { setup: PlayerSetup::not_sure(41.0, now), start_after: false }
    );
}

#[test]
fn skip_all_in_q1_only_keeps_other_answers() {
    let now = Utc::now();
    let mut s = saved();
    s.answered_at_cm = 30.0;
    let draft = QuestionsDraft::from_setup(Some(&s), true, false);
    let UiAction::SaveSetup { setup, start_after } = questions::skip_all(&draft, 41.0, now) else {
        panic!("expected SaveSetup");
    };
    assert!(start_after);
    assert!(setup.problems.is_empty());
    assert_eq!(setup.grip, Some(Grip::Claw));
    assert_eq!(setup.pivot, Some(Pivot::Wrist));
    assert_eq!(setup.pad, Some(PadSize::Medium));
    assert_eq!(setup.answered_at_cm, 41.0);
    assert_eq!(setup.answered_at, now);
}

#[test]
fn cancel_saves_nothing() {
    assert_eq!(questions::cancel(), UiAction::CancelQuestions);
    // Navigating a draft produces no save by itself.
    let mut d = QuestionsDraft::from_setup(None, false, true);
    d.step = 3;
    d.toggle_problem(AimProblem::CantTurnFast);
    let mut actions = Vec::new();
    headless_frame(egui::vec2(1280.0, 720.0), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            questions::show_step(ui, &mut d, 40.0, Utc::now(), &mut actions);
        });
    });
    assert!(actions.iter().all(|a| !matches!(a, UiAction::SaveSetup { .. })));
}

#[test]
fn q1_only_has_one_step() {
    let s = saved();
    let d = QuestionsDraft::from_setup(Some(&s), true, false);
    assert_eq!(d.last_step(), 1);
    assert_eq!(d.problems, s.problems);
    assert_eq!(QuestionsDraft::from_setup(Some(&s), false, false).last_step(), 4);
}

#[test]
fn to_setup_carries_answers() {
    let now = Utc::now();
    let s = saved();
    let d = QuestionsDraft::from_setup(Some(&s), false, true);
    let out = d.to_setup(44.0, now);
    assert_eq!(out.problems, s.problems);
    assert_eq!((out.grip, out.pivot, out.pad), (s.grip, s.pivot, s.pad));
    assert_eq!(out.answered_at_cm, 44.0);
}

#[test]
fn every_step_renders() {
    for q1_only in [false, true] {
        for step in 1..=4u8 {
            let mut d = QuestionsDraft::from_setup(Some(&saved()), q1_only, false);
            d.step = step;
            let mut actions = Vec::new();
            headless_frame(egui::vec2(1280.0, 720.0), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    questions::show_step(ui, &mut d, 40.0, Utc::now(), &mut actions);
                });
            });
        }
    }
}
