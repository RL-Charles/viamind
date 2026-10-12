//! Exact local mechanical-hole drafts shared by detached and placed editors.
use crate::{ViamindApp, actions::UiAction, symbol_library::millimetres};
use viamind_doc::{FootprintHole, ObjectId, Part, PartHole};
use viamind_geom::{Nm, Point};
struct Row {
    widget: ObjectId,
    id: Option<ObjectId>,
    fields: [String; 4],
}
#[derive(Default)]
pub(crate) struct HoleEditor {
    explicit: bool,
    rows: Vec<Row>,
    selected: Option<ObjectId>,
}
fn decimal(n: Nm) -> String {
    let n = i128::from(n.0);
    let f = format!("{:06}", n.abs() % 1_000_000);
    let f = f.trim_end_matches('0');
    format!("{}{}{}{}", if n < 0 { "-" } else { "" }, n.abs() / 1_000_000, if f.is_empty() { "" } else { "." }, f)
}
impl HoleEditor {
    fn load_rows(rows: Option<impl Iterator<Item = (Option<ObjectId>, FootprintHole)>>) -> Self {
        let explicit = rows.is_some();
        let rows: Vec<_> = rows
            .into_iter()
            .flatten()
            .map(|(id, h)| Row { widget: ObjectId::new(), id, fields: [h.key, decimal(h.center.x), decimal(h.center.y), decimal(h.diameter)] })
            .collect();
        let selected = rows.first().map(|r| r.widget);
        Self { explicit, rows, selected }
    }
    pub(crate) fn load(holes: Option<&[FootprintHole]>) -> Self {
        Self::load_rows(holes.map(|h| h.iter().map(|h| (None, h.clone()))))
    }
    fn load_part(part: &Part) -> Self {
        Self::load_rows(part.holes.as_ref().map(|h| h.iter().map(|h| (Some(h.id), h.detached()))))
    }
    pub(crate) fn text_bytes(&self) -> Result<usize, String> {
        if self.rows.len() > 256 {
            return Err("At most256 circular nonplated holes".into());
        }
        let mut bytes = 0usize;
        for row in &self.rows {
            for text in &row.fields {
                if text.len() > 64 {
                    return Err("Hole keys and exact decimal fields support at most64 bytes".into());
                }
                bytes = bytes.checked_add(text.len()).ok_or("Hole text budget overflow")?;
            }
        }
        Ok(bytes)
    }
    pub(crate) fn definition(&self) -> Result<Option<Vec<FootprintHole>>, String> {
        self.text_bytes()?;
        if !self.explicit {
            return Ok(None);
        }
        let mut holes = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let h = FootprintHole {
                key: row.fields[0].clone(),
                center: Point::new(millimetres(&row.fields[1])?, millimetres(&row.fields[2])?),
                diameter: millimetres(&row.fields[3])?,
            };
            h.validate()?;
            holes.push(h);
        }
        viamind_doc::footprint_holes::validate_footprint_holes(&holes)?;
        Ok(Some(holes))
    }
    fn add(&mut self) {
        if self.rows.len() < 256 {
            let widget = ObjectId::new();
            self.rows.push(Row { widget, id: None, fields: Default::default() });
            self.selected = Some(widget);
        }
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = ui.checkbox(&mut self.explicit, "Explicit nonplated holes").changed();
        if !self.explicit {
            ui.label("Legacy: no authored hole policy. Enable Explicit to enter drills or deliberately clear all holes.");
            return changed;
        }
        ui.label("Circular nonplated through holes. Local millimetres; mechanical keys are independent of pad numbers.");
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(self.rows.len() < 256, egui::Button::new("Add blank hole")).clicked() {
                self.add();
                changed = true;
            }
            if ui.button("Clear all holes").clicked() {
                self.rows.clear();
                self.selected = None;
                changed = true;
            }
            ui.label(format!("{} /256 holes", self.rows.len()));
        });
        if self.rows.is_empty() {
            ui.label("Explicit no holes (saved as an empty array).");
            return changed;
        }
        egui::ComboBox::from_id_salt("mechanical-hole-row")
            .selected_text(
                self.selected
                    .and_then(|id| self.rows.iter().position(|r| r.widget == id))
                    .map_or("Choose hole".into(), |i| format!("Hole {}: {}", i + 1, self.rows[i].fields[0])),
            )
            .show_ui(ui, |ui| {
                for (i, row) in self.rows.iter().enumerate() {
                    ui.selectable_value(&mut self.selected, Some(row.widget), format!("Hole {}: {}", i + 1, row.fields[0]));
                }
            });
        let mut remove = None;
        if let Some((index, row)) = self.rows.iter_mut().enumerate().find(|(_, r)| Some(r.widget) == self.selected) {
            for (i, label) in ["Hole key", "Hole X (mm)", "Hole Y (mm)", "Hole diameter (mm)"].into_iter().enumerate() {
                let label = ui.label(label);
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut row.fields[i])
                            .id(egui::Id::new(("nonplated-hole-field", row.widget, i)))
                            .desired_width(180.0)
                            .char_limit(64),
                    )
                    .labelled_by(label.id)
                    .changed();
            }
            if ui.button("Remove selected hole").clicked() {
                remove = Some(index);
            }
        }
        if let Some(index) = remove {
            self.rows.remove(index);
            self.selected = self.rows.get(index.min(self.rows.len().saturating_sub(1))).map(|r| r.widget);
            changed = true;
        }
        changed
    }
}
pub(crate) struct HoleDraft {
    project: ObjectId,
    cursor: viamind_engine::EventCursor,
    expected: Part,
    editor: HoleEditor,
    message: Option<String>,
}
impl ViamindApp {
    pub fn begin_edit_part_holes(&mut self, part: Part) {
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
        let admitted = design.engine.read(viamind_doc::footprint_holes::validate_physical_identities);
        if admitted.is_err() || part.holes.as_ref().is_some_and(|h| viamind_doc::footprint_holes::validate_part_holes(h).is_err()) {
            return;
        }
        self.pads.holes = Some(HoleDraft {
            project: design.engine.read(|p| p.id),
            cursor: design.engine.event_cursor(),
            editor: HoleEditor::load_part(&part),
            expected: part,
            message: None,
        });
    }
    pub(crate) fn part_holes_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(d) = &mut self.pads.holes else { return };
        let (mut apply, mut reload, mut cancel) = (false, false, false);
        let width = (ui.ctx().content_rect().width() - 32.0).clamp(240.0, 600.0);
        let height = (ui.ctx().content_rect().height() - 150.0).max(80.0);
        let modal = egui::Modal::new(egui::Id::new("part-holes-editor")).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.heading(format!("Nonplated holes - {}", d.expected.designator));
            ui.horizontal_wrapped(|ui| {
                apply = ui.button("Apply holes").clicked();
                reload = ui.button("Reload holes").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if let Some(m) = &d.message {
                ui.label(m);
            }
            egui::ScrollArea::vertical().id_salt("part-hole-content").max_height(height).show(ui, |ui| {
                d.editor.show(ui);
            });
        });
        if cancel || modal.should_close() {
            self.pads.cancel();
            return;
        }
        if reload {
            let id = d.expected.id;
            let part = self.design.as_ref().and_then(|design| design.engine.read(|p| p.board.part(&id).cloned()));
            self.pads.cancel();
            if let Some(part) = part {
                self.begin_edit_part_holes(part);
            }
            return;
        }
        if apply {
            let result = (|| {
                let holes = d.editor.definition()?;
                let design = self.design.as_ref().ok_or("No design")?;
                design.engine.read(|p| {
                    viamind_doc::footprint_holes::preflight_footprint_replacement(
                        p,
                        d.expected.id,
                        d.expected.pads.as_ref().map_or(0, |p| p.0.len()),
                        holes.as_deref(),
                    )
                })?;
                // Parsed geometry and borrowed project/prospective limits precede model UUID creation.
                let holes = holes.map(|holes| {
                    holes
                        .into_iter()
                        .zip(&d.editor.rows)
                        .map(|(h, row)| PartHole { id: row.id.unwrap_or_else(ObjectId::new), key: h.key, center: h.center, diameter: h.diameter })
                        .collect()
                });
                let request =
                    viamind_engine::SetPartHoles { expected_project: d.project, expected_cursor: d.cursor, expected: d.expected.clone(), holes };
                UiAction::SetPartHoles.run(&design.engine, serde_json::to_value(request).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
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
    use egui_kittest::{Harness, kittest::Queryable};
    use viamind_engine::{EngineHandle, Origin};
    fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) {
        h.get_by_label(label).scroll_to_me();
        h.run_steps(10);
        let rect = h.get_by_label(label).rect();
        h.hover_at(rect.center());
        h.run_steps(2);
        let rect = h.get_by_label(label).rect();
        let clip = h
            .output()
            .shapes
            .iter()
            .filter_map(|s| {
                let matches = match &s.shape {
                    egui::Shape::Text(t) => t.galley.text() == label,
                    egui::Shape::Rect(r) => r.rect.min.distance(rect.min) <= 0.5 && r.rect.max.distance(rect.max) <= 0.5,
                    _ => false,
                };
                matches.then_some(s.clip_rect.intersect(h.ctx.content_rect()))
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap_or_else(|| panic!("missing current paint for {label}"));
        assert!(clip.contains_rect(rect), "{label}: response {rect:?} outside effective clip {clip:?}");
    }
    fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
        reveal(h, label);
        h.get_by_label(label).click();
        h.run_steps(5);
    }
    fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) -> egui::Id {
        reveal(h, label);
        h.get_by_label(label).focus();
        for pressed in [true, false] {
            h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
            h.run_steps(2);
        }
        h.get_by_label(label).type_text(value);
        h.run_steps(3);
        h.ctx.memory(egui::Memory::focused).unwrap()
    }
    fn fixture() -> (ViamindApp, EngineHandle, Part) {
        let p = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
        let e = EngineHandle::new(p);
        let part = e.read(|p| p.board.parts.values().next().unwrap().clone());
        (ViamindApp::with_design(e.clone(), None), e, part)
    }
    #[test]
    fn exact_local_entry_distinguishes_presence_and_has_no_inferred_geometry() {
        let mut editor = HoleEditor::default();
        assert_eq!(editor.definition().unwrap(), None);
        editor.explicit = true;
        assert_eq!(editor.definition().unwrap(), Some(vec![]));
        editor.add();
        assert!(editor.definition().is_err());
        editor.rows[0].fields = ["mount.A".into(), "-0.000003".into(), "0.000005".into(), "0.000009".into()];
        let expected = FootprintHole { key: "mount.A".into(), center: Point::new(Nm(-3), Nm(5)), diameter: Nm(9) };
        assert_eq!(editor.definition().unwrap(), Some(vec![expected.clone()]));
        assert_eq!(HoleEditor::load(Some(std::slice::from_ref(&expected))).definition().unwrap(), Some(vec![expected]));
        editor.rows[0].fields[3] = "NaN".into();
        assert!(editor.definition().is_err());
        editor.rows[0].fields[0] = "x".repeat(65);
        assert!(editor.text_bytes().is_err());
    }
    #[test]
    fn compact_actual_input_apply_undo_cancel_and_peer_cursor_guard_preserve_other_fields() {
        let (mut app, e, part) = fixture();
        let before = e.session_state();
        let status = e.document_status();
        app.begin_edit_part_holes(part.clone());
        let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
        h.run_steps(6);
        click(&mut h, "Explicit nonplated holes");
        click(&mut h, "Add blank hole");
        for (label, value) in [("Hole key", "mount.A"), ("Hole X (mm)", "-0.000003"), ("Hole Y (mm)", "0.000005"), ("Hole diameter (mm)", "0.000009")]
        {
            input(&mut h, label, value);
        }
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
        click(&mut h, "Apply holes");
        assert!(!h.state().pads.editing());
        let after = e.session_state();
        let mut expected = before.project.clone();
        let holes = after.project.board.parts[&part.id].holes.clone().unwrap();
        assert_eq!((holes[0].key.as_str(), holes[0].center, holes[0].diameter), ("mount.A", Point::new(Nm(-3), Nm(5)), Nm(9)));
        expected.board.parts.get_mut(&part.id).unwrap().holes = Some(holes);
        assert_eq!(after.project, expected);
        assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        h.state_mut().begin_edit_part_holes(part.clone());
        h.run_steps(5);
        click(&mut h, "Explicit nonplated holes");
        click(&mut h, "Cancel");
        assert_eq!(e.read(Clone::clone), before.project);
        h.state_mut().begin_edit_part_holes(part);
        h.run_steps(5);
        click(&mut h, "Explicit nonplated holes");
        let peer = Origin::Agent { id: "hole peer".into() };
        e.execute("add_part", serde_json::json!({"designator":"PEER121","value":"","footprint":"legacy","x_mm":1,"y_mm":2}), peer.clone()).unwrap();
        e.undo(peer).unwrap();
        let stale = e.session_state();
        click(&mut h, "Apply holes");
        assert_eq!(e.session_state(), stale);
        assert!(h.state().pads.holes.as_ref().unwrap().message.is_some());
        click(&mut h, "Reload holes");
        assert!(!h.state().pads.holes.as_ref().unwrap().editor.explicit);
        click(&mut h, "Cancel");
        assert_eq!(e.session_state(), stale);
    }
    #[test]
    fn actual_text_edit_identity_survives_removal_of_preceding_hole() {
        let (mut app, _, part) = fixture();
        app.begin_edit_part_holes(part);
        let editor = &mut app.pads.holes.as_mut().unwrap().editor;
        editor.explicit = true;
        editor.add();
        editor.rows[0].fields = ["first".into(), "0".into(), "0".into(), "1".into()];
        editor.add();
        editor.rows[1].fields = ["second".into(), "0".into(), "0".into(), "1".into()];
        let selected = editor.rows[1].widget;
        let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
        h.run_steps(6);
        let id = input(&mut h, "Hole Y (mm)", "0.000005");
        h.get_by_value("Hole 2: second").scroll_to_me();
        h.run_steps(10);
        let rect = h.get_by_value("Hole 2: second").rect();
        h.hover_at(rect.center());
        h.run_steps(2);
        let rect = h.get_by_value("Hole 2: second").rect();
        let clip = h
            .output()
            .shapes
            .iter()
            .filter_map(|s| {
                let egui::Shape::Rect(r) = &s.shape else { return None };
                (r.rect.min.distance(rect.min) <= 0.5 && r.rect.max.distance(rect.max) <= 0.5).then_some(s.clip_rect.intersect(h.ctx.content_rect()))
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("current combo paint");
        assert!(clip.contains_rect(rect));
        h.get_by_value("Hole 2: second").click();
        h.run_steps(5);
        click(&mut h, "Hole 1: first");
        click(&mut h, "Remove selected hole");
        assert_eq!(h.state().pads.holes.as_ref().unwrap().editor.selected, Some(selected));
        h.run_steps(6);
        assert_eq!(input(&mut h, "Hole Y (mm)", "0.000007"), id);
        let editor = &h.state().pads.holes.as_ref().unwrap().editor;
        assert_eq!(editor.rows[0].fields[0], "second");
        assert_eq!(editor.definition().unwrap().unwrap()[0].center.y, Nm(7));
    }
}
