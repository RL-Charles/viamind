//! One bounded background PCB import job and explicit guarded lifecycle replacement.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};
use viamind_doc::ObjectId;
use viamind_engine::{EngineHandle, EventCursor, PcbImportPreview};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    project: ObjectId,
    cursor: EventCursor,
    path: String,
    generation: u64,
}
struct Worker {
    receiver: Receiver<Result<Value, String>>,
    context: Context,
    apply: bool,
}
#[derive(Default)]
pub struct PcbImportUi {
    open: bool,
    path: String,
    generation: u64,
    worker: Option<Worker>,
    preview: Option<PcbImportPreview>,
    completed_report: Option<viamind_doc::pcb_sexpr::FidelityReport>,
    message: Option<String>,
}
impl PcbImportUi {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    pub fn preview(&self) -> Option<&PcbImportPreview> {
        self.preview.as_ref()
    }
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.completed_report = None;
        self.message = None;
    }
    fn fresh(&self, project: ObjectId, cursor: EventCursor) -> bool {
        self.preview.as_ref().is_some_and(|p| p.expected_project == project && p.expected_cursor == cursor)
    }
    fn poll(&mut self, project: ObjectId, cursor: EventCursor, ctx: &egui::Context) {
        if self.preview.as_ref().is_some_and(|p| p.expected_project != project || p.expected_cursor != cursor)
            && self.worker.as_ref().is_none_or(|worker| !worker.apply)
        {
            self.preview = None;
            self.message = Some("Document changed; preview the source again.".into());
        }
        let Some(worker) = &self.worker else { return };
        let value = match worker.receiver.try_recv() {
            Ok(v) => v,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Import worker disconnected; inspect the current document before retrying.".into()),
        };
        let worker = self.worker.take().unwrap();
        if worker.context.generation != self.generation || worker.context.path != self.path || !self.open {
            return;
        }
        if !worker.apply && (worker.context.project != project || worker.context.cursor != cursor) {
            self.message = Some("Document changed while previewing; preview again.".into());
            return;
        }
        match value {
            Ok(_) if worker.apply => {
                self.completed_report = self.preview.take().map(|p| p.report);
                self.message = Some("Import completed for the guarded source snapshot as an unsaved native document. Inspect the current document and Save As a native file; the completed conversion report remains below.".into());
            }
            Ok(value) => match serde_json::from_value::<PcbImportPreview>(value) {
                Ok(preview) => {
                    if preview.expected_project == project && preview.expected_cursor == cursor {
                        self.preview = Some(preview);
                        self.message = None;
                    } else {
                        self.message = Some("Stale preview discarded; preview again.".into());
                    }
                }
                Err(error) => self.message = Some(error.to_string()),
            },
            Err(error) => {
                self.preview = None;
                self.message = Some(format!("Import failed: {error}"));
            }
        }
    }
    fn start(&mut self, engine: EngineHandle, project: ObjectId, cursor: EventCursor, apply: bool, ctx: egui::Context) {
        self.start_with(engine, project, cursor, apply, ctx, |task| {
            std::thread::Builder::new().name("viamind-pcb-import".into()).spawn(task).map(|_| ())
        });
    }
    fn start_with(
        &mut self,
        engine: EngineHandle,
        project: ObjectId,
        cursor: EventCursor,
        apply: bool,
        ctx: egui::Context,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        if self.busy() {
            return;
        }
        if self.path.is_empty() {
            self.message = Some("Enter a PCB source file path.".into());
            return;
        }
        let (action, input) = if apply {
            let Some(p) = self.preview.as_ref().filter(|p| p.can_import && self.fresh(project, cursor)) else {
                self.message = Some("A current supported preview is required.".into());
                return;
            };
            (
                UiAction::ImportPcb,
                json!({"path":p.path,"expected_project":p.expected_project,"expected_cursor":p.expected_cursor,"expected_source_sha256":p.source_sha256}),
            )
        } else {
            (UiAction::PreviewPcbImport, json!({"path":self.path}))
        };
        let context = Context { project, cursor, path: self.path.clone(), generation: self.generation };
        let (tx, rx) = mpsc::channel();
        match spawn(Box::new(move || {
            let value = action.run(&engine, input).map_err(|e| e.to_string());
            let _ = tx.send(value);
            ctx.request_repaint();
        })) {
            Ok(()) => {
                self.worker = Some(Worker { receiver: rx, context, apply });
                self.message = None;
            }
            Err(error) => self.message = Some(format!("Could not start import worker: {error}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_pcb_import(&mut self) {
        if self.design.is_some() {
            self.pcb_import.open = true;
            self.pcb_import.changed();
        }
    }
    pub(crate) fn pcb_import_window(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let (project, cursor) = engine.with_engine(|e| (e.project().id, e.event_cursor()));
        self.pcb_import.poll(project, cursor, ui.ctx());
        if !self.pcb_import.open {
            return;
        }
        let active_edit = self.selection.placement.moving()
            || self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
            || self.schematic_wires.active()
            || self.schematic_object_draft_active();
        let dirty = engine.document_status().dirty;
        let busy = self.pcb_import.busy();
        let fresh = self.pcb_import.fresh(project, cursor);
        let mut open = true;
        let mut preview_clicked = false;
        let mut apply_clicked = false;
        let mut changed = false;
        egui::Window::new("Import PCB S-expression").open(&mut open).default_width(760.0).max_width((ui.ctx().content_rect().width() - 24.0).max(240.0)).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Standalone .kicad_pcb supported geometry. Full project and fabrication fidelity are not provided.");
            ui.label("Front zero-angle footprints with cardinal rectangular SMD pads (-360..360 degrees), outer straight tracks, through vias and polygon board outline/cutouts.");
            let label = ui.label("PCB source file");
            changed = ui.add(egui::TextEdit::singleline(&mut self.pcb_import.path).desired_width(ui.available_width()).char_limit(4096))
                .labelled_by(label.id).changed();
            preview_clicked = ui.add_enabled(!busy, egui::Button::new("Preview PCB import")).clicked();
            if busy {
                ui.horizontal(|ui| { ui.spinner(); ui.label("Reading and validating bounded source…"); });
            }
            let report = self.pcb_import.preview.as_ref().map(|p| &p.report).or(self.pcb_import.completed_report.as_ref());
            if let Some(report) = report {
                if self.pcb_import.preview.is_none() {
                    ui.label("Completed conversion report for the imported source snapshot; later native edits are not included.");
                }
                show_report(ui, report);
                if let Some(p) = &self.pcb_import.preview { ui.small(format!("Source SHA256 {}", p.source_sha256)); }
            }
            if dirty { ui.label("Save current native changes before importing. Unsaved work is protected."); }
            if active_edit { ui.label("Finish or cancel active editing drafts before importing; drafts are preserved."); }
            let can_apply = !busy && !dirty && !active_edit && fresh && self.pcb_import.preview.as_ref().is_some_and(|p| p.can_import);
            apply_clicked = ui.add_enabled(can_apply, egui::Button::new("Apply PCB import as unsaved native document")).clicked();
            ui.label("Apply replaces this clean session, clears its history/selection and starts a new unsaved native document. Source files are never written.");
            if let Some(message) = &self.pcb_import.message { ui.label(message); }
        });
        if changed {
            self.pcb_import.changed();
        }
        if !open {
            self.pcb_import.open = false;
            self.pcb_import.changed();
        }
        if preview_clicked && open {
            self.pcb_import.start(engine.clone(), project, cursor, false, ui.ctx().clone());
        }
        if apply_clicked && open && !changed && !active_edit && !dirty {
            self.pcb_import.start(engine, project, cursor, true, ui.ctx().clone());
        }
    }
}
fn show_report(ui: &mut Ui, report: &viamind_doc::pcb_sexpr::FidelityReport) {
    let counts = &report.imported;
    ui.label(format!(
        "Exact supported geometry: {} parts / {} pads / {} tracks / {} vias / {} outline vertices",
        counts.parts, counts.pads, counts.tracks, counts.vias, counts.outline_vertices
    ));
    ui.label(format!("Fidelity: {} omitted lists · {} blocked lists · full fidelity: false", report.omitted_lists, report.blocked_lists));
    ui.label(if report.can_import() {
        "Supported geometry can import; listed omissions remain."
    } else {
        "Unsupported or invalid geometry blocks Apply. Repair or export the supported subset."
    });
    egui::CollapsingHeader::new("Per-object fidelity report").default_open(true).show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(300.0).show_rows(ui, 88.0, report.items.len(), |ui, rows| {
            for index in rows {
                let item = &report.items[index];
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(format!("{:?} · {} · {} @byte{}", item.disposition, item.path, item.token, item.byte_offset)).truncate());
                    if let Some(id) = item.source_id {
                        ui.small(format!("Source UUID {id}"));
                    }
                    ui.add(egui::Label::new(&item.reason).truncate()).on_hover_text(&item.reason);
                });
            }
        });
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context {
        Context { project: ObjectId::new(), cursor: EventCursor { stream_id: ObjectId::new(), sequence: 0 }, path: "source".into(), generation: 0 }
    }
    #[test]
    fn stale_context_close_and_path_changes_retain_one_receiver_then_discard_completion() {
        for mutation in 0..3 {
            let source = context();
            let (tx, rx) = mpsc::channel();
            let mut form = PcbImportUi {
                open: true,
                path: source.path.clone(),
                worker: Some(Worker { receiver: rx, context: source.clone(), apply: false }),
                ..Default::default()
            };
            match mutation {
                0 => {
                    form.path = "other".into();
                    form.changed();
                }
                1 => {
                    form.open = false;
                    form.changed();
                }
                _ => {
                    form.changed();
                }
            }
            assert!(form.busy());
            tx.send(Ok(json!({}))).unwrap();
            form.poll(source.project, source.cursor, &egui::Context::default());
            assert!(!form.busy());
            assert!(form.preview.is_none());
            assert!(form.message.is_none());
        }
    }
    #[test]
    fn old_stream_completion_and_disconnected_worker_are_actionable_without_new_preview() {
        let source = context();
        let (tx, rx) = mpsc::channel();
        let mut form = PcbImportUi {
            open: true,
            path: source.path.clone(),
            worker: Some(Worker { receiver: rx, context: source.clone(), apply: false }),
            ..Default::default()
        };
        tx.send(Ok(json!({}))).unwrap();
        let mut next = source.cursor;
        next.stream_id = ObjectId::new();
        form.poll(source.project, next, &egui::Context::default());
        assert!(form.preview.is_none());
        assert!(form.message.as_ref().unwrap().contains("Document changed"));
        let (tx, rx) = mpsc::channel::<Result<Value, String>>();
        drop(tx);
        form.worker = Some(Worker { receiver: rx, context: source.clone(), apply: false });
        form.poll(source.project, source.cursor, &egui::Context::default());
        assert!(form.message.as_ref().unwrap().contains("disconnected"));
    }
    #[test]
    fn failed_spawn_is_actionable_and_duplicate_start_keeps_owned_receiver() {
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let before = engine.session_state();
        let project = before.project.id;
        let cursor = before.cursor;
        let mut form = PcbImportUi { open: true, path: "source".into(), ..Default::default() };
        form.start_with(engine.clone(), project, cursor, false, egui::Context::default(), |_| Err(std::io::Error::other("controlled spawn failure")));
        assert!(!form.busy());
        assert!(form.message.as_ref().unwrap().contains("controlled spawn failure"));
        assert_eq!(engine.session_state(), before);
        let (_sender, receiver) = mpsc::channel();
        form.worker = Some(Worker { receiver, context: Context { project, cursor, path: "source".into(), generation: 0 }, apply: false });
        form.start_with(engine.clone(), project, cursor, false, egui::Context::default(), |_| panic!("must retain existing worker"));
        assert!(form.busy());
        assert_eq!(engine.session_state(), before);
    }
}
