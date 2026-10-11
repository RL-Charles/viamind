//! Desktop editing of persistent electrical pads.
use crate::{ViamindApp, actions::UiAction};
use serde_json::json;
use viamind_doc::{ObjectId, Part};
use viamind_engine::PadInput;

struct Row {
    id: Option<ObjectId>,
    number: String,
    x: String,
    y: String,
    width: String,
    height: String,
    net: String,
}
impl Row {
    fn from_input(p: PadInput) -> Self {
        Self {
            id: p.id,
            number: p.number,
            x: p.x_mm.to_string(),
            y: p.y_mm.to_string(),
            width: p.width_mm.to_string(),
            height: p.height_mm.to_string(),
            net: p.net.unwrap_or_default(),
        }
    }
    fn input(&self) -> Result<PadInput, String> {
        let parse = |name: &str, s: &str| {
            s.trim().parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| format!("Pad {}: {name} must be a finite number", self.number))
        };
        Ok(PadInput {
            id: self.id,
            number: self.number.clone(),
            x_mm: parse("X", &self.x)?,
            y_mm: parse("Y", &self.y)?,
            width_mm: parse("width", &self.width)?,
            height_mm: parse("height", &self.height)?,
            net: Some(self.net.clone()),
        })
    }
}
struct Draft {
    project_id: ObjectId,
    original: Part,
    rows: Vec<Row>,
}
#[derive(Default)]
pub struct PadsUi {
    draft: Option<Draft>,
    message: Option<String>,
    library_path: String,
}
impl PadsUi {
    pub(crate) fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        self.draft = None;
        self.message = None;
    }
}
impl ViamindApp {
    pub fn begin_edit_pads(&mut self, part: Part) {
        if self.routing.editing()
            || self.track_cleanup.editing()
            || self.files.busy()
            || self.copper.editing()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
        {
            return;
        }
        let inputs = match &part.pads {
            Some(pads) => pads
                .0
                .iter()
                .map(|p| PadInput {
                    id: Some(p.id),
                    number: p.number.clone(),
                    x_mm: p.position.x.to_mm(),
                    y_mm: p.position.y.to_mm(),
                    width_mm: p.width.to_mm(),
                    height_mm: p.height.to_mm(),
                    net: p.net.clone(),
                })
                .collect(),
            None => crate::board_scene::preview_pad_inputs(&part),
        };
        let Some(project_id) = self.design.as_ref().map(|d| d.engine.read(|p| p.id)) else { return };
        self.pads.draft = Some(Draft { project_id, original: part, rows: inputs.into_iter().map(Row::from_input).collect() });
        self.pads.message = None;
    }
    pub(crate) fn pads_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(draft) = &mut self.pads.draft else { return };
        let mut apply = false;
        let mut cancel = false;
        let mut reload = false;
        let mut import = false;
        let mut export = false;
        let modal = egui::Modal::new(egui::Id::new("electrical_pads")).show(ui.ctx(), |ui| {
            ui.heading(format!("Electrical pads — {}", draft.original.designator));
            ui.label("Rectangular SMD pads; local dimensions in mm. Blank net means unassigned.");
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                let mut remove = None;
                for (index, row) in draft.rows.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.separator();
                        egui::Grid::new("pad_fields").num_columns(2).show(ui, |ui| {
                            for (name, text) in [
                                ("Number", &mut row.number),
                                ("Local X (mm)", &mut row.x),
                                ("Local Y (mm)", &mut row.y),
                                ("Width (mm)", &mut row.width),
                                ("Height (mm)", &mut row.height),
                                ("Net", &mut row.net),
                            ] {
                                let label = ui.label(format!("{name} {}", index + 1));
                                ui.add(egui::TextEdit::singleline(text).desired_width(180.0)).labelled_by(label.id);
                                ui.end_row();
                            }
                        });
                        if ui.button(format!("Remove pad {}", index + 1)).clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    draft.rows.remove(index);
                }
            });
            if ui.button("Add pad").clicked() {
                let number = (1..).map(|i| i.to_string()).find(|n| draft.rows.iter().all(|r| &r.number != n)).unwrap();
                draft.rows.push(Row::from_input(PadInput { id: None, number, x_mm: 0.0, y_mm: 0.0, width_mm: 1.0, height_mm: 1.0, net: None }));
            }
            ui.separator();
            ui.label("Footprint library · rectangular SMD geometry without nets or placement");
            let label = ui.label("Library file path");
            ui.add(egui::TextEdit::singleline(&mut self.pads.library_path).desired_width(400.0)).labelled_by(label.id);
            ui.label("Export uses applied pads. Apply edits first. Import replaces the draft and applied geometry.");
            ui.horizontal(|ui| {
                export = ui.button("Export applied footprint").clicked();
                import = ui.button("Replace pads from library").clicked();
            });
            if let Some(message) = &self.pads.message {
                ui.label(message);
            }
            ui.horizontal(|ui| {
                apply = ui.button("Apply pads").clicked();
                reload = ui.button("Reload pads").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || modal.should_close() {
            self.pads.cancel();
        } else if import || export {
            let result = self.design.as_ref().ok_or_else(|| "No active design".to_owned()).and_then(|design| {
                let action = if import { UiAction::ImportFootprint } else { UiAction::ExportFootprint };
                action
                    .run(&design.engine, json!({"expected_project":draft.project_id,"expected":draft.original,"path":self.pads.library_path}))
                    .map_err(|e| e.to_string())
            });
            match result {
                Ok(_) if import => {
                    let id = draft.original.id;
                    let part = self.design.as_ref().and_then(|d| d.engine.read(|p| p.board.part(&id).cloned()));
                    self.pads.cancel();
                    if let Some(part) = part {
                        self.begin_edit_pads(part);
                    }
                    self.pads.message = Some("Footprint applied. Matching pad numbers retained their nets; new pads are unassigned.".into());
                }
                Ok(_) => self.pads.message = Some("Applied footprint exported to a new library file.".into()),
                Err(e) => self.pads.message = Some(e),
            }
        } else if reload {
            let id = draft.original.id;
            let current = self.design.as_ref().and_then(|d| d.engine.read(|p| p.board.part(&id).cloned()));
            if let Some(part) = current {
                self.pads.cancel();
                self.begin_edit_pads(part);
            } else {
                self.pads.message = Some("Part no longer exists; cancel and select another part.".into());
            }
        } else if apply {
            let result = draft.rows.iter().map(Row::input).collect::<Result<Vec<_>, _>>().and_then(|pads| {
                let design = self.design.as_ref().ok_or("No active design")?;
                UiAction::SetPartPads
                    .run(&design.engine, json!({"id": draft.original.id, "expected":draft.original, "pads":pads}))
                    .map_err(|e| e.to_string())
            });
            match result {
                Ok(_) => self.pads.cancel(),
                Err(e) => self.pads.message = Some(e),
            }
        }
    }
}
