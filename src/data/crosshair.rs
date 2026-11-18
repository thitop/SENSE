//! How the player's crosshair looks, saved with their profile, and the pixel rectangles it is
//! drawn from. The game and the Settings preview both draw [`CrosshairStyle::shape`], so the
//! preview shows exactly what the game draws.

use serde::{Deserialize, Serialize};

/// The preset colours offered in Settings, as (name, sRGB).
pub const PRESET_COLORS: [(&str, [u8; 3]); 6] = [
    ("Green", [0, 255, 124]),
    ("Yellow", [255, 230, 0]),
    ("Cyan", [0, 230, 255]),
    ("Red", [255, 64, 64]),
    ("Pink", [255, 80, 220]),
    ("White", [255, 255, 255]),
];

/// Slider ranges, in screen pixels (opacity 0..1).
pub const LENGTH_RANGE: std::ops::RangeInclusive<u32> = 0..=20;
pub const THICKNESS_RANGE: std::ops::RangeInclusive<u32> = 1..=6;
pub const GAP_RANGE: std::ops::RangeInclusive<u32> = 0..=12;
pub const DOT_SIZE_RANGE: std::ops::RangeInclusive<u32> = 1..=6;
pub const OPACITY_RANGE: std::ops::RangeInclusive<f32> = 0.1..=1.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrosshairStyle {
    /// sRGB.
    pub color: [u8; 3],
    pub opacity: f32,
    /// Length of each of the four lines; 0 leaves only the centre dot.
    pub length: u32,
    pub thickness: u32,
    /// Space between the centre and each line.
    pub gap: u32,
    pub dot: bool,
    pub dot_size: u32,
    /// A 1 px black outline around every line and the dot.
    pub outline: bool,
}

impl Default for CrosshairStyle {
    /// The crosshair SENSE always had: thin green lines, 8 px long, 3 px from the centre.
    fn default() -> Self {
        Self {
            color: PRESET_COLORS[0].1,
            opacity: 0.95,
            length: 8,
            thickness: 1,
            gap: 3,
            dot: false,
            dot_size: 2,
            outline: false,
        }
    }
}

/// A rectangle in screen pixels, relative to the top-left corner of the centre pixel
/// (y down).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl PixelRect {
    fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// The four 1 px strips around this rectangle: top and bottom (full width, corners
    /// included), then left and right. They don't overlap, so a see-through outline stays even.
    fn outline(&self) -> [PixelRect; 4] {
        let (w, h) = (self.w + 2, self.h);
        [
            PixelRect::new(self.x - 1, self.y - 1, w, 1),
            PixelRect::new(self.x - 1, self.y + h as i32, w, 1),
            PixelRect::new(self.x - 1, self.y, 1, h),
            PixelRect::new(self.x + self.w as i32, self.y, 1, h),
        ]
    }
}

/// What to draw: the outline strips first (black), then the coloured parts on top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CrosshairShape {
    pub outline: Vec<PixelRect>,
    pub fill: Vec<PixelRect>,
}

/// Most rectangles a crosshair can have: four lines and a dot, each with four outline strips.
pub const MAX_RECTS: usize = 5 * 5;

impl CrosshairStyle {
    /// This style with every value inside its slider range (a hand-edited profile can hold
    /// anything), and never invisible.
    pub fn clamped(&self) -> Self {
        let clamp = |v: u32, r: &std::ops::RangeInclusive<u32>| v.clamp(*r.start(), *r.end());
        Self {
            opacity: if self.opacity.is_finite() {
                self.opacity.clamp(*OPACITY_RANGE.start(), *OPACITY_RANGE.end())
            } else {
                1.0
            },
            length: clamp(self.length, &LENGTH_RANGE),
            thickness: clamp(self.thickness, &THICKNESS_RANGE),
            gap: clamp(self.gap, &GAP_RANGE),
            dot_size: clamp(self.dot_size, &DOT_SIZE_RANGE),
            // Without lines the dot is the whole crosshair: it can't be switched off too.
            dot: self.dot || self.length == 0,
            ..*self
        }
    }

    /// The pixel rectangles of this crosshair, centred on the centre pixel. Lines and dot are
    /// laid out the same way on both sides, so an odd thickness sits on the centre pixel and an
    /// even one leans the same way for every line.
    pub fn shape(&self) -> CrosshairShape {
        let s = self.clamped();
        let mut fill = Vec::with_capacity(5);
        let (t, len, gap) = (s.thickness, s.length, s.gap as i32);
        // The centre block the lines run out from: `t` wide, starting `o` before the centre pixel.
        let o = (t / 2) as i32;
        let (near, far) = (-o, t as i32 - o); // its two edges
        if len > 0 {
            let l = len as i32;
            fill.push(PixelRect::new(far + gap, near, len, t)); // right
            fill.push(PixelRect::new(near - gap - l, near, len, t)); // left
            fill.push(PixelRect::new(near, near - gap - l, t, len)); // up
            fill.push(PixelRect::new(near, far + gap, t, len)); // down
        }
        if s.dot {
            let d = s.dot_size;
            let od = (d / 2) as i32;
            fill.push(PixelRect::new(-od, -od, d, d));
        }
        let outline = if s.outline { fill.iter().flat_map(PixelRect::outline).collect() } else { Vec::new() };
        CrosshairShape { outline, fill }
    }

    /// The colour as linear RGBA, for drawing into the game's sRGB surface.
    pub fn linear_rgba(&self) -> [f32; 4] {
        let [r, g, b] = self.color.map(srgb_to_linear);
        [r, g, b, self.clamped().opacity]
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
