//! Retained background fabrication exports, shared bounded request/snapshot behavior with MCP.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};
use viamind_engine::{EngineError, EngineHandle, EventCursor};
use viamind_fab::FabricationExportRequest;

#[derive(Default)]
pub struct FabricationUi {
    open: bool,
    path: String,
    mask: String,
    worker: Option<Receiver<Result<Value, String>>>,
    message: Option<String>,
    result: Option<Value>,
}
impl FabricationUi {
    /// Includes a job continuing while its window is closed.
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn request(&self, cursor: EventCursor) -> Result<Value, String> {
        if self.path.trim().is_empty() {
            return Err("Enter a NEW fabrication directory in an existing parent directory.".into());
        }
        let mask = self
            .mask
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=10.0).contains(v))
            .ok_or("Mask expansion needs a finite number from 0 to 10 mm")?;
        serde_json::to_value(FabricationExportRequest {
            path: std::path::PathBuf::from(self.path.trim()),
            mask_expansion_mm: mask,
            expected_cursor: Some(cursor),
        })
        .map_err(|e| e.to_string())
    }
    fn start(&mut self, engine: EngineHandle, ctx: egui::Context) {
        self.start_with(engine, ctx, |job| std::thread::Builder::new().name("viamind-fabrication-export".into()).spawn(job).map(|_| ()));
    }
    fn start_with(&mut self, engine: EngineHandle, ctx: egui::Context, spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>) {
        if self.busy() {
            return;
        }
        let input = match self.request(engine.event_cursor()) {
            Ok(input) => input,
            Err(error) => {
                self.message = Some(error);
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        let job = Box::new(move || {
            let result = UiAction::ExportFabrication.run(&engine, input).map_err(|e| e.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        match spawn(job) {
            Ok(()) => {
                self.worker = Some(receiver);
                self.message = None;
                self.result = None;
            }
            Err(error) => {
                self.message = Some(format!("Could not start export worker: {error}"));
            }
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.worker else { return };
        match receiver.try_recv() {
            Ok(result) => {
                self.worker = None;
                match result {
                    Ok(value) => {
                        self.message = None;
                        self.result = Some(value);
                    }
                    Err(error) => {
                        self.result = None;
                        self.message = Some(format!("Export failed: {error}"));
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.result = None;
                self.message = Some("Export worker disconnected. Check the destination before retrying.".into());
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
        }
    }
}

pub(crate) fn export_fabrication(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |message: String| EngineError::InvalidInput { command: "export_fabrication".into(), message };
    let input: FabricationExportRequest<EventCursor> = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
    let (snapshot, cursor, native) = engine.with_engine(|e| {
        let cursor = e.event_cursor();
        if input.expected_cursor.is_some_and(|c| c != cursor) {
            return Err(invalid("document changed; retry export from a fresh snapshot".into()));
        }
        input.validate().map_err(|e| invalid(e.to_string()))?;
        let snapshot = viamind_fab::FabricationSnapshot::capture(e.project()).map_err(|e| invalid(e.to_string()))?;
        Ok((snapshot, cursor, e.document_status().path))
    })?;
    let (path, manifest) = snapshot
        .write(viamind_fab::FabricationOptions { mask_expansion_mm: input.mask_expansion_mm }, &input.path, native.as_deref())
        .map_err(|e| invalid(e.to_string()))?;
    Ok(json!({"path":path,"manifest":manifest,"exported_cursor":cursor}))
}
impl ViamindApp {
    pub fn begin_fabrication_export(&mut self) {
        if self.design.is_some() {
            self.fabrication.open = true;
            if self.fabrication.mask.is_empty() {
                self.fabrication.mask = "0".into();
            }
        }
    }
    pub(crate) fn fabrication_window(&mut self, ui: &mut Ui) {
        self.fabrication.poll(ui.ctx());
        if !self.fabrication.open {
            return;
        }
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let cursor = engine.event_cursor();
        let busy = self.fabrication.busy();
        let mut export = false;
        egui::Window::new("Fabrication export").open(&mut self.fabrication.open).default_width(580.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Two outer copper layers, explicit SMD pads, tracks, through vias, mask and polygon profile.");
            ui.label("A NEW directory is required. Existing destinations and native projects are preserved.");
            ui.label("No legacy previews, inner copper, pours, paste, silkscreen or fabrication certification.");
            ui.add_enabled_ui(!busy, |ui| {
                let label = ui.label("New fabrication directory");
                ui.add(egui::TextEdit::singleline(&mut self.fabrication.path).desired_width(500.0).char_limit(4096)).labelled_by(label.id);
                let label = ui.label("Mask expansion (mm)");
                ui.add(egui::TextEdit::singleline(&mut self.fabrication.mask).desired_width(100.0).char_limit(32)).labelled_by(label.id);
                ui.label("Default 0 mm: mask opening equals pad; all vias untented. Export X right/Y up; common top view.");
                export = ui.button("Export fabrication snapshot").clicked();
            });
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Exporting snapshot…");
                });
            }
            if let Some(message) = &self.fabrication.message {
                ui.label(message);
            }
            if let Some(result) = &self.fabrication.result {
                ui.label(format!(
                    "Exported {} files to {}",
                    result["manifest"]["files"].as_array().map_or(0, Vec::len),
                    result["path"].as_str().unwrap_or("")
                ));
                ui.label(format!(
                    "Snapshot {} / {}",
                    result["exported_cursor"]["stream_id"].as_str().unwrap_or(""),
                    result["exported_cursor"]["sequence"]
                ));
                if result["exported_cursor"] != json!(cursor) {
                    ui.label("The design has changed since this exported snapshot.");
                }
            }
        });
        if export {
            self.fabrication.start(engine, ui.ctx().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_form_controls_are_disabled_and_completion_reenables_them() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let engine = EngineHandle::new(viamind_doc::Project::new("pending controls"));
        let mut app = ViamindApp::with_design(engine, None);
        app.begin_fabrication_export();
        let (tx, rx) = mpsc::channel();
        app.fabrication.worker = Some(rx);
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
        h.run_steps(3);
        for label in ["New fabrication directory", "Mask expansion (mm)", "Export fabrication snapshot"] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        }
        tx.send(Err("controlled completion".into())).unwrap();
        h.run_steps(3);
        assert!(!h.state().fabrication.busy());
        for label in ["New fabrication directory", "Mask expansion (mm)", "Export fabrication snapshot"] {
            assert!(!h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        }
    }
    #[test]
    fn retained_hidden_worker_suppresses_duplicate_start_and_recovers_from_disconnect() {
        let (tx, rx) = mpsc::channel();
        let mut state = FabricationUi { worker: Some(rx), ..Default::default() };
        let ctx = egui::Context::default();
        state.start_with(EngineHandle::new(viamind_doc::Project::new("unused")), ctx.clone(), |_| panic!("duplicate spawn"));
        state.poll(&ctx);
        assert!(state.busy());
        assert!(!state.open);
        tx.send(Ok(json!({"path":"result"}))).unwrap();
        state.poll(&ctx);
        assert!(!state.busy());
        assert_eq!(state.result.as_ref().unwrap()["path"], "result");
        let (tx, rx) = mpsc::channel();
        state.worker = Some(rx);
        drop(tx);
        state.poll(&ctx);
        assert!(!state.busy());
        assert!(state.message.as_ref().unwrap().contains("disconnected"));
    }
    #[test]
    fn fallible_spawn_error_preserves_inputs_and_allows_retry() {
        let mut state = FabricationUi { path: "new-fab".into(), mask: "0.05".into(), ..Default::default() };
        let engine = EngineHandle::new(viamind_doc::Project::new("spawn failure"));
        let before = engine.session_state();
        state.start_with(engine.clone(), egui::Context::default(), |_| Err(std::io::Error::other("controlled spawn failure")));
        assert!(!state.busy());
        assert!(state.message.as_ref().unwrap().contains("controlled spawn failure"));
        assert_eq!(state.path, "new-fab");
        assert_eq!(state.mask, "0.05");
        assert_eq!(engine.session_state(), before);
    }
}
