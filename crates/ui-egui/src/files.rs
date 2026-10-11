//! Native document workflows. File I/O shares the same actions as MCP.

use egui::{Context, Key, KeyboardShortcut, Modifiers, Ui};
use serde_json::json;
use viamind_engine::{EventCursor, ReplaceGuard};

use crate::{ViamindApp, actions::UiAction};

pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub const SAVE_AS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
pub const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

#[derive(Clone)]
enum Destination {
    New(String),
    Open(String),
    Quit,
}
struct Pending {
    destination: Destination,
    cursor: EventCursor,
}

/// Local dialog presentation; document path and dirty status live in the engine.
#[derive(Default)]
pub struct FileUi {
    pub(crate) save_as: bool,
    pub(crate) path: String,
    overwrite: bool,
    pub(crate) message: Option<String>,
    focus_path: bool,
    editor: Option<bool>, // true = New, false = Open
    editor_text: String,
    focus_editor: bool,
    pending: Option<Pending>,
    confirm: bool,
    allow_close: bool,
}

impl FileUi {
    pub(crate) fn busy(&self) -> bool {
        self.save_as || self.editor.is_some() || self.pending.is_some()
    }
}

impl ViamindApp {
    pub fn save_design(&mut self) -> bool {
        let Some(design) = &self.design else { return false };
        if design.engine.document_status().path.is_none() {
            self.begin_save_as();
            false
        } else {
            self.run_save(json!({}))
        }
    }

    pub fn begin_save_as(&mut self) {
        let Some(design) = &self.design else { return };
        self.files.path = design.engine.document_status().path.map_or_else(|| "Untitled.viamind.json".into(), |path| path.display().to_string());
        self.files.overwrite = false;
        self.files.message = None;
        self.files.save_as = true;
        self.files.focus_path = true;
    }

    /// Present the New or Open form without changing the document.
    pub fn begin_document(&mut self, new: bool) {
        if self.design.is_none()
            || self.files.busy()
            || self.copper.editing()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
        {
            return;
        }
        self.files.editor = Some(new);
        self.files.editor_text = if new { "Untitled".into() } else { String::new() };
        self.files.focus_editor = true;
        self.files.message = None;
    }

    /// Request a close through the same unsaved-change guard as native window closing.
    pub fn request_close(&mut self, ctx: &Context) {
        if self.files.pending.is_some() {
            return;
        }
        self.files.save_as = false;
        self.files.editor = None;
        self.parts.cancel_placement();
        self.outlines.cancel();
        self.pads.cancel();
        self.copper.cancel();
        self.request_destination(Destination::Quit, ctx);
    }

