//! Each game shows its logo: a white single-colour SVG the UI tints.

use aim_trainer::sensitivity::games::GameDatabase;
use aim_trainer::ui::logos::logo_svg;

#[test]
fn every_listed_game_has_a_logo() {
    for g in GameDatabase::new().list() {
        assert!(logo_svg(&g.id).is_some(), "{} has no logo", g.id);
    }
    assert!(logo_svg("not-a-game").is_none());
}

#[test]
fn logos_are_plain_white_svgs() {
    for g in GameDatabase::new().list() {
        let svg = std::str::from_utf8(logo_svg(&g.id).unwrap()).expect("utf-8");
        // White on the root and no colour of its own anywhere, so the UI's tint decides it.
        assert!(svg.starts_with("<svg fill=\"#FFFFFF\" viewBox=\""), "{}", g.id);
        for colour in ["<style", "class=", "fill=\"#0", "fill=\"#3", "fill=\"#f", "stroke="] {
            assert!(!svg.contains(colour), "{} contains {colour}", g.id);
        }
        // Nothing that runs, loads or links anything.
        for bad in ["<script", "onload", "href", "<image", "foreignObject"] {
            assert!(!svg.contains(bad), "{} contains {bad}", g.id);
        }
    }
}

#[test]
fn logos_load_through_egui() {
    // Render each logo once in a headless frame; a failed load would show egui's error image
    // rather than the texture, so check the loader resolved every one.
    let ctx = egui::Context::default();
    aim_trainer::ui::theme::install(&ctx);
    let ids: Vec<String> = GameDatabase::new().list().iter().map(|g| g.id.clone()).collect();
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for id in &ids {
                    aim_trainer::ui::logos::logo(ui, id, 20.0, egui::Color32::WHITE);
                }
            });
        });
    }
    for id in &ids {
        let uri = aim_trainer::ui::logos::logo_uri(id);
        let poll = ctx.try_load_image(&uri, egui::SizeHint::Size(40, 40));
        let state = match &poll {
            Ok(egui::load::ImagePoll::Ready { .. }) => "ready".to_string(),
            Ok(egui::load::ImagePoll::Pending { .. }) => "pending".to_string(),
            Err(e) => format!("error: {e}"),
        };
        assert_eq!(state, "ready", "{id}");
    }
}
