//! Local property strings; the shared guarded command alone mutates copper.
use crate::{ViamindApp, actions::UiAction, symbol_library::millimetres};
use egui::{Color32, Ui};
use serde_json::{Value, json};
use std::time::Instant;
use viamind_doc::{CopperLayer, ObjectId};
use viamind_engine::{CopperPropertyId, CopperPropertyObject, CopperPropertyQuery, CopperPropertySnapshot, EngineHandle};
use viamind_geom::Nm;

pub(crate) struct PropertyDraft {
    target: Option<CopperPropertyId>,
    original: Option<CopperPropertySnapshot>,
    width: String,
    diameter: String,
    drill: String,
    net: String,
    layer: CopperLayer,
    message: Option<String>,
}
fn decimal(n: Nm) -> String {
    format!("{}.{:06}", n.0 / 1_000_000, n.0 % 1_000_000)
}
impl PropertyDraft {
    fn new(engine: &EngineHandle, target: Result<CopperPropertyId, String>) -> Self {
        let mut draft = Self {
            target: None,
            original: None,
            width: String::new(),
            diameter: String::new(),
            drill: String::new(),
            net: String::new(),
            layer: CopperLayer::Top,
            message: None,
        };
        match target {
            Ok(target) => {
                draft.target = Some(target);
                draft.reload(engine);
            }
            Err(e) => draft.message = Some(e),
        }
        draft
    }
    fn set_original(&mut self, s: CopperPropertySnapshot) {
        match &s.object {
            CopperPropertyObject::Track { track } => {
                self.width = decimal(track.width);
                self.layer = track.layer;
                self.net = track.net.clone().unwrap_or_default();
            }
            CopperPropertyObject::Via { via } => {
                self.diameter = decimal(via.diameter);
                self.drill = decimal(via.drill);
                self.net = via.net.clone().unwrap_or_default();
            }
        }
        self.original = Some(s);
    }
    fn reload(&mut self, engine: &EngineHandle) {
        let Some(target) = self.target else { return };
        match engine.query_copper_properties(&CopperPropertyQuery { target }) {
            Ok(s) => {
                self.set_original(s);
                self.message = None;
            }
            Err(e) => {
                self.original = None;
                self.message = Some(e.to_string());
            }
        }
    }
    fn current(&self, engine: &EngineHandle) -> bool {
        self.original.as_ref().is_some_and(|s| s.source_cursor == engine.event_cursor())
    }
    fn input(&self) -> Result<(UiAction, Value), String> {
        let s = self.original.as_ref().ok_or("Choose an existing track or through via")?;
        let net = if self.net.is_empty() { None } else { Some(self.net.clone()) };
        let size = |text: &str| millimetres(text).and_then(|n| if n.0 > 0 { Ok(n) } else { Err("Copper sizes must be positive".into()) });
        let rules = viamind_drc::DrcInput::default();
        Ok(match &s.object {
            CopperPropertyObject::Track { track } => (
                UiAction::SetTrackProperties,
                json!({"expected_project":s.project_id,"expected_cursor":s.source_cursor,"id":track.id,"expected":track,"width":size(&self.width)?,"layer":self.layer,"net":net,"rules":rules}),
            ),
            CopperPropertyObject::Via { via } => (
                UiAction::SetViaProperties,
                json!({"expected_project":s.project_id,"expected_cursor":s.source_cursor,"id":via.id,"expected":via,"diameter":size(&self.diameter)?,"drill":size(&self.drill)?,"net":net,"rules":rules}),
            ),
        })
    }
    fn apply(&mut self, engine: &EngineHandle) {
        let result = (|| {
            let (action, input) = self.input()?;
            let start = Instant::now();
            let output = action.run(engine, input).map_err(|e| e.to_string())?;
            let old = self.original.as_ref().expect("validated original");
            // A successful non-no-op property command records one design event. Use its
            // returned delta, not a later unguarded query that could capture peer work.
            let delta = output.get("deltas").and_then(Value::as_array).and_then(|v| v.first()).ok_or("Missing committed property delta")?;
            let object = match &old.object {
                CopperPropertyObject::Track { .. } => {
                    CopperPropertyObject::Track { track: serde_json::from_value(delta["after"].clone()).map_err(|e| e.to_string())? }
                }
                CopperPropertyObject::Via { .. } => {
                    CopperPropertyObject::Via { via: serde_json::from_value(delta["after"].clone()).map_err(|e| e.to_string())? }
                }
            };
            let mut cursor = old.source_cursor;
            cursor.sequence = cursor.sequence.checked_add(1).ok_or("Committed property cursor overflow")?;
            let snapshot = CopperPropertySnapshot {
                project_id: old.project_id,
                source_cursor: cursor,
                object,
                document_net_rules: old.document_net_rules.clone(),
            };
            self.set_original(snapshot);
            Ok(format!("Properties applied; scoped checks only ({:.2} ms).", start.elapsed().as_secs_f64() * 1000.))
        })();
        self.message = Some(match result {
            Ok(m) => m,
            Err(e) => e,
        });
    }
}
impl ViamindApp {
    pub fn begin_copper_properties(&mut self) {
        self.begin_copper_properties_for(None);
    }
    pub(crate) fn begin_copper_properties_for(&mut self, target: Option<CopperPropertyId>) {
        if self.schematic.active
            || self.routing.editing()
            || self.track_cleanup.editing()
            || self.net_rules.editing()
            || self.files.busy()
            || self.parts.placing()
            || self.pads.editing()
            || self.outlines.editing()
            || self.copper.editing()
        {
            return;
        }
        let Some(design) = &self.design else { return };
        let target = target.map_or_else(
            || {
                design.engine.with_engine(|e| {
                    if e.selection().ids.len() != 1 {
                        return Err("Select exactly one existing track or through via, or use its copper-row Edit button".into());
                    }
                    let id = e.selection().ids[0];
                    if e.project().board.tracks.contains_key(&id) {
                        Ok(CopperPropertyId::Track { id })
                    } else if e.project().board.vias.contains_key(&id) {
                        Ok(CopperPropertyId::Via { id })
                    } else {
                        Err("Selected object is not a straight track or through via".into())
                    }
                })
            },
            Ok,
        );
        self.selection.cancel_gestures();
        self.copper.properties = Some(PropertyDraft::new(&design.engine, target));
    }
    pub(crate) fn copper_property_dialog(&mut self, ui: &mut Ui) {
        let Some(d) = &mut self.copper.properties else {
            return;
        };
        let Some(design) = &self.design else { return };
        let engine = &design.engine;
        let mut apply = false;
        let mut reload = false;
        let mut cancel = false;
        let mut open = true;
        let content = ui.ctx().content_rect();
        let current = d.current(engine);
        egui::Window::new("Copper properties").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_TOP,[-12.,42.]).default_width(420.).max_width((content.width()-32.).max(180.)).show(ui.ctx(),|ui|{
            egui::ScrollArea::vertical().id_salt("copper_property_fields").max_height((content.height()-185.).clamp(50.,420.)).show(ui,|ui|{
                if let Some(s)=&d.original {
                    let id=match &s.object{CopperPropertyObject::Track{track}=>track.id,CopperPropertyObject::Via{via}=>via.id};
                    ui.label(format!("Stable ID {id}"));
                    match &s.object{CopperPropertyObject::Track{track}=>{ui.label("Straight track; positions remain fixed.");ui.monospace(format!("Start X {} Y {}nm",track.start.x.0,track.start.y.0));ui.monospace(format!("End X {} Y {}nm",track.end.x.0,track.end.y.0));},CopperPropertyObject::Via{via}=>{ui.label("Through via; position and span remain fixed.");ui.monospace(format!("Center X {} Y {}nm",via.position.x.0,via.position.y.0));}}
                    egui::Grid::new(("copper_property_fields",id)).num_columns(2).show(ui,|ui|{
                        match &s.object {CopperPropertyObject::Track{..}=>{field(ui,id,"Property track width (mm)","width",&mut d.width);ui.label("Property copper layer");ui.horizontal(|ui|{ui.radio_value(&mut d.layer,CopperLayer::Top,"Property top copper");ui.radio_value(&mut d.layer,CopperLayer::Bottom,"Property bottom copper");});ui.end_row();},CopperPropertyObject::Via{..}=>{field(ui,id,"Property via diameter (mm)","diameter",&mut d.diameter);field(ui,id,"Property via drill (mm)","drill",&mut d.drill);}}
                        field(ui,id,"Property net (empty = unassigned)","net",&mut d.net);
                    });
                    crate::net_rules::show_document_minima(ui,&s.document_net_rules);
                    ui.label("Apply checks explicit pads, tracks, through vias, outline/cutouts and document minima. Existing unrelated violations and inherited exclusions may remain. No movement, automatic sizing, net inference or physical certification.");
                }
                if !current&&d.original.is_some(){ui.colored_label(Color32::YELLOW,"Session changed. Reload discards local fields before another Apply.");}
                if let Some(m)=&d.message{ui.label(m);}
            });
            ui.separator();ui.label("Apply runs scoped checks; excluded objects are not certified.");
            ui.horizontal(|ui|{apply=ui.add_enabled(current,egui::Button::new("Apply properties")).clicked();reload=ui.add_enabled(d.target.is_some(),egui::Button::new("Reload properties")).on_hover_text("Discard local fields and capture current properties").clicked();cancel=ui.button("Cancel properties").clicked();});
        });
        if cancel || !open || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.copper.cancel();
            return;
        }
        if reload {
            d.reload(engine);
        } else if apply {
            d.apply(engine);
        }
    }
}
fn field(ui: &mut Ui, id: ObjectId, label: &str, key: &str, value: &mut String) {
    let r = ui.label(label);
    ui.add(
        egui::TextEdit::singleline(value)
            .id(egui::Id::new(("copper_properties", id, key)))
            .char_limit(if key == "net" { 128 } else { 32 })
            .desired_width(150.),
    )
    .labelled_by(r.id);
    ui.end_row();
}
