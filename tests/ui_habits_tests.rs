use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::analysis::{Dir, Habit, HabitFinding};
use aim_trainer::habits::trend::Progress;
use aim_trainer::habits::LeftOutNote;
use aim_trainer::ui::habits::{card_lines, left_out_sentence};

fn finding(habit: Habit, value: f32, direction: Option<Dir>) -> HabitFinding {
    HabitFinding { habit, value, direction }
}
fn far_short(direction: Option<Dir>) -> HabitFinding {
    finding(Habit::FlickShort(FlickBand::Far), 0.82, direction)
}

#[test]
fn card_reads_like_the_spec() {
    let l = card_lines(
        &far_short(Some(Dir::Left)),
        &Progress { points: vec![0.85, 0.88], practising_elsewhere: false },
    );
    assert_eq!(l.title, "Far flicks stop short");
    assert_eq!(l.value, "18% short");
    assert_eq!(
        l.why,
        "Still there at your best sensitivity → practise, don't change it. Mostly to the left."
    );
    assert_eq!(l.progress.as_deref(), Some("Since calibrating: 18% → 12%"));
    assert_eq!(l.button, "Practise: Flick");
}

#[test]
fn direction_sentences() {
    let why = |d| {
        card_lines(&far_short(d), &Progress { points: vec![], practising_elsewhere: false }).why
    };
    let base = "Still there at your best sensitivity → practise, don't change it.";
    assert_eq!(why(None), base);
    assert_eq!(why(Some(Dir::Right)), format!("{base} Mostly to the right."));
    assert_eq!(why(Some(Dir::Up)), format!("{base} Mostly upward."));
    assert_eq!(why(Some(Dir::Down)), format!("{base} Mostly downward."));
    assert_eq!(why(Some(Dir::SideToSide)), format!("{base} Mostly side to side."));
    assert_eq!(why(Some(Dir::UpAndDown)), format!("{base} Mostly up and down."));
}

#[test]
fn button_uses_practice_screen_names() {
    let p = Progress { points: vec![], practising_elsewhere: false };
    assert_eq!(card_lines(&finding(Habit::MicroCreep, 0.3, None), &p).button, "Practise: Micro-adjust");
    assert_eq!(card_lines(&finding(Habit::TrackingTrails, 60.0, None), &p).button, "Practise: Tracking");
}

#[test]
fn elsewhere_line() {
    let l = card_lines(&far_short(None), &Progress { points: vec![], practising_elsewhere: true });
    assert_eq!(l.progress.as_deref(), Some("Practise at this sensitivity to track progress."));
}

#[test]
fn no_progress_line_without_sessions() {
    let l = card_lines(&far_short(None), &Progress { points: vec![], practising_elsewhere: false });
    assert_eq!(l.progress, None);
}

#[test]
fn since_line_keeps_the_unit_out_of_the_numbers() {
    let l = card_lines(
        &finding(Habit::TrackingTrails, 62.0, None),
        &Progress { points: vec![48.0], practising_elsewhere: false },
    );
    assert_eq!(l.progress.as_deref(), Some("Since calibrating: 62 → 48"));
}

#[test]
fn left_out_sentence_text() {
    let n = LeftOutNote { test: "Tracking".into(), habit: Habit::TrackingTrails };
    assert_eq!(
        left_out_sentence(&n),
        "Tracking was left out of the result: you trailed the target at every speed tested. That's a habit to practise, not a sensitivity problem."
    );
}

#[test]
fn since_line_never_shows_a_negative_number() {
    let l = card_lines(
        &far_short(None),
        &Progress { points: vec![1.05], practising_elsewhere: false },
    );
    let line = l.progress.unwrap();
    assert_eq!(line, "Since calibrating: 18% → 105%");
    let l = card_lines(
        &finding(Habit::TrackingTrails, 62.0, None),
        &Progress { points: vec![-10.0], practising_elsewhere: false },
    );
    assert_eq!(l.progress.as_deref(), Some("Since calibrating: 62 → 10"));
}

// ---------- Task 8: the result page shows the comparison ----------