    fn request_destination(&mut self, destination: Destination, ctx: &Context) {
        let Some(design) = &self.design else {
            self.files.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        let cursor = design.engine.session_state().cursor;
        self.files.pending = Some(Pending { destination, cursor });
        self.files.confirm = design.engine.document_status().dirty;
        self.files.message = None;
        if !self.files.confirm {
            self.finish_destination(false, ctx);
        }
    }

    fn cancel_files(&mut self) {
        self.files.save_as = false;
        self.files.editor = None;
        self.files.pending = None;
        self.files.confirm = false;
        self.files.message = None;
    }

    fn finish_destination(&mut self, discard: bool, ctx: &Context) {
        let (Some(design), Some(pending)) = (&self.design, &self.files.pending) else { return };
        let guard = ReplaceGuard { discard_dirty: discard, expected_cursor: Some(pending.cursor) };
        let result = match &pending.destination {
            Destination::New(name) => UiAction::NewProject
                .run(
                    &design.engine,
                    json!({
                        "name": name, "discard_dirty": discard, "expected_cursor": pending.cursor
                    }),
                )
                .map(drop),
            Destination::Open(path) => UiAction::OpenProject
                .run(
                    &design.engine,
                    json!({
                        "path": path, "discard_dirty": discard, "expected_cursor": pending.cursor
                    }),
                )
                .map(drop),
            Destination::Quit => design.engine.check_document_guard(&guard),
        };
        match result {
            Ok(()) => {
                if matches!(pending.destination, Destination::Quit) {
                    self.files.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                self.cancel_files();
            }
            Err(error) => {
                self.files.message = Some(error.to_string());
                if let Some(pending) = &mut self.files.pending {
                    pending.cursor = design.engine.session_state().cursor;
                }
                self.files.confirm = true;
            }
        }
    }

    fn resume_after_save(&mut self, ctx: &Context) {
        if self.files.pending.is_none() {
            return;
        }
        let Some(design) = &self.design else { return };
        let cursor = design.engine.session_state().cursor;
        if design.engine.document_status().dirty {
            self.files.message = Some("Newer edits are unsaved; choose how to proceed".into());
            self.files.confirm = true;
            if let Some(pending) = &mut self.files.pending {
                pending.cursor = cursor;
            }
        } else {
            if let Some(pending) = &mut self.files.pending {
                pending.cursor = cursor;
            }
            self.finish_destination(false, ctx);
        }
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
        if ui.input(|input| input.viewport().close_requested()) && !self.files.allow_close {
            // Cancel the native close first; an approved close sends a fresh Close command.
            ui.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close(ui.ctx());
        }
        if self.design.is_none()
            || self.files.busy()
            || self.copper.editing()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
        {
            return;
        }
        if ui.input_mut(|input| input.consume_shortcut(&NEW)) {
            self.begin_document(true);
        } else if ui.input_mut(|input| input.consume_shortcut(&OPEN)) {
            self.begin_document(false);
        } else if ui.input_mut(|input| input.consume_shortcut(&SAVE_AS)) {
            self.begin_save_as();
        } else if ui.input_mut(|input| input.consume_shortcut(&SAVE)) {
            self.save_design();
        }
    }

    pub(crate) fn file_dialogs(&mut self, ui: &mut Ui) {
        if self.files.save_as {
            self.save_dialog(ui);
        } else if self.files.editor.is_some() {
            self.document_dialog(ui);
        } else if self.files.confirm {
            self.confirm_dialog(ui);
        }
    }

    fn save_dialog(&mut self, ui: &mut Ui) {
        let mut save = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("save_project_dialog")).show(ui.ctx(), |ui| {
            ui.heading("Save project as");
            let label = ui.label("Project file");
            let response = ui.add(egui::TextEdit::singleline(&mut self.files.path).desired_width(420.0)).labelled_by(label.id);
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
        if cancel || modal.should_close() {
            self.cancel_files();
        } else if save && self.run_save(json!({ "path": self.files.path, "overwrite": self.files.overwrite })) {
            self.files.save_as = false;
            self.resume_after_save(ui.ctx());
        }
    }

    fn document_dialog(&mut self, ui: &mut Ui) {
        let new = self.files.editor.unwrap_or(true);
        let mut apply = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("document_dialog")).show(ui.ctx(), |ui| {
            ui.heading(if new { "New project" } else { "Open project" });
            let label = ui.label(if new { "Project name" } else { "Project file" });
            let response = ui.add(egui::TextEdit::singleline(&mut self.files.editor_text).desired_width(420.0)).labelled_by(label.id);
            if self.files.focus_editor {
                response.request_focus();
                self.files.focus_editor = false;
            }
            ui.horizontal(|ui| {
                apply = ui.button(if new { "Create project" } else { "Open file" }).clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || modal.should_close() {
            self.cancel_files();
        } else if apply {
            self.files.editor = None;
            let text = self.files.editor_text.clone();
            self.request_destination(if new { Destination::New(text) } else { Destination::Open(text) }, ui.ctx());
        }
    }

    fn confirm_dialog(&mut self, ui: &mut Ui) {
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("unsaved_changes")).show(ui.ctx(), |ui| {
            ui.heading("Save changes before continuing?");
            ui.label("The current project has unsaved changes or could not be replaced.");
            if let Some(message) = &self.files.message {
                ui.label(message);
            }
            ui.horizontal(|ui| {
                save = ui.button("Save changes").clicked();
                discard = ui.button("Discard and continue").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || modal.should_close() {
            self.cancel_files();
        } else if discard {
            self.finish_destination(true, ui.ctx());
        } else if save {
            self.files.confirm = false;
            let saved = self.save_design();
            if !self.files.save_as && saved {
                // Continue only after a successful save; newer edits still require a choice.
                self.resume_after_save(ui.ctx());
            } else if !self.files.save_as {
                self.files.confirm = true;
            }
        }
    }
}
