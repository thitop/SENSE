//! Render every screen and HUD state offscreen to PNG files, for checking the UI without
//! clicking through the game.
//!
//!     cargo run --example ui_preview -- [output_dir]     (default: target/ui-preview)

use std::fs;
use std::path::Path;

use aim_trainer::data::{
    AggregatedTaskMetrics, CalibrationResult, CalibrationRound, EstimateRecord, MetricPoint, NextPick,
    PlayerProfile, RoundGuess, RoundSignals, RoundTrace, RunDiagnostics, SessionHistory, SessionRecord,
    TrackingStats,
};
use aim_trainer::sensitivity::blocks::SignalCounts;
use aim_trainer::game::calibration::{CalibrationProgress, PlayMode, PlaySession};
use aim_trainer::game::calibration_run::CalibrationRun;
use aim_trainer::sensitivity::combiner::TestKind;
use aim_trainer::game::tasks::{TaskMetrics, TaskState};
use aim_trainer::sensitivity::games::GameDatabase;
use aim_trainer::sensitivity::path_recorder::ShotTrajectoryPattern;
use aim_trainer::sensitivity::psa::{PsaCalibrator, PsaRoundConclusion};
use aim_trainer::ui::{render_ui, theme, OnboardingDraft, Screen, UiModel, UiState};
use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::analysis::{Dir, Habit, HabitFinding};
use aim_trainer::habits::record::{AimKind, AimLog, PlayContext, TargetRecord, TrackingRecord};
use aim_trainer::habits::{HabitReport, LeftOutNote};
use chrono::{Duration, Utc};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb; // matches the game's sRGB surface

struct Scene {
    name: &'static str,
    size: (u32, u32),
    state: UiState,
    profile: PlayerProfile,
    history: SessionHistory,
    session: PlaySession,
    task_state: TaskState,
    calibration: Option<CalibrationProgress>,
    metrics: Option<TaskMetrics>,
    task_name: &'static str,
    offscreen: Vec<glam::Vec2>,
    tracking: Option<TrackingStats>,
}

/// One round of the sample run: tested cm, range before it, what it concluded, the counts
/// (too fast, too slow, tie) for Flicking / Micro-adjustment / Tracking, the guess after it, the
/// next pick and the range that pick was allowed within.
struct SampleRound {
    cm: f64,
    range: (f64, f64),
    conclusion: PsaRoundConclusion,
    counts: [(usize, usize, usize); 3],
    guess: RoundGuess,
    next: NextPick,
    pick: (f64, f64),
}

/// A 7-round run that settles on 34.2 cm, as `BlockScheduler` played it with these counts
/// (taken from a real scheduler run). Round 2 leans "too slow" by chance, so the fit runs off
/// below the search range and round 3 goes as far as it can (15 cm); later rounds bring the
/// guess back up past the range, as the fit's reach allows.
fn sample_rounds() -> Vec<SampleRound> {
    use NextPick::*;
    use PsaRoundConclusion::*;
    let guess = |cm_360, rel_uncertainty| RoundGuess::Guess { cm_360, rel_uncertainty };
    let r = |cm, range, conclusion, counts, guess, next, pick| SampleRound {
        cm,
        range,
        conclusion,
        counts,
        guess,
        next,
        pick,
    };
    // The pick range is the bounds after round 1 (bisection), then the fit's reach: the bounds
    // after the round, widened by 20% each way within 15..80.
    vec![
        r(47.5, (15.0, 80.0), TooSlowNeedFaster, [(1, 6, 2), (1, 4, 1), (4, 9, 3)],
            RoundGuess::OneSensitivity, Midpoint { cm_360: 31.25 }, (15.0, 47.5)),
        r(31.25, (15.0, 47.5), TooSlowNeedFaster, [(2, 5, 2), (1, 4, 1), (5, 8, 3)],
            guess(14.17, 3.27), FitClamped { guess_cm: 14.17, cm_360: 15.0 }, (15.0, 37.5)),
        r(15.0, (15.0, 31.25), TooFastNeedSlower, [(8, 0, 1), (5, 1, 0), (11, 3, 2)],
            guess(27.1, 0.20), FitEstimate { cm_360: 27.1 }, (15.0, 37.5)),
        r(27.1, (15.0, 31.25), TooFastNeedSlower, [(8, 1, 0), (5, 1, 0), (12, 2, 2)],
            guess(32.4, 0.15), FitEstimate { cm_360: 32.4 }, (21.03, 37.5)),
        r(32.4, (26.29, 31.25), TooFastNeedSlower, [(6, 2, 1), (3, 2, 1), (8, 6, 2)],
            guess(34.07, 0.14), FitEstimate { cm_360: 34.07 }, (25.0, 37.5)),
        r(34.07, (31.25, 31.25), BalancedNarrow, [(3, 3, 3), (2, 2, 2), (6, 6, 4)],
            guess(34.09, 0.12), FitEstimate { cm_360: 34.09 }, (25.0, 37.5)),
        r(34.09, (31.25, 31.25), TooFastNeedSlower, [(3, 3, 3), (2, 2, 2), (8, 7, 1)],
            guess(34.24, 0.11), Last, (31.25, 31.25)),
    ]
}

