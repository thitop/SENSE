//! Draws the calibration replay (layout A): the graph, the controls, then one card for the
//! current round or the result. The text and numbers come from [`super::replay`].

use std::time::Duration;

use egui::{
    vec2, Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Response, RichText,
    Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType,
};
use egui_phosphor::regular as icon;
use egui_plot::{
    GridMark, HLine, Line, LineStyle, Plot, PlotBounds, PlotPoint, PlotPoints, Points, Polygon,
    Text,
};

use super::replay::{Replay, ReplayState, ResultView, RoundView, TestRow, Tone, STEP_SECS};
use super::theme::*;
use super::widgets;
use crate::sensitivity::blocks::SignalCounts;
use crate::sensitivity::psa::PsaRoundConclusion;

const GRAPH_HEIGHT: f32 = 230.0;
/// cm/360 values labelled on the log-scale axis.
const Y_TICKS: [f64; 6] = [15.0, 20.0, 30.0, 40.0, 60.0, 80.0];
/// The y axis always spans at least this much, so a short run doesn't zoom in.
const Y_MIN_CM: f64 = 12.0;
const Y_MAX_CM: f64 = 82.0;
/// Half the width of a round's search-range band, in rounds.
const BAND_HALF: f64 = 0.46;
/// Half the width of the ± band around a guess that has no neighbour yet.
const LONE_GUESS_HALF: f64 = 0.12;
const NAME_WIDTH: f32 = 150.0;
const VERDICT_WIDTH: f32 = 170.0;
const ROW_HEIGHT: f32 = 30.0;
const BAR_HEIGHT: f32 = 8.0;
const PILL_HEIGHT: f32 = 26.0;

/// The replay under "How we got here". `steps` is `rounds + 1`; the last step is the result.
pub fn show(ui: &mut Ui, state: &mut ReplayState, replay: &Replay) {
    let steps = replay.rounds.len() + 1;
    let now = ui.input(|i| i.time);
    if state.playing {
        state.tick(now, steps);
        if state.playing {
            let left = (STEP_SECS - (now - state.last_step_at)).max(0.0);
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(left));
        }
    }

    let graph_card = widgets::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        graph(ui, replay, state.step);
        ui.add_space(S1);
        legend(ui);
    });
    // An armed autoplay starts once the graph itself is on screen.
    if ui.is_rect_visible(graph_card.response.rect) {
        let before = (state.step, state.playing);
        state.on_visible(now, steps);
        if (state.step, state.playing) != before {
            ui.ctx().request_repaint();
        }
    }
    ui.add_space(S3);
    controls(ui, state, steps, now);
    ui.add_space(S3);
    widgets::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match replay.rounds.get(state.step) {
            Some(round) => round_card(ui, round),
            None => result_card(ui, &replay.result),
        }
    });
}

fn conclusion_color(c: PsaRoundConclusion) -> Color32 {
    match c {
        PsaRoundConclusion::TooFastNeedSlower => DANGER,
        PsaRoundConclusion::TooSlowNeedFaster => SLOW,
        PsaRoundConclusion::BalancedNarrow => TEXT_SECONDARY,
    }
}

/// Same rule as `replay::verdict`, as a colour.
fn verdict_color(c: SignalCounts) -> Color32 {
    if c.too_fast > c.too_slow {
        DANGER
    } else if c.too_slow > c.too_fast {
        SLOW
    } else {
        TEXT_SECONDARY
    }
}

/// The cm/360 span of the y axis, fixed for the whole replay so it doesn't move while playing.
fn y_span(replay: &Replay) -> (f64, f64) {
    let mut lo = Y_MIN_CM;
    let mut hi = Y_MAX_CM;
    for r in &replay.rounds {
        let (l, h) = r.range.unwrap_or((r.tested_cm_360, r.tested_cm_360));
        lo = lo.min(l.min(r.tested_cm_360) * 0.95);
        hi = hi.max(h.max(r.tested_cm_360) * 1.05);
    }
    if let Some(cm) = replay.result.cm_360 {
        lo = lo.min(cm * 0.95);
        hi = hi.max(cm * 1.05);
    }
    (lo, hi)
}

