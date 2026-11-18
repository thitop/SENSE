//! The four "Before you start" questions: what goes wrong, grip, pivot and mousepad.
//! Drawn full screen like first-run setup; every answer is optional ("Not sure").

use chrono::{DateTime, Utc};
use egui::{pos2, vec2, Align, CornerRadius, Key, Layout, RichText, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icon;

use crate::data::setup::{AimProblem, Grip, PadSize, PlayerSetup, Pivot};
use crate::ui::question_art;
use crate::ui::screens::centered;
use crate::ui::theme::*;
use crate::ui::{widgets, UiAction, UiModel, UiState};

/// Questions in progress.
#[derive(Clone, Debug)]
pub struct QuestionsDraft {
    pub step: u8,
    pub problems: Vec<AimProblem>,
    pub problems_not_sure: bool,
    pub grip: Option<Grip>,
    pub pivot: Option<Pivot>,
    pub pad: Option<PadSize>,
    /// Only the first question is asked again (the saved answer no longer fits the sensitivity).
    pub q1_only: bool,
    /// Opened from Settings: finishing saves and returns there instead of starting a run.
    pub from_settings: bool,
}

impl QuestionsDraft {
    pub fn from_setup(setup: Option<&PlayerSetup>, q1_only: bool, from_settings: bool) -> Self {
        let (problems, grip, pivot, pad) = match setup {
            Some(s) => (s.problems.clone(), s.grip, s.pivot, s.pad),
            None => (Vec::new(), None, None, None),
        };
        Self {
            step: 1,
            problems_not_sure: setup.is_some() && problems.is_empty(),
            problems,
            grip,
            pivot,
            pad,
            q1_only,
            from_settings,
        }
    }

    /// Select or unselect a problem; picking one clears "Not sure".
    pub fn toggle_problem(&mut self, p: AimProblem) {
        self.problems_not_sure = false;
        if let Some(i) = self.problems.iter().position(|x| *x == p) {
            self.problems.remove(i);
        } else {
            self.problems.push(p);
        }
    }

    /// "Not sure" clears every problem.
    pub fn set_problems_not_sure(&mut self) {
        self.problems.clear();
        self.problems_not_sure = true;
    }

    pub fn last_step(&self) -> u8 {
        if self.q1_only {
            1
        } else {
            4
        }
    }

    pub fn to_setup(&self, own_cm: f64, now: DateTime<Utc>) -> PlayerSetup {
        PlayerSetup {
            problems: self.problems.clone(),
            grip: self.grip,
            pivot: self.pivot,
            pad: self.pad,
            answered_at_cm: own_cm,
            answered_at: now,
        }
    }
}

/// "Skip all": every answer becomes "Not sure"; the calibration starts unless we came from Settings.
pub fn skip_all(draft: &QuestionsDraft, own_cm: f64, now: DateTime<Utc>) -> UiAction {
    let setup = if draft.q1_only {
        // Only the first question was being asked again: keep the saved grip, pivot and pad.
        let mut kept = draft.clone();
        kept.problems.clear();
        kept.to_setup(own_cm, now)
    } else {
        PlayerSetup::not_sure(own_cm, now)
    };
    UiAction::SaveSetup { setup, start_after: !draft.from_settings }
}

pub fn cancel() -> UiAction {
    UiAction::CancelQuestions
}

pub fn show(ui: &mut egui::Ui, state: &mut UiState, model: &UiModel, actions: &mut Vec<UiAction>) {
    let Some(draft) = state.questions.as_mut() else {
        return;
    };
    let own_cm = model.profile.current_cm_360;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        show_step(ui, draft, own_cm, Utc::now(), actions);
    });
}