fn sample_diagnostics() -> RunDiagnostics {
    let est = |test: &str, cm_360: f64, rel_uncertainty: f64| EstimateRecord {
        test: test.into(),
        cm_360,
        rel_uncertainty,
    };
    let rounds = sample_rounds()
        .into_iter()
        .map(|r| RoundSignals {
            tested_cm_360: r.cm,
            signals: ["Flicking", "Micro-adjustment", "Tracking"]
                .iter()
                .zip(r.counts)
                .map(|(test, (too_fast, too_slow, direct))| {
                    (test.to_string(), SignalCounts { too_fast, too_slow, direct })
                })
                .collect(),
            pairs: vec![],
            trace: Some(RoundTrace {
                low_cm_360: r.range.0,
                high_cm_360: r.range.1,
                pick_low_cm_360: r.pick.0,
                pick_high_cm_360: r.pick.1,
                guess: r.guess,
                next: r.next,
            }),
        })
        .collect();
    RunDiagnostics {
        rounds,
        estimates: vec![
            est("Flicking", 35.0, 0.17),
            est("Micro-adjustment", 32.1, 0.25),
            est("Tracking", 34.6, 0.19),
            est("Flick far", 33.0, 0.25),
        ],
        fit_cm_360: Some(34.2),
        result_rel_uncertainty: Some(0.11),
        ..Default::default()
    }
}

/// A real run with no answer: the player wants it slower than the search allows, so nearly every
/// pair says "too fast" at every sensitivity tested and the dots climb toward 80 cm.
fn no_answer_result(at: chrono::DateTime<Utc>) -> CalibrationResult {
    use aim_trainer::sensitivity::combiner::Signal;
    use aim_trainer::sensitivity::paired::PairResult;
    let mut run = CalibrationRun::new(7);
    let mut n = 0usize;
    while !run.is_completed() {
        let seg = run.segment().unwrap();
        if seg.counted {
            for _ in 0..6 {
                n += 1;
                // One pair in seven leans the other way.
                let signal = if n % 7 == 0 { Signal::TooSlow } else { Signal::TooFast };
                if matches!(seg.kind, TestKind::Micro | TestKind::Tracking) {
                    let (faster_cost, slower_cost) = if signal == Signal::TooFast { (2.0, 1.0) } else { (1.0, 2.0) };
                    run.record_pair(seg.kind, PairResult { faster_cost, slower_cost, signal, band: None });
                } else {
                    let band = (seg.kind == TestKind::Flick).then_some(FlickBand::ALL[n % 3]);
                    run.record(seg.kind, band, signal);
                }
            }
        }
        run.finish_segment();
    }
    let result = run.build_result(vec![], None, at);
    assert!(result.no_answer, "the no-answer scene needs a run without an answer");
    result
}

fn profile(calibrated: bool) -> PlayerProfile {
    let mut p = PlayerProfile::new("default".into(), "Player".into(), 800.0, "valorant".into());
    p.onboarded = true;
    p.current_cm_360 = 41.6;
    if calibrated {
        p.last_calibration = Some(CalibrationResult {
            cm_360: 34.2,
            confidence: 0.82,
            psa_cm: Some(34.8),
            stat_cm: Some(33.3),
            rounds: sample_rounds()
                .iter()
                .map(|r| CalibrationRound { tested_cm_360: r.cm, conclusion: r.conclusion })
                .collect(),
            samples: (0..70)
                .map(|i| {
                    let x = 30.0 + (i as f64 * 7.3) % 20.0;
                    (x, 80.0 - (x - 34.0).powi(2) * 0.4 + (i % 5) as f64 * 2.0)
                })
                .collect(),
            at: Utc::now() - Duration::hours(3),
            diagnostics: Some(sample_diagnostics()),
            pooled_runs: 1,
            no_answer: false,
            runs_disagree: false,
            habits: None,
        });
    }
    p
}

