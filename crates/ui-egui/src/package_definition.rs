//! Detached explicit package editing. Local widget identities never become design identities.
use crate::{
    symbol_definition::{DefinitionEditor, decimal},
    symbol_library::millimetres,
};
use viamind_doc::{
    ObjectId,
    schematic::{Coordinate, PinType},
    symbol_library::{SymbolTemplate, TemplatePin},
    symbol_package::{MAX_PINS, MAX_TEXT, MAX_UNITS, SymbolPackageTemplate, UnitRole, UnitTemplate},
};
use viamind_engine::UnitPose;
use viamind_geom::Nm;

pub(crate) struct UnitDraft {
    widget: ObjectId,
    key: String,
    name: String,
    role: UnitRole,
    geometry: DefinitionEditor,
    x: String,
    y: String,
    quarters: u8,
    mirrored: bool,
}
pub(crate) struct PackageEditor {
    widget: ObjectId,
    name: String,
    value: String,
    footprint: String,
    units: Vec<UnitDraft>,
    selected: ObjectId,
}
fn geometry(unit: &UnitTemplate) -> DefinitionEditor {
    DefinitionEditor::from_template(&SymbolTemplate {
        name: "U".into(),
        value: String::new(),
        footprint: None,
        width: unit.width,
        height: unit.height,
        pins: unit.pins.clone(),
    })
}
fn original_unit(key: &str, number: usize) -> UnitTemplate {
    UnitTemplate {
        key: key.into(),
        name: format!("Unit {key}"),
        role: UnitRole::Functional,
        width: Nm(10_000_000),
        height: Nm(10_000_000),
        pins: vec![TemplatePin {
            number: number.to_string(),
            name: String::new(),
            position: Coordinate { x: Nm(-5_000_000), y: Nm(0) },
            electrical: PinType::Passive,
        }],
    }
}
impl PackageEditor {
    pub(crate) fn new() -> Self {
        Self::from_template(&SymbolPackageTemplate {
            name: "My new package".into(),
            value: String::new(),
            footprint: None,
            units: vec![original_unit("A", 1), original_unit("B", 2)],
        })
    }
    pub(crate) fn from_template(template: &SymbolPackageTemplate) -> Self {
        let units: Vec<_> = template
            .units
            .iter()
            .enumerate()
            .map(|(i, u)| UnitDraft {
                widget: ObjectId::new(),
                key: u.key.clone(),
                name: u.name.clone(),
                role: u.role,
                geometry: geometry(u),
                x: (i * 20).to_string(),
                y: "0".into(),
                quarters: 0,
                mirrored: false,
            })
            .collect();
        let selected = units[0].widget;
        Self {
            widget: ObjectId::new(),
            name: template.name.clone(),
            value: template.value.clone(),
            footprint: template.footprint.clone().unwrap_or_default(),
            units,
            selected,
        }
    }
    fn text_budget(&self) -> Result<(), String> {
        if !(2..=MAX_UNITS).contains(&self.units.len()) {
            return Err("A complete package requires 2..32 units".into());
        }
        let mut pins = 0usize;
        for u in &self.units {
            pins = pins.saturating_add(u.geometry.pin_count());
            if u.geometry.pin_count() == 0 || pins > MAX_PINS {
                return Err("A package requires at least one pin per unit and at most1024 total pins".into());
            }
        }
        let mut bytes = 0usize;
        for text in [&self.name, &self.value, &self.footprint]
            .into_iter()
            .map(String::as_str)
            .chain(self.units.iter().flat_map(|u| [u.key.as_str(), u.name.as_str()].into_iter().chain(u.geometry.pin_text())))
        {
            bytes = bytes.saturating_add(text.len());
            if text.len() > 4096 || bytes > MAX_TEXT {
                return Err("Package text exceeds4096UTF8 bytes per field or256KiB aggregate".into());
            }
        }
        Ok(())
    }
    pub(crate) fn template(&self) -> Result<SymbolPackageTemplate, String> {
        self.text_budget()?;
        // Borrowed text/count admission above precedes every per-unit typed copy.
        let units = self
            .units
            .iter()
            .map(|u| {
                let shape = u.geometry.template()?;
                Ok(UnitTemplate {
                    key: u.key.clone(),
                    name: u.name.clone(),
                    role: u.role,
                    width: shape.width,
                    height: shape.height,
                    pins: shape.pins,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let t = SymbolPackageTemplate {
            name: self.name.clone(),
            value: self.value.clone(),
            footprint: (!self.footprint.is_empty()).then(|| self.footprint.clone()),
            units,
        };
        t.validate()?;
        Ok(t)
    }
    pub(crate) fn placements(&self, template: &SymbolPackageTemplate) -> Result<Vec<UnitPose>, String> {
        if self.units.len() != template.units.len() {
            return Err("Definition changed; prepare complete unit poses again".into());
        }
        self.units
            .iter()
            .zip(&template.units)
            .map(|(u, t)| {
                let position = Coordinate { x: millimetres(&u.x)?, y: millimetres(&u.y)? };
                SymbolPackageTemplate::validate_unit_pose(t, position, u.quarters, u.mirrored)?;
                Ok(UnitPose { key: u.key.clone(), position, rotation_quarters: u.quarters, mirrored: u.mirrored })
            })
            .collect()
    }
    fn add_unit(&mut self) {
        if self.units.len() >= MAX_UNITS || self.units.iter().map(|u| u.geometry.pin_count()).sum::<usize>() >= MAX_PINS {
            return;
        }
        let key = (b'A'..=b'Z')
            .map(|c| char::from(c).to_string())
            .chain((1..=32).map(|n| format!("U{n}")))
            .find(|key| !self.units.iter().any(|u| &u.key == key))
            .expect("bounded free unit key");
        let used: std::collections::BTreeSet<_> = self.units.iter().flat_map(|u| u.geometry.pin_text().step_by(2)).collect();
        let number = (1..=MAX_PINS + 1).find(|n| !used.contains(n.to_string().as_str())).expect("bounded free package pin");
        let t = original_unit(&key, number);
        let widget = ObjectId::new();
        self.units.push(UnitDraft {
            widget,
            key,
            name: t.name.clone(),
            role: t.role,
            geometry: geometry(&t),
            x: (self.units.len() * 20).to_string(),
            y: "0".into(),
            quarters: 0,
            mirrored: false,
        });
        self.selected = widget;
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, ready: bool) {
        let total = self.units.iter().map(|u| u.geometry.pin_count()).sum::<usize>();
        ui.heading("Detached package definition");
        ui.label(format!("{} /32 ordered units; {total} /1024 explicit pins", self.units.len()));
        ui.label("Functional/power roles are descriptive. Pins connect only through actual endpoints and wires/labels.");
        ui.add_enabled_ui(ready, |ui| {
            text(ui, self.widget, "Package definition name", "name", &mut self.name, 128);
            text(ui, self.widget, "Package default value", "value", &mut self.value, 1024);
            text(ui, self.widget, "Package footprint (optional)", "footprint", &mut self.footprint, 128);
            let current = self.units.iter().find(|u| u.widget == self.selected).map_or("Choose unit".into(), |u| format!("{} ({})", u.key, u.name));
            let label = ui.label("Package unit to edit");
            egui::ComboBox::from_id_salt((self.widget, "unit-selector"))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for u in &self.units {
                        ui.selectable_value(&mut self.selected, u.widget, format!("{} ({})", u.key, u.name));
                    }
                })
                .response
                .labelled_by(label.id);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.units.len() < MAX_UNITS && total < MAX_PINS, egui::Button::new("Add package unit")).clicked() {
                    self.add_unit();
                }
                if ui.add_enabled(self.units.len() > 2, egui::Button::new("Remove selected package unit")).clicked() {
                    self.units.retain(|u| u.widget != self.selected);
                    self.selected = self.units[0].widget;
                }
            });
            let total = self.units.iter().map(|u| u.geometry.pin_count()).sum::<usize>();
            if let Some(unit) = self.units.iter_mut().find(|u| u.widget == self.selected) {
                let available = MAX_PINS.saturating_sub(total.saturating_sub(unit.geometry.pin_count()));
                ui.scope_builder(egui::UiBuilder::new().id(egui::Id::new((self.widget, unit.widget, "unit"))), |ui| {
                    text(ui, unit.widget, "Unit key", "key", &mut unit.key, 8);
                    text(ui, unit.widget, "Unit display name", "name", &mut unit.name, 128);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Unit descriptive role");
                        ui.radio_value(&mut unit.role, UnitRole::Functional, "Functional unit");
                        ui.radio_value(&mut unit.role, UnitRole::Power, "Power unit");
                    });
                    unit.geometry.show_geometry(ui, available);
                    if let Ok(shape) = unit.geometry.template() {
                        preview(ui, &shape);
                    }
                });
            }
        });
    }
    pub(crate) fn show_poses(&mut self, ui: &mut egui::Ui, ready: bool) {
        ui.heading("Complete package placement");
        ui.label("Every unit is placed together. Enter an explicit origin/pose for each; no centroid or physical PCB placement is inferred.");
        ui.add_enabled_ui(ready, |ui| {
            egui::ScrollArea::vertical().id_salt((self.widget, "poses")).min_scrolled_height(100.).max_height(250.).show_rows(
                ui,
                100.,
                self.units.len(),
                |ui, rows| {
                    for i in rows {
                        let u = &mut self.units[i];
                        ui.allocate_ui(egui::vec2(ui.available_width(), 100.), |ui| {
                            ui.scope_builder(egui::UiBuilder::new().id(egui::Id::new((self.widget, u.widget, "pose"))), |ui| {
                                ui.label(format!("Unit {} ({})", u.key, u.name));
                                ui.horizontal_wrapped(|ui| {
                                    compact_text(ui, u.widget, &format!("Unit {} X (mm)", u.key), "x", &mut u.x);
                                    compact_text(ui, u.widget, &format!("Unit {} Y (mm)", u.key), "y", &mut u.y);
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("CCW quarter turns");
                                    ui.add(egui::DragValue::new(&mut u.quarters).range(0..=3));
                                    ui.checkbox(&mut u.mirrored, "Mirror unit local X");
                                });
                            });
                        });
                    }
                },
            );
        });
    }
}
fn compact_text(ui: &mut egui::Ui, id: ObjectId, label: &str, key: &str, value: &mut String) {
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).id(egui::Id::new((id, "pose", key))).char_limit(32).desired_width(100.)).labelled_by(l.id);
}
fn text(ui: &mut egui::Ui, id: ObjectId, label: &str, key: &str, value: &mut String, limit: usize) {
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).id(egui::Id::new((id, key))).char_limit(limit).desired_width(280.)).labelled_by(l.id);
}
fn preview(ui: &mut egui::Ui, t: &SymbolTemplate) {
    let (rect, painter) = ui.allocate_painter(egui::vec2(ui.available_width().min(450.), 180.), egui::Sense::hover());
    let rect = rect.rect;
    let center = rect.center();
    let extent = t
        .pins
        .iter()
        .fold(t.width.to_mm().max(t.height.to_mm()) / 2., |n, p| n.max(p.position.x.to_mm().abs()).max(p.position.y.to_mm().abs()))
        .max(0.000001);
    let scale = (f64::from(rect.width().min(rect.height())) - 40.) / (2. * extent);
    let point = |p: Coordinate| center + egui::vec2((p.x.to_mm() * scale) as f32, (p.y.to_mm() * scale) as f32);
    painter.rect_stroke(
        egui::Rect::from_center_size(center, egui::vec2((t.width.to_mm() * scale) as f32, (t.height.to_mm() * scale) as f32)),
        0.,
        egui::Stroke::new(1., egui::Color32::LIGHT_YELLOW),
        egui::StrokeKind::Inside,
    );
    painter.line_segment([center - egui::vec2(4., 0.), center + egui::vec2(4., 0.)], egui::Stroke::new(1., egui::Color32::LIGHT_BLUE));
    painter.line_segment([center - egui::vec2(0., 4.), center + egui::vec2(0., 4.)], egui::Stroke::new(1., egui::Color32::LIGHT_BLUE));
    for p in &t.pins {
        let at = point(p.position);
        painter.circle_stroke(at, 2., egui::Stroke::new(1., egui::Color32::LIGHT_GREEN));
        painter.text(at + egui::vec2(4., 0.), egui::Align2::LEFT_CENTER, &p.number, egui::FontId::proportional(11.), egui::Color32::LIGHT_GREEN);
    }
    ui.label(format!("Local origin cross; body {} x{} mm; pin endpoints/explicit numbers", decimal(t.width), decimal(t.height)));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> SymbolPackageTemplate {
        viamind_doc::symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json"))
            .unwrap()
    }
    #[test]
    fn all_five_units_roundtrip_exact_geometry_poses_and_stable_survivors() {
        let t = fixture();
        let mut e = PackageEditor::from_template(&t);
        assert_eq!(e.template().unwrap(), t);
        let poses = e.placements(&t).unwrap();
        assert_eq!(poses.len(), 5);
        assert_eq!(poses[4].position.x, Nm(80_000_000));
        e.units[4].x = "-0.000007".into();
        e.units[4].y = "1.000003".into();
        e.units[4].quarters = 3;
        e.units[4].mirrored = true;
        let poses = e.placements(&t).unwrap();
        assert_eq!(poses[4].position, Coordinate { x: Nm(-7), y: Nm(1_000_003) });
        let surviving = e.units[1].widget;
        e.units.remove(0);
        assert_eq!(e.units[0].widget, surviving);
        e.add_unit();
        let t = e.template().unwrap();
        assert_eq!(t.units.len(), 5);
        assert!(t.units.last().unwrap().pins[0].number == "1");
        assert_eq!(e.units[0].widget, surviving);
        e.units[0].x = "0.0000001".into();
        assert!(e.placements(&t).is_err());
    }
    #[test]
    fn borrowed_package_text_and_count_precede_geometry_copy() {
        let mut e = PackageEditor::new();
        e.name = "x".repeat(4097);
        e.units[0].x = "invalid".into();
        assert!(e.template().unwrap_err().contains("4096"));
        e.name = "valid".into();
        for _ in 0..40 {
            e.add_unit();
        }
        assert_eq!(e.units.len(), MAX_UNITS);
        e.template().unwrap();
    }
    #[test]
    fn actual_editor_unit_switch_preserves_other_units_and_identity() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000., 1600.))
            .build_ui_state(|ui, e: &mut PackageEditor| e.show(ui, true), PackageEditor::from_template(&fixture()));
        h.run_steps(3);
        let id = h.state().units[0].widget;
        h.get_by_label("Unit display name").click();
        h.run_steps(2);
        h.get_by_label("Unit display name").type_text(" edited");
        h.run_steps(2);
        assert!(h.state().units[0].name.contains("edited"));
        h.get_by_label("Package unit to edit").click();
        h.run_steps(2);
        h.get_by_label("P (Power)").click();
        h.run_steps(3);
        assert_eq!(h.state().selected, h.state().units[4].widget);
        assert_eq!(h.state().units[0].widget, id);
        assert!(h.state().units[0].name.contains("edited"));
        assert_eq!(h.state().units[4].geometry.template().unwrap().pins[0].electrical, PinType::PowerInput);
    }
}
