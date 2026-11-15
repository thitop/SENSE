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

// ---------- Task 2: calibration logic and input helpers ----------

use aim_trainer::game::calibration::{
    delta_text, parse_dpi, parse_game_sens, primary_game, round_change_message, CalibrationProgress,
    ConfidenceLevel, PlayMode, PlaySession,
};
use aim_trainer::game::tasks::{TaskManager, TaskType};
use aim_trainer::sensitivity::games::GameProfile;
use aim_trainer::sensitivity::path_recorder::ShotTrajectoryPattern;
use aim_trainer::sensitivity::psa::PsaCalibrator;
use aim_trainer::sensitivity::SensitivityEngine;

fn game(id: &str) -> GameProfile {
    GameProfile {
        id: id.into(),
        name: id.to_uppercase(),
        yaw: 0.022,
        default_fov: 103.0,
    }
}

#[test]
fn play_session_gates_calibration() {
    let s = |mode, paused| PlaySession { mode, paused };
    assert!(!s(PlayMode::Practice, false).records_calibration());
    assert!(!s(PlayMode::Practice, true).records_calibration());
    assert!(!s(PlayMode::Calibration, true).records_calibration());
    assert!(s(PlayMode::Calibration, false).records_calibration());
}

#[test]
fn progress_reflects_psa() {
    let mut psa = PsaCalibrator::new(15.0, 80.0, 7);
    for _ in 0..4 {
        psa.record_shot(ShotTrajectoryPattern::Direct);
    }
    let p = CalibrationProgress::from_psa(&psa);
    assert_eq!(p.round, 1);
    assert_eq!(p.max_rounds, 7);
    assert_eq!(p.shot_in_round, 4);
    assert_eq!(p.shots_per_round, 10);
    assert_eq!(p.testing_cm_360, 47.5);
    assert_eq!(p.low_cm_360, 15.0);
    assert_eq!(p.high_cm_360, 80.0);
    assert!(!p.completed);

    for _ in 0..6 {
        psa.record_shot(ShotTrajectoryPattern::Overshoot);
    }
    let p = CalibrationProgress::from_psa(&psa);
    assert_eq!(p.round, 2);
    assert_eq!(p.shot_in_round, 0);
    assert_eq!(p.low_cm_360, 47.5);
}

#[test]
fn progress_round_is_clamped_when_completed() {
    let mut psa = PsaCalibrator::new(15.0, 80.0, 1);
    for _ in 0..10 {
        psa.record_shot(ShotTrajectoryPattern::Overshoot);
    }
    let p = CalibrationProgress::from_psa(&psa);
    assert!(p.completed);
    assert_eq!(p.round, 1);
}

#[test]
fn round_change_messages() {
    assert_eq!(
        round_change_message(PsaRoundConclusion::TooFastNeedSlower),
        "Sensitivity changed: slower"
    );
    assert_eq!(
        round_change_message(PsaRoundConclusion::TooSlowNeedFaster),
        "Sensitivity changed: faster"
    );
    assert_eq!(round_change_message(PsaRoundConclusion::BalancedNarrow), "Range narrowed");
}

#[test]
fn confidence_thresholds() {
    assert_eq!(ConfidenceLevel::from_score(0.75), ConfidenceLevel::High);
    assert_eq!(ConfidenceLevel::from_score(0.7499), ConfidenceLevel::Medium);
    assert_eq!(ConfidenceLevel::from_score(0.5), ConfidenceLevel::Medium);
    assert_eq!(ConfidenceLevel::from_score(0.49), ConfidenceLevel::Low);
    assert_eq!(ConfidenceLevel::High.label(), "High confidence");
    assert_eq!(ConfidenceLevel::Medium.label(), "Medium confidence");
    assert_eq!(ConfidenceLevel::Low.label(), "Low confidence");
}

#[test]
fn delta_text_cases() {
    assert_eq!(delta_text(34.2, 41.6), "7.4 cm faster than now");
    assert_eq!(delta_text(45.0, 40.0), "5.0 cm slower than now");
    assert_eq!(delta_text(40.02, 40.0), "Same as now");
}

