// Release builds open without a console window; debug builds keep it for the logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod data;
mod engine;
mod game;
mod habits;
mod sensitivity;
mod ui;

use data::{
    AppSettings, DataManager, DisplayMode, MetricPoint, PlayerProfile, SessionHistory, SessionRecord,
    WindowPlacement,
};
use engine::{
    audio::{AudioManager, Music, Sfx, TrackingTicker},
    camera::Camera, display, input::InputManager, renderer::{self, Renderer}, timing::Timing,
};
use habits::recorder::AimRecorder;
use habits::record::{AimLog, PlayContext};
use game::{
    calibration::{
        esc_closes_menu, records_calibration_shot, round_change_message, CalibrationProgress,
        PlayMode, PlaySession, RunTotals,
    },
    calibration_run::{CalibrationRun, SegmentEnd},
    spawn_cue::SpawnWatcher,
    hit_detection::Ray,
    tasks::{TaskManager, TaskState, TaskType},
};
use data::setup::QuestionsNeeded;
use sensitivity::combiner::TestKind;
use sensitivity::search_plan::SearchPlan;
use ui::screens::questions::QuestionsDraft;
use sensitivity::path_recorder::ShotMovementPath;
use sensitivity::{pooling, SensitivityConverter, SensitivityEngine};
use std::sync::Arc;
use ui::{render_ui, theme, OnboardingDraft, Screen, UiAction, UiModel, UiState};
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowId},
};

struct GameState {
    renderer: Renderer,
    camera: Camera,
    input: InputManager,
    timing: Timing,
    audio_manager: AudioManager,
    task_manager: TaskManager,
    sensitivity_engine: SensitivityEngine,
    data_manager: DataManager,
    /// The app-wide settings as last saved, kept up to date by the Settings screen, the
    /// fullscreen hotkeys and the window's moves.
    settings: AppSettings,
    /// Alt is down, for Alt+Enter.
    alt_held: bool,
    ui_state: UiState,
    session: PlaySession,
    /// Cached session history of the active profile; refreshed whenever a session is recorded.
    history: SessionHistory,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    current_cm_360: f64,
    rad_per_count: f32,
    player_dpi: f64,
    current_session_points: Vec<MetricPoint>,
    /// Score and time banked from earlier segments of a calibration (finished or paused).
    calibration_totals: RunTotals,
    /// The multi-test calibration in progress.
    calibration_run: Option<CalibrationRun>,
    /// Counted flick shots of the segment in progress, for the result's score curve. Added to
    /// the analyzer when the segment finishes, so a replayed segment isn't drawn twice.
    pending_curve: Vec<(f64, f64, f64, ShotMovementPath)>,
    /// The micro target already judged: only the first click on a target says anything, a
    /// later one starts mid-correction.
    /// Spots targets that just appeared, to cue the ones outside the view.
    spawn_watcher: SpawnWatcher,
    shot_counter: u32,
    /// Turns live play into aim records.
    aim: AimRecorder,
    /// Ticks while the crosshair stays on a tracking target.
    track_ticker: TrackingTicker,
    last_countdown_sec: i32,
}

impl GameState {
    async fn new(window: Arc<Window>, data_manager: DataManager, settings: AppSettings) -> Self {
        let size = window.inner_size();
        let renderer = Renderer::new(window.clone()).await;
        let aspect_ratio = size.width as f32 / size.height.max(1) as f32;
        let camera = Camera::new(aspect_ratio);
        let mut input = InputManager::new();
        // The app opens on the menu, so the cursor starts free.
        input.set_cursor_locked(&window, false);

        let timing = Timing::new(1000.0); // 1000 Hz physics tick
        let mut audio_manager = AudioManager::new();

        // Sound settings from last time. With no sound device, effects stay off regardless.
        audio_manager.is_enabled &= settings.sound_effects;
        audio_manager.volume = settings.effects_volume;
        audio_manager.set_music_volume(settings.music_volume);
        let active_profile = data_manager.profile_manager.get_active_profile().cloned();
        let player_dpi = active_profile.as_ref().map(|p| p.dpi).unwrap_or(800.0);
        let current_cm_360 = active_profile
            .as_ref()
            .map(|p| p.current_cm_360)
            .unwrap_or(40.0);

        let sensitivity_engine = SensitivityEngine::new(player_dpi);
        let rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(player_dpi, current_cm_360);

        let mut task_manager = TaskManager::new();
        task_manager.set_task(TaskType::Flick);

        let history = data_manager.load_history(
            data_manager
                .profile_manager
                .active_profile_id
                .as_deref()
                .unwrap_or("default"),
        );
        let egui_ctx = egui::Context::default();
        theme::install(&egui_ctx);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        Self {
            renderer,
            camera,
            input,
            timing,
            audio_manager,
            task_manager,
            sensitivity_engine,
            data_manager,
            settings,
            alt_held: false,
            ui_state: UiState::new(),
            session: PlaySession {
                mode: PlayMode::Practice,
                paused: false,
            },
            history,
            egui_ctx,
            egui_state,
            current_cm_360,
            rad_per_count,
            player_dpi,
            current_session_points: Vec::new(),
            calibration_totals: RunTotals::default(),
            calibration_run: None,
            pending_curve: Vec::new(),
            spawn_watcher: SpawnWatcher::default(),
            shot_counter: 0,
            aim: AimRecorder::new(),
            track_ticker: TrackingTicker::default(),
            last_countdown_sec: 4,
        }
    }

