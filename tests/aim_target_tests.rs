//! Tasks report the target being aimed at, with a stable id per target.

use aim_trainer::game::hit_detection::Ray;
use aim_trainer::game::tasks::flick::{FlickBand, FlickTask};
use aim_trainer::game::tasks::micro_adjust::MicroAdjustTask;
use aim_trainer::game::tasks::moving::MovingTargetTask;
use aim_trainer::game::tasks::speed::SpeedTask;
use aim_trainer::game::tasks::switching::SwitchingTask;
use aim_trainer::game::tasks::tracking::TrackingTask;
use aim_trainer::game::tasks::Task;
use aim_trainer::habits::record::AimKind;
use glam::Vec3;

const EYE: Vec3 = Vec3::new(0.0, 1.5, 0.0);

fn level() -> Ray {
    Ray { origin: EYE, direction: Vec3::NEG_Z }
}

fn at(pos: Vec3) -> Ray {
    Ray { origin: EYE, direction: (pos - EYE).normalize() }
}

#[test]
fn flick_reports_live_target_with_band() {
    let mut t = FlickTask::banded(vec![FlickBand::Far], 7, 0.35);
    t.start();
    t.update(0.016, level());
    let a = t.aim_target().unwrap();
    assert_eq!(a.kind, AimKind::Flick);
    assert_eq!(a.band, Some(FlickBand::Far));
    assert_eq!(a.radius, 0.35);
    let (pos, _) = t.get_render_targets()[0];
    assert_eq!(a.center, pos);
}

#[test]
fn aim_target_id_changes_after_hit() {
    let mut t = FlickTask::banded(vec![FlickBand::Short, FlickBand::Mid], 3, 0.35);
    t.start();
    t.update(0.016, level());
    let first = t.aim_target().unwrap();
    assert!(t.on_click(at(first.center)));
    t.update(0.016, at(first.center));
    let second = t.aim_target().unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(second.band, Some(FlickBand::Mid));
}

#[test]
fn switching_reports_highlighted_target() {
    let mut t = SwitchingTask::new(5, 4, 0.32);
    t.start();
    t.update(0.016, level());
    let a = t.aim_target().unwrap();
    assert_eq!(a.kind, AimKind::Flick);
    assert_eq!(a.band, None);
    // The highlighted target is the one a click on it counts for.
    assert!(t.on_click(at(a.center)));
    t.update(0.016, level());
    assert_ne!(t.aim_target().unwrap().id, a.id);
}

#[test]
fn speed_reports_its_live_target() {
    let mut t = SpeedTask::new(5);
    t.start();
    t.update(0.016, level());
    let a = t.aim_target().unwrap();
    assert_eq!(a.kind, AimKind::Flick);
    assert!(t.on_click(at(a.center)));
    t.update(0.016, level());
    assert_ne!(t.aim_target().unwrap().id, a.id);
}

#[test]
fn micro_and_moving_report_their_kind() {
    let mut m = MicroAdjustTask::new(5, 0.16);
    m.start();
    m.update(0.016, level());
    let a = m.aim_target().unwrap();
    assert_eq!(a.kind, AimKind::Micro);
    assert_eq!(a.radius, 0.16);

    let mut v = MovingTargetTask::seeded(5, 0.35, 3.0, 1);
    v.start();
    v.update(0.016, level());
    let a = v.aim_target().unwrap();
    assert_eq!(a.kind, AimKind::Moving);
    let (pos, _) = v.live_target().unwrap();
    assert_eq!(a.center, pos);
}

#[test]
fn tracking_reports_none() {
    let mut t = TrackingTask::new(30.0, 0.35, 3.8);
    t.start();
    t.update(0.016, level());
    assert!(t.aim_target().is_none());
}
