//! Shared part placement and selected-object property drafts.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use viamind_doc::{ObjectId, Part, Side};
use viamind_engine::EventCursor;

#[derive(Clone)]
struct Draft {
    designator: String,
    value: String,
    footprint: String,
    x: String,
    y: String,
    rotation: String,
    side: Side,
}
impl Draft {
    fn new(designator: String) -> Self {
        Self { designator, value: String::new(), footprint: "R_0603".into(), x: "20".into(), y: "20".into(), rotation: "0".into(), side: Side::Top }
    }
    fn from_part(part: &Part) -> Self {
        Self {
            designator: part.designator.clone(),
            value: part.value.clone(),
            footprint: part.footprint.clone(),
            x: part.position.x.to_mm().to_string(),
            y: part.position.y.to_mm().to_string(),
            rotation: (f64::from(part.rotation_mdeg) / 1000.0).to_string(),
            side: part.side,
        }
    }
    fn input(&self) -> Result<Value, String> {
        let number = |name: &str, text: &str| {
            text.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{name} must be a finite number"))
        };
        Ok(json!({ "designator": self.designator, "value": self.value, "footprint": self.footprint,
            "x_mm": number("X (mm)", &self.x)?, "y_mm": number("Y (mm)", &self.y)?,
            "rotation_deg": number("Rotation (deg)", &self.rotation)?, "side": self.side }))
    }
    fn fields(&mut self, ui: &mut Ui) {
        egui::Grid::new("part_fields").num_columns(2).show(ui, |ui| {
            for (label, text) in [
                ("Designator", &mut self.designator),
                ("Value", &mut self.value),
                ("Footprint", &mut self.footprint),
                ("X (mm)", &mut self.x),
                ("Y (mm)", &mut self.y),
                ("Rotation (deg)", &mut self.rotation),
            ] {
                let label = ui.label(label);
                ui.add(egui::TextEdit::singleline(text).desired_width(150.0)).labelled_by(label.id);
                ui.end_row();
            }
            ui.label("Side");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.side, Side::Top, "Top");
                ui.radio_value(&mut self.side, Side::Bottom, "Bottom");
            });
            ui.end_row();
        });
    }
}
struct Properties {
    original: Part,
    stream: ObjectId,
    draft: Draft,
}
#[derive(Default)]
pub struct PartUi {
    placement: Option<(Draft, ObjectId)>,
    properties: Option<Properties>,
    message: Option<String>,
}
impl PartUi {
    pub(crate) fn cancel_placement(&mut self) {
        self.placement = None;
    }
    pub(crate) fn placing(&self) -> bool {
        self.placement.is_some()
    }
}

impl ViamindApp {
    /// Place an arbitrary part using the same command available to agents.
    pub fn begin_place_part(&mut self) {
        if self.files.busy() || self.parts.placing() || self.outlines.editing() || self.pads.editing() {
            return;
        }
        let Some(design) = &self.design else { return };
        let designator = design.engine.read(|p| (1..).map(|n| format!("R{n}")).find(|name| p.board.part_by_designator(name).is_none()).unwrap());
        self.parts.placement = Some((Draft::new(designator), design.engine.read(|p| p.id)));
        self.parts.message = None;
    }

    pub(crate) fn placement_dialog(&mut self, ui: &mut Ui) {
        let Some((draft, project)) = &mut self.parts.placement else { return };
        let mut place = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("place_part")).show(ui.ctx(), |ui| {
            ui.heading("Place part");
            draft.fields(ui);
            ui.label("Footprint names use the current built-in preview packages.");
            if let Some(message) = &self.parts.message {
                ui.label(message);
            }
            ui.horizontal(|ui| {
                place = ui.button("Place on board").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || modal.should_close() {
            self.parts.placement = None;
            self.parts.message = None;
        } else if place {
            let Some(design) = &self.design else { return };
            let result = draft.input().and_then(|mut input| {
                input["expected_project"] = json!(project);
                UiAction::PlacePart.run(&design.engine, input).map_err(|e| e.to_string())
            });
            match result {
                Ok(result) => {
                    let _ = UiAction::SetSelection.run(&design.engine, json!({ "ids": result["created"] }));
                    self.parts.placement = None;
                    self.parts.message = None;
                }
                Err(error) => self.parts.message = Some(error),
            }
        }
    }

    pub(crate) fn part_properties(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else { return };
        // Read selection, part and stream consistently; avoid cloning the whole project per frame.
        let current = design.engine.with_engine(|e| {
            let selection = e.selection();
            let part = if selection.ids.len() == 1 { e.project().board.part(&selection.ids[0]).cloned() } else { None };
            (part, e.event_cursor())
        });
        let (Some(part), EventCursor { stream_id, .. }) = current else {
            self.parts.properties = None;
            ui.heading("Part properties");
            ui.label("Select one part to edit its properties.");
            if ui.button("Place part…").clicked() {
                self.begin_place_part();
            }
            return;
        };
        if self.parts.properties.as_ref().is_none_or(|p| p.original.id != part.id || p.stream != stream_id) {
            self.parts.properties = Some(Properties { draft: Draft::from_part(&part), original: part.clone(), stream: stream_id });
            self.parts.message = None;
        }
        let Some(properties) = &mut self.parts.properties else { return };
        ui.heading(format!("Part {}", part.designator));
        if properties.original != part {
            ui.label("Part changed; reload to review the latest properties.");
        }
        properties.draft.fields(ui);
        if let Some(message) = &self.parts.message {
            ui.label(message);
        }
        let edit_pads = ui.button("Edit electrical pads…").clicked();
        let mut apply = false;
        let mut reload = false;
        let mut delete = false;
        ui.horizontal(|ui| {
            apply = ui.button("Apply properties").clicked();
            reload = ui.button("Reload").clicked();
        });
        delete |= ui.button("Delete part").clicked();
        if edit_pads {
            self.begin_edit_pads(part);
            return;
        }
        if reload {
            properties.original = part.clone();
            properties.draft = Draft::from_part(&part);
            self.parts.message = None;
        } else if apply {
            let result = properties.draft.input().and_then(|mut input| {
                input["id"] = json!(part.id);
                input["expected"] = json!(properties.original);
                UiAction::UpdatePart.run(&design.engine, input).map_err(|e| e.to_string())
            });
            match result {
                Ok(_) => {
                    // Refresh on the next frame, including any agent changes after Apply.
                    self.parts.properties = None;
                    self.parts.message = None;
                }
                Err(error) => self.parts.message = Some(error),
            }
        } else if delete {
            match UiAction::DeletePart.run(&design.engine, json!({ "id": part.id })) {
                Ok(_) => {
                    self.parts.properties = None;
                    self.parts.message = None;
                }
                Err(error) => self.parts.message = Some(error.to_string()),
            }
        }
    }
}
