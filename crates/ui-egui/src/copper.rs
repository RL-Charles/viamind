//! Accessible coordinate entry for persistent copper, through shared commands.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use viamind_doc::{CopperLayer, ObjectId};

struct Draft {
    project: ObjectId,
    page: usize,
    via: bool,
    x: String,
    y: String,
    end_x: String,
    end_y: String,
    width: String,
    diameter: String,
    drill: String,
    net: String,
    layer: CopperLayer,
    message: Option<String>,
}
#[derive(Default)]
pub struct CopperUi {
    draft: Option<Draft>,
}
impl CopperUi {
    pub(crate) fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        self.draft = None;
    }
}
impl ViamindApp {
    pub fn begin_copper(&mut self) {
        if self.routing.editing()
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
        self.copper.draft = Some(Draft {
            project: design.engine.read(|p| p.id),
            page: 0,
            via: false,
            x: "10".into(),
            y: "10".into(),
            end_x: "20".into(),
            end_y: "10".into(),
            width: "0.25".into(),
            diameter: "0.6".into(),
            drill: "0.3".into(),
            net: String::new(),
            layer: CopperLayer::Top,
            message: None,
        });
    }
    pub(crate) fn copper_dialog(&mut self, ui: &mut Ui) {
        let Some(draft) = &mut self.copper.draft else { return };
        let Some(design) = &self.design else { return };
        let (track_count, via_count, page, tracks, vias) = design.engine.read(|p| {
            let track_count = p.board.tracks.len();
            let via_count = p.board.vias.len();
            let page = draft.page.min((track_count + via_count).saturating_sub(1) / 50);
            let offset = page * 50;
            let tracks: Vec<_> = p.board.tracks.values().skip(offset).take(50).cloned().collect();
            let vias: Vec<_> = p.board.vias.values().skip(offset.saturating_sub(track_count)).take(50 - tracks.len()).cloned().collect();
            (track_count, via_count, page, tracks, vias)
        });
        draft.page = page;
        let mut add = false;
        let mut close = false;
        let mut deletion: Option<(UiAction, Value)> = None;
        let modal = egui::Modal::new(egui::Id::new("copper_dialog")).show(ui.ctx(), |ui| {
            ui.heading("Copper tracks and vias");
            ui.label("Coordinates and sizes in mm; Y increases down the board.");
            ui.horizontal(|ui| {
                ui.radio_value(&mut draft.via, false, "Straight track");
                ui.radio_value(&mut draft.via, true, "Through via");
            });
            egui::Grid::new("copper_fields").num_columns(2).show(ui, |ui| {
                field(ui, "Copper X (mm)", &mut draft.x);
                field(ui, "Copper Y (mm)", &mut draft.y);
                if draft.via {
                    field(ui, "Via diameter (mm)", &mut draft.diameter);
                    field(ui, "Via drill (mm)", &mut draft.drill);
                } else {
                    field(ui, "End X (mm)", &mut draft.end_x);
                    field(ui, "End Y (mm)", &mut draft.end_y);
                    field(ui, "Track width (mm)", &mut draft.width);
                    ui.label("Copper layer");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut draft.layer, CopperLayer::Top, "Top copper");
                        ui.radio_value(&mut draft.layer, CopperLayer::Bottom, "Bottom copper");
                    });
                    ui.end_row();
                }
                field(ui, "Copper net (optional)", &mut draft.net);
            });
            ui.label("Run Board → Design rule check for scoped track/via clearance and drill checks; guides remain pad-only.");
            if let Some(message) = &draft.message {
                ui.label(message);
            }
            ui.horizontal(|ui| {
                add = ui.button("Add copper").clicked();
                close = ui.button("Close copper").clicked();
            });
            ui.separator();
            ui.label(format!("Existing copper: {track_count} tracks, {via_count} through vias"));
            ui.horizontal(|ui| {
                if ui.add_enabled(draft.page > 0, egui::Button::new("Previous copper page")).clicked() {
                    draft.page -= 1;
                }
                ui.label(format!("Page {}", draft.page + 1));
                if ui.add_enabled((draft.page + 1) * 50 < track_count + via_count, egui::Button::new("Next copper page")).clicked() {
                    draft.page += 1;
                }
            });
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for track in &tracks {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Track {:?}: ({}, {}) → ({}, {}) mm, width {} mm",
                            track.layer,
                            track.start.x.to_mm(),
                            track.start.y.to_mm(),
                            track.end.x.to_mm(),
                            track.end.y.to_mm(),
                            track.width.to_mm()
                        ));
                        if ui.button(format!("Delete track {}", track.id)).clicked() {
                            deletion = Some((UiAction::DeleteTrack, json!({"id":track.id,"expected":track,"expected_project":draft.project})));
                        }
                    });
                }
                for via in &vias {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Through via ({}, {}) mm, diameter {}, drill {} mm",
                            via.position.x.to_mm(),
                            via.position.y.to_mm(),
                            via.diameter.to_mm(),
                            via.drill.to_mm()
                        ));
                        if ui.button(format!("Delete via {}", via.id)).clicked() {
                            deletion = Some((UiAction::DeleteVia, json!({"id":via.id,"expected":via,"expected_project":draft.project})));
                        }
                    });
                }
            });
        });
        if close || modal.should_close() {
            self.copper.cancel();
            return;
        }
        let result = if let Some((action, input)) = deletion {
            Some(action.run(&design.engine, input).map_err(|e| e.to_string()))
        } else if add {
            Some(draft.input().and_then(|(action, input)| action.run(&design.engine, input).map_err(|e| e.to_string())))
        } else {
            None
        };
        if let Some(result) = result {
            draft.message = Some(match result {
                Ok(_) => "Copper edit applied".into(),
                Err(error) => error,
            });
        }
    }
}
fn field(ui: &mut Ui, name: &str, text: &mut String) {
    let label = ui.label(name);
    ui.add(egui::TextEdit::singleline(text).desired_width(125.0)).labelled_by(label.id);
    ui.end_row();
}
impl Draft {
    fn input(&self) -> Result<(UiAction, Value), String> {
        let n =
            |name: &str, text: &str| text.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{name} needs a finite number"));
        let net = if self.net.trim().is_empty() { None } else { Some(self.net.trim()) };
        let x = n("X", &self.x)?;
        let y = n("Y", &self.y)?;
        Ok(if self.via {
            (
                UiAction::AddVia,
                json!({"expected_project":self.project,"x_mm":x,"y_mm":y,"diameter_mm":n("Diameter", &self.diameter)?,"drill_mm":n("Drill", &self.drill)?,"net":net}),
            )
        } else {
            (
                UiAction::AddTrack,
                json!({"expected_project":self.project,"start_x_mm":x,"start_y_mm":y,"end_x_mm":n("End X", &self.end_x)?,"end_y_mm":n("End Y", &self.end_y)?,"width_mm":n("Width", &self.width)?,"layer":self.layer,"net":net}),
            )
        })
    }
}