    // ---------- small helpers ----------

    fn now(&self) -> f64 {
        self.egui_ctx.input(|i| i.time)
    }

    fn profile_cm_360(&self) -> f64 {
        self.data_manager
            .profile_manager
            .get_active_profile()
            .map(|p| p.current_cm_360)
            .unwrap_or(40.0)
    }

    /// Mutate the active profile and persist it.
    fn update_profile(&mut self, f: impl FnOnce(&mut PlayerProfile)) {
        let pm = &mut self.data_manager.profile_manager;
        if let Some(p) = pm.get_active_profile_mut() {
            f(p);
            let copy = p.clone();
            let _ = pm.save_profile(&copy);
        }
    }

    fn set_cm_360(&mut self, cm: f64) {
        self.current_cm_360 = cm;
        self.rad_per_count = SensitivityConverter::cm_360_to_rad_per_count(self.player_dpi, cm);
    }

    fn set_dpi(&mut self, dpi: f64) {
        self.player_dpi = dpi;
        self.sensitivity_engine.player_dpi = dpi;
        self.set_cm_360(self.current_cm_360);
    }

    fn save_settings(&self) {
        if let Err(e) = self.data_manager.save_settings(&self.settings) {
            log::warn!("Could not save the settings: {e}");
        }
    }

    /// Switch between window and fullscreen right away and remember it for the next launch.
    /// Leaving fullscreen puts the window back where it was.
    fn set_display_mode(&mut self, mode: DisplayMode) {
        let window = &self.renderer.window;
        let fullscreen = match mode {
            DisplayMode::Window => None,
            DisplayMode::Fullscreen => match window.current_monitor().or_else(|| window.primary_monitor()) {
                Some(monitor) => Some(display::fullscreen_on(monitor)),
                None => {
                    log::warn!("No screen found to go fullscreen on");
                    return;
                }
            },
        };
        self.settings.display_mode = mode;
        window.set_fullscreen(fullscreen);
        self.save_settings();
    }

    /// Note where the window is in window mode, so the next launch opens it there. Saved to
    /// disk with the other settings (on quit at the latest).
    fn track_window_placement(&mut self) {
        let window = &self.renderer.window;
        if window.fullscreen().is_some() || window.is_minimized() == Some(true) {
            return;
        }
        let maximized = window.is_maximized();
        // A maximised window keeps the size it goes back to when restored.
        let placement = match (maximized, self.settings.window) {
            (true, Some(before)) => WindowPlacement { maximized: true, ..before },
            _ => {
                let Ok(pos) = window.outer_position() else { return };
                let size = window.inner_size();
                if !renderer::can_draw(size) {
                    return;
                }
                WindowPlacement { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized }
            }
        };
        self.settings.window = Some(placement);
    }

    fn lock_cursor(&mut self, locked: bool) {
        self.input.set_cursor_locked(&self.renderer.window, locked);
    }

    fn open_menu(&mut self, screen: Screen) {
        self.ui_state.menu_open = true;
        self.ui_state.screen = screen;
        self.lock_cursor(false);
    }

    fn close_menu(&mut self) {
        self.ui_state.menu_open = false;
        self.lock_cursor(true);
    }

    /// (Re)start `task` with a countdown, keeping the shots collected so far.
    fn restart_countdown(&mut self, task: TaskType) {
        self.last_countdown_sec = 4;
        self.task_manager.set_task(task);
        self.task_manager.countdown_duration = 3.0;
        self.task_manager.start_task_with_countdown();
        self.sensitivity_engine.path_recorder.start_shot();
    }

    /// Start the calibration run's current segment, after a countdown when `countdown_secs` > 0
    /// (the run's start and resuming). Segments within a run follow each other directly.
    fn begin_segment(&mut self, countdown_secs: f32) {
        let Some(run) = &self.calibration_run else { return };
        let task = run.make_task();
        self.task_manager.set_custom_task(task, TaskType::Calibration);
        if countdown_secs > 0.0 {
            self.last_countdown_sec = countdown_secs.ceil() as i32 + 1;
            self.task_manager.countdown_duration = countdown_secs;
            self.task_manager.start_task_with_countdown();
        } else {
            self.task_manager.start_task_now();
        }
        self.spawn_watcher.clear();
        self.aim.forget_current();
        self.sensitivity_engine.path_recorder.start_shot();
        self.pending_curve.clear();
    }