fn graph(ui: &mut Ui, replay: &Replay, step: usize) {
    let n = replay.rounds.len();
    let shown = &replay.rounds[..(step + 1).min(n)];
    let current = (step < n).then_some(step);
    let at_result = step >= n;
    let (lo, hi) = y_span(replay);
    let x = |i: usize| i as f64 + 1.0;

    ui.scope(|ui| {
        // Axis labels take the text colour.
        ui.visuals_mut().override_text_color = Some(TEXT_FAINT);
        Plot::new("calibration_replay")
            .height(GRAPH_HEIGHT)
            .allow_scroll(false)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_background(false)
            .show_grid(false)
            .show_x(false)
            .show_y(false)
            .x_grid_spacer(move |_| {
                (1..=n)
                    .map(|i| GridMark {
                        value: i as f64,
                        step_size: 1.0,
                    })
                    .collect()
            })
            .y_grid_spacer(|_| {
                Y_TICKS
                    .iter()
                    .map(|v| GridMark {
                        value: v.ln(),
                        step_size: 1.0,
                    })
                    .collect()
            })
            .x_axis_formatter(|m, _| format!("R{}", m.value.round() as i64))
            .y_axis_formatter(|m, _| format!("{:.0}", m.value.exp()))
            .y_axis_label("cm/360°")
            .show(ui, |p| {
                p.set_plot_bounds(PlotBounds::from_min_max(
                    [0.5, lo.ln()],
                    [n as f64 + 0.5, hi.ln()],
                ));
                for v in Y_TICKS {
                    p.hline(
                        HLine::new(v.ln())
                            .color(BORDER)
                            .width(1.0_f32)
                            .allow_hover(false),
                    );
                }

                // The range each round's sensitivity was picked within.
                for (i, r) in shown.iter().enumerate() {
                    if let Some((l, h)) = r.range {
                        let range = ACCENT.gamma_multiply(0.07);
                        p.polygon(band(
                            x(i) - BAND_HALF,
                            x(i) + BAND_HALF,
                            (l, h),
                            (l, h),
                            range,
                        ));
                    }
                }

                // Best guess so far, with its ± band.
                let guesses: Vec<(f64, f64, f64)> = shown
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| r.guess_point.map(|(cm, rel)| (x(i), cm, rel)))
                    .collect();
                let spread = SUCCESS.gamma_multiply(0.16);
                // Symmetric on the log axis.
                let plus_minus = |cm: f64, rel: f64| (cm / (1.0 + rel), cm * (1.0 + rel));
                if let [(gx, cm, rel)] = guesses[..] {
                    // A lone guess has no segment to span; give its ± some width.
                    let pm = plus_minus(cm, rel);
                    p.polygon(band(
                        gx - LONE_GUESS_HALF,
                        gx + LONE_GUESS_HALF,
                        pm,
                        pm,
                        spread,
                    ));
                }
                // One quad per segment: the polygon fill only handles convex shapes.
                for w in guesses.windows(2) {
                    let ((x0, c0, r0), (x1, c1, r1)) = (w[0], w[1]);
                    p.polygon(band(x0, x1, plus_minus(c0, r0), plus_minus(c1, r1), spread));
                }
                let guess_line: Vec<[f64; 2]> =
                    guesses.iter().map(|&(gx, cm, _)| [gx, cm.ln()]).collect();
                p.line(
                    Line::new(PlotPoints::from(guess_line.clone()))
                        .color(SUCCESS)
                        .width(1.5_f32)
                        .style(LineStyle::Dashed { length: 4.0 })
                        .allow_hover(false),
                );
                for &pt in &guess_line {
                    p.points(dot(pt, SUCCESS, 2.0));
                }

                // What each round tested.
                let tested: Vec<[f64; 2]> = shown
                    .iter()
                    .enumerate()
                    .map(|(i, r)| [x(i), r.tested_cm_360.ln()])
                    .collect();
                p.line(
                    Line::new(PlotPoints::from(tested.clone()))
                        .color(ACCENT.gamma_multiply(0.5))
                        .width(1.5_f32)
                        .allow_hover(false),
                );
                for (i, (r, pt)) in shown.iter().zip(tested).enumerate() {
                    let radius = if current == Some(i) {
                        p.points(dot(pt, ACCENT, 7.0));
                        5.0
                    } else {
                        4.0
                    };
                    p.points(dot(pt, conclusion_color(r.conclusion), radius));
                }

                if let Some(cm) = replay.result.cm_360.filter(|_| at_result) {
                    p.hline(
                        HLine::new(cm.ln())
                            .color(ACCENT)
                            .width(1.5_f32)
                            .allow_hover(false),
                    );
                    p.text(
                        Text::new(
                            PlotPoint::new(n as f64 + 0.45, cm.ln() + 0.02),
                            // The colour must be on the text: the scope's override would win.
                            RichText::new(format!("{cm:.1} cm"))
                                .font(medium(12.0))
                                .color(ACCENT),
                        )
                        .anchor(egui::Align2::RIGHT_BOTTOM)
                        .allow_hover(false),
                    );
                }
            });
    });
}

