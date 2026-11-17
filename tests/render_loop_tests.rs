//! What the render loop does when the window surface can't give a frame.

use aim_trainer::engine::renderer::{can_draw, on_surface_error, SurfaceAction};
use winit::dpi::PhysicalSize;

#[test]
fn outdated_and_lost_surfaces_are_set_up_again() {
    // Outdated: the window changed size or state (resize, minimise, alt-tab) since the last
    // configure. Before, it was only logged as an error, every frame.
    assert_eq!(on_surface_error(&wgpu::SurfaceError::Outdated), SurfaceAction::Reconfigure);
    assert_eq!(on_surface_error(&wgpu::SurfaceError::Lost), SurfaceAction::Reconfigure);
}

#[test]
fn a_timeout_skips_the_frame_and_out_of_memory_quits() {
    assert_eq!(on_surface_error(&wgpu::SurfaceError::Timeout), SurfaceAction::SkipFrame);
    assert_eq!(on_surface_error(&wgpu::SurfaceError::OutOfMemory), SurfaceAction::Exit);
    assert_eq!(on_surface_error(&wgpu::SurfaceError::Other), SurfaceAction::Report);
}

#[test]
fn a_minimised_window_is_not_drawn() {
    assert!(!can_draw(PhysicalSize::new(0, 0)));
    assert!(!can_draw(PhysicalSize::new(1280, 0)));
    assert!(!can_draw(PhysicalSize::new(0, 720)));
    assert!(can_draw(PhysicalSize::new(1280, 720)));
}
