//! Desktop native save workflow. File I/O shares the same action as MCP.

use egui::{Key, KeyboardShortcut, Modifiers, Ui};
use serde_json::json;

use crate::{ViamindApp, actions::UiAction};

/// Save current native file.
pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
/// Choose a native destination.
pub const SAVE_AS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);

/// Local dialog presentation; active file and dirty status live in the engine.
#[derive(Default)]
pub struct FileUi {
    pub(crate) save_as: bool,
    pub(crate) path: String,
    overwrite: bool,
    pub(crate) message: Option<String>,
    focus_path: bool,
}

impl ViamindApp {
    /// Save to the current native file, or ask for a destination.
    pub fn save_design(&mut self) {
        let Some(design) = &self.design else { return };
        if design.engine.document_status().path.is_none() {
            self.begin_save_as();
        } else {
            self.run_save(json!({}));
        }
    }

    /// Start Save As without writing anything.
    pub fn begin_save_as(&mut self) {
        let Some(design) = &self.design else { return };
        self.files.path = design.engine.document_status().path.map_or_else(|| "Untitled.viamind.json".into(), |path| path.display().to_string());
        self.files.overwrite = false;
        self.files.message = None;
        self.files.save_as = true;
        self.files.focus_path = true;
    }

    fn run_save(&mut self, input: serde_json::Value) -> bool {
        let Some(design) = &self.design else { return false };
        match UiAction::SaveProject.run(&design.engine, input) {
            Ok(result) => {
                self.files.message = Some(if result["dirty"] == true {
                    "Saved snapshot; newer edits are still unsaved".into()
                } else {
                    format!("Saved {}", result["path"].as_str().unwrap_or("project"))
                });
                true
            }
            Err(error) => {
                self.files.message = Some(error.to_string());
                false
            }
        }
    }

    pub(crate) fn file_shortcuts(&mut self, ui: &mut Ui) {
        if self.design.is_none() || self.files.save_as {
            return;
        }
        if ui.input_mut(|input| input.consume_shortcut(&SAVE_AS)) {
            self.begin_save_as();
        } else if ui.input_mut(|input| input.consume_shortcut(&SAVE)) {
            self.save_design();
        }
    }

    pub(crate) fn file_dialogs(&mut self, ui: &mut Ui) {
        if !self.files.save_as {
            return;
        }
        let mut save = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("save_project_dialog")).show(ui.ctx(), |ui| {
            ui.heading("Save project as");
            let label = ui.label("Project file");
            let response =
                ui.add(egui::TextEdit::singleline(&mut self.files.path).desired_width(420.0).hint_text("design.viamind.json")).labelled_by(label.id);
            if self.files.focus_path {
                response.request_focus();
                self.files.focus_path = false;
            }
            ui.checkbox(&mut self.files.overwrite, "Replace existing file");
            if let Some(message) = &self.files.message {
                ui.label(message);
            }
            ui.horizontal(|ui| {
                save = ui.button("Save file").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        cancel |= modal.should_close();
        if cancel {
            self.files.save_as = false;
            self.files.message = None;
        } else if save && self.run_save(json!({ "path": self.files.path, "overwrite": self.files.overwrite })) {
            self.files.save_as = false;
        }
    }
}