/// A filled quad from `x0` to `x1`, between (low, high) cm/360 at each end.
fn band(x0: f64, x1: f64, at0: (f64, f64), at1: (f64, f64), color: Color32) -> Polygon<'static> {
    let quad = vec![
        [x0, at0.0.ln()],
        [x1, at1.0.ln()],
        [x1, at1.1.ln()],
        [x0, at0.1.ln()],
    ];
    Polygon::new(PlotPoints::from(quad))
        .fill_color(color)
        .stroke(Stroke::NONE)
        .allow_hover(false)
}

fn dot(pt: [f64; 2], color: Color32, radius: f32) -> Points<'static> {
    Points::new(PlotPoints::from(vec![pt]))
        .color(color)
        .radius(radius)
        .allow_hover(false)
}

fn legend(ui: &mut Ui) {
    let items = [
        (DANGER, "Too fast"),
        (SLOW, "Too slow"),
        (TEXT_SECONDARY, "Tie"),
        (ACCENT.gamma_multiply(0.3), "Search range"),
        (SUCCESS.gamma_multiply(0.5), "Best guess"),
    ];
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(width, 16.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            for (i, (color, label)) in items.into_iter().enumerate() {
                if i > 0 {
                    ui.add_space(S3);
                }
                let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().rect_filled(rect, CornerRadius::same(2), color);
                ui.label(RichText::new(label).font(body(12.0)).color(TEXT_MUTED));
            }
        },
    );
}

fn controls(ui: &mut Ui, state: &mut ReplayState, steps: usize, now: f64) {
    let before = (state.step, state.playing);
    ui.horizontal(|ui| {
        // The UI fonts have no ❚❚; the icon font draws both states.
        let (glyph, label) = if state.playing {
            (icon::PAUSE, "Pause")
        } else {
            (icon::PLAY, "Play")
        };
        if widgets::secondary_button(ui, Some(glyph), label).clicked() {
            if state.playing {
                state.pause();
            } else {
                state.play(now, steps);
            }
        }
        if widgets::secondary_button(ui, None, "◀").clicked() {
            state.prev(steps);
        }
        if widgets::secondary_button(ui, None, "▶").clicked() {
            state.next(steps);
        }
        ui.add_space(S2);
        ui.spacing_mut().item_spacing.x = S1;
        for i in 0..steps {
            let label = if i + 1 == steps {
                "Result".to_string()
            } else {
                (i + 1).to_string()
            };
            if step_pill(ui, &label, i < state.step, i == state.step).clicked() {
                state.go_to(i, steps);
            }
        }
    });
    // The graph and card above were drawn before the click.
    if (state.step, state.playing) != before {
        ui.ctx().request_repaint();
    }
}

/// A round button: filled once played, accent outline on the current step.
fn step_pill(ui: &mut Ui, label: &str, done: bool, current: bool) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), body(12.0), Color32::PLACEHOLDER);
    let size = vec2((galley.size().x + 14.0).max(PILL_HEIGHT), PILL_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), current, label));
    if ui.is_rect_visible(rect) {
        let t = ui
            .ctx()
            .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        let fill = if done {
            SURFACE_RAISED
        } else {
            SURFACE_RAISED.gamma_multiply(t * 0.6)
        };
        let stroke = if current {
            ACCENT
        } else {
            BORDER_STRONG.lerp_to_gamma(TEXT_FAINT, t)
        };
        let text = if current {
            ACCENT
        } else if done {
            TEXT_SECONDARY.lerp_to_gamma(TEXT, t)
        } else {
            TEXT_MUTED.lerp_to_gamma(TEXT, t)
        };
        ui.painter().rect(
            rect,
            CornerRadius::same((PILL_HEIGHT / 2.0) as u8),
            fill,
            Stroke::new(1.0_f32, stroke),
            StrokeKind::Inside,
        );
        ui.painter()
            .galley(rect.center() - galley.size() / 2.0, galley, text);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Title on the left, an optional accent value on the right.
fn card_header(ui: &mut Ui, title: &str, value: Option<String>) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(width, 22.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            ui.label(RichText::new(title).font(medium(15.0)).color(TEXT));
            if let Some(value) = value {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(value).font(display_bold(16.0)).color(ACCENT));
                });
            }
        },
    );
    ui.add_space(S1);
}