const TRACKING: TrackingStats = TrackingStats { on_target_pct: 62.4, time_on_target_secs: 18.7, longest_on_target_secs: 3.4 };

fn history() -> SessionHistory {
    let mut h = SessionHistory::new("default".into());
    let now = Utc::now();
    for (i, (task, cm)) in [("Flick Shot", 41.6), ("Calibration", 34.2), ("Smooth Tracking", 41.6), ("Micro-Adjustment", 34.2)]
        .iter()
        .enumerate()
    {
        let points = (0..30)
            .map(|n| MetricPoint {
                shot_number: n + 1,
                hit: n % 6 != 0,
                reaction_time_ms: 380.0 + ((n * 37) % 120) as f32,
                offset_from_center_m: 0.1,
                target_radius_m: 0.35,
                precision_percent: 85.0,
                mouse_distance_px: 400.0,
                peak_velocity_px_s: 2000.0,
            })
            .collect();
        h.add_session(SessionRecord {
            session_id: format!("s{i}"),
            task_type: task.to_string(),
            timestamp: now - Duration::hours(5 * i as i64),
            duration_secs: 40.0,
            tested_cm_360: *cm,
            tested_dpi: 800.0,
            metrics: if *task == "Smooth Tracking" {
                AggregatedTaskMetrics::tracking(TRACKING)
            } else {
                AggregatedTaskMetrics::compute(points, 12_000 - i as u64 * 900, 0)
            },
            aim: None,
            habits: None,
        });
    }
    h
}


fn flick_log(landing: f32, strokes: u8) -> AimLog {
    let target = TargetRecord {
        kind: AimKind::Flick,
        offset_deg: [10.0, 0.0],
        radius_deg: 1.0,
        first_landing: Some(landing),
        strokes,
        crossed_center: false,
        overshoot_deg: 0.0,
        time_to_hit_ms: None,
        misses: vec![],
        band: Some(FlickBand::Far),
        ctx: PlayContext::default(),
    };
    AimLog { targets: vec![target; 14], tracking: vec![] }
}

fn track_log(lag: f32) -> AimLog {
    let w = TrackingRecord {
        mean_along_deg: 0.0,
        lag_ms: lag,
        cross_rms_deg: 0.1,
        mean_speed_deg_s: 10.0,
        coverage: 1.0,
        radius_deg: 1.0,
        horizontal: true,
        ctx: PlayContext::default(),
    };
    AimLog { targets: vec![], tracking: vec![w; 14] }
}

/// Practice sessions played after the calibration, carrying what the aim did.
fn with_practice(mut h: SessionHistory) -> SessionHistory {
    let now = Utc::now();
    let sessions = [
        ("p1", "Flick Shot", 34.0, 150, flick_log(0.9, 1)),
        ("p2", "Flick Shot", 34.5, 100, flick_log(0.86, 1)),
        ("p3", "Flick Shot", 34.2, 20, flick_log(0.8, 3)),
        ("p4", "Smooth Tracking", 41.6, 60, track_log(62.0)),
    ];
    for (id, task, cm, mins_ago, log) in sessions {
        let mut r = h.sessions[0].clone();
        r.session_id = id.into();
        r.task_type = task.into();
        r.timestamp = now - Duration::minutes(mins_ago);
        r.tested_cm_360 = cm;
        r.aim = Some(log);
        h.add_session(r);
    }
    h
}

fn findings() -> Vec<HabitFinding> {
    vec![
        HabitFinding { habit: Habit::FlickShort(FlickBand::Far), value: 0.82, direction: Some(Dir::Left) },
        HabitFinding { habit: Habit::TrackingTrails, value: 62.0, direction: None },
        HabitFinding { habit: Habit::MicroOvershoot, value: 0.41, direction: Some(Dir::SideToSide) },
    ]
}

fn with_habits(scene: &mut Scene, report: HabitReport) {
    scene.history = with_practice(std::mem::take(&mut scene.history));
    if let Some(c) = scene.profile.last_calibration.as_mut() {
        c.habits = Some(report);
    }
}

fn base(name: &'static str, size: (u32, u32), screen: Screen, calibrated: bool) -> Scene {
    let mut state = UiState::new();
    state.screen = screen;
    Scene {
        name,
        size,
        state,
        profile: profile(calibrated),
        history: history(),
        session: PlaySession { mode: PlayMode::Practice, paused: false },
        task_state: TaskState::NotStarted,
        calibration: None,
        metrics: None,
        task_name: "Flick Shot",
        offscreen: Vec::new(),
        tracking: None,
    }
}

