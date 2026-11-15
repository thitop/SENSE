use crate::ui::{UiAction, UiModel, UiState};

pub fn show(ui: &mut egui::Ui, _state: &mut UiState, _model: &UiModel, _actions: &mut Vec<UiAction>) {
    ui.heading("history");
}
