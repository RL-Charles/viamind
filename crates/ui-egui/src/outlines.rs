//! Native polygon outline form, shared with the MCP command.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use std::io::Read;
use viamind_doc::dxf::{DxfUnits, MAX_DXF_BYTES};
use viamind_engine::ExpectedOutline;

struct OutlineDraft {
    outer: String,
    cutouts: String,
    expected: ExpectedOutline,
    message: Option<String>,
    dxf_path: String,
    dxf_units: DxfUnits,
    dxf_layer: String,
}
#[derive(Default)]
pub struct OutlineUi {
    draft: Option<OutlineDraft>,
}
impl OutlineUi {
    pub(crate) fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        self.draft = None;
    }
}
fn text(ring: &viamind_geom::Polygon) -> String {
    ring.points.iter().map(|p| format!("{},{}", p.x.to_mm(), p.y.to_mm())).collect::<Vec<_>>().join("\n")
}
fn parse_ring(lines: &[&str]) -> Result<Value, String> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let (x, y) = line.split_once(',').ok_or_else(|| format!("Line {} needs X,Y in mm", index + 1))?;
            let number = |value: &str| {
                value.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("Line {} needs finite X,Y numbers", index + 1))
            };
            Ok(json!({ "x_mm": number(x)?, "y_mm": number(y)? }))
        })
        .collect::<Result<Vec<Value>, String>>()
        .map(Value::Array)
}
impl ViamindApp {
    pub fn begin_board_outline(&mut self) {
        if self.routing.editing()
            || self.track_cleanup.editing()
            || self.net_rules.editing()
            || self.files.busy()
            || self.copper.editing()
            || self.parts.placing()
            || self.pads.editing()
            || self.outlines.editing()
        {
            return;
        }
        let Some(design) = &self.design else { return };
        let (expected, outer, cutouts) = design.engine.read(|project| {
            let expected = ExpectedOutline { project_id: project.id, outline: project.board.outline.clone() };
            if let Some(outline) = &project.board.outline {
                (expected, text(&outline.outer), outline.cutouts.iter().map(text).collect::<Vec<_>>().join("\n\n"))
            } else {
                let b = crate::board_scene::board_outline(project);
                (
                    expected,
                    format!("{},{}\n{},{}\n{},{}\n{},{}", b.min.x, b.min.y, b.max.x, b.min.y, b.max.x, b.max.y, b.min.x, b.max.y),
                    String::new(),
                )
            }
        });
        self.outlines.draft = Some(OutlineDraft {
            outer,
            cutouts,
            expected,
            message: None,
            dxf_path: String::new(),
            dxf_units: DxfUnits::Header,
            dxf_layer: String::new(),
        });
    }
    pub(crate) fn outline_dialog(&mut self, ui: &mut Ui) {
        let Some(draft) = &mut self.outlines.draft else { return };
        let mut apply = false;
        let mut cancel = false;
        let mut remove = false;
        let mut import = false;
        let modal = egui::Modal::new(egui::Id::new("board_outline_dialog")).show(ui.ctx(), |ui| {
            ui.heading("Board outline");
            ui.label("Vertices: one X,Y pair in mm per line. Rings close automatically.");
            let outer = ui.label("Outer vertices");
            ui.add(egui::TextEdit::multiline(&mut draft.outer).desired_width(380.0).desired_rows(5)).labelled_by(outer.id);
            let cutouts = ui.label("Cutout vertices");
            ui.add(egui::TextEdit::multiline(&mut draft.cutouts).desired_width(380.0).desired_rows(5)).labelled_by(cutouts.id);
            ui.label("Separate cutout rings with an empty line.");
            ui.collapsing("Import ASCII DXF", |ui| {
                let path = ui.label("DXF file path");
                ui.text_edit_singleline(&mut draft.dxf_path).labelled_by(path.id);
                ui.horizontal(|ui| {
                    ui.label("DXF units");
                    ui.selectable_value(&mut draft.dxf_units, DxfUnits::Header, "Drawing header");
                    ui.selectable_value(&mut draft.dxf_units, DxfUnits::Mm, "Millimetres");
                    ui.selectable_value(&mut draft.dxf_units, DxfUnits::Inches, "Inches");
                });
                let layer = ui.label("DXF layer (empty = all)");
                ui.text_edit_singleline(&mut draft.dxf_layer).labelled_by(layer.id);
                ui.label("Straight closed polylines or joined LINE loops. Curves require conversion.");
                import = ui.button("Import DXF outline").clicked();
            });
            if let Some(message) = &draft.message {
                ui.label(message);
            }
            ui.horizontal(|ui| {
                apply = ui.button("Apply outline").clicked();
                remove = ui.button("Remove outline").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || modal.should_close() {
            self.outlines.cancel();
        } else if apply || remove || import {
            let Some(design) = &self.design else { return };
            let result = (|| {
                if import {
                    let file = std::fs::File::open(&draft.dxf_path).map_err(|e| format!("Could not open DXF: {e}"))?;
                    let mut text = String::new();
                    file.take((MAX_DXF_BYTES + 1) as u64).read_to_string(&mut text).map_err(|e| format!("Could not read ASCII DXF: {e}"))?;
                    let layer = if draft.dxf_layer.is_empty() { None } else { Some(&draft.dxf_layer) };
                    return UiAction::ImportBoardOutlineDxf
                        .run(&design.engine, json!({"text":text,"units":draft.dxf_units,"layer":layer,"expected":draft.expected}))
                        .map_err(|e| e.to_string());
                }
                if remove {
                    return UiAction::SetBoardOutline
                        .run(&design.engine, json!({ "outline": null, "expected": draft.expected }))
                        .map_err(|e| e.to_string());
                }
                let outer = draft.outer.lines().filter(|line| !line.trim().is_empty()).collect::<Vec<_>>();
                let lines = draft.cutouts.lines().collect::<Vec<_>>();
                let cutouts =
                    lines.split(|line| line.trim().is_empty()).filter(|ring| !ring.is_empty()).map(parse_ring).collect::<Result<Vec<_>, _>>()?;
                let input = json!({ "outline": { "outer": parse_ring(&outer)?, "cutouts": cutouts }, "expected": draft.expected });
                UiAction::SetBoardOutline.run(&design.engine, input).map_err(|e| e.to_string())
            })();
            match result {
                Ok(_) => self.outlines.cancel(),
                Err(error) => draft.message = Some(error),
            }
        }
    }
}
