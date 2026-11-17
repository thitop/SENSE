use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::game::tasks::TaskType;
use aim_trainer::habits::analysis::*;
use aim_trainer::habits::record::*;
use aim_trainer::sensitivity::combiner::Signal;

fn flick(band: FlickBand, landing: f32, strokes: u8) -> TargetRecord {
    TargetRecord {
        kind: AimKind::Flick,
        offset_deg: [10.0, 0.0],
        radius_deg: 1.0,
        first_landing: Some(landing),
        strokes,
        crossed_center: false,
        overshoot_deg: 0.0,
        time_to_hit_ms: None,
        misses: vec![],
        band: Some(band),
        ctx: PlayContext::default(),
    }
}
fn micro(crossed: bool, strokes: u8) -> TargetRecord {
    TargetRecord {
        kind: AimKind::Micro,
        crossed_center: crossed,
        strokes,
        band: None,
        ..flick(FlickBand::Short, 1.0, strokes)
    }
}
fn window(lag_ms: f32, cross_rms: f32, radius: f32) -> TrackingRecord {
    TrackingRecord {
        mean_along_deg: 0.0,
        lag_ms,
        cross_rms_deg: cross_rms,
        mean_speed_deg_s: 10.0,
        coverage: 1.0,
        radius_deg: radius,
        horizontal: true,
        ctx: PlayContext::default(),
    }
}
fn log_t(t: Vec<TargetRecord>) -> AimLog {
    AimLog { targets: t, tracking: vec![] }
}
fn log_w(w: Vec<TrackingRecord>) -> AimLog {
    AimLog { targets: vec![], tracking: w }
}
fn find(vals: &[MeasureValue], m: Measure) -> Option<MeasureValue> {
    vals.iter().copied().find(|v| v.measure == m)
}

#[test]
fn far_flicks_short_are_flagged() {
    let log = log_t((0..10).map(|_| flick(FlickBand::Far, 0.78, 1)).collect());
    let v = find(&session_values(&log), Measure::FlickLanding(FlickBand::Far)).unwrap();
    assert!((v.value - 0.78).abs() < 1e-4);
    assert_eq!(v.habit, Some(Habit::FlickShort(FlickBand::Far)));
}

#[test]
fn flick_past_is_flagged() {
    let log = log_t((0..10).map(|_| flick(FlickBand::Mid, 1.25, 1)).collect());
    let v = find(&session_values(&log), Measure::FlickLanding(FlickBand::Mid)).unwrap();
    assert_eq!(v.habit, Some(Habit::FlickPast(FlickBand::Mid)));
}

#[test]
fn landing_inside_band_is_not_flagged() {
    let log = log_t((0..10).map(|_| flick(FlickBand::Far, 0.95, 1)).collect());
    let v = find(&session_values(&log), Measure::FlickLanding(FlickBand::Far)).unwrap();
    assert_eq!(v.habit, None);
}

#[test]
fn per_band_minimum_is_eight() {
    let m = Measure::FlickLanding(FlickBand::Far);
    let seven = log_t((0..7).map(|_| flick(FlickBand::Far, 0.8, 1)).collect());
    assert!(m.value(&seven, |_| true).is_none());
    let eight = log_t((0..8).map(|_| flick(FlickBand::Far, 0.8, 1)).collect());
    assert_eq!(m.value(&eight, |_| true).map(|x| x.1), Some(8));
}

#[test]
fn none_landings_are_skipped() {
    let mut t: Vec<_> = (0..8).map(|_| flick(FlickBand::Far, 0.8, 1)).collect();
    t[0].first_landing = None;
    assert!(Measure::FlickLanding(FlickBand::Far).value(&log_t(t), |_| true).is_none());
}

#[test]
fn too_few_records_is_silent() {
    let log = AimLog {
        targets: (0..5)
            .map(|_| flick(FlickBand::Far, 0.5, 4))
            .chain((0..3).map(|_| micro(true, 3)))
            .collect(),
        tracking: (0..4).map(|_| window(80.0, 2.0, 1.0)).collect(),
    };
    assert!(session_values(&log).is_empty());
}