/// One step of the questions. Split from `show` so it can be drawn without a full `UiModel`.
pub fn show_step(
    ui: &mut egui::Ui,
    draft: &mut QuestionsDraft,
    own_cm: f64,
    now: DateTime<Utc>,
    actions: &mut Vec<UiAction>,
) {
    if draft.step < 1 || draft.step > draft.last_step() {
        draft.step = 1;
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        actions.push(cancel());
    }
    ui.add_space(S6);
    centered(ui, 860.0, |ui| {
        ui.spacing_mut().item_spacing.y = S2;
        ui.horizontal(|ui| {
            let eyebrow = if draft.q1_only {
                "Before you start".to_string()
            } else {
                format!("Before you start · {} of 4", draft.step)
            };
            widgets::eyebrow(ui, &eyebrow);
            if !draft.q1_only {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| step_dots(ui, draft.step));
            }
        });
        ui.add_space(S2);
        match draft.step {
            1 => step_problems(ui, draft),
            2 => step_grip(ui, draft),
            3 => step_pivot(ui, draft),
            _ => step_pad(ui, draft),
        }
        ui.add_space(S4);
        buttons(ui, draft, own_cm, now, actions);
    });
}

fn step_dots(ui: &mut egui::Ui, step: u8) {
    for i in (1..=4u8).rev() {
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 4.0), Sense::hover());
        let color = if i <= step { ACCENT } else { BORDER_STRONG };
        ui.painter().rect_filled(rect, CornerRadius::same(2), color);
    }
}

fn title(ui: &mut egui::Ui, text: &str, sub: &str) {
    ui.label(RichText::new(text).font(display_bold(28.0)).color(TEXT));
    ui.label(RichText::new(sub).font(body(14.0)).color(TEXT_SECONDARY));
    ui.add_space(S3);
}

fn buttons(
    ui: &mut egui::Ui,
    draft: &mut QuestionsDraft,
    own_cm: f64,
    now: DateTime<Utc>,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        if widgets::ghost_button(ui, None, "Skip all").clicked() {
            actions.push(skip_all(draft, own_cm, now));
        }
        if draft.from_settings && widgets::secondary_button(ui, None, "Cancel").clicked() {
            actions.push(cancel());
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let last = draft.step >= draft.last_step();
            let label = match (last, draft.from_settings) {
                (false, _) => "Continue",
                (true, true) => "Save",
                (true, false) => "Start calibration",
            };
            if widgets::primary_button(ui, (label != "Save").then_some(icon::ARROW_RIGHT), label).clicked() {
                if last {
                    actions.push(UiAction::SaveSetup {
                        setup: draft.to_setup(own_cm, now),
                        start_after: !draft.from_settings,
                    });
                } else {
                    draft.step += 1;
                }
            }
            if draft.step > 1 && widgets::secondary_button(ui, Some(icon::ARROW_LEFT), "Back").clicked() {
                draft.step -= 1;
            }
        });
    });
}

/// Radio circle in a card's top-right corner, plus the faint accent fill when selected.
fn mark_selected(ui: &egui::Ui, rect: egui::Rect, selected: bool) {
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(R_CARD), ACCENT.gamma_multiply(0.07));
    }
    let c = pos2(rect.right() - 16.0, rect.top() + 16.0);
    if selected {
        painter.circle_filled(c, 6.5, ACCENT);
        painter.circle_stroke(c, 6.5, Stroke::new(1.5_f32, ACCENT));
        painter.circle_filled(c, 3.0, SURFACE);
    } else {
        painter.circle_stroke(c, 6.5, Stroke::new(1.5_f32, TEXT_FAINT));
    }
    if selected {
        painter.rect_stroke(rect, CornerRadius::same(R_CARD), Stroke::new(1.0_f32, ACCENT), StrokeKind::Inside);
    }
}

fn labels(ui: &mut egui::Ui, label: &str, desc: &str) {
    ui.label(RichText::new(label).font(medium(14.0)).color(TEXT));
    if !desc.is_empty() {
        ui.label(RichText::new(desc).font(body(12.0)).color(TEXT_MUTED));
    }
}

/// Text-only card; returns true when clicked.
fn text_card(ui: &mut egui::Ui, selected: bool, label: &str, desc: &str) -> bool {
    let resp = widgets::clickable_card(ui, selected, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        labels(ui, label, desc);
    });
    mark_selected(ui, resp.rect, selected);
    resp.clicked()
}

