use egui_plot::{Line, Plot, PlotPoints, Points};

pub struct PerformanceGraphs;

impl PerformanceGraphs {
    /// Render Accuracy vs Sensitivity (cm/360°) curve & samples
    pub fn render_accuracy_curve(ui: &mut egui::Ui, samples: &[(f64, f64)], optimal_cm: Option<f64>) {
        ui.heading("📈 Sensitivity Performance Curve");
        ui.label("Plots composite performance against cm/360° sensitivity with fitted peak.");

        let plot_points: PlotPoints = samples.iter().map(|&(x, y)| [x, y]).collect();
        let points = Points::new(plot_points)
            .radius(4.5_f32)
            .color(egui::Color32::from_rgb(255, 215, 0));

        Plot::new("accuracy_vs_sens_plot")
            .view_aspect(2.2_f32)
            .x_axis_label("Sensitivity (cm/360°)")
            .y_axis_label("Score / Performance")
            .show(ui, |plot_ui| {
                plot_ui.points(points);

                if let Some(opt_x) = optimal_cm {
                    let vline = Line::new(vec![[opt_x, 0.0], [opt_x, 100.0]])
                        .color(egui::Color32::from_rgb(0, 255, 128))
                        .name(format!("Optimal: {:.1} cm", opt_x));
                    plot_ui.line(vline);
                }
            });
    }

    /// Render Reaction Time Distribution across shots
    pub fn render_reaction_time_plot(ui: &mut egui::Ui, reaction_times_ms: &[f32]) {
        ui.heading("⚡ Reaction Time per Shot (ms)");

        let plot_points: PlotPoints = reaction_times_ms
            .iter()
            .enumerate()
            .map(|(i, &rt)| [i as f64 + 1.0, rt as f64])
            .collect();

        let line = Line::new(plot_points)
            .color(egui::Color32::from_rgb(80, 200, 255))
            .width(2.0_f32);

        Plot::new("rt_distribution_plot")
            .view_aspect(2.2_f32)
            .x_axis_label("Shot #")
            .y_axis_label("Reaction Time (ms)")
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });
    }

    /// Render PSA Convergence history across calibration rounds
    pub fn render_psa_convergence(ui: &mut egui::Ui, round_cms: &[f64]) {
        ui.heading("🎯 PSA Convergence Flow");
        ui.label("Shows how sensitivity narrows down across rounds.");

        let plot_points: PlotPoints = round_cms
            .iter()
            .enumerate()
            .map(|(i, &cm)| [i as f64 + 1.0, cm])
            .collect();

        let line = Line::new(plot_points)
            .color(egui::Color32::from_rgb(255, 100, 100))
            .width(2.5_f32);

        Plot::new("psa_convergence_plot")
            .view_aspect(2.2_f32)
            .x_axis_label("Round #")
            .y_axis_label("Tested cm/360°")
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });
    }
}