#[test]
fn flick_strokes_mean() {
    let log = log_t((0..12).map(|_| flick(FlickBand::Mid, 1.0, 3)).collect());
    let v = find(&session_values(&log), Measure::FlickStrokes).unwrap();
    assert_eq!(v.habit, Some(Habit::FlickCorrections));
    assert!((v.value - 3.0).abs() < 1e-5);
}

#[test]
fn micro_overshoot_share() {
    let log = log_t((0..12).map(|i| micro(i < 5, 1)).collect());
    let v = find(&session_values(&log), Measure::MicroOvershoot).unwrap();
    assert!((v.value - 5.0 / 12.0).abs() < 1e-4);
    assert_eq!(v.habit, Some(Habit::MicroOvershoot));
}

#[test]
fn creep_needs_three_strokes_without_crossing() {
    let mut t: Vec<_> = (0..5).map(|_| micro(false, 3)).collect();
    t.extend((0..3).map(|_| micro(true, 3)));
    t.extend((0..4).map(|_| micro(false, 2)));
    let v = find(&session_values(&log_t(t)), Measure::MicroCreep).unwrap();
    assert!((v.value - 5.0 / 12.0).abs() < 1e-4);
    assert_eq!(v.habit, Some(Habit::MicroCreep));
}

#[test]
fn tracking_lag_both_ways() {
    let run = |lag: f32| {
        let v = session_values(&log_w((0..12).map(|_| window(lag, 0.0, 1.0)).collect()));
        find(&v, Measure::TrackingLag).unwrap().habit
    };
    assert_eq!(run(55.0), Some(Habit::TrackingTrails));
    assert_eq!(run(-50.0), Some(Habit::TrackingAhead));
    assert_eq!(run(30.0), None);
}

#[test]
fn shake_is_relative_to_radius() {
    let log = log_w((0..12).map(|_| window(0.0, 0.6, 1.0)).collect());
    let v = find(&session_values(&log), Measure::TrackingShake).unwrap();
    assert!((v.value - 0.6).abs() < 1e-5);
    assert_eq!(v.habit, Some(Habit::TrackingShakes));
    let big = log_w((0..12).map(|_| window(0.0, 0.6, 2.0)).collect());
    assert_eq!(find(&session_values(&big), Measure::TrackingShake).unwrap().habit, None);
}

fn with_misses(n: usize, along: f32) -> AimLog {
    let mut t: Vec<_> = (0..n).map(|_| flick(FlickBand::Mid, 1.0, 1)).collect();
    for r in &mut t {
        r.misses = vec![[along, 0.1]];
    }
    log_t(t)
}

#[test]
fn miss_lean_needs_ten_misses() {
    assert!(Measure::MissLeanFlick.value(&with_misses(9, 0.8), |_| true).is_none());
    let (v, n) = Measure::MissLeanFlick.value(&with_misses(10, 0.8), |_| true).unwrap();
    assert_eq!(n, 10);
    assert!((v - 0.8).abs() < 1e-5);
    let s = session_values(&with_misses(10, 0.8));
    assert_eq!(find(&s, Measure::MissLeanFlick).unwrap().habit, Some(Habit::MissLeanFlick));
}

#[test]
fn statistic_has_no_practice_minimum() {
    let log = log_t(vec![flick(FlickBand::Far, 0.7, 1), flick(FlickBand::Far, 0.9, 1)]);
    let (v, n) = Measure::FlickLanding(FlickBand::Far).statistic(&log, |_| true).unwrap();
    assert_eq!(n, 2);
    assert!((v - 0.8).abs() < 1e-5);
    assert!(Measure::FlickLanding(FlickBand::Far).value(&log, |_| true).is_none());
    assert!(Measure::MissLeanFlick.statistic(&log, |_| true).is_none());
    assert!(Measure::FlickLanding(FlickBand::Far).statistic(&log, |c| c.cm_360 > 1.0).is_none());
}

