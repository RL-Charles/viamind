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
    sourcing: SourcingDraft,
}
struct SourcingDraft {
    manufacturer: String,
    mpn: String,
    supplier: String,
    supplier_sku: String,
    populate: bool,
    mounting: Option<viamind_doc::MountingTechnology>,
    calibrated: bool,
    centroid_x: String,
    centroid_y: String,
    zero_axis: String,
}
impl SourcingDraft {
    fn from_part(part: &Part) -> Self {
        let m = part.assembly.as_ref();
        Self {
            manufacturer: m.and_then(|m| m.manufacturer.clone()).unwrap_or_default(),
            mpn: m.and_then(|m| m.mpn.clone()).unwrap_or_default(),
            supplier: m.and_then(|m| m.supplier.clone()).unwrap_or_default(),
            supplier_sku: m.and_then(|m| m.supplier_sku.clone()).unwrap_or_default(),
            populate: m.is_none_or(|m| m.populate),
            mounting: m.and_then(|m| m.mounting),
            calibrated: m.is_some_and(|m| m.placement.is_some()),
            centroid_x: m.and_then(|m| m.placement).map_or_else(|| "0".into(), |p| p.centroid_local.x.to_mm().to_string()),
            centroid_y: m.and_then(|m| m.placement).map_or_else(|| "0".into(), |p| p.centroid_local.y.to_mm().to_string()),
            zero_axis: m.and_then(|m| m.placement).map_or_else(|| "0".into(), |p| (f64::from(p.zero_axis_mdeg) / 1000.0).to_string()),
        }
    }
    fn metadata(&self) -> Result<viamind_doc::AssemblyMetadata, String> {
        let placement = if self.calibrated {
            let number = |label, text: &str, scale: f64, min: i64, max: i64| {
                let value = text.parse::<f64>().ok().filter(|value| value.is_finite()).map(|value| value * scale);
                value
                    .filter(|value| *value >= min as f64 && *value <= max as f64)
                    .map(|value| value.round() as i64)
                    .ok_or_else(|| format!("{label} must be a finite value within the documented range"))
            };
            let centroid = |label, text: &str| number(label, text, 1_000_000.0, -10_000_000_000, 10_000_000_000).map(viamind_geom::Nm);
            Some(viamind_doc::AssemblyPlacement {
                centroid_local: viamind_geom::Point::new(
                    centroid("Local centroid X", &self.centroid_x)?,
                    centroid("Local centroid Y", &self.centroid_y)?,
                ),
                zero_axis_mdeg: i32::try_from(number("Local zero-axis marker", &self.zero_axis, 1000.0, 0, 359_999)?).expect("bounded millidegrees"),
            })
        } else {
            None
        };
        for s in [&self.manufacturer, &self.mpn, &self.supplier, &self.supplier_sku].into_iter().filter(|s| !s.is_empty()) {
            viamind_doc::AssemblyMetadata::validate_field(s).map_err(str::to_owned)?;
        }
        let field = |s: &String| (!s.is_empty()).then(|| s.clone());
        let metadata = viamind_doc::AssemblyMetadata {
            manufacturer: field(&self.manufacturer),
            mpn: field(&self.mpn),
            supplier: field(&self.supplier),
            supplier_sku: field(&self.supplier_sku),
            populate: self.populate,
            mounting: self.mounting,
            placement,
        };
        metadata.validate().map_err(str::to_owned)?;
        Ok(metadata)
    }
    fn fields(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.heading("Assembly metadata");
        for (label, text) in [
            ("Manufacturer", &mut self.manufacturer),
            ("Manufacturer part number", &mut self.mpn),
            ("Supplier", &mut self.supplier),
            ("Supplier stock number", &mut self.supplier_sku),
        ] {
            let label = ui.label(label);
            ui.add(egui::TextEdit::singleline(text).char_limit(512).desired_width(150.0)).labelled_by(label.id);
        }
        ui.checkbox(&mut self.populate, "Populate component");
        ui.label("Explicit sourcing fields allow 512 UTF-8 bytes each. Template BOMs and calibrated placement apply population; ordinary BOM/placement and saved jobs inventory all components.");
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.mounting, None, "Mounting unspecified");
            for (value, label) in [
                (viamind_doc::MountingTechnology::SurfaceMount, "Surface mount"),
                (viamind_doc::MountingTechnology::ThroughHole, "Through hole"),
                (viamind_doc::MountingTechnology::Hybrid, "Hybrid"),
            ] {
                ui.selectable_value(&mut self.mounting, Some(value), label);
            }
        });
        ui.checkbox(&mut self.calibrated, "Explicit placement calibration");
        ui.add_enabled_ui(self.calibrated, |ui| {
            for (label, text) in [
                ("Local centroid X (mm)", &mut self.centroid_x),
                ("Local centroid Y (mm)", &mut self.centroid_y),
                ("Local zero-axis marker (deg)", &mut self.zero_axis),
            ] {
                let label = ui.label(label);
                ui.add(egui::TextEdit::singleline(text).char_limit(64).desired_width(150.0)).labelled_by(label.id);
            }
        });
        ui.label("Local X right/Y down, centroid ±10000 mm. Marker 0=+X, 90=−Y; range 0..359.999°. Calibrated CSV uses global X right/Y up, CCW top view, and reflects the marker on bottom (r+180−marker). No universal vendor zero or inferred centroid.");
    }
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
        if self.routing.editing()
            || self.track_cleanup.editing()
            || self.net_rules.editing()
            || self.files.busy()
            || self.copper.editing()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
        {
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
        if self.routing.editing() || self.track_cleanup.editing() || self.net_rules.editing() {
            return;
        }
        let Some(design) = &self.design else { return };
        // Read selection, part and stream consistently; avoid cloning the whole project per frame.
        let current = design.engine.with_engine(|e| {
            let selection = e.selection();
            let part = if selection.ids.len() == 1 { e.project().board.part(&selection.ids[0]) } else { None };
            let part = part
                .map(|p| {
                    if let Some(m) = &p.assembly {
                        m.validate()?;
                    }
                    Ok::<_, &'static str>(p.clone())
                })
                .transpose();
            (part, e.event_cursor(), e.project().id)
        });
        let (part, EventCursor { stream_id, .. }, project_id) = current;
        let part = match part {
            Ok(part) => part,
            Err(error) => {
                ui.label(error);
                return;
            }
        };
        let Some(part) = part else {
            self.parts.properties = None;
            ui.heading("Part properties");
            ui.label("Select one part to edit its properties.");
            if ui.button("Place part…").clicked() {
                self.begin_place_part();
            }
            return;
        };
        if self.parts.properties.as_ref().is_none_or(|p| p.original.id != part.id || p.stream != stream_id) {
            self.parts.properties = Some(Properties {
                draft: Draft::from_part(&part),
                sourcing: SourcingDraft::from_part(&part),
                original: part.clone(),
                stream: stream_id,
            });
            self.parts.message = None;
        }
        let Some(properties) = &mut self.parts.properties else { return };
        ui.heading(format!("Part {}", part.designator));
        if properties.original != part {
            ui.label("Part changed; reload to review the latest properties.");
        }
        properties.draft.fields(ui);
        let mut apply_sourcing = false;
        let mut clear_sourcing = false;
        let enabled =
            !self.files.busy() && !self.copper.editing() && self.parts.placement.is_none() && !self.outlines.editing() && !self.pads.editing();
        if let Some(message) = &self.parts.message {
            ui.label(message);
        }
        let edit_pads = ui.button("Edit electrical pads…").clicked();
        let edit_artwork = ui.button("Edit footprint artwork").clicked();
        let edit_holes = ui.button("Edit nonplated footprint holes").clicked();
        let edit_apertures =
            ui.add_enabled(part.pads.as_ref().is_some_and(|p| !p.0.is_empty()), egui::Button::new("Edit pad mask and paste openings")).clicked();
        let mut apply = false;
        let mut reload = false;
        let mut delete = false;
        ui.horizontal(|ui| {
            apply = ui.button("Apply properties").clicked();
            reload = ui.button("Reload").clicked();
        });
        delete |= ui.button("Delete part").clicked();
        ui.add_enabled_ui(enabled, |ui| {
            properties.sourcing.fields(ui);
            apply_sourcing = ui.button("Apply assembly metadata").clicked();
            clear_sourcing = ui.button("Clear assembly metadata").clicked();
        });
        if edit_holes {
            self.begin_edit_part_holes(part.clone());
            return;
        }
        if edit_apertures {
            self.begin_edit_pad_apertures(part.clone());
            return;
        }
        if edit_artwork {
            self.begin_edit_footprint_artwork(part.clone());
            return;
        }
        if edit_pads {
            self.begin_edit_pads(part);
            return;
        }
        if reload {
            properties.original = part.clone();
            properties.draft = Draft::from_part(&part);
            properties.sourcing = SourcingDraft::from_part(&part);
            self.parts.message = None;
        } else if apply_sourcing || clear_sourcing {
            let metadata = if clear_sourcing { Ok(None) } else { properties.sourcing.metadata().map(Some) };
            let result = metadata.and_then(|metadata| {
                UiAction::SetPartAssembly
                    .run(&design.engine, json!({"id":part.id,"assembly":metadata,"expected":properties.original,"expected_project":project_id}))
                    .map_err(|e| e.to_string())
            });
            match result {
                Ok(_) => {
                    self.parts.properties = None;
                    self.parts.message = None;
                }
                Err(error) => self.parts.message = Some(error),
            }
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
