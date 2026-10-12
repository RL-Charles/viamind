//! Local exact opening entry shared by detached and placed-pad editors.
use crate::{actions::UiAction, symbol_library::millimetres};
use viamind_doc::{ApertureRect, ObjectId, PadApertures};
use viamind_geom::{Nm, Point};

#[derive(Default)]
pub(crate) struct ApertureEditor {
    widget: ObjectId,
    explicit: bool,
    mask: bool,
    paste: bool,
    fields: [[String; 2]; 2],
    margin: String,
    message: Option<String>,
}
fn decimal(n: Nm) -> String {
    let n = i128::from(n.0);
    let f = format!("{:06}", n.abs() % 1_000_000);
    let f = f.trim_end_matches('0');
    format!("{}{}{}{}", if n < 0 { "-" } else { "" }, n.abs() / 1_000_000, if f.is_empty() { "" } else { "." }, f)
}
impl ApertureEditor {
    pub(crate) fn load(policy: Option<&PadApertures>) -> Self {
        let Some(p) = policy else { return Self::default() };
        Self {
            explicit: true,
            mask: p.mask.is_some(),
            paste: p.paste.is_some(),
            fields: [p.mask, p.paste].map(|r| r.map(|r| [decimal(r.width), decimal(r.height)]).unwrap_or_default()),
            ..Default::default()
        }
    }
    pub(crate) fn text_bytes(&self) -> Result<usize, String> {
        let mut n = 0usize;
        for s in self.fields.iter().flatten().chain(std::iter::once(&self.margin)) {
            if s.len() > 64 {
                return Err("Opening decimal fields support at most64 bytes".into());
            }
            n = n.saturating_add(s.len());
        }
        Ok(n)
    }
    pub(crate) fn definition(&self, center: Point) -> Result<Option<PadApertures>, String> {
        self.text_bytes()?;
        if !self.explicit {
            return Ok(None);
        }
        let parse = |i: usize, enabled: bool| -> Result<Option<ApertureRect>, String> {
            if !enabled {
                return Ok(None);
            }
            let r = ApertureRect { width: millimetres(&self.fields[i][0])?, height: millimetres(&self.fields[i][1])? };
            r.validate_at(center)?;
            Ok(Some(r))
        };
        Ok(Some(PadApertures { mask: parse(0, self.mask)?, paste: parse(1, self.paste)? }))
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, copper: Option<(Nm, Nm)>) -> bool {
        let mut changed = ui.checkbox(&mut self.explicit, "Explicit pad openings").changed();
        if !self.explicit {
            ui.label("Legacy mask behavior; no inferred paste. Enable to enter absolute rectangles or intentional omissions.");
            return changed;
        }
        ui.label(
            "Centered pad-local rectangles, millimetres. Unchecked opening is intentionally omitted; dimensions remain independent of copper resize.",
        );
        for (i, (key, enabled)) in [("Mask", &mut self.mask), ("Paste", &mut self.paste)].into_iter().enumerate() {
            changed |= ui.checkbox(enabled, format!("{key} rectangle")).changed();
            if *enabled {
                for (j, axis) in ["width", "height"].into_iter().enumerate() {
                    let label = ui.label(format!("{key} {axis}"));
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.fields[i][j])
                                .id(egui::Id::new(("aperture", self.widget, key, axis)))
                                .desired_width(180.0)
                                .char_limit(64),
                        )
                        .labelled_by(label.id)
                        .changed();
                }
            }
        }
        let label = ui.label("Equal margin calculator (signed mm)");
        changed |= ui
            .add(egui::TextEdit::singleline(&mut self.margin).id(egui::Id::new(("aperture-margin", self.widget))).desired_width(180.0).char_limit(64))
            .labelled_by(label.id)
            .changed();
        for (i, key) in ["Mask", "Paste"].into_iter().enumerate() {
            if ui.add_enabled(copper.is_some(), egui::Button::new(format!("Calculate {key} dimensions from copper"))).clicked() {
                let result = (|| {
                    let (w, h) = copper.ok_or("Enter valid copper dimensions first")?;
                    let r = ApertureRect::equal_margin(w, h, millimetres(&self.margin)?)?;
                    Ok::<_, String>([decimal(r.width), decimal(r.height)])
                })();
                match result {
                    Ok(values) => {
                        self.fields[i] = values;
                        if i == 0 {
                            self.mask = true;
                        } else {
                            self.paste = true;
                        }
                        changed = true;
                        self.message = None;
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        }
        if let Some(m) = &self.message {
            ui.label(m);
        }
        changed
    }
}
pub(crate) struct ApertureDraft {
    project: ObjectId,
    cursor: viamind_engine::EventCursor,
    expected: viamind_doc::Part,
    pad_id: ObjectId,
    editor: ApertureEditor,
    message: Option<String>,
}
impl crate::ViamindApp {
    pub fn begin_edit_pad_apertures(&mut self, part: viamind_doc::Part) {
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
        let Some(pads) = &part.pads else { return };
        if pads.0.is_empty() || pads.0.len() > 1024 {
            return;
        }
        let Some(design) = &self.design else { return };
        let pad = &pads.0[0];
        self.pads.apertures = Some(ApertureDraft {
            project: design.engine.read(|p| p.id),
            cursor: design.engine.event_cursor(),
            pad_id: pad.id,
            editor: ApertureEditor::load(pad.apertures.as_ref()),
            expected: part,
            message: None,
        });
    }
    pub(crate) fn pad_apertures_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(d) = &mut self.pads.apertures else { return };
        let (mut apply, mut reload, mut cancel) = (false, false, false);
        let mut chosen = None;
        let width = (ui.ctx().content_rect().width() - 32.0).clamp(240.0, 600.0);
        let height = (ui.ctx().content_rect().height() - 150.0).max(80.0);
        let modal = egui::Modal::new(egui::Id::new("pad-aperture-editor")).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.heading(format!("Pad openings - {}", d.expected.designator));
            ui.horizontal(|ui| {
                apply = ui.button("Apply pad openings").clicked();
                reload = ui.button("Reload pad openings").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if let Some(m) = &d.message {
                ui.label(m);
            }
            egui::ScrollArea::vertical().id_salt("pad-opening-fields").max_height(height).show(ui, |ui| {
                egui::ComboBox::from_id_salt("pad-opening-selection")
                    .selected_text(d.expected.pads.as_ref().unwrap().0.iter().find(|p| p.id == d.pad_id).map_or("Missing pad", |p| p.number.as_str()))
                    .show_ui(ui, |ui| {
                        for p in &d.expected.pads.as_ref().unwrap().0 {
                            if ui.selectable_label(p.id == d.pad_id, format!("Pad {}", p.number)).clicked() {
                                chosen = Some(p.id);
                            }
                        }
                    });
                if let Some(p) = d.expected.pads.as_ref().unwrap().0.iter().find(|p| p.id == d.pad_id) {
                    ui.push_id(d.pad_id, |ui| d.editor.show(ui, Some((p.width, p.height))));
                }
            });
        });
        if cancel || modal.should_close() {
            self.pads.cancel();
            return;
        }
        if let Some(id) = chosen
            && let Some(p) = d.expected.pads.as_ref().unwrap().0.iter().find(|p| p.id == id)
        {
            d.pad_id = id;
            d.editor = ApertureEditor::load(p.apertures.as_ref());
            d.message = None;
        }
        if reload {
            let id = d.expected.id;
            let part = self.design.as_ref().and_then(|design| design.engine.read(|p| p.board.part(&id).cloned()));
            self.pads.cancel();
            if let Some(p) = part {
                self.begin_edit_pad_apertures(p);
            }
            return;
        }
        if apply {
            let result = (|| {
                let pad = d.expected.pads.as_ref().unwrap().0.iter().find(|p| p.id == d.pad_id).ok_or("Pad missing")?;
                let apertures = d.editor.definition(pad.position)?;
                let design = self.design.as_ref().ok_or("No design")?;
                let input = viamind_engine::SetPadApertures {
                    expected_project: d.project,
                    expected_cursor: d.cursor,
                    expected: d.expected.clone(),
                    pad_id: d.pad_id,
                    apertures,
                };
                UiAction::SetPadApertures.run(&design.engine, serde_json::to_value(input).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })();
            match result {
                Ok(_) => self.pads.cancel(),
                Err(e) => d.message = Some(e),
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_entry_has_no_defaults_and_preserves_absolute_values_and_stable_owner() {
        let mut e = ApertureEditor::default();
        assert_eq!(e.definition(Point::ORIGIN).unwrap(), None);
        e.explicit = true;
        assert_eq!(e.definition(Point::ORIGIN).unwrap(), Some(PadApertures { mask: None, paste: None }));
        e.mask = true;
        assert!(e.definition(Point::ORIGIN).is_err());
        e.fields[0] = ["0.000003".into(), "0.000005".into()];
        let p = e.definition(Point::ORIGIN).unwrap().unwrap();
        assert_eq!(p.mask, Some(ApertureRect { width: Nm(3), height: Nm(5) }));
        let loaded = ApertureEditor::load(Some(&p));
        assert_ne!(e.widget, loaded.widget);
        assert_eq!(loaded.definition(Point::ORIGIN).unwrap(), Some(p));
        e.fields[0][0] = "NaN".into();
        assert!(e.definition(Point::ORIGIN).is_err());
        e.fields[0][0] = "x".repeat(65);
        assert!(e.text_bytes().is_err());
    }
}
