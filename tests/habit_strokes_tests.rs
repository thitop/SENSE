//! Stroke features in degrees: where the first stroke landed, how many strokes, overshoot.

use aim_trainer::game::tasks::flick::FlickBand;
use aim_trainer::habits::record::{AimKind, PlayContext, TargetRecord};
use aim_trainer::habits::strokes::{band_for_angle, stroke_features, DegSample};
use aim_trainer::sensitivity::paired::Side;
use glam::Vec2;

/// One bell-shaped stroke: `n` samples between the two times that add up to `total`.
fn stroke(from_ms: f32, to_ms: f32, total: Vec2, n: usize) -> Vec<DegSample> {
    let weights: Vec<f32> = (0..n)
        .map(|i| (std::f32::consts::PI * (i as f32 + 0.5) / n as f32).sin())
        .collect();
    let sum: f32 = weights.iter().sum();
    (0..n)
        .map(|i| DegSample {
            t_ms: from_ms + (to_ms - from_ms) * (i as f32 + 1.0) / n as f32,
            d: total * (weights[i] / sum),
        })
        .collect()
}

#[test]
fn single_stroke_stopping_at_70_percent_lands_short() {
    let f = stroke_features(&stroke(0., 120., Vec2::new(7.0, 0.), 24), Vec2::new(10.0, 0.)).unwrap();
    assert!((f.first_landing - 0.7).abs() < 0.03);
    assert_eq!(f.strokes, 1);
    assert!(!f.crossed_center);
}

#[test]
fn going_past_then_back_crosses_center() {
    let samples = [
        stroke(0., 120., Vec2::new(12., 0.), 24),
        stroke(180., 240., Vec2::new(-2., 0.), 12),
    ]
    .concat();
    let f = stroke_features(&samples, Vec2::new(10., 0.)).unwrap();
    assert!(f.first_landing > 1.15);
    assert!(f.crossed_center);
    assert!((f.overshoot_deg - 2.0).abs() < 0.2);
    assert_eq!(f.strokes, 2);
}

#[test]
fn three_short_strokes_count_three() {
    let samples = [
        stroke(0., 100., Vec2::new(6., 0.), 20),
        stroke(160., 240., Vec2::new(2.5, 0.), 16),
        stroke(300., 360., Vec2::new(1.5, 0.), 12),
    ]
    .concat();
    let f = stroke_features(&samples, Vec2::new(10., 0.)).unwrap();
    assert_eq!(f.strokes, 3);
    assert!(!f.crossed_center);
}

#[test]
fn no_movement_is_none() {
    assert!(stroke_features(&[], Vec2::new(10., 0.)).is_none());
}

#[test]
fn zero_distance_target_is_none() {
    assert!(stroke_features(&stroke(0., 100., Vec2::new(3., 0.), 10), Vec2::ZERO).is_none());
}

#[test]
fn bands_follow_flick_ranges() {
    for b in FlickBand::ALL {
        let (lo, hi) = b.range_deg();
        assert_eq!(band_for_angle((lo + hi) / 2.0), Some(b));
    }
    assert_eq!(band_for_angle(13.0), None);
}

#[test]
fn target_record_round_trips_json() {
    let r = TargetRecord {
        kind: AimKind::Flick,
        offset_deg: [8.0, -3.0],
        radius_deg: 1.2,
        first_landing: Some(0.9),
        strokes: 2,
        crossed_center: false,
        overshoot_deg: 0.0,
        time_to_hit_ms: Some(412.0),
        misses: vec![[1.0, -0.5]],
        band: Some(FlickBand::Short),
        ctx: PlayContext { cm_360: 30.0, side: Some(Side::Faster), block: Some(2), warmup: false },
    };
    let back: TargetRecord = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back, r);
}
