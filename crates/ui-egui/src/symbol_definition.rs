//! Bounded local definition editing; engine changes require explicit guarded placement.
use crate::symbol_library::millimetres;
use viamind_doc::{
    ObjectId,
    schematic::{Coordinate, PinType},
    symbol_library::{MAX_PINS, MAX_TEXT_BYTES, SymbolTemplate, TemplatePin},
};
use viamind_engine::SaveSymbolTemplate;
use viamind_geom::Nm;

pub(crate) struct PinDraft {
    widget: ObjectId,
    number: String,
    name: String,
    x: String,
    y: String,
    electrical: PinType,
}
pub(crate) struct DefinitionEditor {
    widget: ObjectId,
    name: String,
    value: String,
    footprint: String,
    width: String,
    height: String,
    pins: Vec<PinDraft>,
    path: String,
    error: Option<String>,
}
pub(crate) enum Decision {
    Use(SymbolTemplate),
    Save(SaveSymbolTemplate),
    Cancel,
}
pub(crate) fn decimal(value: Nm) -> String {
    let n = i128::from(value.0);
    let magnitude = n.abs();
    let fraction = format!("{:06}", magnitude % 1_000_000);
    let fraction = fraction.trim_end_matches('0');
    format!("{}{}{}{}", if n < 0 { "-" } else { "" }, magnitude / 1_000_000, if fraction.is_empty() { "" } else { "." }, fraction)
}
impl DefinitionEditor {
    pub(crate) fn new() -> Self {
        Self::from_template(&SymbolTemplate {
            name: "My new symbol".into(),
            value: String::new(),
            footprint: None,
            width: Nm(10_000_000),
            height: Nm(10_000_000),
            pins: vec![TemplatePin {
                number: "1".into(),
                name: String::new(),
                position: Coordinate { x: Nm(-5_000_000), y: Nm(0) },
                electrical: PinType::Passive,
            }],
        })
    }
    pub(crate) fn from_template(template: &SymbolTemplate) -> Self {
        Self {
            widget: ObjectId::new(),
            name: template.name.clone(),
            value: template.value.clone(),
            footprint: template.footprint.clone().unwrap_or_default(),
            width: decimal(template.width),
            height: decimal(template.height),
            pins: template
                .pins
                .iter()
                .map(|pin| PinDraft {
                    widget: ObjectId::new(),
                    number: pin.number.clone(),
                    name: pin.name.clone(),
                    x: decimal(pin.position.x),
                    y: decimal(pin.position.y),
                    electrical: pin.electrical,
                })
                .collect(),
            path: String::new(),
            error: None,
        }
    }
    pub(crate) fn template(&self) -> Result<SymbolTemplate, String> {
        if self.pins.len() > MAX_PINS {
            return Err("At most 1024 pins per definition".into());
        }
        let mut text = 0usize;
        for field in [&self.name, &self.value, &self.footprint].into_iter().chain(self.pins.iter().flat_map(|p| [&p.number, &p.name])) {
            text = text.saturating_add(field.len());
            if field.len() > 4096 || text > MAX_TEXT_BYTES {
                return Err("Definition text exceeds 4096 bytes per field or 256 KiB total".into());
            }
        }
        // Parse all geometry before cloning definition text.
        let width = millimetres(&self.width)?;
        let height = millimetres(&self.height)?;
        let positions: Vec<_> =
            self.pins.iter().map(|p| Ok(Coordinate { x: millimetres(&p.x)?, y: millimetres(&p.y)? })).collect::<Result<_, String>>()?;
        let template = SymbolTemplate {
            name: self.name.clone(),
            value: self.value.clone(),
            footprint: (!self.footprint.is_empty()).then(|| self.footprint.clone()),
            width,
            height,
            pins: self
                .pins
                .iter()
                .zip(positions)
                .map(|(p, position)| TemplatePin { number: p.number.clone(), name: p.name.clone(), position, electrical: p.electrical })
                .collect(),
        };
        template.validate()?;
        Ok(template)
    }
    fn add_pin(&mut self) {
        if self.pins.len() >= MAX_PINS {
            return;
        }
        let number = (1..=MAX_PINS + 1).map(|n| n.to_string()).find(|n| !self.pins.iter().any(|p| &p.number == n)).expect("bounded free pin number");
        self.pins.push(PinDraft { widget: ObjectId::new(), number, name: String::new(), x: "0".into(), y: "0".into(), electrical: PinType::Passive });
    }
    pub(crate) fn pin_count(&self) -> usize {
        self.pins.len()
    }
    pub(crate) fn pin_text(&self) -> impl Iterator<Item = &str> {
        self.pins.iter().flat_map(|p| [p.number.as_str(), p.name.as_str()])
    }
    pub(crate) fn show_geometry(&mut self, ui: &mut egui::Ui, max_pins: usize) {
        text(ui, self.widget, "Definition width (mm)", &mut self.width, 32);
        text(ui, self.widget, "Definition height (mm)", &mut self.height, 32);
        ui.label(format!("{} / 1024 ordered pins", self.pins.len()));
        let row_height = ui.spacing().interact_size.y * 6.0 + ui.spacing().item_spacing.y * 5.0 + 8.0;
        let mut remove = None;
        // Reserve one complete row even when preceding outer-window content
        // extends beyond the screen. The outer scroll area can then reveal
        // this region instead of collapsing its inner clip to a few points.
        egui::ScrollArea::vertical().id_salt("definition_pins").min_scrolled_height(row_height).max_height(280.0).show_rows(
            ui,
            row_height,
            self.pins.len(),
            |ui, rows| {
                for i in rows {
                    let pin = &mut self.pins[i];
                    ui.allocate_ui(egui::vec2(ui.available_width(), row_height), |ui| {
                        // A root scope also keeps the electrical popup stable through row movement.
                        ui.scope_builder(egui::UiBuilder::new().id(egui::Id::new(("definition-pin", pin.widget))), |ui| {
                            egui::Grid::new("fields").show(ui, |ui| {
                                for (key, label, field, limit) in [
                                    ("number", format!("Pin {} number", i + 1), &mut pin.number, 128),
                                    ("name", format!("Pin {} name", i + 1), &mut pin.name, 128),
                                    ("x", format!("Pin {} X (mm)", i + 1), &mut pin.x, 32),
                                    ("y", format!("Pin {} Y (mm)", i + 1), &mut pin.y, 32),
                                ] {
                                    let label = ui.label(label);
                                    ui.add(
                                        egui::TextEdit::singleline(field).id(egui::Id::new((pin.widget, key))).desired_width(150.0).char_limit(limit),
                                    )
                                    .labelled_by(label.id);
                                    ui.end_row();
                                }
                                let label = ui.label(format!("Pin {} electrical type", i + 1));
                                egui::ComboBox::from_id_salt("type")
                                    .selected_text(format!("{:?}", pin.electrical))
                                    .show_ui(ui, |ui| {
                                        for kind in [
                                            PinType::Unspecified,
                                            PinType::Passive,
                                            PinType::Input,
                                            PinType::Output,
                                            PinType::Bidirectional,
                                            PinType::TriState,
                                            PinType::OpenCollector,
                                            PinType::OpenEmitter,
                                            PinType::PowerInput,
                                            PinType::PowerOutput,
                                            PinType::NoConnect,
                                        ] {
                                            ui.selectable_value(&mut pin.electrical, kind, format!("{kind:?}"));
                                        }
                                    })
                                    .response
                                    .labelled_by(label.id);
                                ui.end_row();
                                if ui.button(format!("Remove pin {}", i + 1)).clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            });
                        });
                    });
                }
            },
        );
        if let Some(i) = remove {
            self.pins.remove(i);
        }
        if ui.add_enabled(self.pins.len() < max_pins, egui::Button::new("Add definition pin")).clicked() {
            self.add_pin();
        }
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, ready: bool) -> Option<Decision> {
        let mut decision = None;
        ui.separator();
        ui.heading("Local symbol definition");
        ui.label("Edits stay in this form. Use explicitly for new placement, or save a new definition file.");
        ui.add_enabled_ui(ready, |ui| {
            text(ui, self.widget, "Definition name", &mut self.name, 128);
            text(ui, self.widget, "Definition default value", &mut self.value, 1024);
            text(ui, self.widget, "Definition footprint (optional)", &mut self.footprint, 128);
            self.show_geometry(ui, MAX_PINS);
            text(ui, self.widget, "NEW definition file path", &mut self.path, 4096);
            if ui.button("Use edited definition").clicked() {
                match self.template() {
                    Ok(template) => {
                        self.error = None;
                        decision = Some(Decision::Use(template));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Save NEW definition file").clicked() {
                match self.template().and_then(|template| {
                    if self.path.is_empty() {
                        return Err("Enter a NEW definition file path".into());
                    }
                    Ok(SaveSymbolTemplate { template, path: self.path.clone().into() })
                }) {
                    Ok(input) => {
                        self.error = None;
                        decision = Some(Decision::Save(input));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Cancel definition edits").clicked() {
                decision = Some(Decision::Cancel);
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        decision
    }
}
fn text(ui: &mut egui::Ui, widget: ObjectId, label: &str, value: &mut String, limit: usize) {
    let id = egui::Id::new((widget, label));
    let label = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).id(id).char_limit(limit).desired_width(300.0)).labelled_by(label.id);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_editor_roundtrip_pin_limit_and_surviving_widget_identity() {
        let mut editor = DefinitionEditor::new();
        editor.width = "10.000001".into();
        editor.height = "0.000001".into();
        editor.pins[0].x = "-9999.999999".into();
        editor.pins[0].y = "0.000001".into();
        let original = editor.template().unwrap();
        assert_eq!(DefinitionEditor::from_template(&original).template().unwrap(), original);
        editor.add_pin();
        let surviving = editor.pins[1].widget;
        editor.pins.remove(0);
        assert_eq!(editor.pins[0].widget, surviving);
        for _ in 0..MAX_PINS + 1 {
            editor.add_pin();
        }
        assert_eq!(editor.pins.len(), MAX_PINS);
        editor.template().unwrap();
        editor.pins[0].name = "x".repeat(4097);
        assert!(editor.template().unwrap_err().contains("4096"));
        editor.pins[0].name.clear();
        for pin in &mut editor.pins {
            pin.name = "x".repeat(1024);
        }
        assert!(editor.template().unwrap_err().contains("256 KiB"));
    }
}