#[test]
fn parse_dpi_cases() {
    assert_eq!(parse_dpi(" 800 "), Ok(800.0));
    assert_eq!(parse_dpi(""), Err("Enter a number"));
    assert_eq!(parse_dpi("abc"), Err("Enter a number"));
    // Spec: error "if outside 50–50 000", so both ends are valid.
    assert_eq!(parse_dpi("50"), Ok(50.0));
    assert_eq!(parse_dpi("50000"), Ok(50000.0));
    for bad in ["0", "49.9", "50000.5", "60000", "-800"] {
        assert_eq!(parse_dpi(bad), Err("DPI must be between 50 and 50000"), "{bad}");
    }
    assert_eq!(parse_dpi("NaN"), Err("Enter a number"));
}

#[test]
fn cm_360_must_be_plausible() {
    use aim_trainer::game::calibration::validate_cm_360;
    const MSG: &str = "That's outside 5–300 cm/360°. Check the value.";
    assert_eq!(validate_cm_360(46.65), Ok(46.65));
    assert_eq!(validate_cm_360(5.0), Ok(5.0));
    assert_eq!(validate_cm_360(300.0), Ok(300.0));
    // "35" typed instead of "0.35" in Valorant at 800 DPI is ~1.3 cm/360.
    assert_eq!(validate_cm_360(1.3), Err(MSG));
    assert_eq!(validate_cm_360(300.1), Err(MSG));
    assert_eq!(validate_cm_360(f64::INFINITY), Err(MSG));
    assert_eq!(validate_cm_360(f64::NAN), Err(MSG));
}

#[test]
fn parse_game_sens_cases() {
    assert_eq!(parse_game_sens("0,35"), Ok(0.35));
    assert_eq!(parse_game_sens("0.35"), Ok(0.35));
    assert_eq!(parse_game_sens("-1"), Err("Sensitivity must be greater than 0"));
    assert_eq!(parse_game_sens("0"), Err("Sensitivity must be greater than 0"));
    assert_eq!(parse_game_sens("x"), Err("Enter a number"));
    assert_eq!(parse_game_sens("inf"), Err("Enter a number"));
}

#[test]
fn primary_game_falls_back() {
    let games = vec![game("valorant"), game("cs2")];
    assert_eq!(primary_game(&games, "cs2").unwrap().id, "cs2");
    assert_eq!(primary_game(&games, "gone").unwrap().id, "valorant");
    assert!(primary_game(&[], "valorant").is_none());
}

#[test]
fn reset_and_build_result() {
    let mut engine = SensitivityEngine::new(800.0);
    for _ in 0..10 {
        engine.psa.record_shot(ShotTrajectoryPattern::Overshoot);
    }
    assert_eq!(engine.psa.history.len(), 1);
    engine.reset_calibration();
    assert!(engine.psa.history.is_empty());
    assert!(engine.analyzer.samples.is_empty());
    assert_eq!(engine.psa.current_test_cm_360, 47.5);

    for _ in 0..10 {
        engine.psa.record_shot(ShotTrajectoryPattern::Overshoot);
    }
    let r = engine.build_calibration_result(Utc::now());
    assert_eq!(r.rounds.len(), 1);
    assert_eq!(r.rounds[0].conclusion, PsaRoundConclusion::TooFastNeedSlower);
    assert_eq!(r.rounds[0].tested_cm_360, 47.5);
}

// ---------- Task 3: theme, fonts, icons ----------

use aim_trainer::ui::theme;

#[test]
fn theme_installs_font_families() {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let _ = ctx.run(Default::default(), |_| {});
    let families = ctx.fonts(|f| f.families());
    for name in ["display", "display_bold", "medium"] {
        assert!(
            families.contains(&egui::FontFamily::Name(name.into())),
            "missing font family {name}"
        );
    }
    assert_eq!(ctx.style().visuals.panel_fill, theme::BG);
}