/// One round: header, a row per test, then the footer. Part B's clips go between rows and footer.
fn round_card(ui: &mut Ui, r: &RoundView) {
    card_header(
        ui,
        &r.header,
        Some(format!("{:.1} cm/360°", r.tested_cm_360)),
    );
    round_rows(ui, &r.rows);
    ui.add_space(S2);
    round_footer(ui, r);
}

fn round_rows(ui: &mut Ui, rows: &[TestRow]) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        let bar_width = (width - NAME_WIDTH - VERDICT_WIDTH - 2.0 * S3).max(40.0);
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                let (left, y) = (ui.cursor().left(), ui.cursor().top());
                ui.painter()
                    .hline(left..=left + width, y, Stroke::new(1.0_f32, BORDER));
            }
            ui.allocate_ui_with_layout(
                vec2(width, ROW_HEIGHT),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.x = S3;
                    ui.allocate_ui_with_layout(
                        vec2(NAME_WIDTH, ROW_HEIGHT),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_width(NAME_WIDTH);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&row.test).font(body(13.0)).color(TEXT),
                                )
                                .truncate(),
                            );
                        },
                    );
                    split_bar(ui, row.counts, bar_width);
                    ui.add(
                        egui::Label::new(
                            RichText::new(row.verdict)
                                .font(body(12.5))
                                .color(verdict_color(row.counts)),
                        )
                        .truncate(),
                    );
                },
            );
        }
    });
}

/// A test's pairs split into too fast / tie / too slow.
fn split_bar(ui: &mut Ui, c: SignalCounts, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, BAR_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let r = (BAR_HEIGHT / 2.0) as u8;
    painter.rect_filled(rect, CornerRadius::same(r), SURFACE_RAISED);
    let total = c.too_fast + c.direct + c.too_slow;
    if total == 0 {
        return;
    }
    let parts: Vec<(usize, Color32)> = [
        (c.too_fast, DANGER),
        (c.direct, BORDER_STRONG),
        (c.too_slow, SLOW),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .collect();
    let mut left = rect.left();
    for (k, (count, color)) in parts.iter().enumerate() {
        let w = rect.width() * *count as f32 / total as f32;
        let seg = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(left + w, rect.bottom()),
        );
        let (first, last) = (k == 0, k + 1 == parts.len());
        let corners = CornerRadius {
            nw: if first { r } else { 0 },
            sw: if first { r } else { 0 },
            ne: if last { r } else { 0 },
            se: if last { r } else { 0 },
        };
        painter.rect_filled(seg, corners, *color);
        left += w;
    }
}

fn round_footer(ui: &mut Ui, r: &RoundView) {
    Frame::NONE
        .fill(SURFACE_RAISED)
        .corner_radius(CornerRadius::same(R_CONTROL))
        .inner_margin(Margin::symmetric(S3 as i8, 9))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            if let Some(line) = &r.guess_line {
                ui.label(
                    RichText::new(widgets::arrows(line))
                        .font(body(13.0))
                        .color(TEXT),
                );
            }
            ui.label(
                RichText::new(widgets::arrows(&r.next_line))
                    .font(body(13.0))
                    .color(TEXT_SECONDARY),
            );
        });
}

fn result_card(ui: &mut Ui, r: &ResultView) {
    card_header(ui, "Result", None);
    if let Some(cm) = r.cm_360 {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{cm:.1}"))
                    .font(display_bold(34.0))
                    .color(ACCENT),
            );
            let unit = match r.rel_uncertainty {
                Some(rel) => format!("cm/360° ± {:.0}%", rel * 100.0),
                None => "cm/360°".into(),
            };
            ui.label(RichText::new(unit).font(body(14.0)).color(TEXT_SECONDARY));
        });
    }
    ui.spacing_mut().item_spacing.y = S1;
    for (line, tone) in &r.lines {
        let color = match tone {
            Tone::Normal => TEXT,
            Tone::Muted => TEXT_SECONDARY,
            Tone::Warn => WARN,
        };
        ui.label(
            RichText::new(widgets::arrows(line))
                .font(body(13.0))
                .color(color),
        );
    }
}
