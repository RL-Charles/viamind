//! Detached rectangular definitions: local drafts and explicit NEW file publication.
use crate::{ViamindApp, actions::UiAction, symbol_library::millimetres};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver, sync_channel};
use viamind_doc::{
    ObjectId,
    footprint::{Footprint, FootprintPad, LocalPosition},
};
use viamind_engine::{EngineHandle, FootprintDefinitionSaved, validate_definition};
use viamind_geom::Nm;

struct Row {
    widget: ObjectId,
    number: String,
    x: String,
    y: String,
    width: String,
    height: String,
}
struct Pending {
    receiver: Receiver<Result<Value, String>>,
    generation: u64,
    load: bool,
}
#[derive(Default)]
pub struct FootprintDefinitionUi {
    open: bool,
    name: String,
    rows: Vec<Row>,
    source: String,
    destination: String,
    generation: u64,
    pending: Option<Pending>,
    preview: Option<Footprint>,
    message: Option<String>,
}
fn decimal(value: Nm) -> String {
    let n = i128::from(value.0);
    let fraction = format!("{:06}", n.abs() % 1_000_000);
    let fraction = fraction.trim_end_matches('0');
    format!("{}{}{}{}", if n < 0 { "-" } else { "" }, n.abs() / 1_000_000, if fraction.is_empty() { "" } else { "." }, fraction)
}
impl FootprintDefinitionUi {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn preview(&self) -> Option<&Footprint> {
        self.preview.as_ref()
    }
    /// Explicitly validate current local entry; never modifies any placed component.
    pub fn definition(&self) -> Result<Footprint, String> {
        if self.rows.len() > 1024 {
            return Err("At most 1024 rectangular SMD pads per definition".into());
        }
        let mut bytes = 0usize;
        for (field, limit) in std::iter::once((&self.name, 512))
            .chain(self.rows.iter().flat_map(|r| [(&r.number, 512), (&r.x, 64), (&r.y, 64), (&r.width, 64), (&r.height, 64)]))
        {
            bytes = bytes.saturating_add(field.len());
            if field.len() > limit || bytes > 1024 * 1024 {
                return Err("Definition text exceeds a field byte bound or 1 MiB aggregate".into());
            }
        }
        // Parse geometry before cloning strings, using the exact integer decimal parser.
        let geometry = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let parse = |s: &str| millimetres(s).map_err(|e| format!("Pad {}: {e}", i + 1));
                Ok((LocalPosition { x: parse(&r.x)?, y: parse(&r.y)? }, parse(&r.width)?, parse(&r.height)?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let footprint = Footprint {
            name: self.name.clone(),
            pads: self
                .rows
                .iter()
                .zip(geometry)
                .map(|(r, (position, width, height))| FootprintPad { number: r.number.clone(), position, width, height })
                .collect(),
        };
        validate_definition(&footprint)?;
        Ok(footprint)
    }
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.message = None;
    }
    fn add(&mut self) {
        if self.busy() || self.rows.len() >= 1024 {
            return;
        }
        self.rows.push(Row {
            widget: ObjectId::new(),
            number: String::new(),
            x: String::new(),
            y: String::new(),
            width: String::new(),
            height: String::new(),
        });
        self.changed();
    }
    fn reset(&mut self) {
        if self.busy() {
            return;
        }
        self.name.clear();
        self.rows.clear();
        self.preview = None;
        self.changed();
    }
    fn loaded(&mut self, footprint: Footprint) -> Result<(), String> {
        validate_definition(&footprint)?;
        self.name = footprint.name.clone();
        self.rows = footprint
            .pads
            .iter()
            .map(|p| Row {
                widget: ObjectId::new(),
                number: p.number.clone(),
                x: decimal(p.position.x),
                y: decimal(p.position.y),
                width: decimal(p.width),
                height: decimal(p.height),
            })
            .collect();
        self.preview = Some(footprint);
        Ok(())
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Definition worker disconnected; inspect the requested file before retrying".into()),
        };
        let pending = self.pending.take().unwrap();
        if pending.generation != self.generation {
            self.message = Some("Detached draft changed; discarded the previous worker result. Inspect the requested file before retrying.".into());
            return;
        }
        self.message = Some(match result {
            Ok(value) if pending.load => match serde_json::from_value::<Footprint>(value).map_err(|e| e.to_string()).and_then(|f| self.loaded(f)) {
                Ok(()) => "Loaded detached definition. Edits and Save NEW do not update placed components.".into(),
                Err(error) => error,
            },
            Ok(value) => match serde_json::from_value::<FootprintDefinitionSaved>(value) {
                Ok(saved) => format!(
                    "Created {} ({} pads). Project, history and selection unchanged. Reuse separately in the electrical pad editor.",
                    saved.path.display(),
                    saved.pad_count
                ),
                Err(error) => error.to_string(),
            },
            Err(error) => format!("Definition file operation failed: {error}"),
        });
    }
    fn start(&mut self, engine: EngineHandle, load: bool, ctx: egui::Context) {
        self.start_with(engine, load, ctx, |job| std::thread::Builder::new().name("viamind-footprint-definition".into()).spawn(job).map(|_| ()));
    }
    fn start_with(
        &mut self,
        engine: EngineHandle,
        load: bool,
        ctx: egui::Context,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        if self.busy() {
            return;
        }
        let result = (|| {
            let path = if load { &self.source } else { &self.destination };
            if path.is_empty() || path.len() > 4096 {
                return Err("Choose a nonempty file path at most 4096 UTF-8 bytes".into());
            }
            Ok(if load {
                (UiAction::LoadFootprintDefinition, json!({"path":path}))
            } else {
                (UiAction::SaveFootprintDefinition, json!({"footprint":self.definition()?,"path":path}))
            })
        })();
        let (action, input) = match result {
            Ok(value) => value,
            Err(error) => {
                self.message = Some(error);
                return;
            }
        };
        let (sender, receiver) = sync_channel(1);
        match spawn(Box::new(move || {
            let value = action.run(&engine, input).map_err(|e| e.to_string());
            let _ = sender.try_send(value);
            ctx.request_repaint();
        })) {
            Ok(()) => {
                self.pending = Some(Pending { receiver, generation: self.generation, load });
                self.message = None;
            }
            Err(error) => self.message = Some(format!("Could not start definition worker: {error}")),
        }
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String, chars: usize) -> bool {
    let label = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width().min(260.0)).char_limit(chars)).labelled_by(label.id).changed()
}
fn preview(ui: &mut egui::Ui, footprint: &Footprint) {
    ui.label(format!(
        "Validated local preview: {} rectangular SMD pads. X right, Y down; origin marked. No courtyard, mask, paste or manufacturing checks.",
        footprint.pads.len()
    ));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 200.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let mut low = [0.0f64; 2];
    let mut high = [0.0f64; 2];
    for pad in &footprint.pads {
        for (axis, (center, size)) in [(pad.position.x.0, pad.width.0), (pad.position.y.0, pad.height.0)].into_iter().enumerate() {
            low[axis] = low[axis].min(center as f64 - size as f64 / 2.0);
            high[axis] = high[axis].max(center as f64 + size as f64 / 2.0);
        }
    }
    let scale = (f64::from((rect.width() - 30.0).max(1.0)) / (high[0] - low[0]).max(1.0))
        .min(f64::from((rect.height() - 30.0).max(1.0)) / (high[1] - low[1]).max(1.0));
    let at = |x: f64, y: f64| {
        egui::pos2(
            rect.center().x + ((x - (low[0] + high[0]) / 2.0) * scale) as f32,
            rect.center().y + ((y - (low[1] + high[1]) / 2.0) * scale) as f32,
        )
    };
    let origin = at(0.0, 0.0);
    painter.line_segment([egui::pos2(rect.min.x, origin.y), egui::pos2(rect.max.x, origin.y)], (1.0, egui::Color32::DARK_GRAY));
    painter.line_segment([egui::pos2(origin.x, rect.min.y), egui::pos2(origin.x, rect.max.y)], (1.0, egui::Color32::DARK_GRAY));
    for pad in &footprint.pads {
        let center = at(pad.position.x.0 as f64, pad.position.y.0 as f64);
        let size = egui::vec2((pad.width.0 as f64 * scale) as f32, (pad.height.0 as f64 * scale) as f32);
        painter.rect_filled(egui::Rect::from_center_size(center, size), 0.0, egui::Color32::from_rgb(170, 105, 45));
        if size.x >= 16.0 && size.y >= 16.0 {
            painter.text(center, egui::Align2::CENTER_CENTER, &pad.number, egui::FontId::proportional(12.0), egui::Color32::WHITE);
        }
    }
    painter.circle_stroke(origin, 4.0, (1.5, egui::Color32::LIGHT_BLUE));
}
impl ViamindApp {
    pub fn begin_footprint_definition(&mut self) {
        if self.design.is_some() {
            self.footprint_definition.open = true;
        }
    }
    pub(crate) fn footprint_definition_window(&mut self, ui: &mut egui::Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let form = &mut self.footprint_definition;
        form.poll(ui.ctx());
        if !form.open {
            return;
        }
        let mut open = true;
        let busy = form.busy();
        let mut load = false;
        let mut save = false;
        let mut changed = false;
        egui::Window::new("Detached footprint definition").open(&mut open).default_width(620.0)
            .max_width((ui.ctx().content_rect().width() - 24.0).max(240.0)).vscroll(true).show(ui.ctx(), |ui| {
                ui.label("Create or edit a native rectangular SMD library independently of placed components. Exact local mm entry; no inferred pitch or manufacturing defaults.");
                ui.add_enabled_ui(!busy, |ui| {
                    changed |= field(ui, "Native footprint source path", &mut form.source, 4096);
                    load = ui.button("Load native footprint definition").clicked();
                    if ui.button("Reset to empty footprint").clicked() { form.reset(); }
                    changed |= field(ui, "Footprint definition name", &mut form.name, 128);
                    ui.label(format!("{} / 1024 ordered rectangular pads", form.rows.len()));
                    let row_height = ui.spacing().interact_size.y * 13.0 + 24.0;
                    let mut remove = None;
                    egui::ScrollArea::vertical().id_salt("footprint_definition_rows").max_height(300.0).show_rows(ui, row_height, form.rows.len(), |ui, rows| {
                        for i in rows { let row = &mut form.rows[i]; ui.allocate_ui(egui::vec2(ui.available_width(), row_height), |ui| { ui.push_id(row.widget, |ui| {
                            for (name, value, limit) in [("number", &mut row.number, 128), ("X (mm)", &mut row.x, 64), ("Y (mm)", &mut row.y, 64), ("width (mm)", &mut row.width, 64), ("height (mm)", &mut row.height, 64)] {
                                changed |= field(ui, &format!("Pad {} {name}", i + 1), value, limit);
                            }
                            if ui.button(format!("Remove definition pad {}", i + 1)).clicked() { remove = Some(i); }
                        }); }); }
                    });
                    if let Some(i) = remove { form.rows.remove(i); changed = true; }
                    if ui.add_enabled(form.rows.len() < 1024, egui::Button::new("Add blank rectangular pad")).clicked() { form.add(); }
                    if ui.button("Preview detached footprint").clicked() { match form.definition() { Ok(footprint) => { form.preview = Some(footprint); form.message = None; }, Err(error) => form.message = Some(error) } }
                    changed |= field(ui, "NEW footprint definition path", &mut form.destination, 4096);
                    save = ui.button("Save NEW footprint definition").clicked();
                });
                if busy { ui.horizontal(|ui| { ui.spinner(); ui.label("Definition file operation pending; draft fields and Reset are disabled."); }); }
                if changed { form.changed(); }
                if let Some(footprint) = &form.preview { preview(ui, footprint); }
                ui.label("Saving preserves the project and editing drafts. Reuse the saved library separately in the electrical pad editor; matching numbers retain IDs and nets.");
                if let Some(message) = &form.message { ui.label(message); }
            });
        form.open = open;
        if load {
            form.start(engine.clone(), true, ui.ctx().clone());
        }
        if save {
            form.start(engine, false, ui.ctx().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn original() -> Footprint {
        Footprint {
            name: "Original".into(),
            pads: vec![FootprintPad {
                number: "1".into(),
                position: LocalPosition { x: Nm(-500003), y: Nm(250001) },
                width: Nm(1200003),
                height: Nm(800001),
            }],
        }
    }
    #[test]
    fn exact_entry_stable_rows_and_bounds_never_infer_geometry() {
        let mut form = FootprintDefinitionUi::default();
        form.add();
        assert!(form.definition().is_err());
        assert!(form.rows[0].number.is_empty() && form.rows[0].x.is_empty() && form.rows[0].width.is_empty());
        form.loaded(original()).unwrap();
        assert_eq!(form.definition().unwrap(), original());
        assert_eq!((&form.rows[0].x, &form.rows[0].width), (&"-0.500003".to_owned(), &"1.200003".to_owned()));
        form.add();
        let retained = form.rows[1].widget;
        form.rows.remove(0);
        assert_eq!(form.rows[0].widget, retained);
        for invalid in ["NaN", "1e-6", "0.0000001", "10001", "+1", ""] {
            form.loaded(original()).unwrap();
            form.rows[0].width = invalid.into();
            assert!(form.definition().is_err(), "{invalid}");
        }
        form.loaded(original()).unwrap();
        form.name = "x".repeat(513);
        assert!(form.definition().unwrap_err().contains("byte"));
        form.reset();
        for _ in 0..1025 {
            form.add();
        }
        assert_eq!(form.rows.len(), 1024);
    }
    #[test]
    fn retained_worker_blocks_reset_and_duplicates_and_close_still_delivers_load() {
        let mut form = FootprintDefinitionUi { source: "source.json".into(), ..Default::default() };
        form.loaded(original()).unwrap();
        let stable = form.rows[0].widget;
        let (sender, receiver) = sync_channel(1);
        form.pending = Some(Pending { receiver, generation: form.generation, load: true });
        form.reset();
        form.add();
        assert_eq!(form.rows.len(), 1);
        assert_eq!(form.rows[0].widget, stable);
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        form.start_with(engine, true, egui::Context::default(), |_| panic!("duplicate worker"));
        form.open = false;
        let mut loaded = original();
        loaded.name = "Loaded while closed".into();
        sender.send(Ok(serde_json::to_value(&loaded).unwrap())).unwrap();
        form.poll(&egui::Context::default());
        assert!(!form.busy());
        assert!(!form.open);
        assert_eq!(form.definition().unwrap(), loaded);
    }
    #[test]
    fn changed_generation_and_failed_spawn_disconnection_retain_draft_without_applying() {
        let mut form = FootprintDefinitionUi { source: "source.json".into(), ..Default::default() };
        form.loaded(original()).unwrap();
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let before = engine.session_state();
        form.start_with(engine.clone(), true, egui::Context::default(), |_| Err(std::io::Error::other("refused")));
        assert!(!form.busy());
        assert!(form.message.as_ref().unwrap().contains("Could not start"));
        assert_eq!(form.definition().unwrap(), original());
        form.start_with(engine.clone(), true, egui::Context::default(), |job| {
            drop(job);
            Ok(())
        });
        form.poll(&egui::Context::default());
        assert!(!form.busy());
        assert!(form.message.as_ref().unwrap().contains("disconnected"));
        let (sender, receiver) = sync_channel(1);
        form.pending = Some(Pending { receiver, generation: form.generation, load: true });
        form.changed();
        sender.send(Ok(json!({}))).unwrap();
        form.poll(&egui::Context::default());
        assert!(form.message.as_ref().unwrap().contains("discarded"));
        assert_eq!(form.definition().unwrap(), original());
        assert_eq!(engine.session_state(), before);
    }
}