use aim_trainer::data::setup::{AimProblem, PlayerSetup};
use aim_trainer::data::{CalibrationResult, PlayerProfile, SessionHistory};
use aim_trainer::game::calibration::{PlayMode, PlaySession};
use aim_trainer::game::tasks::TaskState;
use aim_trainer::habits::answers::{AnswerCheck, AnswerVerdict};
use aim_trainer::habits::HabitReport;
use aim_trainer::sensitivity::games::GameDatabase;
use aim_trainer::sensitivity::search_plan::SearchPlan;
use aim_trainer::ui::habits::{badge_text, not_seen_line, PAD_LIMIT};
use aim_trainer::ui::{render_ui, theme, Screen, UiModel, UiState};
use chrono::Utc;

#[test]
fn badge_texts() {
    assert_eq!(badge_text(AnswerVerdict::Agrees), "You said this too");
    assert_eq!(badge_text(AnswerVerdict::Opposite), "The test saw the opposite");
    assert_eq!(badge_text(AnswerVerdict::NotSeen), "Not seen at this sensitivity");
}

#[test]
fn not_seen_line_text() {
    assert_eq!(
        not_seen_line(AimProblem::ShakySmall),
        "You said small adjustments are shaky: not seen at this sensitivity."
    );
}

fn result(plan: Option<SearchPlan>, cm: f64, problems: Vec<AimProblem>, report: HabitReport) -> CalibrationResult {
    CalibrationResult {
        cm_360: cm,
        confidence: 0.8,
        psa_cm: Some(cm),
        stat_cm: None,
        rounds: vec![],
        samples: vec![],
        at: Utc::now(),
        diagnostics: None,
        pooled_runs: 1,
        no_answer: false,
        runs_disagree: false,
        habits: Some(report),
        plan,
        setup: Some(PlayerSetup {
            problems,
            grip: None,
            pivot: None,
            pad: None,
            answered_at_cm: 40.0,
            answered_at: Utc::now(),
        }),
        own_cm_at_start: Some(40.0),
    }
}

/// Every text the Sensitivity screen draws for this result.
fn sensitivity_texts(cal: CalibrationResult) -> Vec<String> {
    let mut profile = PlayerProfile::new("p".into(), "P".into(), 800.0, "valorant".into());
    profile.onboarded = true;
    profile.current_cm_360 = 40.0;
    profile.last_calibration = Some(cal);
    let db = GameDatabase::new();
    let history = SessionHistory::new("p".into());
    let profiles = vec![profile.clone()];
    let model = UiModel {
        profile: &profile,
        profiles: &profiles,
        games: db.list(),
        history: &history,
        session: PlaySession { mode: PlayMode::Practice, paused: false },
        task_state: TaskState::NotStarted,
        task_name: "Flick Shot",
        last_metrics: None,
        calibration: None,
        current_cm_360: 40.0,
        audio_enabled: true,
        volume: 0.5,
        music_volume: 0.5,
        fps: 240,
        offscreen_targets: &[],
    };
    let mut state = UiState::new();
    state.screen = Screen::Sensitivity;
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 3000.0))),
        ..Default::default()
    };
    let _ = ctx.run(input(), |_| {});
    let out = ctx.run(input(), |ctx| {
        render_ui(ctx, &mut state, &model);
    });
    out.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn notes_include_direction_sentences() {
    let report = HabitReport {
        answer_checks: vec![AnswerCheck { problem: AimProblem::Overshoot, verdict: AnswerVerdict::NotSeen }],
        ..Default::default()
    };
    let texts = sensitivity_texts(result(None, 45.6, vec![AimProblem::Overshoot], report));
    assert!(
        texts.iter().any(|t| t.contains("You said you overshoot") && t.contains("14% slower")),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t == &not_seen_line(AimProblem::Overshoot)), "{texts:?}");
}

#[test]
fn pad_note_only_when_limited() {
    let plan = SearchPlan { min_cm: 20.0, max_cm: 45.0, start_cm: None };
    let limited = sensitivity_texts(result(Some(plan), 45.0, vec![], HabitReport::default()));
    assert!(limited.iter().any(|t| t == PAD_LIMIT), "{limited:?}");
    let free = sensitivity_texts(result(Some(plan), 30.0, vec![], HabitReport::default()));
    assert!(!free.iter().any(|t| t == PAD_LIMIT));
}
