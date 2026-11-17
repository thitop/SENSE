//! Calibration and session plots, styled with the theme.

use egui_plot::{Line, Plot, PlotPoints, Points, VLine};

use crate::data::CalibrationRound;
use crate::ui::theme::*;

const PLOT_HEIGHT: f32 = 220.0;

fn base(id: &str, x_label: &str, y_label: &str) -> Plot<'static> {
    Plot::new(id.to_string())
        .height(PLOT_HEIGHT)
        .x_axis_label(x_label.to_string())
        .y_axis_label(y_label.to_string())
        .allow_scroll(false)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .show_background(false)
}

/// Tested cm/360 per PSA round.
pub fn psa_convergence(ui: &mut egui::Ui, rounds: &[CalibrationRound]) {
    let pts: Vec<[f64; 2]> = rounds
        .iter()
        .enumerate()
        .map(|(i, r)| [i as f64 + 1.0, r.tested_cm_360])
        .collect();
    base("psa_convergence", "Round", "cm/360°").show(ui, |p| {
        p.line(Line::new(PlotPoints::from(pts.clone())).color(ACCENT).width(2.0_f32));
        p.points(Points::new(PlotPoints::from(pts)).color(ACCENT).radius(3.5_f32));
    });
}

/// Composite score per shot against the sensitivity it was taken at.
pub fn performance_curve(ui: &mut egui::Ui, samples: &[(f64, f64)], result: Option<f64>) {
    let pts: Vec<[f64; 2]> = samples.iter().map(|&(x, y)| [x, y]).collect();
    base("performance_curve", "cm/360°", "Score").show(ui, |p| {
        p.points(
            Points::new(PlotPoints::from(pts))
                .color(ACCENT.gamma_multiply(0.8))
                .radius(3.0_f32),
        );
        if let Some(x) = result {
            p.vline(
                VLine::new(x)
                    .color(SUCCESS)
                    .width(1.5_f32)
                    .name(format!("Your result: {x:.1} cm")),
            );
        }
    });
}

/// Reaction time per shot in one session.
pub fn reaction_times(ui: &mut egui::Ui, rts: &[f32]) {
    let pts: Vec<[f64; 2]> = rts
        .iter()
        .enumerate()
        .map(|(i, &rt)| [i as f64 + 1.0, rt as f64])
        .collect();
    base("reaction_times", "Shot", "ms").show(ui, |p| {
        p.line(Line::new(PlotPoints::from(pts)).color(ACCENT).width(2.0_f32));
    });
}

const SPARK_HEIGHT: f32 = 28.0;

/// A small trend line with no axes: first value to latest, scaled to its own range.
pub fn sparkline(ui: &mut egui::Ui, values: &[f32]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), SPARK_HEIGHT),
        egui::Sense::hover(),
    );
    if values.len() < 2 || !ui.is_rect_visible(rect) {
        return;
    }
    let (lo, hi) = values
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(f32::EPSILON);
    let pad = 4.0;
    let n = (values.len() - 1) as f32;
    let pts: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            egui::pos2(
                rect.left() + pad + (rect.width() - 2.0 * pad) * i as f32 / n,
                rect.bottom() - pad - (rect.height() - 2.0 * pad) * (v - lo) / span,
            )
        })
        .collect();
    let painter = ui.painter();
    painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.5_f32, ACCENT)));
    if let Some(&last) = pts.last() {
        painter.circle_filled(last, 3.0, ACCENT);
    }
}