// ---------- Task 4: widget kit ----------

use aim_trainer::ui::widgets;

fn headless_frame(size: egui::Vec2, ui_fn: impl FnMut(&egui::Context)) -> egui::Context {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };
    // First frame applies fonts; second frame renders with them.
    let _ = ctx.run(input(), |_| {});
    let _ = ctx.run(input(), ui_fn);
    ctx
}

#[test]
fn toast_alpha_curve() {
    assert_eq!(widgets::toast_alpha(0.0), 0.0);
    assert!((widgets::toast_alpha(0.15) - 1.0).abs() < 1e-4);
    assert_eq!(widgets::toast_alpha(1.0), 1.0);
    assert!((widgets::toast_alpha(2.625) - 0.5).abs() < 0.01);
    assert_eq!(widgets::toast_alpha(3.0), 0.0);
    assert_eq!(widgets::toast_alpha(-1.0), 0.0);
}

#[test]
fn widgets_render_headless() {
    headless_frame(egui::vec2(1280.0, 720.0), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let play = egui_phosphor::regular::PLAY;
            widgets::primary_button(ui, Some(play), "Start calibration");
            widgets::secondary_button(ui, None, "Warm up first");
            widgets::ghost_button(ui, None, "Skip");
            widgets::card(ui, |ui| ui.label("inside"));
            widgets::highlight_card(ui, |ui| ui.label("highlight"));
            widgets::stat_card(ui, "Your current", "41.6", Some("cm/360"), "Valorant 0.350");
            widgets::stat_card(ui, "Last result", "—", None, "Not calibrated yet");
            widgets::nav_item(ui, egui_phosphor::regular::HOUSE, "Home", true);
            widgets::nav_item(ui, egui_phosphor::regular::GEAR, "Settings", false);
            widgets::badge(ui, "High confidence", ConfidenceLevel::High);
            widgets::badge(ui, "Low confidence", ConfidenceLevel::Low);
            widgets::eyebrow(ui, "Your sensitivity");
            widgets::field_error(ui, "Enter a number");
        });
    });
}

// ---------- Task 5: app shell ----------

use aim_trainer::data::SessionHistory;
use aim_trainer::game::tasks::TaskState;
use aim_trainer::sensitivity::games::GameDatabase;
use aim_trainer::ui::{render_ui, Screen, UiModel, UiState};

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 720.0), egui::vec2(1024.0, 640.0)];

fn onboarded_profile() -> PlayerProfile {
    let mut p = PlayerProfile::new("default".into(), "Player".into(), 800.0, "valorant".into());
    p.onboarded = true;
    p.current_cm_360 = 41.6;
    p
}

struct Fixture {
    profile: PlayerProfile,
    profiles: Vec<PlayerProfile>,
    db: GameDatabase,
    history: SessionHistory,
}

impl Fixture {
    fn new(profile: PlayerProfile) -> Self {
        Self {
            profiles: vec![profile.clone()],
            profile,
            db: GameDatabase::new(),
            history: SessionHistory::new("default".into()),
        }
    }

    fn model(&self) -> UiModel<'_> {
        UiModel {
            profile: &self.profile,
            profiles: &self.profiles,
            games: self.db.list(),
            history: &self.history,
            session: PlaySession {
                mode: PlayMode::Practice,
                paused: false,
            },
            task_state: TaskState::NotStarted,
            task_name: "Flick Shot",
            last_metrics: None,
            calibration: None,
            current_cm_360: self.profile.current_cm_360,
            audio_enabled: true,
            volume: 0.5,
            fps: 240,
        }
    }
}

/// Render the UI headlessly for two frames and return the actions from the last one.
fn render_with(state: &mut UiState, fx: &Fixture, size: egui::Vec2) -> Vec<aim_trainer::ui::UiAction> {
    let mut actions = Vec::new();
    let model = fx.model();
    headless_frame(size, |ctx| {
        actions = render_ui(ctx, state, &model);
    });
    actions
}

