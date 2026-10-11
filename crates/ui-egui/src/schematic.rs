//! Dedicated schematic capture workspace. All persisted edits use shared sheet commands.
use crate::{
    ViamindApp,
    actions::UiAction,
    camera::{Camera, WorldPos, WorldRect},
};
use egui::{Align2, Color32, FontId, Key, PointerButton, Sense, Stroke, Ui};
use serde_json::{Value, json};
use viamind_doc::{
    ObjectId, Sheet,
    schematic::{Coordinate, PinType, SchematicContent, SchematicSymbol},
};
use viamind_geom::Nm;

fn nm(text: &str) -> Result<i64, String> {
    let text = text.trim();
    let (negative, number) = if let Some(s) = text.strip_prefix('-') { (true, s) } else { (false, text.strip_prefix('+').unwrap_or(text)) };
    let mut parts = number.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
    {
        return Err("Use millimetres with at most six decimal places".into());
    }
    let whole: i64 = whole.parse().map_err(|_| "Coordinate is too large")?;
    let fraction: i64 =
        if fraction.is_empty() { 0 } else { fraction.parse::<i64>().map_err(|_| "Invalid coordinate")? * 10_i64.pow(6 - fraction.len() as u32) };
    let value = whole.checked_mul(1_000_000).and_then(|v| v.checked_add(fraction)).ok_or("Coordinate is too large")?;
    Ok(if negative { -value } else { value })
}
fn mm(n: Nm) -> String {
    format!("{:.6}", n.to_mm())
}
fn point(x: &str, y: &str) -> Result<Value, String> {
    Ok(json!({"x":nm(x)?,"y":nm(y)?}))
}
#[derive(Clone)]
struct PinDraft {
    number: String,
    name: String,
    x: String,
    y: String,
    electrical: PinType,
}
impl Default for PinDraft {
    fn default() -> Self {
        Self { number: "1".into(), name: String::new(), x: "-3".into(), y: "0".into(), electrical: PinType::Passive }
    }
}
struct Draft {
    project: ObjectId,
    expected: Sheet,
    id: Option<ObjectId>,
    kind: &'static str,
    name: String,
    value: String,
    footprint: String,
    x: String,
    y: String,
    end_x: String,
    end_y: String,
    width: String,
    height: String,
    quarters: u8,
    mirrored: bool,
    scope: &'static str,
    pins: Vec<PinDraft>,
}
impl Draft {
    fn new(project: ObjectId, expected: Sheet, kind: &'static str) -> Self {
        Self {
            project,
            expected,
            id: None,
            kind,
            name: if kind == "symbol" { "U1".into() } else { "VIN".into() },
            value: String::new(),
            footprint: String::new(),
            x: "0".into(),
            y: "0".into(),
            end_x: "5".into(),
            end_y: "0".into(),
            width: "4".into(),
            height: "6".into(),
            quarters: 0,
            mirrored: false,
            scope: "local",
            pins: vec![PinDraft::default()],
        }
    }
    fn from_object(project: ObjectId, sheet: Sheet, id: ObjectId) -> Option<Self> {
        let c = sheet.content.as_ref()?;
        let mut d = Self::new(project, sheet.clone(), "junction");
        d.id = Some(id);
        if let Some(s) = c.symbols.get(&id) {
            d.kind = "symbol";
            d.name = s.designator.clone();
            d.value = s.value.clone();
            d.footprint = s.footprint.clone().unwrap_or_default();
            d.x = mm(s.position.x);
            d.y = mm(s.position.y);
            d.width = mm(s.width);
            d.height = mm(s.height);
            d.quarters = s.rotation_quarters;
            d.mirrored = s.mirrored;
            d.pins = s
                .pins
                .iter()
                .map(|p| PinDraft {
                    number: p.number.clone(),
                    name: p.name.clone(),
                    x: mm(p.position.x),
                    y: mm(p.position.y),
                    electrical: p.electrical,
                })
                .collect();
        } else if let Some(w) = c.wires.get(&id) {
            d.kind = "wire";
            d.x = mm(w.start.x);
            d.y = mm(w.start.y);
            d.end_x = mm(w.end.x);
            d.end_y = mm(w.end.y);
        } else if let Some(j) = c.junctions.get(&id) {
            d.x = mm(j.position.x);
            d.y = mm(j.position.y);
        } else {
            let l = c.labels.get(&id)?;
            d.kind = "label";
            d.x = mm(l.position.x);
            d.y = mm(l.position.y);
            d.name = l.name.clone();
            d.scope = match l.scope {
                viamind_doc::schematic::LabelScope::Local => "local",
                viamind_doc::schematic::LabelScope::Global => "global",
                viamind_doc::schematic::LabelScope::Power => "power",
            };
        }
        Some(d)
    }
    fn edit(&self) -> Result<Value, String> {
        let position = point(&self.x, &self.y)?;
        Ok(match self.kind {
            "symbol" => {
                let pins = self
                    .pins
                    .iter()
                    .map(|p| Ok(json!({"number":p.number,"name":p.name,"position":point(&p.x,&p.y)?,"electrical":p.electrical})))
                    .collect::<Result<Vec<_>, String>>()?;
                json!({"kind":"symbol","symbol":{"id":self.id,"designator":self.name,"value":self.value,"footprint":if self.footprint.is_empty(){Value::Null}else{json!(self.footprint)},"position":position,"width":nm(&self.width)?,"height":nm(&self.height)?,"rotation_quarters":self.quarters,"mirrored":self.mirrored,"pins":pins}})
            }
            "wire" => json!({"kind":"wire","id":self.id,"start":position,"end":point(&self.end_x,&self.end_y)?}),
            "label" => json!({"kind":"label","id":self.id,"position":position,"name":self.name,"scope":self.scope}),
            _ => json!({"kind":"junction","id":self.id,"position":position}),
        })
    }
    fn fields(&mut self, ui: &mut Ui) {
        ui.heading(format!("{} {}", if self.id.is_some() { "Edit" } else { "Add" }, self.kind));
        egui::Grid::new("schematic_fields").num_columns(2).show(ui, |ui| {
            if self.kind == "symbol" {
                field(ui, "Reference", &mut self.name);
                field(ui, "Symbol value", &mut self.value);
                field(ui, "Symbol footprint", &mut self.footprint);
                field(ui, "Body width (mm)", &mut self.width);
                field(ui, "Body height (mm)", &mut self.height);
            }
            if self.kind == "label" {
                field(ui, "Label name", &mut self.name);
            }
            field(ui, "Schematic X (mm)", &mut self.x);
            field(ui, "Schematic Y (mm)", &mut self.y);
            if self.kind == "wire" {
                field(ui, "Wire end X (mm)", &mut self.end_x);
                field(ui, "Wire end Y (mm)", &mut self.end_y);
            }
        });
        if self.kind == "symbol" {
            ui.horizontal(|ui| {
                ui.label("CCW quarter turns");
                ui.add(egui::DragValue::new(&mut self.quarters).range(0..=3));
            });
            ui.checkbox(&mut self.mirrored, "Mirror local X");
            ui.label("Pins: local endpoints in mm");
            let mut remove = None;
            for (i, p) in self.pins.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.group(|ui| {
                        egui::Grid::new("pin_fields").num_columns(2).show(ui, |ui| {
                            field(ui, &format!("Pin {} number", i + 1), &mut p.number);
                            field(ui, &format!("Pin {} name", i + 1), &mut p.name);
                            field(ui, &format!("Pin {} X (mm)", i + 1), &mut p.x);
                            field(ui, &format!("Pin {} Y (mm)", i + 1), &mut p.y);
                        });
                        egui::ComboBox::from_id_salt("electrical").selected_text(format!("{:?}", p.electrical)).show_ui(ui, |ui| {
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
                                ui.selectable_value(&mut p.electrical, kind, format!("{kind:?}"));
                            }
                        });
                        if ui.button("Remove pin").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                self.pins.remove(i);
            }
            if ui.add_enabled(self.pins.len() < 1024, egui::Button::new("Add pin")).clicked() {
                self.pins.push(PinDraft { number: (self.pins.len() + 1).to_string(), ..Default::default() });
            }
        }
        if self.kind == "label" {
            ui.horizontal(|ui| {
                for (scope, label) in [("local", "Local label"), ("global", "Global label"), ("power", "Power port")] {
                    ui.radio_value(&mut self.scope, scope, label);
                }
            });
        }
    }
}
fn field(ui: &mut Ui, label: &str, text: &mut String) {
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(text).desired_width(120.0)).labelled_by(l.id);
    ui.end_row();
}

