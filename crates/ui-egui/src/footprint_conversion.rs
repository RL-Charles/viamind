//! Detached conversion with one retained worker; never applies geometry to a part.
use crate::{ViamindApp, actions::UiAction};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};
use viamind_doc::ObjectId;
use viamind_engine::{ConvertedFootprintFile, EngineHandle, EventCursor, FootprintConversionPreview};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    project: ObjectId,
    cursor: EventCursor,
    source: String,
    destination: String,
    generation: u64,
}
struct Worker {
    receiver: Receiver<Result<Value, String>>,
    context: Context,
    convert: bool,
}
#[derive(Default)]
pub struct FootprintConversionUi {
    open: bool,
    source: String,
    destination: String,
    generation: u64,
    worker: Option<Worker>,
    preview: Option<FootprintConversionPreview>,
    preview_context: Option<Context>,
    completed: Option<ConvertedFootprintFile>,
    message: Option<String>,
}
impl FootprintConversionUi {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    pub fn preview(&self) -> Option<&FootprintConversionPreview> {
        self.preview.as_ref()
    }
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.preview_context = None;
        self.completed = None;
        self.message = None;
    }
    fn poll(&mut self, project: ObjectId, cursor: EventCursor, ctx: &egui::Context) {
        if self.preview_context.as_ref().is_some_and(|c| c.project != project || c.cursor != cursor) {
            self.preview = None;
            self.preview_context = None;
            self.message = Some("Document changed; preview the detached source again.".into());
        }
        let Some(worker) = &self.worker else { return };
        let result = match worker.receiver.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Conversion worker disconnected; inspect the output path before retrying.".into()),
        };
        let worker = self.worker.take().unwrap();
        // Publication may already have completed for the explicitly requested destination.
        // Closing/changing the form discards the old display, never starts a second worker.
        if !self.open
            || worker.context.generation != self.generation
            || worker.context.source != self.source
            || worker.context.destination != self.destination
        {
            return;
        }
        if worker.context.project != project || worker.context.cursor != cursor {
            self.preview = None;
            self.message = Some(if worker.convert {
                "Document changed during detached conversion. Inspect the requested output path; the conversion does not change the project. Preview again.".into()
            } else {
                "Document changed while previewing; preview again.".into()
            });
            return;
        }
        match result {
            Ok(value) if worker.convert => match serde_json::from_value::<ConvertedFootprintFile>(value) {
                Ok(completed) => {
                    self.preview = None;
                    self.message = Some(format!(
                        "Created {}. Project, history and selection are unchanged. Reuse this native library separately in the pad editor.",
                        completed.path.display()
                    ));
                    self.completed = Some(completed);
                }
                Err(error) => self.message = Some(error.to_string()),
            },
            Ok(value) => match serde_json::from_value(value) {
                Ok(preview) => {
                    self.preview = Some(preview);
                    self.preview_context = Some(worker.context);
                    self.message = None;
                }
                Err(error) => self.message = Some(error.to_string()),
            },
            Err(error) => {
                self.preview = None;
                self.message = Some(format!("Conversion failed: {error}"));
            }
        }
    }
    fn start(&mut self, engine: EngineHandle, project: ObjectId, cursor: EventCursor, convert: bool, ctx: egui::Context) {
        self.start_with(engine, project, cursor, convert, ctx, |task| {
            std::thread::Builder::new().name("viamind-footprint-convert".into()).spawn(task).map(|_| ())
        });
    }
    fn start_with(
        &mut self,
        engine: EngineHandle,
        project: ObjectId,
        cursor: EventCursor,
        convert: bool,
        ctx: egui::Context,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        if self.busy() {
            return;
        }
        if self.source.is_empty() {
            self.message = Some("Enter a standalone footprint source file path.".into());
            return;
        }
        let (action, input) = if convert {
            let Some(preview) = self.preview.as_ref().filter(|p| p.can_convert) else {
                self.message = Some("A supported source preview is required.".into());
                return;
            };
            if self.destination.is_empty() {
                self.message = Some("Enter a new native footprint output path.".into());
                return;
            }
            (
                UiAction::ConvertFootprintFile,
                json!({"path":preview.path,"expected_source_sha256":preview.source_sha256,"destination":self.destination}),
            )
        } else {
            (UiAction::PreviewFootprintConversion, json!({"path":self.source}))
        };
        let context = Context { project, cursor, source: self.source.clone(), destination: self.destination.clone(), generation: self.generation };
        let (tx, receiver) = mpsc::channel();
        match spawn(Box::new(move || {
            let value = action.run(&engine, input).map_err(|e| e.to_string());
            let _ = tx.send(value);
            ctx.request_repaint();
        })) {
            Ok(()) => {
                self.worker = Some(Worker { receiver, context, convert });
                self.message = None;
            }
            Err(error) => self.message = Some(format!("Could not start conversion worker: {error}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_footprint_conversion(&mut self) {
        if self.design.is_some() {
            self.footprint_conversion.open = true;
            self.footprint_conversion.changed();
        }
    }
    pub(crate) fn footprint_conversion_window(&mut self, ui: &mut egui::Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let (project, cursor) = engine.with_engine(|e| (e.project().id, e.event_cursor()));
        self.footprint_conversion.poll(project, cursor, ui.ctx());
        if !self.footprint_conversion.open {
            return;
        }
        let busy = self.footprint_conversion.busy();
        let mut open = true;
        let mut preview_clicked = false;
        let mut convert_clicked = false;
        let mut changed = false;
        egui::Window::new("Convert footprint S-expression").open(&mut open).default_width(760.0)
            .max_width((ui.ctx().content_rect().width() - 24.0).max(240.0)).vscroll(true).show(ui.ctx(), |ui| {
                ui.label("Standalone .kicad_mod → detached native footprint library. Front zero-angle rectangular SMD pads only.");
                ui.label("Source UUIDs, cosmetic/model data and mask/paste membership are omitted. Meaningful source nets and unsupported geometry block conversion. Full fidelity: false.");
                let label = ui.label("Footprint source file");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.footprint_conversion.source).desired_width(ui.available_width()).char_limit(4096)).labelled_by(label.id).changed();
                let label = ui.label("New native footprint file");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.footprint_conversion.destination).desired_width(ui.available_width()).char_limit(4096)).labelled_by(label.id).changed();
                preview_clicked = ui.add_enabled(!busy, egui::Button::new("Preview footprint conversion")).clicked();
                if busy { ui.horizontal(|ui| { ui.spinner(); ui.label("Reading and validating bounded source…"); }); }
                let report = self.footprint_conversion.preview.as_ref().map(|p| &p.report).or(self.footprint_conversion.completed.as_ref().map(|c| &c.report));
                if let Some(report) = report {
                    if self.footprint_conversion.completed.is_some() { ui.label("Completed source snapshot report; later file changes are not included."); }
                    ui.label(format!("{} rectangular pads · {} omitted lists · {} blocked lists · full fidelity: false", report.imported.pads, report.omitted_lists, report.blocked_lists));
                    ui.label(if report.can_import() { "Supported detached geometry can convert; listed omissions remain." } else { "Unsupported source blocks conversion. Inspect the per-list reasons." });
                    egui::CollapsingHeader::new("Footprint per-list fidelity report").default_open(false).show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(240.0).show_rows(ui, 88.0, report.items.len(), |ui, rows| {
                            for index in rows { let item = &report.items[index]; ui.vertical(|ui| {
                                ui.add(egui::Label::new(format!("{:?} · {} · {} @byte{}", item.disposition, item.path, item.token, item.byte_offset)).truncate());
                                if let Some(id) = item.source_id { ui.small(format!("Source UUID {id} (report only)")); }
                                ui.add(egui::Label::new(&item.reason).truncate()).on_hover_text(&item.reason);
                            }); }
                        });
                    });
                    if let Some(preview) = &self.footprint_conversion.preview { ui.add(egui::Label::new(format!("Source SHA256 {}", preview.source_sha256)).truncate()); }
                }
                convert_clicked = ui.add_enabled(!busy && self.footprint_conversion.preview.as_ref().is_some_and(|p| p.can_convert) && !self.footprint_conversion.destination.is_empty(), egui::Button::new("Create new native footprint file")).clicked();
                ui.label("Conversion preserves dirty work and editing drafts. Existing files are never replaced. It does not apply pads to the project.");
                if let Some(message) = &self.footprint_conversion.message { ui.label(message); }
            });
        if changed {
            self.footprint_conversion.changed();
        }
        if !open {
            self.footprint_conversion.open = false;
            self.footprint_conversion.changed();
        }
        if preview_clicked && open {
            self.footprint_conversion.start(engine.clone(), project, cursor, false, ui.ctx().clone());
        }
        if convert_clicked && open && !changed {
            self.footprint_conversion.start(engine, project, cursor, true, ui.ctx().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context {
        Context {
            project: ObjectId::new(),
            cursor: EventCursor { stream_id: ObjectId::new(), sequence: 0 },
            source: "source".into(),
            destination: "new-output".into(),
            generation: 0,
        }
    }
    #[test]
    fn close_path_change_and_reopen_retain_one_worker_and_discard_completion() {
        for mutation in 0..4 {
            let source = context();
            let (tx, receiver) = mpsc::channel();
            let mut form = FootprintConversionUi {
                open: true,
                source: source.source.clone(),
                destination: source.destination.clone(),
                worker: Some(Worker { receiver, context: source.clone(), convert: true }),
                ..Default::default()
            };
            match mutation {
                0 => form.source = "other".into(),
                1 => form.destination = "other".into(),
                2 => form.open = false,
                _ => form.generation += 1,
            }
            assert!(form.busy());
            tx.send(Ok(json!({}))).unwrap();
            form.poll(source.project, source.cursor, &egui::Context::default());
            assert!(!form.busy());
            assert!(form.preview.is_none() && form.completed.is_none() && form.message.is_none());
        }
    }
    #[test]
    fn old_stream_completion_disconnection_and_failed_spawn_are_actionable() {
        let source = context();
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let before = engine.session_state();
        let mut form =
            FootprintConversionUi { open: true, source: source.source.clone(), destination: source.destination.clone(), ..Default::default() };
        form.start_with(engine.clone(), source.project, source.cursor, false, egui::Context::default(), |_| {
            Err(std::io::Error::other("controlled failure"))
        });
        assert!(!form.busy());
        assert!(form.message.as_ref().unwrap().contains("controlled failure"));
        let (tx, receiver) = mpsc::channel();
        form.worker = Some(Worker { receiver, context: source.clone(), convert: false });
        form.start_with(engine.clone(), source.project, source.cursor, false, egui::Context::default(), |_| panic!("second worker forbidden"));
        tx.send(Ok(json!({}))).unwrap();
        let mut cursor = source.cursor;
        cursor.stream_id = ObjectId::new();
        form.poll(source.project, cursor, &egui::Context::default());
        assert!(form.message.as_ref().unwrap().contains("Document changed"));
        let (tx, receiver) = mpsc::channel::<Result<Value, String>>();
        drop(tx);
        form.worker = Some(Worker { receiver, context: source.clone(), convert: false });
        form.poll(source.project, source.cursor, &egui::Context::default());
        assert!(form.message.as_ref().unwrap().contains("disconnected"));
        assert_eq!(engine.session_state(), before);
    }
}
