//! Local annotation review; all stored changes use the shared guarded command.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use viamind_engine::{AnnotationMode, AnnotationOptions, AnnotationOrder, AnnotationPreview};
pub struct AnnotationUi {
    open: bool,
    stream: Option<viamind_doc::ObjectId>,
    options: AnnotationOptions,
    start: String,
    preview: Option<AnnotationPreview>,
    message: Option<String>,
}
impl Default for AnnotationUi {
    fn default() -> Self {
        Self { open: false, stream: None, options: AnnotationOptions::default(), start: "1".into(), preview: None, message: None }
    }
}
impl ViamindApp {
    pub(crate) fn annotation_panel(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let cursor = engine.with_engine(|e| e.event_cursor());
        if self.annotation.stream != Some(cursor.stream_id) {
            self.annotation = AnnotationUi { stream: Some(cursor.stream_id), ..Default::default() };
        }
        if ui.button("Annotate symbols").clicked() {
            self.annotation.open = true;
        }
        if !self.annotation.open {
            return;
        }
        let available = ui.ctx().content_rect().shrink(8.0);
        let mut open = true;
        egui::Window::new("Annotate schematic references")
            .id(egui::Id::new("schematic-annotation-dialog"))
            .open(&mut open)
            .default_width(560.0)
            .max_width(available.width())
            .max_height(available.height())
            .constrain_to(available)
            .show(ui.ctx(), |ui| {
                ui.label("One reference per symbol or explicit package. Sheet order is UUID order. Pins and PCB stay unchanged.");
                let before = self.annotation.options.clone();
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut self.annotation.options.mode, AnnotationMode::KeepExisting, "Keep existing numbers");
                    ui.radio_value(&mut self.annotation.options.mode, AnnotationMode::ResetAll, "Renumber all symbols");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut self.annotation.options.order, AnnotationOrder::XThenY, "Sort symbols by X then Y");
                    ui.radio_value(&mut self.annotation.options.order, AnnotationOrder::YThenX, "Sort symbols by Y then X");
                });
                let changed = ui
                    .horizontal(|ui| {
                        let label = ui.label("Annotation starting number");
                        ui.add(egui::TextEdit::singleline(&mut self.annotation.start).id_source("annotation-start").desired_width(100.0))
                            .labelled_by(label.id)
                            .changed()
                    })
                    .inner;
                if changed || before != self.annotation.options {
                    self.annotation.preview = None;
                    self.annotation.message = None;
                }
                let stale = self.annotation.preview.as_ref().is_some_and(|p| p.source_cursor != cursor);
                let can_apply = self.annotation.preview.as_ref().is_some_and(|p| !p.changes.is_empty()) && !stale && !self.files.busy();
                let (mut prepare, mut apply, mut cancel) = (false, false, false);
                ui.horizontal_wrapped(|ui| {
                    prepare = ui.button("Prepare annotation").clicked();
                    apply = ui.add_enabled(can_apply, egui::Button::new("Apply annotation")).clicked();
                    cancel = ui.button("Cancel annotation preview").clicked();
                });
                if prepare {
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
                if apply {
                    let command = serde_json::to_value(&self.annotation.preview.as_ref().expect("enabled preview").command)
                        .expect("annotation input is serializable");
                    match UiAction::AnnotateSchematic.run(&engine, command) {
                        Ok(_) => {
                            self.annotation.preview = None;
                            self.annotation.message = Some("Annotation applied; Undo restores all affected sheets.".into());
                        }
                        Err(e) => self.annotation.message = Some(format!("{e}; prepare annotation again to reload.")),
                    }
                }
                if cancel {
                    self.annotation.preview = None;
                    self.annotation.message = None;
                }
                if let Some(message) = &self.annotation.message {
                    ui.label(message);
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
                    if preview.source_cursor != cursor {
                        ui.label("Schematic session changed; prepare annotation again to reload.");
                    }
                    egui::CollapsingHeader::new("Proposed annotation changes").default_open(true).show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("annotation-changes").max_height(ui.available_height().clamp(20.0, 120.0)).show_rows(
                            ui,
                            18.0,
                            preview.changes.len(),
                            |ui, rows| {
                                for change in &preview.changes[rows] {
                                    let owner = change
                                        .package_id
                                        .zip(change.unit_key.as_deref())
                                        .map_or_else(|| " - single".into(), |(id, key)| format!(" - unit {key} - package {id}"));
                                    let row = format!(
                                        "{} to {}{owner} - symbol {} - sheet {}",
                                        change.before, change.after, change.symbol_id, change.sheet_id
                                    );
                                    ui.add(egui::Label::new(egui::RichText::new(&row).monospace()).truncate()).on_hover_text(row);
                                }
                            },
                        );
                    });
                }
            });
        if !open {
            self.annotation.open = false;
            self.annotation.preview = None;
        }
    }
}