#[test]
fn shell_renders_each_screen_headless() {
    let fx = Fixture::new(onboarded_profile());
    for screen in [
        Screen::Home,
        Screen::Practice,
        Screen::Sensitivity,
        Screen::History,
        Screen::Settings,
    ] {
        for size in SIZES {
            let mut state = UiState::new();
            state.screen = screen;
            let actions = render_with(&mut state, &fx, size);
            assert!(actions.is_empty(), "{screen:?} at {size:?} emitted {actions:?}");
            assert!(state.onboarding.is_none());
        }
    }
}

#[test]
fn onboarding_draft_created_for_new_profile() {
    let fx = Fixture::new(PlayerProfile::new("p".into(), "P".into(), 1600.0, "cs2".into()));
    let mut state = UiState::new();
    render_with(&mut state, &fx, SIZES[0]);
    let draft = state.onboarding.as_ref().expect("setup draft for a new profile");
    assert_eq!(draft.step, 1);
    assert_eq!(draft.dpi, "1600");
    assert_eq!(draft.game_id, "cs2");
}

// ---------- Task 6: onboarding, home, practice, sensitivity ----------

use aim_trainer::data::{AggregatedTaskMetrics, SessionRecord};
use aim_trainer::ui::screens::home::sessions_this_week;
use aim_trainer::ui::OnboardingDraft;
use chrono::Duration;

fn session_at(at: chrono::DateTime<Utc>, task: &str) -> SessionRecord {
    SessionRecord {
        session_id: format!("s{}", at.timestamp_millis()),
        task_type: task.into(),
        timestamp: at,
        duration_secs: 30.0,
        tested_cm_360: 40.0,
        tested_dpi: 800.0,
        metrics: AggregatedTaskMetrics::default(),
    }
}

fn sample_calibration() -> CalibrationResult {
    CalibrationResult {
        cm_360: 34.2,
        confidence: 0.8,
        psa_cm: Some(35.0),
        stat_cm: Some(33.0),
        rounds: vec![
            CalibrationRound {
                tested_cm_360: 47.5,
                conclusion: PsaRoundConclusion::TooFastNeedSlower,
            },
            CalibrationRound {
                tested_cm_360: 31.25,
                conclusion: PsaRoundConclusion::BalancedNarrow,
            },
        ],
        samples: (0..12).map(|i| (30.0 + i as f64, 50.0 + i as f64)).collect(),
        at: Utc::now(),
    }
}

#[test]
fn sessions_this_week_counts_last_7_days() {
    let now = Utc::now();
    let mut h = SessionHistory::new("p".into());
    h.add_session(session_at(now - Duration::days(1), "Flick Shot"));
    h.add_session(session_at(now - Duration::days(6) - Duration::hours(23), "Flick Shot"));
    h.add_session(session_at(now - Duration::days(8), "Flick Shot"));
    assert_eq!(sessions_this_week(&h, now), 2);
}

#[test]
fn sensitivity_screen_renders_with_and_without_result() {
    for cal in [None, Some(sample_calibration())] {
        let mut p = onboarded_profile();
        p.last_calibration = cal;
        let fx = Fixture::new(p);
        for size in SIZES {
            let mut state = UiState::new();
            state.screen = Screen::Sensitivity;
            state.show_all_games = true;
            assert!(render_with(&mut state, &fx, size).is_empty());
        }
    }
}

#[test]
fn onboarding_renders_each_step() {
    let fx = Fixture::new(PlayerProfile::new("p".into(), "P".into(), 800.0, "valorant".into()));
    for step in 1..=3 {
        for attempted in [false, true] {
            let mut state = UiState::new();
            let mut draft = OnboardingDraft::from_profile(&fx.profile);
            draft.step = step;
            draft.attempted = attempted;
            draft.dpi = "abc".into();
            draft.sens = "0,35".into();
            state.onboarding = Some(draft);
            assert!(render_with(&mut state, &fx, SIZES[1]).is_empty());
        }
    }
}

