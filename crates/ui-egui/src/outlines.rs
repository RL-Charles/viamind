//! Native polygon outline form, shared with the MCP command.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use viamind_engine::ExpectedOutline;

struct OutlineDraft {
    outer: String,
    cutouts: String,
    expected: ExpectedOutline,
    message: Option<String>,
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
        if self.files.busy() || self.parts.placing() || self.outlines.editing() {
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
        self.outlines.draft = Some(OutlineDraft { outer, cutouts, expected, message: None });
    }
    pub(crate) fn outline_dialog(&mut self, ui: &mut Ui) {
        let Some(draft) = &mut self.outlines.draft else { return };
        let mut apply = false;
        let mut cancel = false;
        let mut remove = false;
        let modal = egui::Modal::new(egui::Id::new("board_outline_dialog")).show(ui.ctx(), |ui| {
            ui.heading("Board outline");
            ui.label("Vertices: one X,Y pair in mm per line. Rings close automatically.");
            let outer = ui.label("Outer vertices");
            ui.add(egui::TextEdit::multiline(&mut draft.outer).desired_width(380.0).desired_rows(5)).labelled_by(outer.id);
            let cutouts = ui.label("Cutout vertices");
            ui.add(egui::TextEdit::multiline(&mut draft.cutouts).desired_width(380.0).desired_rows(5)).labelled_by(cutouts.id);
            ui.label("Separate cutout rings with an empty line.");
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
        } else if apply || remove {
            let Some(design) = &self.design else { return };
            let result = (|| {
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
