//! Human generic BOM/placement exports through the same bounded snapshot writer as MCP.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};
use viamind_engine::{EngineError, EngineHandle, EventCursor};
use viamind_fab::{AssemblyCsv, AssemblyExportRequest, AssemblySnapshot};

pub(crate) fn export(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |message: String| EngineError::InvalidInput { command: "export_assembly_csv".into(), message };
    let request: AssemblyExportRequest<EventCursor> = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
    if request.path.as_os_str().is_empty() {
        return Err(invalid("path must not be empty".into()));
    }
    let (snapshot, cursor, native) = engine.with_engine(|e| {
        let cursor = e.event_cursor();
        if request.expected_cursor.is_some_and(|expected| expected != cursor) {
            return Err(invalid("document changed; retry export from a fresh snapshot".into()));
        }
        let snapshot = AssemblySnapshot::capture(e.project()).map_err(|e| invalid(e.to_string()))?;
        Ok((snapshot, cursor, e.document_status().path))
    })?;
    let count = snapshot.component_count();
    let path = snapshot
        .write(request.kind, &request.path, request.overwrite, native.as_deref())
        .map_err(|e| invalid(format!("could not export CSV: {e}")))?;
    Ok(json!({"path":path,"exported_cursor":cursor,"component_count":count,"kind":request.kind}))
}

pub struct AssemblyExportUi {
    open: bool,
    kind: AssemblyCsv,
    path: String,
    overwrite: bool,
    worker: Option<Receiver<Result<Value, String>>>,
    message: Option<String>,
    result: Option<Value>,
}
impl Default for AssemblyExportUi {
    fn default() -> Self {
        Self { open: false, kind: AssemblyCsv::Bom, path: String::new(), overwrite: false, worker: None, message: None, result: None }
    }
}
impl AssemblyExportUi {
    /// Includes work continuing while its window is closed.
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn request(&self, cursor: EventCursor) -> Result<Value, String> {
        let path = PathBuf::from(self.path.trim());
        if self.path.trim().is_empty() {
            return Err("Enter a CSV output file path in an existing directory.".into());
        }
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")) {
            return Err("Output path must end in .csv.".into());
        }
        serde_json::to_value(AssemblyExportRequest { kind: self.kind, path, overwrite: self.overwrite, expected_cursor: Some(cursor) })
            .map_err(|e| e.to_string())
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
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.result = None;
                self.message = Some("Export worker disconnected. Check the destination before retrying.".into());
            }
        }
    }
    fn start(&mut self, engine: EngineHandle, ctx: egui::Context) {
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
        match std::thread::Builder::new().name("viamind-assembly-export".into()).spawn(move || {
            let result = UiAction::ExportAssemblyCsv.run(&engine, input).map_err(|e| e.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        }) {
            Ok(_) => {
                self.worker = Some(receiver);
                self.message = None;
                self.result = None;
            }
            Err(error) => {
                self.message = Some(format!("Could not start export worker: {error}"));
            }
        }
    }
}
impl ViamindApp {
    pub fn begin_assembly_export(&mut self) {
        if self.design.is_some() {
            self.assembly_export.open = true;
        }
    }
    pub(crate) fn assembly_export_window(&mut self, ui: &mut Ui) {
        self.assembly_export.poll(ui.ctx());
        if !self.assembly_export.open {
            return;
        }
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let cursor = engine.event_cursor();
        let busy = self.assembly_export.busy();
        let mut start = false;
        egui::Window::new("Export assembly CSV").open(&mut self.assembly_export.open).default_width(580.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Generic component tables from an immutable board snapshot.");
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.assembly_export.kind, AssemblyCsv::Bom, "BOM");
                    ui.selectable_value(&mut self.assembly_export.kind, AssemblyCsv::Placement, "Placement");
                });
                ui.label(match self.assembly_export.kind {
                    AssemblyCsv::Bom => "Groups exact value and footprint; lists component IDs and references.",
                    AssemblyCsv::Placement => "Component origins in global mm: X right, Y down, CCW top view. Bottom keeps the stored transform.",
                });
                let label = ui.label("CSV output file");
                ui.add(egui::TextEdit::singleline(&mut self.assembly_export.path).desired_width(520.0).char_limit(4096)).labelled_by(label.id);
                ui.checkbox(&mut self.assembly_export.overwrite, "Allow replacing this CSV output");
                ui.label("Default: a NEW file is required. The active native project is always protected.");
                ui.label("At most 100000 components, 2 MiB aggregate text and 32 MiB CSV. No sourcing IDs or manufacturer preset.");
                start = ui.button(if self.assembly_export.overwrite { "Export CSV (replace allowed)" } else { "Export CSV to NEW file" }).clicked();
            });
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Exporting CSV snapshot…");
                });
            }
            if let Some(message) = &self.assembly_export.message {
                ui.label(message);
            }
            if let Some(result) = &self.assembly_export.result {
                ui.label(format!("Exported {}", result["path"].as_str().unwrap_or("")));
                ui.label(format!("{} components · {}", result["component_count"], if result["kind"] == "bom" { "BOM" } else { "Placement" }));
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
        if start {
            self.assembly_export.start(engine, ui.ctx().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_worker_is_retained_while_hidden_and_prevents_duplicate_start() {
        let (tx, rx) = mpsc::channel();
        let mut ui = AssemblyExportUi { worker: Some(rx), ..Default::default() };
        let ctx = egui::Context::default();
        ui.start(EngineHandle::new(viamind_doc::Project::new("unused")), ctx.clone());
        assert!(ui.busy());
        assert!(ui.message.is_none());
        ui.poll(&ctx);
        assert!(ui.busy());
        tx.send(Ok(json!({"kind":"bom"}))).unwrap();
        ui.poll(&ctx);
        assert!(!ui.busy());
        assert_eq!(ui.result.as_ref().unwrap()["kind"], "bom");
        let (tx, rx) = mpsc::channel();
        ui.worker = Some(rx);
        drop(tx);
        ui.poll(&ctx);
        assert!(!ui.busy());
        assert!(ui.message.unwrap().contains("disconnected"));
    }
}
