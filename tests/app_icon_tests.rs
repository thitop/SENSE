//! The window shows the app's name and the crosshair from the sidebar as its icon.

use aim_trainer::ui::app_icon::{app_icon_rgba, APP_NAME, ICON_SIZE};

#[test]
fn the_window_is_named_after_the_app() {
    assert_eq!(APP_NAME, "SENSE");
}

#[test]
fn the_icon_is_the_accent_crosshair_on_transparent() {
    let n = ICON_SIZE as usize;
    let px = app_icon_rgba(ICON_SIZE);
    assert_eq!(px.len(), n * n * 4);
    let at = |x: usize, y: usize| &px[(y * n + x) * 4..(y * n + x) * 4 + 4];
    // Corners are see-through.
    for (x, y) in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1)] {
        assert_eq!(at(x, y)[3], 0, "corner {x},{y}");
    }
    // Drawn pixels are the accent yellow, and there are plenty of them.
    let drawn: Vec<&[u8]> = px.chunks(4).filter(|p| p[3] > 200).collect();
    assert!(drawn.len() > n * n / 20, "{} drawn pixels", drawn.len());
    assert!(drawn.iter().all(|p| p[..3] == [0xFF, 0xD2, 0x3F]));
    // Centred: as much ink left of the middle as right of it, and above as below.
    let (mut left, mut right, mut top, mut bottom) = (0u64, 0u64, 0u64, 0u64);
    for y in 0..n {
        for x in 0..n {
            let a = at(x, y)[3] as u64;
            if x < n / 2 { left += a } else { right += a }
            if y < n / 2 { top += a } else { bottom += a }
        }
    }
    let close = |a: u64, b: u64| (a as f64 - b as f64).abs() / (a.max(b) as f64) < 0.05;
    assert!(close(left, right), "{left} {right}");
    assert!(close(top, bottom), "{top} {bottom}");
}

#[test]
fn the_ico_holds_every_size_as_a_32_bit_image() {
    use aim_trainer::ui::app_icon::{ico_bytes, ICO_SIZES};
    let ico = ico_bytes(&ICO_SIZES);
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]) as usize;
    let u32_at = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]) as usize;
    assert_eq!((u16_at(0), u16_at(2), u16_at(4)), (0, 1, ICO_SIZES.len()));
    for (k, &size) in ICO_SIZES.iter().enumerate() {
        let e = 6 + 16 * k;
        // 256 is stored as 0.
        assert_eq!(ico[e] as u32, size % 256);
        assert_eq!(u16_at(e + 6), 32, "bits per pixel");
        let (len, at) = (u32_at(e + 8), u32_at(e + 12));
        assert!(at + len <= ico.len());
        // A bitmap header with twice the height (image + mask).
        assert_eq!(u32_at(at), 40);
        assert_eq!((u32_at(at + 4), u32_at(at + 8)), (size as usize, 2 * size as usize));
    }
}

#[test]
fn the_exe_icon_file_is_up_to_date() {
    // Regenerate with `cargo run --example make_icon` after changing the icon.
    use aim_trainer::ui::app_icon::{ico_bytes, ICO_SIZES};
    let saved = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon/sense.ico")).unwrap();
    assert!(saved == ico_bytes(&ICO_SIZES), "assets/icon/sense.ico is stale");
}
