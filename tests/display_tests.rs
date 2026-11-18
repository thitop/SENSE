//! Fullscreen picks the screen's own resolution at its best refresh rate, and a saved window
//! only comes back where it was if that spot is still on a screen.

use aim_trainer::data::WindowPlacement;
use aim_trainer::engine::display::{best_mode, screen_of, ModeInfo, ScreenRect};

fn mode(width: u32, height: u32, hz: u32, bit_depth: u16) -> ModeInfo {
    ModeInfo { width, height, refresh_millihertz: hz * 1000, bit_depth }
}

#[test]
fn fullscreen_uses_the_native_resolution_at_the_highest_refresh_rate() {
    let modes = [
        mode(1920, 1080, 60, 32),
        mode(2560, 1440, 60, 32),
        mode(1920, 1080, 240, 32),
        mode(1920, 1080, 144, 32),
        mode(1280, 720, 360, 32),
    ];
    assert_eq!(best_mode((1920, 1080), &modes), Some(2));
}

#[test]
fn at_equal_refresh_the_deeper_colour_wins() {
    let modes = [mode(1920, 1080, 144, 16), mode(1920, 1080, 144, 32)];
    assert_eq!(best_mode((1920, 1080), &modes), Some(1));
}

#[test]
fn without_the_native_resolution_the_largest_mode_is_used() {
    let modes = [mode(1280, 720, 144, 32), mode(1600, 900, 60, 32), mode(1024, 768, 75, 32)];
    assert_eq!(best_mode((1920, 1080), &modes), Some(1));
}

#[test]
fn no_modes_means_no_exclusive_fullscreen() {
    assert_eq!(best_mode((1920, 1080), &[]), None);
}

fn placement(x: i32, y: i32) -> WindowPlacement {
    WindowPlacement { x, y, width: 1280, height: 720, maximized: false }
}

const SCREENS: [ScreenRect; 2] = [
    ScreenRect { x: 0, y: 0, width: 1920, height: 1080 },
    // A second screen to the left.
    ScreenRect { x: -2560, y: -200, width: 2560, height: 1440 },
];

#[test]
fn a_window_on_either_screen_comes_back_there() {
    assert_eq!(screen_of(&placement(100, 100), &SCREENS), Some(0));
    assert_eq!(screen_of(&placement(-2000, 0), &SCREENS), Some(1));
}

#[test]
fn a_window_hanging_off_the_edge_counts_while_its_title_bar_is_on_screen() {
    // Mostly off the right edge, but the middle of the title bar is still on screen.
    assert_eq!(screen_of(&placement(1200, 500), &SCREENS), Some(0));
}

#[test]
fn a_window_on_an_unplugged_screen_opens_where_windows_puts_it() {
    assert_eq!(screen_of(&placement(4000, 100), &SCREENS), None);
    assert_eq!(screen_of(&placement(-2000, 0), &SCREENS[..1]), None);
    // Title bar above the top of every screen.
    assert_eq!(screen_of(&placement(100, -400), &SCREENS), None);
}