    /// A segment's task completed: count it, then move on to the next segment or block.
    fn on_segment_finished(&mut self) {
        if let Some(m) = self.task_manager.last_completed_metrics.clone() {
            self.calibration_totals.add_segment(m.score, m.duration_secs);
        }
        self.aim.commit();
        let Some(run) = self.calibration_run.as_mut() else { return };
        let end = run.finish_segment();
        for (cm, accuracy, rt, path) in self.pending_curve.drain(..) {
            self.sensitivity_engine.analyzer.record_sample(cm, accuracy, rt, &path);
        }
        match end {
            SegmentEnd::Next => self.begin_segment(0.0),
            SegmentEnd::SensitivityChanged(conclusion) => {
                let player_cm = self.profile_cm_360();
                let cm = self.calibration_run.as_ref().map_or(player_cm, |r| r.test_cm(player_cm));
                self.set_cm_360(cm);
                let now = self.now();
                let msg = conclusion.map_or("Warm-up done. Testing starts", round_change_message);
                self.ui_state.show_toast(msg, now);
                self.audio_manager.play(Sfx::SensChange, None);
                self.begin_segment(0.0);
            }
            SegmentEnd::Completed => self.finish_calibration(),
        }
    }

    /// Start a fresh run of `task` in `mode`.
    fn start_run(&mut self, mode: PlayMode, task: TaskType) {
        self.session = PlaySession { mode, paused: false };
        self.current_session_points.clear();
        self.shot_counter = 0;
        self.aim.reset();
        self.restart_countdown(task);
        self.close_menu();
    }

    fn reload_history(&mut self) {
        let id = self
            .data_manager
            .profile_manager
            .active_profile_id
            .clone()
            .unwrap_or_else(|| "default".into());
        self.history = self.data_manager.load_history(&id);
    }

    /// Re-read everything that depends on the active profile.
    /// A practice run paused in the menu can't continue under different settings: its shots
    /// would be saved to the wrong profile or DPI. (Calibration never has the menu open.)
    fn end_paused_practice(&mut self) {
        if self.session.mode == PlayMode::Practice && self.task_manager.abandon() {
            self.current_session_points.clear();
            self.shot_counter = 0;
            self.aim.reset();
        }
    }

    fn after_profile_switch(&mut self) {
        self.end_paused_practice();
        self.aim.reset();
        if let Some(p) = self.data_manager.profile_manager.get_active_profile().cloned() {
            self.set_dpi(p.dpi);
            self.set_cm_360(p.current_cm_360);
        }
        self.reload_history();
        self.ui_state.dpi_input = None;
        self.ui_state.dpi_error = None;
        self.ui_state.open_session = None;
        self.ui_state.onboarding = None; // re-created next frame if the new profile needs setup
        self.ui_state.questions = None;
    }

    /// Ask the setup questions first when the profile needs them; otherwise begin the run.
    fn start_calibration(&mut self) {
        let Some(p) = self.data_manager.profile_manager.get_active_profile().cloned() else { return };
        if let Some(needed) = p.questions_needed() {
            let q1_only = needed == QuestionsNeeded::Q1Only;
            self.ui_state.questions = Some(QuestionsDraft::from_setup(p.setup.as_ref(), q1_only, false));
            return;
        }
        self.sensitivity_engine.reset_calibration();
        self.calibration_totals = RunTotals::default();
        self.aim.reset();
        let seed = chrono::Utc::now().timestamp_millis() as u64;
        let own_cm = p.sens_known().then_some(p.current_cm_360);
        let run = CalibrationRun::with_plan(seed, SearchPlan::for_profile(&p), p.setup.clone(), own_cm);
        self.set_cm_360(run.test_cm(self.profile_cm_360()));
        self.calibration_run = Some(run);
        self.session = PlaySession { mode: PlayMode::Calibration, paused: false };
        self.current_session_points.clear();
        self.shot_counter = 0;
        self.begin_segment(3.0);
        self.close_menu();
    }

    fn record_session(
        &mut self,
        task_type: String,
        timestamp: chrono::DateTime<chrono::Utc>,
        tested_cm_360: f64,
        score: u64,
        duration_secs: f32,
        aim: Option<AimLog>,
        habits: Option<habits::HabitReport>,
        tracking: Option<data::TrackingStats>,
    ) {
        let points = std::mem::take(&mut self.current_session_points);
        // Tracking is scored by time on target; clicks there are not shots.
        let metrics = match tracking {
            Some(t) => data::AggregatedTaskMetrics::tracking(t),
            None => data::AggregatedTaskMetrics::compute(points, score, 0),
        };
        let record = SessionRecord {
            session_id: format!("sess_{}", chrono::Utc::now().timestamp_millis()),
            task_type,
            timestamp,
            duration_secs,
            tested_cm_360,
            tested_dpi: self.player_dpi,
            metrics,
            aim,
            habits,
        };
        let _ = self.data_manager.record_session(record);
        self.reload_history();
    }