// ---------- Task 7: history, settings, HUD ----------

use aim_trainer::game::tasks::TaskMetrics;

fn render_model(state: &mut UiState, model: &UiModel, size: egui::Vec2) -> Vec<aim_trainer::ui::UiAction> {
    let mut actions = Vec::new();
    headless_frame(size, |ctx| {
        actions = render_ui(ctx, state, model);
    });
    actions
}

#[test]
fn history_renders_empty_and_with_sessions() {
    let mut fx = Fixture::new(onboarded_profile());
    for size in SIZES {
        let mut state = UiState::new();
        state.screen = Screen::History;
        assert!(render_with(&mut state, &fx, size).is_empty());
    }
    let now = Utc::now();
    for (i, task) in ["Flick Shot", "Calibration", "Smooth Tracking"].iter().enumerate() {
        fx.history
            .add_session(session_at(now - Duration::hours(i as i64), task));
    }
    for selected in [None, Some(0), Some(99)] {
        let mut state = UiState::new();
        state.screen = Screen::History;
        state.selected_session = selected;
        assert!(render_with(&mut state, &fx, SIZES[1]).is_empty());
    }
}

#[test]
fn settings_renders_with_pending_delete_and_errors() {
    let mut fx = Fixture::new(onboarded_profile());
    let mut other = onboarded_profile();
    other.id = "second".into();
    other.name = "Second".into();
    fx.profiles.push(other);
    for size in SIZES {
        let mut state = UiState::new();
        state.screen = Screen::Settings;
        state.pending_delete = Some("second".into());
        state.dpi_input = Some("abc".into());
        state.dpi_error = Some("Enter a number");
        assert!(render_with(&mut state, &fx, size).is_empty());
    }
}

#[test]
fn hud_renders_each_state() {
    let fx = Fixture::new(onboarded_profile());
    let mut psa = PsaCalibrator::new(15.0, 80.0, 7);
    for _ in 0..13 {
        psa.record_shot(ShotTrajectoryPattern::Overshoot);
    }
    let metrics = TaskMetrics {
        task_name: "Flick Shot".into(),
        score: 12_400,
        accuracy: 86.7,
        avg_reaction_time_ms: 412.0,
        shots_fired: 30,
        hits: 26,
        misses: 4,
        duration_secs: 41.0,
        custom_metric_label: "",
        custom_metric_value: String::new(),
    };

    let cases: Vec<(PlayMode, bool, TaskState, bool)> = vec![
        (PlayMode::Practice, false, TaskState::Running, false),
        (PlayMode::Practice, false, TaskState::Countdown { remaining_secs: 2.4 }, false),
        (PlayMode::Practice, false, TaskState::Countdown { remaining_secs: 0.0 }, false),
        (PlayMode::Practice, false, TaskState::Finished, false),
        (PlayMode::Calibration, false, TaskState::Running, true),
        (PlayMode::Calibration, true, TaskState::Running, false),
    ];
    for (mode, paused, task_state, with_toast) in cases {
        let mut model = fx.model();
        model.session = PlaySession { mode, paused };
        model.task_state = task_state;
        model.last_metrics = Some(&metrics);
        if mode == PlayMode::Calibration {
            model.calibration = Some(CalibrationProgress::from_psa(&psa));
            model.task_name = "Calibration";
        }
        let mut state = UiState::new();
        state.menu_open = false;
        if with_toast {
            state.show_toast("Sensitivity changed: slower", 0.0);
        }
        for size in SIZES {
            assert!(render_model(&mut state, &model, size).is_empty(), "{mode:?} {task_state:?}");
        }
    }
}

