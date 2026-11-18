//! Pictures for the player questions: flat SVGs in `assets/questions/`, drawn with inline
//! presentation attributes only. Sources are in `assets/questions/SOURCES.md`.

use egui::{Image, Ui, Vec2};

/// Every picture id, one per file in `assets/questions/`.
pub const QUESTION_ART: [&str; 11] = [
    "grip_palm",
    "grip_claw",
    "grip_fingertip",
    "pivot_wrist",
    "pivot_fingers",
    "pivot_arm",
    "pivot_shoulder",
    "pad_small",
    "pad_medium",
    "pad_large",
    "not_sure",
];

pub fn question_svg(id: &str) -> Option<&'static [u8]> {
    Some(match id {
        "grip_palm" => include_bytes!("../../assets/questions/grip_palm.svg"),
        "grip_claw" => include_bytes!("../../assets/questions/grip_claw.svg"),
        "grip_fingertip" => include_bytes!("../../assets/questions/grip_fingertip.svg"),
        "pivot_wrist" => include_bytes!("../../assets/questions/pivot_wrist.svg"),
        "pivot_fingers" => include_bytes!("../../assets/questions/pivot_fingers.svg"),
        "pivot_arm" => include_bytes!("../../assets/questions/pivot_arm.svg"),
        "pivot_shoulder" => include_bytes!("../../assets/questions/pivot_shoulder.svg"),
        "pad_small" => include_bytes!("../../assets/questions/pad_small.svg"),
        "pad_medium" => include_bytes!("../../assets/questions/pad_medium.svg"),
        "pad_large" => include_bytes!("../../assets/questions/pad_large.svg"),
        "not_sure" => include_bytes!("../../assets/questions/not_sure.svg"),
        _ => return None,
    })
}

/// The URI a picture is loaded under; the `.svg` ending picks egui's SVG loader.
pub fn question_uri(id: &str) -> String {
    format!("bytes://question/{id}.svg")
}

/// Draw picture `id` inside `size`, keeping its proportions. Unknown ids leave the space empty.
pub fn picture(ui: &mut Ui, id: &str, size: Vec2) {
    match question_svg(id) {
        Some(bytes) => {
            ui.add(
                Image::from_bytes(question_uri(id), bytes)
                    .fit_to_exact_size(size)
                    .maintain_aspect_ratio(true),
            );
        }
        None => {
            ui.allocate_space(size);
        }
    }
}