/// Card with a picture above its label; returns true when clicked.
fn picture_card(ui: &mut egui::Ui, selected: bool, art: &str, label: &str, desc: &str) -> bool {
    let resp = widgets::clickable_card(ui, selected, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(190.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.vertical_centered(|ui| {
            question_art::picture(ui, art, vec2((ui.available_width() - S4).max(40.0), 120.0));
        });
        ui.add_space(S2);
        labels(ui, label, desc);
    });
    mark_selected(ui, resp.rect, selected);
    resp.clicked()
}

fn step_problems(ui: &mut egui::Ui, draft: &mut QuestionsDraft) {
    title(
        ui,
        "What's going wrong?",
        "Pick anything that sounds like you. This tells us which direction to move.",
    );
    let items = [
        (AimProblem::Overshoot, "I overshoot my target", "Fly past it, then drag back"),
        (AimProblem::CantTurnFast, "I can't turn fast enough", "Someone flicks and I'm still turning"),
        (AimProblem::ShakySmall, "Small adjustments are shaky", "Fine at range, jittery up close"),
        (AimProblem::RunOutOfPad, "I run out of mousepad", "Lifting mid-fight to reset"),
    ];
    for pair in items.chunks(2) {
        ui.columns(2, |cols| {
            for (col, (p, label, desc)) in cols.iter_mut().zip(pair) {
                if text_card(col, draft.problems.contains(p), label, desc) {
                    draft.toggle_problem(*p);
                }
            }
        });
    }
    if text_card(ui, draft.problems_not_sure, "Not sure", "") {
        draft.set_problems_not_sure();
    }
}

fn pick_row<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut Option<T>,
    options: &[(T, &str, &str, &str)],
    not_sure_art: bool,
) {
    let cols = options.len() + usize::from(not_sure_art);
    ui.columns(cols, |c| {
        for (col, (v, art, label, desc)) in c.iter_mut().zip(options) {
            if picture_card(col, *value == Some(*v), art, label, desc) {
                *value = Some(*v);
            }
        }
        if not_sure_art {
            let col = &mut c[options.len()];
            if picture_card(col, value.is_none(), "not_sure", "Not sure", "") {
                *value = None;
            }
        }
    });
}

fn step_grip(ui: &mut egui::Ui, draft: &mut QuestionsDraft) {
    title(ui, "How do you hold it?", "Look down at your hand. Whichever picture matches.");
    pick_row(
        ui,
        &mut draft.grip,
        &[
            (Grip::Palm, "grip_palm", "Palm grip", "Hand flat, full contact"),
            (Grip::Claw, "grip_claw", "Claw grip", "Arched, palm rear only"),
            (Grip::Fingertip, "grip_fingertip", "Fingertip grip", "Only the tips touch"),
        ],
        true,
    );
}

fn step_pivot(ui: &mut egui::Ui, draft: &mut QuestionsDraft) {
    title(ui, "What moves when you flick?", "Best guess is fine.");
    pick_row(
        ui,
        &mut draft.pivot,
        &[
            (Pivot::Wrist, "pivot_wrist", "Wrist", "Hand turns, forearm still"),
            (Pivot::Fingers, "pivot_fingers", "Fingers", "Mouse barely moves"),
            (Pivot::Arm, "pivot_arm", "Arm", "Elbow stays on the desk"),
            (Pivot::Shoulder, "pivot_shoulder", "Shoulder", "Whole arm sweeps"),
        ],
        false,
    );
    if text_card(ui, draft.pivot.is_none(), "Not sure", "") {
        draft.pivot = None;
    }
}

fn step_pad(ui: &mut egui::Ui, draft: &mut QuestionsDraft) {
    title(
        ui,
        "How big is your mousepad?",
        "This is your hard limit - no sensitivity works if you run out of room.",
    );
    pick_row(
        ui,
        &mut draft.pad,
        &[
            (PadSize::Small, "pad_small", "Small mousepad", "Mouse only, tight"),
            (PadSize::Medium, "pad_medium", "Medium mousepad", "Room for full sweep"),
            (PadSize::Large, "pad_large", "Large mousepad", "Desk mat - keyboard sits on it"),
        ],
        true,
    );
}