#[test]
fn history_rows_are_newest_first_and_selection_clamps() {
    use aim_trainer::ui::screens::history::{rows, selected_index};
    let now = Utc::now();
    let mut h = SessionHistory::new("p".into());
    h.add_session(session_at(now - Duration::hours(2), "Old"));
    h.add_session(session_at(now, "New"));
    h.add_session(session_at(now - Duration::hours(1), "Mid"));
    let r = rows(&h);
    let names: Vec<_> = r.iter().map(|s| s.task_type.as_str()).collect();
    assert_eq!(names, ["New", "Mid", "Old"]);
    assert_eq!(selected_index(3, None), Some(0));
    assert_eq!(selected_index(3, Some(2)), Some(2));
    assert_eq!(selected_index(3, Some(99)), Some(0));
    assert_eq!(selected_index(0, Some(1)), None);
}

#[test]
fn countdown_label_counts_down_to_go() {
    use aim_trainer::ui::hud::countdown_label;
    assert_eq!(countdown_label(2.4), "3");
    assert_eq!(countdown_label(1.0), "1");
    assert_eq!(countdown_label(0.5), "1");
    assert_eq!(countdown_label(0.0), "Go");
    assert_eq!(countdown_label(-0.2), "Go");
}

#[test]
fn hud_clears_expired_toast() {
    let fx = Fixture::new(onboarded_profile());
    let mut state = UiState::new();
    state.menu_open = false;
    state.show_toast("old news", -10.0);
    render_with(&mut state, &fx, SIZES[0]);
    assert!(state.toast.is_none());
}

#[test]
fn calibration_task_type_is_endless_flick() {
    let mut tm = TaskManager::new();
    tm.set_task(TaskType::Calibration);
    assert_eq!(tm.current_type, TaskType::Calibration);
    assert_eq!(tm.current_task_name(), "Calibration");
}

// ---------- Final review fixes ----------

#[test]
fn calibration_shots_only_count_while_targets_are_live() {
    use aim_trainer::game::calibration::records_calibration_shot;
    let cal = PlaySession {
        mode: PlayMode::Calibration,
        paused: false,
    };
    assert!(records_calibration_shot(cal, TaskState::Running));
    assert!(!records_calibration_shot(cal, TaskState::Countdown { remaining_secs: 2.0 }));
    assert!(!records_calibration_shot(cal, TaskState::NotStarted));
    let paused = PlaySession { paused: true, ..cal };
    assert!(!records_calibration_shot(paused, TaskState::Running));
    let practice = PlaySession {
        mode: PlayMode::Practice,
        paused: false,
    };
    assert!(!records_calibration_shot(practice, TaskState::Running));
}

#[test]
fn esc_only_returns_to_a_run_in_progress() {
    use aim_trainer::game::calibration::esc_closes_menu;
    assert!(esc_closes_menu(TaskState::Running, false));
    assert!(esc_closes_menu(TaskState::Countdown { remaining_secs: 1.0 }, false));
    assert!(!esc_closes_menu(TaskState::NotStarted, false));
    assert!(!esc_closes_menu(TaskState::Finished, false));
    assert!(!esc_closes_menu(TaskState::Running, true));
}

#[test]
fn new_profile_ids_never_collide() {
    use aim_trainer::data::profile::unique_profile_id;
    let existing = vec![
        PlayerProfile::new("alice".into(), "Alice".into(), 800.0, "valorant".into()),
        PlayerProfile::new("alice-2".into(), "Alice".into(), 800.0, "valorant".into()),
        PlayerProfile::new("default".into(), "Player".into(), 800.0, "valorant".into()),
    ];
    assert_eq!(unique_profile_id("Bob", &existing), "bob");
    assert_eq!(unique_profile_id("Alice", &existing), "alice-3");
    assert_eq!(unique_profile_id("Default", &existing), "default-2");
    assert!(unique_profile_id("!!!", &existing).starts_with("profile_"));
}

#[test]
fn current_metrics_reports_running_task() {
    let mut tm = TaskManager::new();
    tm.set_task(TaskType::Calibration);
    let m = tm.current_metrics();
    assert_eq!(m.task_name, "Calibration");
    assert_eq!(m.shots_fired, 0);
}
