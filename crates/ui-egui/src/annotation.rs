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
            ui.label("One reference per symbol or explicit package. Sheet order follows the sheet list (UUID order). Pins and PCB stay unchanged.");
            let before = self.annotation.options.clone();
            ui.radio_value(&mut self.annotation.options.mode, AnnotationMode::KeepExisting, "Keep existing numbers");
            ui.radio_value(&mut self.annotation.options.mode, AnnotationMode::ResetAll, "Renumber all symbols");
            ui.radio_value(&mut self.annotation.options.order, AnnotationOrder::XThenY, "Sort symbols by X then Y");
            ui.radio_value(&mut self.annotation.options.order, AnnotationOrder::YThenX, "Sort symbols by Y then X");
            let label = ui.label("Annotation starting number");
            let changed =
                ui.add(egui::TextEdit::singleline(&mut self.annotation.start).id_source("annotation-start")).labelled_by(label.id).changed();
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
                    _ => self.annotation.message = Some("Starting number must be 1..999999999".into()),
                }
            }
            if let Some(preview) = &self.annotation.preview {
                if preview.packages == 0 {
                    ui.label(format!("{} symbols, {} proposed reference changes", preview.symbols, preview.changes.len()));
                } else {
                    ui.label(format!(
                        "{} symbols/units, {} packages, {} proposed member reference changes",
                        preview.symbols,
                        preview.packages,
                        preview.changes.len()
                    ));
                }
                ui.collapsing("Proposed annotation changes", |ui| {
                    egui::ScrollArea::vertical().id_salt("annotation-changes").max_height(120.0).show_rows(
                        ui,
                        18.0,
                        preview.changes.len(),
                        |ui, rows| {
                            for change in &preview.changes[rows] {
                                let owner = change
                                    .package_id
                                    .zip(change.unit_key.as_deref())
                                    .map_or_else(String::new, |(id, key)| format!(" - package {id} unit {key}"));
                                ui.monospace(format!(
                                    "{} to {} - sheet {} - symbol {}{owner}",
                                    change.before, change.after, change.sheet_id, change.symbol_id
                                ));
                            }
                        },
                    );
                });
                let stale = engine.with_engine(|e| e.event_cursor()) != preview.source_cursor;
                if stale {
                    ui.label("Schematic session changed; prepare annotation again to reload.");
                }
                if ui.add_enabled(!preview.changes.is_empty() && !stale && !self.files.busy(), egui::Button::new("Apply annotation")).clicked() {
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
            if self.annotation.preview.is_some() && ui.button("Cancel annotation preview").clicked() {
                self.annotation.preview = None;
                self.annotation.message = None;
            }
            if let Some(message) = &self.annotation.message {
                ui.label(message);
            }
        });
    }
}