    fn finish_calibration(&mut self) {
        let Some(run) = self.calibration_run.take() else { return };
        let analyzer = &self.sensitivity_engine.analyzer;
        let curve = analyzer.samples.iter().map(|s| (s.cm_360, s.composite_score)).collect();
        let mut result = run.build_result(curve, analyzer.find_optimal_cm_360(), chrono::Utc::now());
        // Every segment, the last one included, was banked when it finished.
        let totals = self.calibration_totals;
        let aim = self.aim.take_log();
        self.aim.reset();
        // Before pooling: the report reads this run's own answer and left-out tests.
        let report = habits::trust::habit_report(&aim, &result);
        result.habits = Some(report.clone());
        // Stamped with the result's time so History can tell which session is the latest calibration.
        self.record_session("Calibration".into(), result.at, result.cm_360, totals.score, totals.duration_secs, Some(aim), Some(report), None);
        let profile_id = self.data_manager.profile_manager.active_profile_id.clone().unwrap_or_else(|| "default".into());
        if let Err(e) = self.data_manager.archive_calibration(&profile_id, &result) {
            log::warn!("Could not archive the calibration: {e}");
        }
        // Pool with recent runs at this DPI: one run is ±13–19%, and runs agree within that.
        let now = chrono::Utc::now();
        let record = run.run_record(self.player_dpi, result.at);

        self.update_profile(|p| {
            p.recent_runs.push(record);
            pooling::keep_recent(&mut p.recent_runs, now);
            let picked = pooling::runs_to_pool(&p.recent_runs, now, p.dpi);
            // A run with no answer of its own isn't pooled; then it shows as no answer rather
            // than as a pool of the runs before it.
            let latest_in = picked.iter().any(|r| r.at == result.at);
            if latest_in && picked.len() > 1 {
                // Only the newest runs that agree are pooled: fitting runs that found very
                // different values together gives a result wider than any of them.
                let agreeing = pooling::newest_agreeing(&picked);
                match (agreeing.len() > 1).then(|| pooling::pool(&agreeing)).flatten() {
                    Some(pooled) => result.apply_pool(&pooled),
                    None => result.runs_disagree = true,
                }
            }
            p.last_calibration = Some(result);
        });
        self.audio_manager.play_complete();

        self.session = PlaySession {
            mode: PlayMode::Practice,
            paused: false,
        };
        self.task_manager.set_task(TaskType::Flick);
        self.set_cm_360(self.profile_cm_360());
        self.ui_state.show_all_games = false;
        self.ui_state.replay.arm_autoplay();
        self.open_menu(Screen::Sensitivity);
    }

    fn quit_calibration(&mut self) {
        self.sensitivity_engine.reset_calibration();
        self.calibration_run = None;
        self.pending_curve.clear();
        self.aim.reset();
        self.session = PlaySession {
            mode: PlayMode::Practice,
            paused: false,
        };
        self.current_session_points.clear();
        self.task_manager.set_task(TaskType::Flick);
        self.set_cm_360(self.profile_cm_360());
        self.open_menu(Screen::Home);
    }

    fn resume(&mut self) {
        if self.session.mode == PlayMode::Calibration {
            self.session.paused = false;
            // The interrupted segment is played again from the start.
            if let Some(run) = self.calibration_run.as_mut() {
                run.restart_segment();
            }
            self.aim.discard_pending();
            self.begin_segment(3.0);
        }
        self.close_menu();
    }

    fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::Navigate(screen) => self.open_menu(screen),
            UiAction::StartCalibration => self.start_calibration(),
            UiAction::QuitCalibration => self.quit_calibration(),
            UiAction::StartPractice(task) => {
                self.set_cm_360(self.profile_cm_360());
                self.start_run(PlayMode::Practice, task);
            }
            UiAction::Retry => {
                if self.session.mode == PlayMode::Practice {
                    let task = self.task_manager.current_type;
                    self.start_run(PlayMode::Practice, task);
                }
            }
            UiAction::Resume => self.resume(),
            UiAction::UseLatestRunOnly => self.update_profile(|p| {
                let Some(cal) = p.last_calibration.as_mut() else { return };
                p.recent_runs.retain(|r| r.at == cal.at);
                let latest: Vec<&pooling::RunRecord> = p.recent_runs.iter().collect();
                if let Some(alone) = pooling::pool(&latest) {
                    cal.apply_pool(&alone);
                }
            }),
            UiAction::ApplySensitivity(cm) => {
                self.update_profile(|p| {
                    p.current_cm_360 = cm;
                    p.sens_known = Some(true);
                });
                self.set_cm_360(cm);
            }
            UiAction::CompleteOnboarding { dpi, game_id, cm_360, sens_known } => {
                self.update_profile(|p| {
                    p.dpi = dpi;
                    p.primary_game_id = game_id;
                    p.current_cm_360 = cm_360;
                    p.sens_known = Some(sens_known);
                    p.onboarded = true;
                });
                self.set_dpi(dpi);
                self.set_cm_360(cm_360);
                self.ui_state.onboarding = None;
                self.ui_state.dpi_input = None;
                self.ui_state.screen = Screen::Home;
            }
            UiAction::RerunSetup => {
                if let Some(p) = self.data_manager.profile_manager.get_active_profile() {
                    self.ui_state.onboarding = Some(OnboardingDraft::from_profile(p));
                }
            }
            UiAction::SaveSetup { setup, start_after } => {
                self.update_profile(|p| p.setup = Some(setup));
                self.ui_state.questions = None;
                if start_after {
                    self.start_calibration();
                }
            }
            UiAction::OpenSetupQuestions => {
                if let Some(p) = self.data_manager.profile_manager.get_active_profile() {
                    self.ui_state.questions = Some(QuestionsDraft::from_setup(p.setup.as_ref(), false, true));
                }
            }
            UiAction::CancelQuestions => self.ui_state.questions = None,
            UiAction::SaveDpi(dpi) => {
                self.end_paused_practice();
                self.update_profile(|p| p.dpi = dpi);
                self.set_dpi(dpi);
            }
            UiAction::SetMainGame(id) => self.update_profile(|p| p.primary_game_id = id),
            UiAction::SetAudio { enabled, volume, music_volume } => {
                self.audio_manager.is_enabled = enabled;
                self.audio_manager.volume = volume;
                self.audio_manager.set_music_volume(music_volume);
                self.settings.sound_effects = enabled;
                self.settings.effects_volume = volume;
                self.settings.music_volume = music_volume;
                self.save_settings();
            }
            UiAction::SetDisplayMode(mode) => self.set_display_mode(mode),
            UiAction::SetCrosshair(style) => self.update_profile(|p| p.crosshair = style),
            UiAction::CreateProfile(name) => {
                let game = self
                    .data_manager
                    .profile_manager
                    .get_active_profile()
                    .map(|p| p.primary_game_id.clone())
                    .unwrap_or_else(|| "valorant".into());
                // create_profile makes the new profile active; it starts with first-run setup.
                self.data_manager
                    .profile_manager
                    .create_profile(&name, self.player_dpi, &game);
                self.after_profile_switch();
            }
            UiAction::SelectProfile(id) => {
                if self.data_manager.profile_manager.set_active_profile(&id) {
                    self.after_profile_switch();
                }
            }
            UiAction::DeleteProfile(id) => {
                let was_active =
                    self.data_manager.profile_manager.active_profile_id.as_deref() == Some(id.as_str());
                if self.data_manager.profile_manager.delete_profile(&id) && was_active {
                    self.after_profile_switch();
                }
            }
        }
    }

    // ---------- frame ----------

    fn update(&mut self) {
        let dt = self.timing.update();
        let playing = !self.ui_state.menu_open && !self.session.paused;

        // Paired micro and tracking play a slightly faster or slower side of the block's
        // sensitivity; everything else plays at factor 1.0. The HUD keeps the block's value.
        let factor = match self.session.mode {
            PlayMode::Calibration => self.task_manager.sens_factor() as f64,
            PlayMode::Practice => 1.0,
        };
        self.rad_per_count =
            SensitivityConverter::cm_360_to_rad_per_count(self.player_dpi, self.current_cm_360 * factor);

        // 1. Raw mouse deltas (only while the cursor is captured)
        if self.input.is_locked() {
            let (dx, dy) = self.input.consume_mouse_delta();
            if dx != 0.0 || dy != 0.0 {
                self.sensitivity_engine.path_recorder.record_sample(dx, dy);
                let now = self.now();
                self.aim.push_mouse(now, dx, dy, self.rad_per_count.to_degrees());
                self.camera.rotate(dx, dy, self.rad_per_count);
            }
        }

        let crosshair_ray = Ray {
            origin: self.camera.position,
            direction: self.camera.forward(),
        };

        // 2. Shooting — only once targets are live; clicks during the countdown are ignored.
        if self.input.consume_left_click()
            && self.input.is_locked()
            && playing
            && self.task_manager.state == TaskState::Running
            && self.task_manager.takes_shots()
        {
            self.on_shot(crosshair_ray);
        }

        // Countdown beeps
        if let TaskState::Countdown { remaining_secs } = self.task_manager.state {
            let current_sec = remaining_secs.ceil() as i32;
            if current_sec < self.last_countdown_sec && current_sec > 0 {
                self.audio_manager.play_countdown_beep(false);
                self.last_countdown_sec = current_sec;
            }
        }

        // 3. Targets advance only while actually playing
        if playing {
            let was_running = self.task_manager.state == TaskState::Running;
            let was_counting = matches!(self.task_manager.state, TaskState::Countdown { .. });
            let expired_before = self.task_manager.expired_targets();
            self.task_manager.update(dt, crosshair_ray);

            // The first shot's timer and path start when targets appear, not when the countdown does.
            // A target that timed out unshot starts a new one too, so the next shot's path and
            // reaction time don't include aiming at a target that is gone.
            let target_expired = self.task_manager.expired_targets() > expired_before;
            if was_counting && self.task_manager.state == TaskState::Running {
                self.audio_manager.play_countdown_beep(true);
            }
            if (was_counting && self.task_manager.state == TaskState::Running) || target_expired {
                self.sensitivity_engine.path_recorder.start_shot();
            }

            // A tracking target ticks while the crosshair stays on it: that is when it scores.
            if self.track_ticker.update(dt, self.task_manager.on_target_now()) {
                self.audio_manager.play(Sfx::TrackTick, None);
            }

            // A target that appeared outside the view gets a sound from its side.
            let targets = self.task_manager.get_render_targets();
            for position in self.spawn_watcher.update(&targets) {
                if let Some(dir) = self.camera.offscreen_direction(position) {
                    self.audio_manager.play_spawn_cue(dir.x);
                }
            }

            // Aim habits: the setting being played, the aim on the live target, tracking windows.
            let calibration = (self.session.mode == PlayMode::Calibration)
                .then(|| self.calibration_run.as_ref())
                .flatten()
                .map(|run| {
                    let counted = !run.in_warmup() && run.segment().is_some_and(|s| s.counted);
                    (run.block_number() as u8, counted)
                });
            // A new target takes the side it is played at, known only after the update;
            // tracking windows that just completed belong to the side before it.
            let factor_now = match self.session.mode {
                PlayMode::Calibration => self.task_manager.sens_factor() as f64,
                PlayMode::Practice => 1.0,
            };
            let target_ctx = PlayContext::for_play(self.current_cm_360, factor_now, calibration);
            let tracking_ctx = PlayContext::for_play(self.current_cm_360, factor, calibration);
            let now = self.now();
            let windows = self.task_manager.take_tracking_windows();
            self.aim.observe_frame(
                now,
                self.camera.position,
                self.camera.forward(),
                self.task_manager.current_aim_target(),
                target_ctx,
                tracking_ctx,
                &windows,
            );
            // Pairs of the paired micro and tracking tests finished this tick.
            let pairs = self.task_manager.take_pair_results();
            if self.session.records_calibration() {
                if let Some(run) = self.calibration_run.as_mut() {
                    if let Some(seg) = run.segment() {
                        for pair in pairs {
                            run.record_pair(seg.kind, pair);
                        }
                    }
                }
            }

            if was_running
                && self.task_manager.state == TaskState::Finished
                && self.session.mode == PlayMode::Calibration
            {
                self.on_segment_finished();
            }

            if was_running
                && self.task_manager.state == TaskState::Finished
                && self.task_manager.current_type != TaskType::Calibration
            {
                self.audio_manager.play_complete();
                if let Some(m) = self.task_manager.last_completed_metrics.clone() {
                    self.aim.commit();
                    let aim = self.aim.take_log();
                    self.aim.reset();
                    let tracking = self.task_manager.tracking_stats();
                    self.record_session(m.task_name, chrono::Utc::now(), self.current_cm_360, m.score, m.duration_secs, Some(aim), None, tracking);
                }
                // Free the cursor so the result card's buttons can be clicked.
                self.lock_cursor(false);
            }
        }

        // Menu music in menus and on result cards, quieter game music while targets are up.
        let in_game = !self.ui_state.menu_open
            && matches!(self.task_manager.state, TaskState::Countdown { .. } | TaskState::Running);
        self.audio_manager.set_music(Some(if in_game { Music::Game } else { Music::Menu }));

        // 4. Camera uniform to GPU
        self.renderer.update_camera(&self.camera);
    }

    fn on_shot(&mut self, ray: Ray) {
        let hit = self.task_manager.on_click(ray);
        let now = self.now();
        let off = self.aim.on_click(now, ray.origin, ray.direction, hit);
        self.shot_counter += 1;
        if hit {
            self.audio_manager.play_hit();
        } else {
            self.audio_manager.play_miss();
        }

        if let Some(path) = self.sensitivity_engine.path_recorder.end_shot() {
            let rt_ms = path.end_time.duration_since(path.start_time).as_millis() as f32;
            self.current_session_points.push(MetricPoint {
                shot_number: self.shot_counter,
                hit,
                reaction_time_ms: rt_ms,
                offset_from_center_m: off.map_or(0.0, |o| o.offset_m),
                target_radius_m: off.map_or(0.0, |o| o.radius_m),
                precision_percent: off.map_or(0.0, |o| {
                    if hit && o.radius_m > 0.0 {
                        (100.0 * (1.0 - o.offset_m / o.radius_m)).clamp(0.0, 100.0)
                    } else {
                        0.0
                    }
                }),
                mouse_distance_px: path.total_mouse_distance,
                peak_velocity_px_s: path.peak_velocity,
            });

            if records_calibration_shot(self.session, self.task_manager.state) {
                if let Some(run) = self.calibration_run.as_mut() {
                    match run.segment() {
                        // Flick is judged by its pairs (time to hit); the shot path only feeds
                        // the score graph now.
                        Some(seg) if seg.kind == TestKind::Flick => {
                            if seg.counted {
                                let accuracy = if hit { 100.0 } else { 0.0 };
                                self.pending_curve.push((self.current_cm_360, accuracy, rt_ms as f64, path));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        self.sensitivity_engine.path_recorder.start_shot();
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let targets = self.task_manager.get_render_targets();

        let raw_input = self.egui_state.take_egui_input(&self.renderer.window);
        let profile = self
            .data_manager
            .profile_manager
            .get_active_profile()
            .cloned()
            .unwrap_or_else(|| {
                PlayerProfile::new("default".into(), "Player".into(), self.player_dpi, "valorant".into())
            });
        // Arrows at the screen edge toward targets outside the view.
        let offscreen_targets: Vec<glam::Vec2> = self
            .task_manager
            .get_render_targets()
            .iter()
            .filter_map(|(p, _)| self.camera.offscreen_direction(*p))
            .collect();
        let model = UiModel {
            profile: &profile,
            profiles: &self.data_manager.profile_manager.profiles,
            games: self.sensitivity_engine.db.list(),
            history: &self.history,
            session: self.session,
            task_state: self.task_manager.state,
            task_name: self.task_manager.current_task_name(),
            last_metrics: self.task_manager.last_completed_metrics.as_ref(),
            calibration: self
                .calibration_run
                .as_ref()
                .filter(|_| self.session.mode == PlayMode::Calibration)
                .map(|r| CalibrationProgress::from_run(r, profile.current_cm_360)),
            current_cm_360: self.current_cm_360,
            audio_enabled: self.audio_manager.is_enabled,
            volume: self.audio_manager.volume,
            music_volume: self.audio_manager.music_volume(),
            display_mode: self.settings.display_mode,
            fps: self.timing.fps(),
            offscreen_targets: &offscreen_targets,
            tracking: self.task_manager.tracking_stats(),
        };
        let ui_state = &mut self.ui_state;
        let mut actions = Vec::new();
        let mut full_output = self.egui_ctx.run(raw_input, |ctx| {
            actions = render_ui(ctx, ui_state, &model);
        });
        drop(model);
        for action in actions {
            self.apply_action(action);
        }

        // egui-winit shows the cursor whenever its icon changes or the pointer re-enters the
        // window; while aiming, keep it hidden.
        if self.input.is_locked() {
            full_output.platform_output.cursor_icon = egui::CursorIcon::None;
        }
        self.egui_state
            .handle_platform_output(&self.renderer.window, full_output.platform_output);

        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        for (id, delta) in &full_output.textures_delta.set {
            self.renderer
                .egui_renderer
                .update_texture(&self.renderer.device, &self.renderer.queue, *id, delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.renderer.size.width, self.renderer.size.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        // The active profile's crosshair, also after a profile switch.
        let crosshair = self.data_manager.profile_manager.get_active_profile().map(|p| p.crosshair).unwrap_or_default();
        self.renderer.set_crosshair(crosshair);
        let result = self
            .renderer
            .render(&targets, self.task_manager.on_target_now(), &clipped_primitives, &screen_descriptor);

        for id in &full_output.textures_delta.free {
            self.renderer.egui_renderer.free_texture(id);
        }

        result
    }

    // ---------- keyboard ----------

    fn on_escape(&mut self) {
        let in_calibration = self.session.mode == PlayMode::Calibration;
        if in_calibration && !self.ui_state.menu_open {
            if self.session.paused {
                self.resume();
            } else {
                // Bank this segment now: the task's clock keeps running while paused, and
                // Resume restarts the task.
                if self.task_manager.state == TaskState::Running {
                    let m = self.task_manager.current_metrics();
                    self.calibration_totals.add_segment(m.score, m.duration_secs);
                }
                self.session.paused = true;
                self.lock_cursor(false);
            }
            return;
        }
        if self.ui_state.menu_open {
            // Esc closes the questions: no run starts and nothing is saved.
            if self.ui_state.questions.is_some() {
                self.apply_action(UiAction::CancelQuestions);
                return;
            }
            // Esc cancels "Run setup again" (first-run setup can't be skipped).
            if self.ui_state.onboarding.as_ref().is_some_and(|d| d.cancellable) {
                self.ui_state.onboarding = None;
                return;
            }
            // Back to a run in progress only; with nothing to play the menu stays open.
            if esc_closes_menu(
                self.task_manager.state,
                self.ui_state.onboarding.is_some() || self.ui_state.questions.is_some(),
            ) {
                self.close_menu();
            }
        } else if self.task_manager.state == TaskState::Finished {
            self.open_menu(Screen::Practice);
        } else {
            let screen = self.ui_state.screen;
            self.open_menu(screen);
        }
    }

    fn on_key(&mut self, key: KeyCode) {
        let practicing = !self.ui_state.menu_open && self.session.mode == PlayMode::Practice;
        let practice_task = match key {
            KeyCode::Digit1 => Some(TaskType::Flick),
            KeyCode::Digit2 => Some(TaskType::Tracking),
            KeyCode::Digit3 => Some(TaskType::Switching),
            KeyCode::Digit4 => Some(TaskType::MicroAdjust),
            KeyCode::Digit5 => Some(TaskType::Speed),
            KeyCode::Digit6 => Some(TaskType::MovingTargets),
            _ => None,
        };
        match key {
            KeyCode::F11 => self.set_display_mode(self.settings.display_mode.toggled()),
            KeyCode::Enter | KeyCode::NumpadEnter if self.alt_held => {
                self.set_display_mode(self.settings.display_mode.toggled())
            }
            KeyCode::Escape => self.on_escape(),
            _ if practicing && practice_task.is_some() => {
                self.apply_action(UiAction::StartPractice(practice_task.unwrap()));
            }
            KeyCode::KeyR if practicing => self.apply_action(UiAction::Retry),
            KeyCode::KeyC if practicing => self.open_menu(Screen::Sensitivity),
            _ => {}
        }
    }
}

#[derive(Default)]
struct App {
    state: Option<GameState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let icon = ui::app_icon::app_icon_rgba(ui::app_icon::ICON_SIZE);
            let icon = winit::window::Icon::from_rgba(icon, ui::app_icon::ICON_SIZE, ui::app_icon::ICON_SIZE).ok();
            let data_manager = DataManager::new();
            let settings = data_manager.load_settings();
            let monitors: Vec<_> = event_loop.available_monitors().collect();
            let screens: Vec<_> = monitors.iter().map(display::screen_rect).collect();
            // The window comes back where it was, if that screen is still there.
            let placement = settings.window.and_then(|p| display::screen_of(&p, &screens).map(|i| (p, i)));
            let mut window_attributes = Window::default_attributes()
                .with_title(ui::app_icon::APP_NAME)
                .with_window_icon(icon.clone())
                .with_min_inner_size(winit::dpi::LogicalSize::new(1024, 640));
            window_attributes = match placement {
                Some((p, _)) => window_attributes
                    .with_inner_size(winit::dpi::PhysicalSize::new(p.width, p.height))
                    .with_position(winit::dpi::PhysicalPosition::new(p.x, p.y))
                    .with_maximized(p.maximized),
                None => window_attributes.with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
            };
            if settings.display_mode == DisplayMode::Fullscreen {
                let monitor = placement
                    .map(|(_, i)| monitors[i].clone())
                    .or_else(|| event_loop.primary_monitor())
                    .or_else(|| monitors.first().cloned());
                window_attributes = window_attributes.with_fullscreen(monitor.map(display::fullscreen_on));
            }
            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::WindowAttributesExtWindows;
                window_attributes = window_attributes.with_taskbar_icon(icon);
            }

            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
            let state = pollster::block_on(GameState::new(window, data_manager, settings));
            self.state = Some(state);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(state) = self.state.as_mut() {
            if let DeviceEvent::MouseMotion { delta } = event {
                if state.input.is_locked() {
                    state.input.add_mouse_delta(delta.0 as f32, delta.1 as f32);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(s) if s.renderer.window.id() == window_id => s,
            _ => return,
        };

        let response = state.egui_state.on_window_event(&state.renderer.window, &event);
        if response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                state.input.set_cursor_locked(&state.renderer.window, false);
                state.track_window_placement();
                state.save_settings();
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.renderer.resize(physical_size);
                state.camera.update_aspect_ratio(physical_size.width, physical_size.height);
                state.input.recenter_if_locked(&state.renderer.window);
                state.track_window_placement();
            }
            WindowEvent::Moved(_) => {
                state.input.recenter_if_locked(&state.renderer.window);
                state.track_window_placement();
            }
            WindowEvent::Focused(true) => {
                state.input.recenter_if_locked(&state.renderer.window);
            }
            // Exclusive fullscreen stays on top of everything: step aside when another app
            // takes over (Alt+Tab), as fullscreen games do.
            WindowEvent::Focused(false) => {
                // Alt+Tab: the Alt release goes to the other app.
                state.alt_held = false;
                if matches!(state.renderer.window.fullscreen(), Some(Fullscreen::Exclusive(_))) {
                    state.renderer.window.set_minimized(true);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.alt_held = modifiers.state().alt_key();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => state.on_key(key),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: btn_state,
                ..
            } => {
                state.input.set_left_mouse(btn_state == ElementState::Pressed);
                let can_capture = !state.ui_state.menu_open
                    && !state.session.paused
                    && state.task_manager.state != TaskState::Finished;
                if btn_state == ElementState::Pressed && !state.input.is_locked() && can_capture {
                    state.input.set_cursor_locked(&state.renderer.window, true);
                }
            }
            WindowEvent::RedrawRequested => {
                state.update();

                // Minimised: nothing to draw into until the window comes back.
                if renderer::can_draw(state.renderer.window.inner_size()) {
                    if let Err(e) = state.render() {
                        match renderer::on_surface_error(&e) {
                            renderer::SurfaceAction::Reconfigure => {
                                state.renderer.reconfigure();
                                let size = state.renderer.size;
                                state.camera.update_aspect_ratio(size.width, size.height);
                            }
                            renderer::SurfaceAction::SkipFrame => log::debug!("Frame skipped: {e:?}"),
                            renderer::SurfaceAction::Exit => event_loop.exit(),
                            renderer::SurfaceAction::Report => log::error!("Render error: {e:?}"),
                        }
                    }
                }

                state.renderer.window.request_redraw();
            }
            _ => (),
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
