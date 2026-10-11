//! Read-only export window; snapshot work and disk I/O run outside the UI thread.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};
use viamind_engine::{EngineError, EngineHandle, EventCursor};
#[derive(Default)]
pub struct FabricationUi {
    open: bool,
    path: String,
    mask: String,
    worker: Option<Receiver<Result<Value, String>>>,
    message: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    path: PathBuf,
    #[serde(default)]
    mask_expansion_mm: f64,
    #[serde(default)]
    expected_cursor: Option<EventCursor>,
}
pub(crate) fn export_fabrication(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |message: String| EngineError::InvalidInput { command: "export_fabrication".into(), message };
    let input: Export = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
    let (snapshot, native) = engine.with_engine(|e| (e.session_state(), e.document_status().path));
    if input.expected_cursor.is_some_and(|c| c != snapshot.cursor) {
        return Err(invalid("document changed; retry export from a fresh snapshot".into()));
    }
    let (path, manifest) = viamind_fab::write_fabrication(
        &snapshot.project,
        viamind_fab::FabricationOptions { mask_expansion_mm: input.mask_expansion_mm },
        &input.path,
        native.as_deref(),
    )
    .map_err(|e| invalid(e.to_string()))?;
    Ok(json!({"path":path,"manifest":manifest,"exported_cursor":snapshot.cursor}))
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
        if let Some(receiver) = &self.fabrication.worker {
            match receiver.try_recv() {
                Ok(result) => {
                    self.fabrication.worker = None;
                    self.fabrication.message = Some(match result {
                        Ok(value) => format!(
                            "Exported {} files to {} (snapshot {})",
                            value["manifest"]["files"].as_array().map_or(0, Vec::len),
                            value["path"].as_str().unwrap_or(""),
                            value["exported_cursor"]["sequence"]
                        ),
                        Err(e) => e,
                    });
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.fabrication.worker = None;
                    self.fabrication.message = Some("Export worker disconnected".into());
                }
                Err(mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(50)),
            }
        }
        if !self.fabrication.open {
            return;
        }
        let Some(design) = &self.design else {
            return;
        };
        let engine = design.engine.clone();
        let mut export = false;
        egui::Window::new("Fabrication export").open(&mut self.fabrication.open).default_width(580.0).show(ui.ctx(), |ui| {
            ui.label("Two outer copper layers, explicit SMD pads, tracks, through vias, mask and polygon profile.");
            ui.label("A new directory is required. Existing destinations are always preserved.");
            ui.label("No legacy footprint previews, inner copper, pours, paste, silkscreen or fabrication certification.");
            let label = ui.label("New fabrication directory");
            ui.add(egui::TextEdit::singleline(&mut self.fabrication.path).desired_width(500.0)).labelled_by(label.id);
            let label = ui.label("Mask expansion (mm)");
            ui.add(egui::TextEdit::singleline(&mut self.fabrication.mask).desired_width(100.0)).labelled_by(label.id);
            ui.label("Default0: mask opening equals pad; all vias untented. Export X right/Y up; common top view.");
            export = ui.add_enabled(self.fabrication.worker.is_none(), egui::Button::new("Export fabrication snapshot")).clicked();
            if self.fabrication.worker.is_some() {
                ui.label("Exporting snapshot…");
            }
            if let Some(message) = &self.fabrication.message {
                ui.label(message);
            }
        });
        if export {
            let Some(mask) = self.fabrication.mask.parse::<f64>().ok().filter(|v| v.is_finite() && (0.0..=10.0).contains(v)) else {
                self.fabrication.message = Some("Mask expansion needs a finite number from 0 to 10 mm".into());
                return;
            };
            let input = json!({"path":self.fabrication.path,"mask_expansion_mm":mask,"expected_cursor":engine.session_state().cursor});
            let (sender, receiver) = mpsc::channel();
            self.fabrication.worker = Some(receiver);
            self.fabrication.message = None;
            std::thread::spawn(move || {
                let _ = sender.send(UiAction::ExportFabrication.run(&engine, input).map_err(|e| e.to_string()));
            });
        }
    }
}
