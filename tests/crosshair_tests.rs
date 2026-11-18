//! The crosshair style saved with a profile, and the pixel rectangles the game and the
//! Settings preview draw it from.

use aim_trainer::data::crosshair::{PixelRect, MAX_RECTS};
use aim_trainer::data::{CrosshairStyle, PlayerProfile};
use aim_trainer::engine::renderer::crosshair_vertices;

fn rect(x: i32, y: i32, w: u32, h: u32) -> PixelRect {
    PixelRect { x, y, w, h }
}

#[test]
fn the_default_is_the_crosshair_sense_always_had() {
    // Thin lines 8 px long, starting 3 px from the centre pixel on every side.
    let shape = CrosshairStyle::default().shape();
    assert_eq!(
        shape.fill,
        [rect(4, 0, 8, 1), rect(-11, 0, 8, 1), rect(0, -11, 1, 8), rect(0, 4, 1, 8)]
    );
    assert!(shape.outline.is_empty());
}

#[test]
fn lines_sit_the_same_distance_from_the_centre_on_every_side() {
    for thickness in 1..=6 {
        for gap in [0, 3, 12] {
            let s = CrosshairStyle { thickness, gap, length: 5, ..Default::default() };
            let f = s.shape().fill;
            let (right, left, up, down) = (f[0], f[1], f[2], f[3]);
            // Centre block edges: [-o, t - o).
            let o = (thickness / 2) as i32;
            let far = thickness as i32 - o;
            assert_eq!(right.x - far, -o - (left.x + left.w as i32), "t={thickness} gap={gap}");
            assert_eq!(down.y - far, -o - (up.y + up.h as i32), "t={thickness} gap={gap}");
            assert_eq!(right.x - far, gap as i32);
            assert_eq!((right.y, right.h, up.x, up.w), (-o, thickness, -o, thickness));
        }
    }
}

#[test]
fn the_dot_is_centred_on_the_lines() {
    for size in 1..=6 {
        let s = CrosshairStyle { thickness: size, dot: true, dot_size: size, ..Default::default() };
        let f = s.shape().fill;
        let dot = f[4];
        assert_eq!((dot.x, dot.y, dot.w), (f[2].x, f[0].y, f[2].w), "size {size}");
    }
}

#[test]
fn the_outline_rings_every_part_without_overlapping_itself() {
    let s = CrosshairStyle { dot: true, outline: true, ..Default::default() };
    let shape = s.shape();
    assert_eq!(shape.outline.len(), shape.fill.len() * 4);
    assert!(shape.outline.len() + shape.fill.len() <= MAX_RECTS);
    // Around the right line (4, 0, 8×1): 10 px strips above and below, 1 px ends.
    assert_eq!(
        shape.outline[..4],
        [rect(3, -1, 10, 1), rect(3, 1, 10, 1), rect(3, 0, 1, 1), rect(12, 0, 1, 1)]
    );
}

#[test]
fn without_lines_the_dot_stays_so_the_crosshair_never_disappears() {
    let s = CrosshairStyle { length: 0, dot: false, dot_size: 3, ..Default::default() };
    assert!(s.clamped().dot);
    assert_eq!(s.shape().fill, [rect(-1, -1, 3, 3)]);
}

#[test]
fn hand_edited_values_are_kept_in_range() {
    let s = CrosshairStyle { length: 500, thickness: 0, gap: 99, dot_size: 40, opacity: f32::NAN, ..Default::default() };
    let c = s.clamped();
    assert_eq!((c.length, c.thickness, c.gap, c.dot_size, c.opacity), (20, 1, 12, 6, 1.0));
    assert_eq!(CrosshairStyle { opacity: 0.0, ..Default::default() }.clamped().opacity, 0.1);
}

#[test]
fn the_default_colour_is_the_old_green_in_linear_light() {
    let [r, g, b, a] = CrosshairStyle::default().linear_rgba();
    assert_eq!((r, g, a), (0.0, 1.0, 0.95));
    assert!((b - 0.2).abs() < 0.005, "{b}");
}

#[test]
fn the_game_draws_two_triangles_per_rectangle_around_the_screen_centre() {
    let s = CrosshairStyle { dot: true, outline: true, ..Default::default() };
    let v = crosshair_vertices(&s, 1920, 1080);
    assert_eq!(v.len(), (5 + 20) * 6);
    // Outline (black) first, colour last.
    assert_eq!(&v[0].color[..3], &[0.0, 0.0, 0.0]);
    assert_eq!(v.last().unwrap().color, s.linear_rgba());
    // The dot (2×2 from pixel -1) spans pixels 959..961 × 539..541 of a 1920×1080 screen.
    let dot = &v[v.len() - 6..];
    let x = |px: f32| px / 1920.0 * 2.0 - 1.0;
    let y = |py: f32| 1.0 - py / 1080.0 * 2.0;
    assert_eq!(dot[0].position, [x(959.0), y(539.0)]);
    assert_eq!(dot[2].position, [x(961.0), y(541.0)]);
}

#[test]
fn profiles_saved_before_crosshair_settings_get_the_default() {
    let mut p = PlayerProfile::new("p".into(), "P".into(), 800.0, "cs2".into());
    p.crosshair = CrosshairStyle { outline: true, ..Default::default() };
    let mut json: serde_json::Value = serde_json::to_value(&p).unwrap();
    assert_eq!(json["crosshair"]["outline"], true);
    json.as_object_mut().unwrap().remove("crosshair");
    let old: PlayerProfile = serde_json::from_value(json).unwrap();
    assert_eq!(old.crosshair, CrosshairStyle::default());
}
