use egui::{vec2, Align, Layout, RichText};
use egui_phosphor::regular as icon;

use crate::data::{MetricPoint, SessionHistory, SessionRecord};
use crate::sensitivity::converter::SensitivityConverter;
use crate::sensitivity::games::{GameProfile, ScopedSetting};
use crate::ui::screens::{header, section};
use crate::ui::theme::*;
use crate::ui::{graphs, widgets, UiAction, UiModel, UiState};

const MAX_ROWS: usize = 50;
/// Column widths as fractions of the row: scenario, date, score, accuracy, cm/360.
const COLS: [f32; 5] = [0.32, 0.24, 0.14, 0.14, 0.16];

/// Sessions newest first.
pub fn rows(h: &SessionHistory) -> Vec<&SessionRecord> {
    let mut v: Vec<_> = h.sessions.iter().collect();
    v.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    v
}

pub fn find_session<'a>(h: &'a SessionHistory, id: &str) -> Option<&'a SessionRecord> {
    h.sessions.iter().find(|s| s.session_id == id)
}

/// "41 s" under a minute, "2:05" from a minute up.
pub fn duration_text(secs: f32) -> String {
    let total = secs.max(0.0).round() as u32;
    if total < 60 {
        format!("{total} s")
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

/// Longest run of consecutive hits. Counted from the shots because sessions are saved with
/// `max_streak` 0.
pub fn best_streak(points: &[MetricPoint]) -> u32 {
    points
        .iter()
        .scan(0u32, |run, p| {
            *run = if p.hit { *run + 1 } else { 0 };
            Some(*run)
        })
        .max()
        .unwrap_or(0)
}

/// A session's sensitivity as one game's in-game value.
#[derive(Clone, Debug, PartialEq)]
pub struct GameValue {
    /// Game id from `data/games.json`; picks the logo.
    pub id: String,
    pub name: String,
    pub sens: f64,
    /// The player's own game; listed first.
    pub main: bool,
    pub scoped: Vec<ScopedSetting>,
    pub scoped_note: Option<String>,
}

/// The session's cm/360 at the session's own DPI in every game, the player's main game first.
pub fn game_values(s: &SessionRecord, games: &[GameProfile], main_id: &str) -> Vec<GameValue> {
    let mut v: Vec<GameValue> = games
        .iter()
        .map(|g| GameValue {
            id: g.id.clone(),
            name: g.name.clone(),
            sens: SensitivityConverter::cm_360_to_game_sens(g, s.tested_dpi, s.tested_cm_360),
            main: g.id == main_id,
            scoped: g.scoped.clone(),
            scoped_note: g.scoped_note.clone(),
        })
        .collect();
    // Stable: the other games keep their order.
    v.sort_by_key(|g| !g.main);
    v
}

/// One tile on a session's detail page.
#[derive(Clone, Debug, PartialEq)]
pub struct DetailStat {
    pub label: &'static str,
    pub value: String,
    pub unit: Option<&'static str>,
    pub caption: String,
}

fn stat(label: &'static str, value: String, unit: Option<&'static str>, caption: String) -> DetailStat {
    DetailStat { label, value, unit, caption }
}

/// The tiles of a session's detail page, in display order. Values a session didn't record
/// (no shots, e.g. tracking) show "—".
pub fn detail_stats(s: &SessionRecord) -> Vec<DetailStat> {
    let m = &s.metrics;
    let shots = m.total_shots > 0;
    let hit = m.hits > 0;
    let dash = || "—".to_string();
    vec![
        stat("Score", m.total_score.to_string(), None, s.task_type.clone()),
        stat(
            "Accuracy",
            if shots { format!("{:.0}", m.accuracy_pct) } else { dash() },
            shots.then_some("%"),
            format!("{} of {} hit", m.hits, m.total_shots),
        ),
        stat(
            "Reaction time",
            if hit { format!("{:.0}", m.avg_reaction_time_ms) } else { dash() },
            hit.then_some("ms"),
            if hit { format!("Median {:.0} ms", m.median_reaction_time_ms) } else { "No hits".into() },
        ),
        stat("Sensitivity", format!("{:.1}", s.tested_cm_360), Some("cm/360"), format!("{:.0} DPI", s.tested_dpi)),
        stat("Duration", duration_text(s.duration_secs), None, "Time played".into()),
        stat(
            "Best streak",
            if shots { best_streak(&m.points).to_string() } else { dash() },
            None,
            "Hits in a row".into(),
        ),
        stat("Shots", m.total_shots.to_string(), None, format!("{} missed", m.misses)),
        stat(
            "Avg. offset",
            if hit { format!("{:.1}", m.avg_offset_m * 100.0) } else { dash() },
            hit.then_some("cm"),
            "From target centre on hits".into(),
        ),
    ]
}

fn cells(ui: &mut egui::Ui, cols: &[f32; 5], values: [RichText; 5]) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, v) in values.into_iter().enumerate() {
            let layout = if i == 0 {
                Layout::left_to_right(Align::Center)
            } else {
                Layout::right_to_left(Align::Center)
            };
            ui.allocate_ui_with_layout(vec2(width * cols[i], 20.0), layout, |ui| {
                ui.set_width(width * cols[i]);
                ui.add(egui::Label::new(v).truncate());
            });
        }
    });
}