fn scenes() -> Vec<Scene> {
    const L: (u32, u32) = (1280, 720);
    const S: (u32, u32) = (1024, 640);
    let mut v = vec![
        base("home", L, Screen::Home, false),
        base("home-calibrated", L, Screen::Home, true),
        base("practice", L, Screen::Practice, true),
        base("practice-small", S, Screen::Practice, true),
        base("sensitivity-result", L, Screen::Sensitivity, true),
        base("sensitivity-result-small", S, Screen::Sensitivity, true),
        base("history", L, Screen::History, true),
        base("settings", L, Screen::Settings, true),
    ];

    let mut detail = base("history-detail", L, Screen::History, true);
    detail.state.open_session = Some("s0".into());
    v.push(detail);
    // Tall enough to show the per-game values below the graph.
    let mut detail_full = base("history-detail-full", (1280, 2200), Screen::History, true);
    detail_full.state.open_session = Some("s0".into());
    v.push(detail_full);

    let mut hab = base("sensitivity-habits", (1280, 1300), Screen::Sensitivity, true);
    with_habits(&mut hab, HabitReport { findings: findings(), ..Default::default() });
    v.push(hab);

    let mut hab_small = base("sensitivity-habits-small", (1024, 1300), Screen::Sensitivity, true);
    with_habits(&mut hab_small, HabitReport { findings: findings(), ..Default::default() });
    v.push(hab_small);

    let mut hab_none = base("sensitivity-habits-none", (1280, 1300), Screen::Sensitivity, true);
    with_habits(&mut hab_none, HabitReport::default());
    v.push(hab_none);

    let mut hab_warm = base("sensitivity-habits-warmup", (1280, 1300), Screen::Sensitivity, true);
    with_habits(
        &mut hab_warm,
        HabitReport {
            findings: findings()[..1].to_vec(),
            warming_up: true,
            left_out: vec![LeftOutNote { test: "Tracking".into(), habit: Habit::TrackingTrails }],
            ..Default::default()
        },
    );
    v.push(hab_warm);

    let mut hist = base("history-habits", (1280, 1000), Screen::History, true);
    hist.history = with_practice(std::mem::take(&mut hist.history));
    hist.state.open_session = Some("p3".into());
    v.push(hist);

    let mut low = base("sensitivity-low-confidence", L, Screen::Sensitivity, true);
    if let Some(c) = low.profile.last_calibration.as_mut() {
        c.cm_360 = 31.8;
        c.confidence = 0.2;
        let est = |test: &str, cm_360: f64, rel_uncertainty: f64| aim_trainer::data::EstimateRecord {
            test: test.into(),
            cm_360,
            rel_uncertainty,
        };
        c.diagnostics = Some(aim_trainer::data::RunDiagnostics {
            estimates: vec![
                est("Flicking", 32.4, 0.12),
                est("Micro-adjustment", 30.9, 0.21),
                est("Tracking", 14.2, 0.30),
                est("Flick far", 33.0, 0.25),
            ],
            left_out: vec!["Tracking".into()],
            ..Default::default()
        });
    }
    v.push(low);

    let mut pooled = base("sensitivity-pooled", L, Screen::Sensitivity, true);
    if let Some(c) = pooled.profile.last_calibration.as_mut() {
        c.pooled_runs = 3;
    }
    v.push(pooled);

    let mut none = base("sensitivity-no-answer", L, Screen::Sensitivity, true);
    if let Some(c) = none.profile.last_calibration.as_mut() {
        c.no_answer = true;
        c.confidence = 0.3;
    }
    v.push(none);

    // The calibration replay under "How we got here", on a page tall enough to show it unscrolled.
    const TALL: (u32, u32) = (1280, 1600);
    let replay_scene = |name, size, step: usize, edit: fn(&mut CalibrationResult)| {
        let mut s = base(name, size, Screen::Sensitivity, true);
        if let Some(c) = s.profile.last_calibration.as_mut() {
            edit(c);
        }
        s.state.replay.go_to(step, sample_rounds().len() + 1);
        s
    };
    let result_step = sample_rounds().len();
    v.push(replay_scene("sensitivity-replay-round", TALL, 2, |_| {}));
    v.push(replay_scene("sensitivity-replay-clamped", TALL, 1, |_| {}));
    v.push(replay_scene("sensitivity-replay-result", TALL, result_step, |_| {}));
    v.push(replay_scene("sensitivity-replay-pooled", TALL, result_step, |c| c.pooled_runs = 3));
    v.push(replay_scene("sensitivity-replay-no-answer", TALL, result_step, |c| {
        *c = no_answer_result(c.at)
    }));
    v.push(replay_scene("sensitivity-replay-legacy", TALL, 2, |c| {
        for r in c.diagnostics.iter_mut().flat_map(|d| d.rounds.iter_mut()) {
            r.trace = None;
        }
    }));
    v.push(replay_scene("sensitivity-replay-small", (1024, 1600), 2, |_| {}));

    let mut empty = base("sensitivity-empty", L, Screen::Sensitivity, false);
    empty.state.convert_value = "0.35".into();
    v.push(empty);

    for (step, name) in [(1, "setup-1"), (2, "setup-2"), (3, "setup-3")] {
        let mut s = base(name, L, Screen::Home, false);
        s.profile.onboarded = false;
        let mut d = OnboardingDraft::from_profile(&s.profile);
        d.step = step;
        d.sens = "0.35".into();
        s.state.onboarding = Some(d);
        v.push(s);
    }

    // "Run setup again" on a profile that's already set up: Cancel is offered.
    let mut rerun = base("setup-rerun", L, Screen::Settings, true);
    rerun.state.onboarding = Some(OnboardingDraft::from_profile(&rerun.profile));
    v.push(rerun);

    // "35" typed instead of "0.35": flagged as implausible.
    let mut typo = base("setup-typo", L, Screen::Home, false);
    typo.profile.onboarded = false;
    let mut d = OnboardingDraft::from_profile(&typo.profile);
    d.step = 3;
    d.sens = "35".into();
    typo.state.onboarding = Some(d);
    v.push(typo);

    let mut psa = PsaCalibrator::new(15.0, 80.0, 7);
    for i in 0..24 {
        psa.record_shot(if i < 10 { ShotTrajectoryPattern::Overshoot } else { ShotTrajectoryPattern::Undershoot });
    }
    let progress = CalibrationProgress::from_psa(&psa);

    let mut cal = base("hud-calibration", L, Screen::Home, false);
    cal.state.menu_open = false;
    cal.session = PlaySession { mode: PlayMode::Calibration, paused: false };
    cal.task_state = TaskState::Running;
    cal.calibration = Some(progress);
    cal.task_name = "Calibration";
    cal.state.show_toast("Sensitivity changed: faster", 0.5);
    v.push(cal);

    let mut paused = base("hud-paused", L, Screen::Home, false);
    paused.state.menu_open = false;
    paused.session = PlaySession { mode: PlayMode::Calibration, paused: true };
    paused.task_state = TaskState::Running;
    paused.calibration = Some(progress);
    paused.task_name = "Calibration";
    v.push(paused);

    // Multi-test run: warm-up, then round 2 on its tracking segment, and the countdown before it.
    let mut run = CalibrationRun::new(7);
    let warm = CalibrationProgress::from_run(&run, 41.6);
    let mut warmup = base("hud-run-warmup", L, Screen::Home, false);
    warmup.state.menu_open = false;
    warmup.session = PlaySession { mode: PlayMode::Calibration, paused: false };
    warmup.task_state = TaskState::Running;
    warmup.calibration = Some(warm);
    warmup.task_name = "Flick Shot";
    v.push(warmup);

    while run.in_warmup() || run.block_number() < 2 {
        run.finish_segment();
    }
    while run.segment().map(|s| s.kind) != Some(TestKind::Tracking) {
        run.finish_segment();
    }
    let block = CalibrationProgress::from_run(&run, 41.6);
    let mut tracking = base("hud-run-tracking", L, Screen::Home, false);
    tracking.state.menu_open = false;
    tracking.session = PlaySession { mode: PlayMode::Calibration, paused: false };
    tracking.task_state = TaskState::Running;
    tracking.calibration = Some(block);
    tracking.task_name = "Smooth Tracking";
    v.push(tracking);

    let mut offscreen = base("hud-run-offscreen", L, Screen::Home, false);
    offscreen.state.menu_open = false;
    offscreen.session = PlaySession { mode: PlayMode::Calibration, paused: false };
    offscreen.task_state = TaskState::Running;
    offscreen.calibration = Some(block);
    offscreen.offscreen = vec![glam::Vec2::new(-0.96, 0.28), glam::Vec2::new(0.5, -0.87)];
    v.push(offscreen);

    let mut next = base("hud-run-countdown", L, Screen::Home, false);
    next.state.menu_open = false;
    next.session = PlaySession { mode: PlayMode::Calibration, paused: false };
    next.task_state = TaskState::Countdown { remaining_secs: 1.4 };
    next.calibration = Some(block);
    next.task_name = "Smooth Tracking";
    next.state.show_toast("Sensitivity changed: slower", 0.5);
    v.push(next);

    let mut countdown = base("hud-countdown", L, Screen::Home, false);
    countdown.state.menu_open = false;
    countdown.task_state = TaskState::Countdown { remaining_secs: 2.3 };
    v.push(countdown);

    let mut finished = base("hud-finished", L, Screen::Home, false);
    finished.state.menu_open = false;
    finished.task_state = TaskState::Finished;
    finished.metrics = Some(TaskMetrics {
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
    });
    v.push(finished);

    // Practice tracking: live score in the bar, the target green while on it.
    let mut live = base("hud-practice-tracking", L, Screen::Home, false);
    live.state.menu_open = false;
    live.task_state = TaskState::Running;
    live.task_name = "Smooth Tracking";
    live.tracking = Some(TRACKING);
    v.push(live);

    let mut tracked = base("hud-tracking-finished", L, Screen::Home, false);
    tracked.state.menu_open = false;
    tracked.task_state = TaskState::Finished;
    tracked.task_name = "Smooth Tracking";
    tracked.tracking = Some(TRACKING);
    tracked.metrics = Some(TaskMetrics {
        task_name: "Smooth Tracking".into(),
        score: TRACKING.score(),
        accuracy: TRACKING.on_target_pct,
        avg_reaction_time_ms: 0.0,
        shots_fired: 0,
        hits: 0,
        misses: 0,
        duration_secs: 30.0,
        custom_metric_label: "",
        custom_metric_value: String::new(),
    });
    v.push(tracked);

    let mut track_detail = base("history-tracking", L, Screen::History, true);
    track_detail.state.open_session = Some("s2".into());
    v.push(track_detail);
    v
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

async fn gpu() -> Gpu {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("no GPU adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .expect("no GPU device");
    Gpu { device, queue }
}

fn render(gpu: &Gpu, mut scene: Scene, out_dir: &Path) {
    let (w, h) = scene.size;
    let db = GameDatabase::new();
    let profiles = vec![scene.profile.clone()];
    let model = UiModel {
        profile: &scene.profile,
        profiles: &profiles,
        games: db.list(),
        history: &scene.history,
        session: scene.session,
        task_state: scene.task_state,
        task_name: scene.task_name,
        last_metrics: scene.metrics.as_ref(),
        calibration: scene.calibration,
        current_cm_360: scene.profile.current_cm_360,
        audio_enabled: true,
        volume: 0.6,
        music_volume: 0.5,
        fps: 240,
        offscreen_targets: &scene.offscreen,
        tracking: scene.tracking,
    };

    let ctx = egui::Context::default();
    theme::install(&ctx);
    let mut renderer = egui_wgpu::Renderer::new(&gpu.device, FORMAT, None, 1, false);
    let input = |t: f64| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32))),
        time: Some(t),
        ..Default::default()
    };

    // A few frames: fonts load on the first, layouts settle and fade-ins finish on the next.
    let mut output = None;
    for i in 0..4 {
        let o = ctx.run(input(0.5 + i as f64 * 0.4), |ctx| {
            render_ui(ctx, &mut scene.state, &model);
        });
        for (id, delta) in &o.textures_delta.set {
            renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
        output = Some(o);
    }
    let output = output.unwrap();
    let prims = ctx.tessellate(output.shapes, output.pixels_per_point);
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point: 1.0 };

    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let extra = renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &prims, &screen);
    {
        // Stand-in for the 3D scene behind HUD overlays (#15171B, as linear).
        let bg = wgpu::Color { r: 0.0075, g: 0.0085, b: 0.0116, a: 1.0 };
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(bg), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &prims, &screen);
    }
    let row = w * 4; // 1280*4 and 1024*4 are both multiples of 256
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(extra.into_iter().chain([encoder.finish()]));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map failed"));
    gpu.device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range().to_vec();

    let path = out_dir.join(format!("{}.png", scene.name));
    let file = fs::File::create(&path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("{}", path.display());
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "target/ui-preview".into());
    let out = Path::new(&out);
    fs::create_dir_all(out).expect("create output dir");
    let gpu = pollster::block_on(gpu());
    for scene in scenes() {
        render(&gpu, scene, out);
    }
}
