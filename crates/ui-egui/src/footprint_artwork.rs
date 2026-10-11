//! Local artwork entry reused by the detached library and guarded component editor.
use crate::symbol_library::millimetres;
use viamind_doc::{FootprintArtwork, ObjectId, RectangularCourtyard, SilkscreenSegment};
use viamind_geom::{Nm, Point};

struct LineRow {
    id: ObjectId,
    fields: [String; 5],
}
#[derive(Default)]
pub(crate) struct ArtworkEditor {
    explicit: bool,
    rows: Vec<LineRow>,
    courtyard: bool,
    courtyard_fields: [String; 5],
}
fn decimal(n: Nm) -> String {
    let n = i128::from(n.0);
    let f = format!("{:06}", n.abs() % 1_000_000);
    let f = f.trim_end_matches('0');
    format!("{}{}{}{}", if n < 0 { "-" } else { "" }, n.abs() / 1_000_000, if f.is_empty() { "" } else { "." }, f)
}
fn fields(a: Point, b: Point, width: Nm) -> [String; 5] {
    [a.x, a.y, b.x, b.y, width].map(decimal)
}
impl ArtworkEditor {
    pub(crate) fn load(artwork: Option<&FootprintArtwork>) -> Result<Self, String> {
        let Some(artwork) = artwork else { return Ok(Self::default()) };
        artwork.validate()?;
        Ok(Self {
            explicit: true,
            rows: artwork.silkscreen.iter().map(|s| LineRow { id: ObjectId::new(), fields: fields(s.start, s.end, s.width) }).collect(),
            courtyard: artwork.courtyard.is_some(),
            courtyard_fields: artwork.courtyard.as_ref().map(|c| fields(c.min, c.max, c.width)).unwrap_or_default(),
        })
    }
    pub(crate) fn definition(&self) -> Result<Option<Box<FootprintArtwork>>, String> {
        if !self.explicit {
            return Ok(None);
        }
        if self.rows.len() > 256 {
            return Err("At most 256 silkscreen lines".into());
        }
        let parse = |values: &[String; 5]| -> Result<(Point, Point, Nm), String> {
            let mut n = [Nm(0); 5];
            for (out, text) in n.iter_mut().zip(values) {
                if text.len() > 64 {
                    return Err("Artwork decimal field exceeds 64 bytes".into());
                }
                *out = millimetres(text)?;
            }
            Ok((Point::new(n[0], n[1]), Point::new(n[2], n[3]), n[4]))
        };
        let mut silkscreen = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let (start, end, width) = parse(&row.fields)?;
            silkscreen.push(SilkscreenSegment { start, end, width });
        }
        let courtyard = if self.courtyard {
            let (min, max, width) = parse(&self.courtyard_fields)?;
            Some(RectangularCourtyard { min, max, width })
        } else {
            None
        };
        let artwork = FootprintArtwork { silkscreen, courtyard };
        artwork.validate()?;
        Ok(Some(Box::new(artwork)))
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = ui.checkbox(&mut self.explicit, "Explicit footprint artwork").changed();
        ui.label(if self.explicit {
            "Stored lines replace the synthetic body. An empty list intentionally draws no silk."
        } else {
            "Legacy body preview; no stored silkscreen or courtyard."
        });
        if !self.explicit {
            return changed;
        }
        ui.label("Mounted side, local millimetres; no clipping or courtyard clearance checks.");
        changed |= ui.checkbox(&mut self.courtyard, "Rectangular courtyard").changed();
        if self.courtyard {
            for (key, value) in ["Courtyard min X", "Courtyard min Y", "Courtyard max X", "Courtyard max Y", "Courtyard stroke"]
                .into_iter()
                .zip(&mut self.courtyard_fields)
            {
                let label = ui.label(key);
                changed |= ui
                    .add(egui::TextEdit::singleline(value).id(ui.make_persistent_id(("artwork-courtyard", key))).desired_width(180.0).char_limit(64))
                    .labelled_by(label.id)
                    .changed();
            }
        }
        ui.label(format!("{} / 256 silkscreen lines", self.rows.len()));
        if ui.add_enabled(self.rows.len() < 256, egui::Button::new("Add blank silkscreen line")).clicked() {
            self.rows.push(LineRow { id: ObjectId::new(), fields: Default::default() });
            changed = true;
        }
        let row_height = ui.spacing().interact_size.y * 12.0 + 24.0;
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("artwork-line-rows").max_height(260.0).show_rows(ui, row_height, self.rows.len(), |ui, range| {
            for i in range {
                let row = &mut self.rows[i];
                ui.allocate_ui(egui::vec2(ui.available_width(), row_height), |ui| {
                    ui.push_id(row.id, |ui| {
                        for (key, value) in ["Start X", "Start Y", "End X", "End Y", "Stroke width"].into_iter().zip(&mut row.fields) {
                            let label = ui.label(format!("Silk {} {key}", i + 1));
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(value)
                                        .id(egui::Id::new(("artwork-line", row.id, key)))
                                        .desired_width(180.0)
                                        .char_limit(64),
                                )
                                .labelled_by(label.id)
                                .changed();
                        }
                        if ui.button(format!("Remove silkscreen line {}", i + 1)).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = remove {
            self.rows.remove(i);
            changed = true;
        }
        changed
    }
}

pub(crate) struct ArtworkDraft {
    pub project: ObjectId,
    pub cursor: viamind_engine::EventCursor,
    pub expected: viamind_doc::Part,
    pub editor: ArtworkEditor,
    pub message: Option<String>,
}
impl crate::ViamindApp {
    pub fn begin_edit_footprint_artwork(&mut self, part: viamind_doc::Part) {
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
        let Ok(editor) = ArtworkEditor::load(part.artwork.as_deref()) else { return };
        let (project, cursor) = design.engine.with_engine(|e| (e.project().id, e.event_cursor()));
        self.pads.artwork = Some(ArtworkDraft { project, cursor, expected: part, editor, message: None });
    }
    pub(crate) fn footprint_artwork_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(draft) = &mut self.pads.artwork else { return };
        let mut apply = false;
        let mut reload = false;
        let mut cancel = false;
        let width = (ui.ctx().content_rect().width() - 32.0).clamp(240.0, 620.0);
        let height = (ui.ctx().content_rect().height() - 160.0).max(80.0);
        let modal = egui::Modal::new(egui::Id::new("footprint-artwork-editor")).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.heading(format!("Footprint artwork - {}", draft.expected.designator));
            ui.horizontal(|ui| {
                apply = ui.button("Apply artwork").clicked();
                reload = ui.button("Reload artwork").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if let Some(message) = &draft.message {
                ui.label(message);
            }
            egui::ScrollArea::vertical().id_salt("component-artwork-fields").max_height(height).show(ui, |ui| {
                draft.editor.show(ui);
            });
        });
        if cancel || modal.should_close() {
            self.pads.cancel();
            return;
        }
        if reload {
            let id = draft.expected.id;
            let part = self.design.as_ref().and_then(|d| d.engine.read(|p| p.board.part(&id).cloned()));
            self.pads.cancel();
            if let Some(part) = part {
                self.begin_edit_footprint_artwork(part);
            }
            return;
        }
        if apply {
            let result = draft.editor.definition().and_then(|artwork| {
                let design = self.design.as_ref().ok_or("No active design")?;
                let command = viamind_engine::SetFootprintArtwork {
                    expected_project: draft.project,
                    expected_cursor: draft.cursor,
                    expected: draft.expected.clone(),
                    artwork,
                };
                crate::actions::UiAction::SetFootprintArtwork
                    .run(&design.engine, serde_json::to_value(command).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            });
            match result {
                Ok(_) => self.pads.cancel(),
                Err(error) => draft.message = Some(error),
            }
        }
    }
}