fn table_header(ui: &mut egui::Ui, cols: &[f32; 5], names: [&str; 5]) {
    ui.horizontal(|ui| {
        ui.add_space(S4);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - S4);
            let muted = |s: &str| RichText::new(s).font(body(12.0)).color(TEXT_MUTED);
            cells(ui, cols, names.map(muted));
        });
    });
    ui.add_space(S1);
}

fn when(s: &SessionRecord, fmt: &str) -> String {
    s.timestamp.with_timezone(&chrono::Local).format(fmt).to_string()
}

pub fn show(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    if let Some(id) = state.open_session.clone() {
        match find_session(model.history, &id) {
            Some(s) => return detail(ui, state, model, s, actions),
            None => state.open_session = None,
        }
    }

    header(ui, "History", Some("Every finished calibration and practice run. Click one to see its details."));
    let all = rows(model.history);
    if all.is_empty() {
        widgets::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("No sessions yet").font(medium(15.0)).color(TEXT));
            ui.label(
                RichText::new("Finish a calibration or practice run and it shows up here.")
                    .font(body(13.0))
                    .color(TEXT_SECONDARY),
            );
        });
        return;
    }

    table_header(ui, &COLS, ["Scenario", "Date", "Score", "Accuracy", "cm/360"]);
    for s in all.iter().take(MAX_ROWS) {
        let resp = widgets::clickable_row(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            cells(
                ui,
                &COLS,
                [
                    RichText::new(&s.task_type).font(medium(13.0)).color(TEXT),
                    RichText::new(when(s, "%b %d %H:%M")).font(body(13.0)).color(TEXT_SECONDARY),
                    RichText::new(s.metrics.total_score.to_string()).font(display(14.0)).color(TEXT),
                    RichText::new(format!("{:.0}%", s.metrics.accuracy_pct)).font(display(14.0)).color(TEXT),
                    RichText::new(format!("{:.1}", s.tested_cm_360)).font(display(14.0)).color(TEXT),
                ],
            );
        });
        if resp.clicked() {
            state.open_session = Some(s.session_id.clone());
        }
    }
    if all.len() > MAX_ROWS {
        ui.label(
            RichText::new(format!("Showing the latest {MAX_ROWS} of {} sessions", all.len()))
                .font(body(12.0))
                .color(TEXT_MUTED),
        );
    }
}

fn detail(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, s: &SessionRecord, actions: &mut Vec<UiAction>) {
    if widgets::ghost_button(ui, Some(icon::ARROW_LEFT), "All sessions").clicked() {
        state.open_session = None;
    }
    ui.add_space(S3);
    let subtitle = when(s, "%A, %b %d %Y · %H:%M");
    header(ui, &s.task_type, Some(&subtitle));

    let stats = detail_stats(s);
    for row in stats.chunks(4) {
        ui.columns(row.len(), |cols| {
            for (col, st) in cols.iter_mut().zip(row) {
                widgets::stat_card(col, st.label, &st.value, st.unit, &st.caption);
            }
        });
        ui.add_space(S3);
    }

    if let Some(report) = &s.habits {
        ui.add_space(S3);
        crate::ui::habits::cards(ui, model, report, s.timestamp, s.tested_cm_360, actions);
    } else {
        let rows = crate::habits::trend::session_rows(model.history, s);
        crate::ui::habits::session_section(ui, &rows);
    }

    let points = &s.metrics.points;
    ui.add_space(S3);
    section(ui, "Reaction time per shot");
    widgets::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if points.is_empty() {
            ui.label(RichText::new("No shots were recorded in this session.").font(body(13.0)).color(TEXT_MUTED));
        } else {
            let rts: Vec<f32> = points.iter().map(|p| p.reaction_time_ms).collect();
            graphs::reaction_times(ui, &rts);
        }
    });

    ui.add_space(S6);
    section(ui, "Sensitivity in each game");
    ui.label(
        RichText::new(format!(
            "This session's {:.1} cm/360° at {:.0} DPI, as each game's in-game value. Scoped settings keep aim near the crosshair feeling the same zoomed in.",
            s.tested_cm_360, s.tested_dpi
        ))
        .font(body(13.0))
        .color(TEXT_SECONDARY),
    );
    ui.add_space(S3);
    let values = game_values(s, model.games, &model.profile.primary_game_id);
    for (r, pair) in values.chunks(2).enumerate() {
        // Both cards of a row take the taller one's height, measured on the frame before.
        let id = ui.id().with(("game_row_height", r));
        let row_height: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
        let mut tallest = 0.0_f32;
        ui.columns(2, |cols| {
            for (col, g) in cols.iter_mut().zip(pair) {
                let card = widgets::GameCard {
                    game_id: &g.id,
                    name: &g.name,
                    sens: g.sens,
                    main: g.main,
                    scoped: &g.scoped,
                    scoped_note: g.scoped_note.as_deref(),
                };
                let h = widgets::game_card(col, &card, row_height);
                tallest = tallest.max(h);
            }
        });
        if (tallest - row_height).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(id, tallest));
            ui.ctx().request_repaint();
        }
        ui.add_space(S2);
    }
}
