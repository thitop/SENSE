//! The app's name and window icon: the crosshair from the sidebar, in the accent colour.

use ab_glyph::{Font, FontRef, PxScale};

use super::theme::ACCENT;

pub const APP_NAME: &str = "SENSE";
/// Pixel size of the window icon; Windows scales it down for the title bar and taskbar.
pub const ICON_SIZE: u32 = 64;

/// The crosshair glyph (Phosphor, as the sidebar draws it) centred in a `size`×`size` RGBA
/// image on a transparent background.
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let Ok(font) = FontRef::try_from_slice(egui_phosphor::Variant::Regular.font_bytes()) else {
        return rgba;
    };
    let ch = egui_phosphor::regular::CROSSHAIR.chars().next().unwrap_or(' ');
    let glyph = font.glyph_id(ch).with_scale(PxScale::from(size as f32));
    let Some(outline) = font.outline_glyph(glyph) else {
        return rgba;
    };
    let b = outline.px_bounds();
    // Centre the drawn shape, whatever margins the font gives it.
    let ox = ((size as f32 - b.width()) * 0.5).round() as i64;
    let oy = ((size as f32 - b.height()) * 0.5).round() as i64;
    outline.draw(|x, y, coverage| {
        let (px, py) = (x as i64 + ox, y as i64 + oy);
        if px < 0 || py < 0 || px >= size as i64 || py >= size as i64 {
            return;
        }
        let i = ((py as u32 * size + px as u32) * 4) as usize;
        rgba[i..i + 4].copy_from_slice(&[ACCENT.r(), ACCENT.g(), ACCENT.b(), (coverage.clamp(0.0, 1.0) * 255.0).round() as u8]);
    });
    rgba
}

/// Sizes in the .exe icon: what Explorer, the taskbar and shortcuts pick from.
pub const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// A Windows .ico with the icon at each of `sizes`, as 32-bit bitmaps (alpha included).
pub fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| ico_bitmap(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (&size, image) in sizes.iter().zip(&images) {
        out.push((size % 256) as u8); // width; 256 is stored as 0
        out.push((size % 256) as u8); // height
        out.push(0); // no palette
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += image.len();
    }
    for image in images {
        out.extend_from_slice(&image);
    }
    out
}

/// One .ico image: a bitmap header, BGRA rows bottom-up, then an all-clear 1-bit mask (the
/// alpha channel already says what's see-through).
fn ico_bitmap(size: u32) -> Vec<u8> {
    let rgba = app_icon_rgba(size);
    let n = size as usize;
    let mask_row = (n.div_ceil(32)) * 4;
    let mut out = Vec::with_capacity(40 + n * n * 4 + mask_row * n);
    out.extend_from_slice(&40u32.to_le_bytes()); // header size
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(2 * size as i32).to_le_bytes()); // image and mask
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // no compression
    out.extend_from_slice(&((n * n * 4 + mask_row * n) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution and palette: unused
    for y in (0..n).rev() {
        for x in 0..n {
            let p = &rgba[(y * n + x) * 4..(y * n + x) * 4 + 4];
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out.resize(out.len() + mask_row * n, 0);
    out
}