pub struct SchematicUi {
    pub active: bool,
    project: Option<ObjectId>,
    stream: Option<ObjectId>,
    sheet_guard: Option<Sheet>,
    sheet: Option<ObjectId>,
    selected: Option<ObjectId>,
    draft: Option<Draft>,
    sheet_name: String,
    message: Option<String>,
    pub camera: Camera,
    fit: bool,
    pub viewport: egui::Rect,
    cursor_world: Option<WorldPos>,
    object_count: usize,
}
impl Default for SchematicUi {
    fn default() -> Self {
        Self {
            active: false,
            project: None,
            stream: None,
            sheet_guard: None,
            sheet: None,
            selected: None,
            draft: None,
            sheet_name: "Main".into(),
            message: None,
            camera: Camera::default(),
            fit: true,
            viewport: egui::Rect::NOTHING,
            cursor_world: None,
            object_count: 0,
        }
    }
}
impl SchematicUi {
    pub(crate) fn with_active(active: bool) -> Self {
        Self { active, ..Default::default() }
    }
}
impl ViamindApp {
    pub fn set_schematic_mode(&mut self, active: bool) {
        if active && (self.files.busy() || self.parts.placing() || self.outlines.editing() || self.pads.editing() || self.copper.editing()) {
            return;
        }
        if self.schematic.active != active {
            self.selection.cancel_gestures();
        }
        self.schematic.active = active;
    }
    pub(crate) fn fit_schematic(&mut self) {
        self.schematic.fit = true;
    }
    pub(crate) fn schematic_status(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(p) = self.schematic.cursor_world {
                ui.monospace(format!("X {:.3} Y {:.3} mm", p.x, p.y));
            }
            ui.monospace(format!("zoom {:.3} px/mm · {} schematic objects", self.schematic.camera.zoom, self.schematic.object_count));
            if let Some(design) = &self.design {
                let status = design.engine.document_status();
                ui.label(if status.dirty {
                    "Unsaved changes"
                } else if status.path.is_some() {
                    "Saved"
                } else {
                    "Not saved"
                });
                ui.monospace(design.mcp_url.as_deref().map_or_else(|| "MCP off".into(), |url| format!("MCP {url}")));
            }
        });
    }
    pub(crate) fn schematic_panel(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let (project, stream, sheets) = engine.with_engine(|e| {
            (e.project().id, e.event_cursor().stream_id, e.project().schematic.sheets.values().map(|s| (s.id, s.name.clone())).collect::<Vec<_>>())
        });
        if self.schematic.project != Some(project) || self.schematic.stream != Some(stream) {
            self.schematic.stream = Some(stream);
            self.schematic.sheet_guard = None;
            self.schematic.project = Some(project);
            self.schematic.sheet = None;
            self.schematic.selected = None;
            self.schematic.draft = None;
            self.schematic.fit = true;
            self.schematic.message = None;
        }
        if self.schematic.sheet.is_none()
            && let Some((id, name)) = sheets.first()
        {
            self.schematic.sheet = Some(*id);
            self.schematic.sheet_name = name.clone();
            self.schematic.sheet_guard = None;
        }
        ui.heading("Schematic capture");
        let mut selected = self.schematic.sheet;
        egui::ComboBox::from_id_salt("schematic_sheet")
            .selected_text(sheets.iter().find(|(id, _)| Some(*id) == selected).map_or("Choose sheet", |(_, name)| name.as_str()))
            .show_ui(ui, |ui| {
                for (id, name) in &sheets {
                    ui.selectable_value(&mut selected, Some(*id), name);
                }
            });
        if selected != self.schematic.sheet {
            self.schematic.sheet = selected;
            self.schematic.sheet_guard = None;
            self.schematic.selected = None;
            self.schematic.draft = None;
            self.schematic.fit = true;
            if let Some((_, name)) = sheets.iter().find(|(id, _)| Some(*id) == selected) {
                self.schematic.sheet_name = name.clone();
            }
        }
        let label = ui.label("Sheet name");
        ui.text_edit_singleline(&mut self.schematic.sheet_name).labelled_by(label.id);
        if ui.button("Create schematic sheet").clicked() {
            match UiAction::EditSchematicSheet.run(&engine, json!({"expected_project":project,"name":self.schematic.sheet_name,"edits":[]})) {
                Ok(v) => {
                    self.schematic.sheet = serde_json::from_value(v["created"][0].clone()).ok();
                    self.schematic.sheet_guard = None;
                    self.schematic.draft = None;
                    self.schematic.selected = None;
                    self.schematic.fit = true;
                    self.schematic.message = None;
                }
                Err(e) => self.schematic.message = Some(e.to_string()),
            }
        }
        let sheet = engine.read(|p| self.schematic.sheet.and_then(|id| p.schematic.sheets.get(&id).cloned()));
        if let Some(sheet) = sheet {
            let guard = self.schematic.sheet_guard.get_or_insert_with(|| sheet.clone()).clone();
            if guard != sheet {
                ui.label("Sheet changed; reload sheet details before renaming or deleting.");
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Rename sheet").clicked() {
                    self.schematic.message = UiAction::EditSchematicSheet
                        .run(
                            &engine,
                            json!({"expected_project":project,"sheet_id":guard.id,"expected":guard,"name":self.schematic.sheet_name,"edits":[]}),
                        )
                        .err()
                        .map(|e| e.to_string());
                    if self.schematic.message.is_none() {
                        self.schematic.sheet_guard = None;
                    }
                }
                if ui.button("Delete sheet").clicked() {
                    self.schematic.message = UiAction::DeleteSchematicSheet
                        .run(&engine, json!({"expected_project":project,"expected":guard}))
                        .err()
                        .map(|e| e.to_string());
                }
            });
            if ui.button("Reload sheet details").clicked() {
                self.schematic.sheet_guard = Some(sheet.clone());
                self.schematic.sheet_name = sheet.name.clone();
                self.schematic.message = None;
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for (kind, label) in [("symbol", "Add symbol"), ("wire", "Add wire"), ("junction", "Add junction"), ("label", "Add label")] {
                    if ui.button(label).clicked() {
                        self.schematic.draft = Some(Draft::new(project, sheet.clone(), kind));
                        self.schematic.selected = None;
                        self.schematic.message = None;
                    }
                }
            });
            if ui.button("Fit schematic").clicked() {
                self.schematic.fit = true;
            }
            ui.collapsing("Schematic objects", |ui| {
                if let Some(c) = &sheet.content {
                    let mut objects: Vec<_> = c
                        .symbols
                        .values()
                        .map(|s| (s.id, format!("Symbol {}", s.designator)))
                        .chain(c.wires.keys().map(|id| (*id, format!("Wire {id}"))))
                        .chain(c.junctions.keys().map(|id| (*id, format!("Junction {id}"))))
                        .chain(c.labels.values().map(|l| (l.id, format!("Label {}", l.name))))
                        .collect();
                    objects.sort_by_key(|(id, _)| *id);
                    for (id, label) in objects {
                        if ui.selectable_label(self.schematic.selected == Some(id), label).clicked() {
                            self.schematic.selected = Some(id);
                            self.schematic.draft = Draft::from_object(project, sheet.clone(), id);
                            self.schematic.message = None;
                        }
                    }
                }
            });
            if let Some(d) = &mut self.schematic.draft {
                ui.separator();
                if d.expected != sheet {
                    ui.label("Sheet changed; reload before applying this draft.");
                }
                d.fields(ui);
                let mut apply = false;
                let mut reload = false;
                let mut delete = false;
                let mut cancel = false;
                ui.horizontal(|ui| {
                    apply = ui.button("Apply schematic object").clicked();
                    reload = ui.button("Reload schematic draft").clicked();
                    cancel = ui.button("Cancel schematic draft").clicked();
                });
                if d.id.is_some() {
                    delete = ui.button("Delete schematic object").clicked();
                }
                if apply || delete {
                    let edit = if delete { Ok(json!({"kind":"delete","id":d.id})) } else { d.edit() };
                    let result=edit.and_then(|edit|UiAction::EditSchematicSheet.run(&engine,json!({"expected_project":d.project,"sheet_id":d.expected.id,"expected":d.expected,"name":d.expected.name,"edits":[edit]})).map_err(|e|e.to_string()));
                    match result {
                        Ok(_) => {
                            self.schematic.draft = None;
                            self.schematic.sheet_guard = None;
                            self.schematic.selected = None;
                            self.schematic.message = None;
                        }
                        Err(e) => self.schematic.message = Some(e),
                    }
                } else if reload {
                    self.schematic.draft =
                        if let Some(id) = d.id { Draft::from_object(project, sheet, id) } else { Some(Draft::new(project, sheet, d.kind)) };
                    self.schematic.message = None;
                } else if cancel {
                    self.schematic.draft = None;
                    self.schematic.message = None;
                }
            }
        } else if self.schematic.sheet.is_some() {
            self.schematic.sheet = None;
            self.schematic.selected = None;
            self.schematic.draft = None;
        }
        if let Some(message) = &self.schematic.message {
            ui.label(message);
        }
        ui.separator();
        ui.label("Middle drag / Space+drag pan · wheel zoom · Home fit");
        ui.label("Electrical connectivity and ERC are not evaluated.");
    }
    pub(crate) fn schematic_canvas(&mut self, ui: &mut Ui) {
        let sheet = self.design.as_ref().and_then(|d| d.engine.read(|p| self.schematic.sheet.and_then(|id| p.schematic.sheets.get(&id).cloned())));
        let content = sheet.as_ref().and_then(|s| s.content.as_ref());
        self.schematic.object_count = content.map_or(0, SchematicContent::object_count);
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let viewport = response.rect;
        if self.schematic.fit || self.schematic.viewport.size() != viewport.size() || ui.input(|i| i.key_pressed(Key::Home)) {
            self.schematic.camera.zoom_to_fit(bounds(content), viewport, 50.0);
            self.schematic.fit = false;
        }
        self.schematic.viewport = viewport;
        self.schematic.cursor_world = response.hover_pos().map(|p| self.schematic.camera.screen_to_world(p, viewport));
        if response.dragged_by(PointerButton::Middle) || (response.dragged_by(PointerButton::Primary) && ui.input(|i| i.key_down(Key::Space))) {
            self.schematic.camera.pan_by_screen(response.drag_delta());
        }
        if let Some(at) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            self.schematic.camera.zoom_about(at, viewport, f64::from(pinch) * (f64::from(scroll) * 0.0025).exp());
        }
        painter.rect_filled(viewport, 0.0, Color32::from_rgb(22, 25, 30));
        let Some(c) = content else {
            painter.text(viewport.center(), Align2::CENTER_CENTER, "Create or choose a schematic sheet", FontId::proportional(18.0), Color32::WHITE);
            return;
        };
        let cam = self.schematic.camera;
        let screen = |p: Coordinate| cam.world_to_screen(world(p), viewport);
        let wire_color = Color32::from_rgb(100, 220, 150);
        let symbol_color = Color32::from_rgb(235, 210, 130);
        let selected = self.schematic.selected;
        let color = |id, normal| if selected == Some(id) { Color32::from_rgb(255, 160, 60) } else { normal };
        for w in c.wires.values() {
            painter.line_segment([screen(w.start), screen(w.end)], Stroke::new(2.0, color(w.id, wire_color)));
        }
        for s in c.symbols.values() {
            let corners = body_corners(s);
            let points: Vec<_> = corners.into_iter().map(&screen).collect();
            painter.add(egui::Shape::closed_line(points, Stroke::new(2.0, color(s.id, symbol_color))));
            let at = screen(s.position);
            painter.text(at, Align2::CENTER_CENTER, format!("{}\n{}", s.designator, s.value), FontId::proportional(13.0), symbol_color);
            for pin in &s.pins {
                if let Ok(p) = s.pin_position(pin.position) {
                    let body = Coordinate {
                        x: Nm(pin.position.x.0.clamp(-s.width.0 / 2, s.width.0 - s.width.0 / 2)),
                        y: Nm(pin.position.y.0.clamp(-s.height.0 / 2, s.height.0 - s.height.0 / 2)),
                    };
                    if let Ok(body) = s.pin_position(body) {
                        painter.line_segment([screen(body), screen(p)], Stroke::new(1.5, symbol_color));
                    }
                    let at = screen(p);
                    painter.circle_stroke(at, 3.0, Stroke::new(1.5, symbol_color));
                    painter.text(
                        at + egui::vec2(5.0, -4.0),
                        Align2::LEFT_BOTTOM,
                        format!("{} {}", pin.number, pin.name),
                        FontId::proportional(11.0),
                        symbol_color,
                    );
                }
            }
        }
        for j in c.junctions.values() {
            painter.circle_filled(screen(j.position), 4.0, color(j.id, wire_color));
        }
        for l in c.labels.values() {
            let at = screen(l.position);
            let tint = color(l.id, Color32::from_rgb(160, 200, 255));
            painter.circle_stroke(at, 3.0, Stroke::new(1.0, tint));
            painter.text(at + egui::vec2(6.0, -4.0), Align2::LEFT_BOTTOM, format!("{} ({:?})", l.name, l.scope), FontId::proportional(13.0), tint);
        }
        if response.clicked()
            && !ui.input(|i| i.key_down(Key::Space))
            && !self.files.busy()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let near = anchors(c)
                .into_iter()
                .map(|(id, p)| (id, screen(p).distance(pointer)))
                .filter(|(_, distance)| *distance < 14.0)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((id, _)) = near {
                self.schematic.selected = Some(id);
                if let (Some(project), Some(sheet)) = (self.schematic.project, sheet) {
                    self.schematic.draft = Draft::from_object(project, sheet, id);
                    self.schematic.message = None;
                }
            }
        }
    }
}
fn world(p: Coordinate) -> WorldPos {
    WorldPos::new(p.x.to_mm(), p.y.to_mm())
}
fn body_corners(s: &SchematicSymbol) -> [Coordinate; 4] {
    let x = s.width.0 / 2;
    let y = s.height.0 / 2;
    [(-x, -y), (s.width.0 - x, -y), (s.width.0 - x, s.height.0 - y), (-x, s.height.0 - y)].map(|(mut x, mut y)| {
        if s.mirrored {
            x = -x;
        }
        for _ in 0..s.rotation_quarters {
            (x, y) = (y, -x);
        }
        Coordinate { x: Nm(s.position.x.0 + x), y: Nm(s.position.y.0 + y) }
    })
}
fn anchors(c: &SchematicContent) -> Vec<(ObjectId, Coordinate)> {
    c.symbols
        .values()
        .map(|s| (s.id, s.position))
        .chain(c.wires.values().flat_map(|w| [(w.id, w.start), (w.id, w.end)]))
        .chain(c.junctions.values().map(|j| (j.id, j.position)))
        .chain(c.labels.values().map(|l| (l.id, l.position)))
        .collect()
}
fn bounds(c: Option<&SchematicContent>) -> WorldRect {
    let mut points = vec![];
    if let Some(c) = c {
        points.extend(anchors(c).into_iter().map(|(_, p)| p));
        for s in c.symbols.values() {
            points.extend(body_corners(s));
            points.extend(s.pins.iter().filter_map(|p| s.pin_position(p.position).ok()));
        }
    }
    let mut bounds = WorldRect::new(WorldPos::new(-5.0, -5.0), WorldPos::new(5.0, 5.0));
    if let Some(first) = points.first() {
        bounds = WorldRect::new(world(*first), world(*first));
        for p in points {
            let p = world(p);
            bounds.min.x = bounds.min.x.min(p.x);
            bounds.min.y = bounds.min.y.min(p.y);
            bounds.max.x = bounds.max.x.max(p.x);
            bounds.max.y = bounds.max.y.max(p.y);
        }
    }
    bounds.expand(3.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_mm_input_rejects_sub_nm_and_overflow() {
        assert_eq!(nm("-1.234567").unwrap(), -1234567);
        assert_eq!(nm("+0.000001").unwrap(), 1);
        for s in ["NaN", "inf", "1.0000001", "1e2", "--1", "9223372036854775807", ".5"] {
            assert!(nm(s).is_err(), "{s}");
        }
    }
}

#[cfg(test)]
mod drawing_tests {
    use super::*;
    #[test]
    fn regulator_fixture_body_transforms_and_fit_bounds_are_exact() {
        let project = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
        let content = project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
        assert_eq!((content.object_count(), content.pin_count()), (12, 7));
        assert_eq!(bounds(Some(content)), WorldRect::new(WorldPos::new(6.0, 14.0), WorldPos::new(34.0, 31.0)));
        let mut symbol = content.symbols.values().find(|s| s.designator == "U1").unwrap().clone();
        for (mirrored, cases) in [
            (
                false,
                [
                    [(-2, -3), (2, -3), (2, 3), (-2, 3)],
                    [(-3, 2), (-3, -2), (3, -2), (3, 2)],
                    [(2, 3), (-2, 3), (-2, -3), (2, -3)],
                    [(3, -2), (3, 2), (-3, 2), (-3, -2)],
                ],
            ),
            (
                true,
                [
                    [(2, -3), (-2, -3), (-2, 3), (2, 3)],
                    [(-3, -2), (-3, 2), (3, 2), (3, -2)],
                    [(-2, 3), (2, 3), (2, -3), (-2, -3)],
                    [(3, 2), (3, -2), (-3, -2), (-3, 2)],
                ],
            ),
        ] {
            symbol.mirrored = mirrored;
            for (quarter, offsets) in cases.into_iter().enumerate() {
                symbol.rotation_quarters = quarter as u8;
                let expected = offsets.map(|(x, y)| Coordinate { x: Nm(20_000_000 + x * 1_000_000), y: Nm(20_000_000 + y * 1_000_000) });
                assert_eq!(body_corners(&symbol), expected);
            }
        }
    }
}