#[test]
fn severity_is_share_beyond_threshold() {
    assert!((Habit::FlickShort(FlickBand::Far).severity(0.68).unwrap() - 0.2).abs() < 1e-3);
    assert!(Habit::FlickShort(FlickBand::Far).severity(0.9).is_none());
    assert!((Habit::TrackingAhead.severity(-48.0).unwrap() - 0.2).abs() < 1e-5);
    assert!(Habit::TrackingAhead.severity(-30.0).is_none());
    assert!((Habit::FlickPast(FlickBand::Mid).severity(1.32).unwrap() - 0.2).abs() < 1e-5);
}

#[test]
fn direction_named_only_when_alone() {
    let mk = |left: f32, other: f32| {
        let mut t = vec![];
        for (dir, landing) in
            [([-10.0, 0.0], left), ([10.0, 0.0], other), ([0.0, 10.0], other), ([0.0, -10.0], other)]
        {
            for _ in 0..5 {
                let mut r = flick(FlickBand::Far, landing, 1);
                r.offset_deg = dir;
                t.push(r);
            }
        }
        log_t(t)
    };
    let h = Habit::FlickShort(FlickBand::Far);
    assert_eq!(worst_direction(h, &mk(0.7, 0.95), |_| true), Some(Dir::Left));
    assert_eq!(worst_direction(h, &mk(0.7, 0.7), |_| true), None);
}

#[test]
fn tracking_direction_splits_by_horizontal() {
    let mut w: Vec<_> = (0..5).map(|_| window(70.0, 0.0, 1.0)).collect();
    w.extend((0..5).map(|_| TrackingRecord { horizontal: false, ..window(10.0, 0.0, 1.0) }));
    assert_eq!(worst_direction(Habit::TrackingTrails, &log_w(w), |_| true), Some(Dir::SideToSide));
}

#[test]
fn value_texts() {
    let b = FlickBand::Far;
    assert_eq!(Habit::FlickShort(b).value_text(0.82), "18% short");
    assert_eq!(Habit::FlickPast(b).value_text(1.25), "25% past");
    assert_eq!(Habit::FlickCorrections.value_text(2.84), "2.8 moves per target");
    assert_eq!(Habit::MicroOvershoot.value_text(0.4167), "42% overshoot");
    assert_eq!(Habit::MicroCreep.value_text(0.5), "50% creep");
    assert_eq!(Habit::TrackingTrails.value_text(62.4), "62 ms behind");
    assert_eq!(Habit::TrackingAhead.value_text(-50.2), "50 ms ahead");
    assert_eq!(Habit::TrackingShakes.value_text(0.62), "0.6\u{d7} target size");
    assert_eq!(Habit::MissLeanFlick.value_text(0.5), "0.5\u{d7} target size off");
}

#[test]
fn titles_phrases_and_mappings() {
    assert_eq!(Habit::FlickShort(FlickBand::Far).title(), "Far flicks stop short");
    assert_eq!(Habit::FlickPast(FlickBand::Short).title(), "Short flicks go past");
    assert_eq!(Habit::MissLeanMicro.title(), "Small-aim misses lean one way");
    assert_eq!(Habit::TrackingTrails.phrase(), "you trailed the target");
    assert_eq!(Habit::MissLeanFlick.phrase(), "");
    assert_eq!(Habit::MicroCreep.practice(), TaskType::MicroAdjust);
    assert_eq!(Habit::MissLeanFlick.practice(), TaskType::Flick);
    assert_eq!(Habit::TrackingShakes.test_label(), "Tracking");
    assert_eq!(Habit::MissLeanMicro.test_label(), "Micro-adjustment");
    assert_eq!(Habit::FlickPast(FlickBand::Mid).lean(), Some(Signal::TooFast));
    assert_eq!(Habit::TrackingTrails.lean(), Some(Signal::TooSlow));
    assert_eq!(Habit::FlickCorrections.lean(), None);
    assert_eq!(Measure::ALL.len(), 10);
    assert_eq!(Measure::TrackingLag.habits(), &[Habit::TrackingTrails, Habit::TrackingAhead]);
    assert!((Measure::FlickLanding(FlickBand::Far).badness(0.8) - 0.2).abs() < 1e-5);
    assert!((Measure::TrackingLag.badness(-50.0) - 50.0).abs() < 1e-5);
}
