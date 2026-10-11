//! Local annotation preview; model changes use the same command as MCP agents.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use viamind_engine::{AnnotationMode, AnnotationOptions, AnnotationOrder, AnnotationPreview};
pub struct AnnotationUi {
    stream: Option<viamind_doc::ObjectId>,
    options: AnnotationOptions,
    start: String,
    preview: Option<AnnotationPreview>,
    message: Option<String>,
}
impl Default for AnnotationUi {
    fn default() -> Self {
        Self { stream: None, options: AnnotationOptions::default(), start: "1".into(), preview: None, message: None }
    }
}
impl ViamindApp {
    pub(crate) fn annotation_panel(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else {
            return;
        };
        let engine = design.engine.clone();
        let stream = engine.with_engine(|e| e.event_cursor().stream_id);
        if self.annotation.stream != Some(stream) {
            self.annotation = AnnotationUi { stream: Some(stream), ..Default::default() };
        }
        ui.collapsing("Annotate symbols", |ui| {
            ui.label("Flat schematic references. Sheet order follows the sheet list (UUID order). Pins and PCB stay unchanged.");
            let before = self.annotation.options.clone();
            ui.radio_value(&mut self.annotation.options.mode, AnnotationMode::KeepExisting, "Keep existing numbers");
            ui.radio_value(&mut self.annotation.options.mode, AnnotationMode::ResetAll, "Renumber all symbols");
            ui.radio_value(&mut self.annotation.options.order, AnnotationOrder::XThenY, "Sort symbols by X then Y");
            ui.radio_value(&mut self.annotation.options.order, AnnotationOrder::YThenX, "Sort symbols by Y then X");
            let label = ui.label("Annotation starting number");
            let changed = ui.text_edit_singleline(&mut self.annotation.start).labelled_by(label.id).changed();
            if changed || before != self.annotation.options {
                self.annotation.preview = None;
                self.annotation.message = None;
            }
            if ui.button("Prepare annotation").clicked() {
                self.annotation.preview = None;
                self.annotation.message = None;
                match self.annotation.start.trim().parse::<u32>() {
                    Ok(start) if (1..=999_999_999).contains(&start) => {
                        self.annotation.options.start = start;
                        match engine.preview_schematic_annotation(self.annotation.options.clone()) {
                            Ok(preview) => self.annotation.preview = Some(preview),
                            Err(e) => self.annotation.message = Some(e.to_string()),
                        }
                    }
                    _ => self.annotation.message = Some("Starting number must be1..999999999".into()),
                }
            }
            if let Some(preview) = &self.annotation.preview {
                ui.label(format!("{} symbols, {} proposed reference changes", preview.symbols, preview.changes.len()));
                ui.collapsing("Proposed annotation changes", |ui| {
                    for change in &preview.changes {
                        ui.monospace(format!("{} → {} · sheet {} · symbol {}", change.before, change.after, change.sheet_id, change.symbol_id));
                    }
                });
                if ui.add_enabled(!preview.changes.is_empty() && !self.files.busy(), egui::Button::new("Apply annotation")).clicked() {
                    let command = serde_json::to_value(&preview.command).expect("annotation input is serializable");
                    match UiAction::AnnotateSchematic.run(&engine, command) {
                        Ok(_) => {
                            self.annotation.preview = None;
                            self.annotation.message = Some("Annotation applied; Undo restores all affected sheets.".into());
                        }
                        Err(e) => self.annotation.message = Some(format!("{e}; prepare annotation again to reload.")),
                    }
                }
            }
            if let Some(message) = &self.annotation.message {
                ui.label(message);
            }
        });
    }
}
