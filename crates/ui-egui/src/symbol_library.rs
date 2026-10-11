//! Detached native symbol files with one retained background worker and guarded placement.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, schematic::Coordinate, symbol_library::SymbolTemplate};
use viamind_engine::{
    EngineHandle, EventCursor, ExportSymbolTemplate, LoadSymbolTemplate, PlaceSymbolTemplate, SymbolSheetQuery, SymbolSheetSnapshot,
    SymbolTemplateExportResult,
};
use viamind_geom::Nm;

type Context = (ObjectId, EventCursor, ObjectId);
#[derive(Debug)]
enum Completed {
    Sheet(SymbolSheetSnapshot),
    Loaded(SymbolTemplate),
    Exported(SymbolTemplateExportResult),
}
struct Pending {
    source: Context,
    receiver: Receiver<Result<Completed, String>>,
}
pub struct SymbolLibraryUi {
    open: bool,
    context: Option<Context>,
    guard: Option<SymbolSheetSnapshot>,
    template: Option<SymbolTemplate>,
    source_symbol: Option<ObjectId>,
    pending: Option<Pending>,
    path: String,
    name: String,
    reference: String,
    x: String,
    y: String,
    rotation: u8,
    mirrored: bool,
    message: Option<String>,
}
impl Default for SymbolLibraryUi {
    fn default() -> Self {
        Self {
            open: false,
            context: None,
            guard: None,
            template: None,
            source_symbol: None,
            pending: None,
            path: String::new(),
            name: "My symbol".into(),
            reference: "U2".into(),
            x: "40".into(),
            y: "40".into(),
            rotation: 0,
            mirrored: false,
            message: None,
        }
    }
}
fn identity(context: Context) -> (ObjectId, ObjectId, ObjectId) {
    (context.0, context.1.stream_id, context.2)
}
fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}
/// Exact decimal millimetres with at most six fractional places (one nanometre).
fn millimetres(value: &str) -> Result<Nm, String> {
    let fail = || "Coordinates require decimal mm with at most six fractional places, within ±10000 mm".to_owned();
    if value.len() > 32 || value.is_empty() {
        return Err(fail());
    }
    let (negative, digits) = value.strip_prefix('-').map_or((false, value), |s| (true, s));
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() || !whole.bytes().all(|c| c.is_ascii_digit()) || fraction.len() > 6 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return Err(fail());
    }
    let whole: i64 = whole.parse().map_err(|_| fail())?;
    let fractional: i64 = if fraction.is_empty() { 0 } else { fraction.parse().map_err(|_| fail())? };
    let raw = whole.checked_mul(1_000_000).and_then(|n| n.checked_add(fractional * 10i64.pow(6 - fraction.len() as u32))).ok_or_else(fail)?;
    if raw > 10_000_000_000 {
        return Err(fail());
    }
    Ok(Nm(if negative { -raw } else { raw }))
}
impl SymbolLibraryUi {
    fn poll(&mut self, context: Option<Context>) -> bool {
        let replaced = self.context.map(identity) != context.map(identity);
        self.context = context;
        if replaced {
            self.guard = None;
            self.template = None;
            self.source_symbol = None;
            self.message = None;
        }
        let Some(pending) = &self.pending else { return replaced };
        let result = match pending.receiver.try_recv() {
            Ok(value) => value,
            Err(TryRecvError::Empty) => return replaced,
            Err(TryRecvError::Disconnected) => Err("Symbol library worker stopped; retry the operation".into()),
        };
        let pending = self.pending.take().expect("completed worker");
        if Some(identity(pending.source)) != context.map(identity) {
            return replaced;
        }
        match result {
            Ok(Completed::Exported(result)) => {
                self.message = Some(format!(
                    "Created {} · {} pins · source {}:{}{}",
                    result.path.display(),
                    result.pin_count,
                    result.exported_cursor.stream_id,
                    result.exported_cursor.sequence,
                    if context.is_some_and(|c| c.1 != result.exported_cursor) { " · document has newer edits" } else { "" }
                ));
            }
            Ok(_) if Some(pending.source) != context => self.message = Some("Document changed; refresh the sheet and load the template again".into()),
            Ok(Completed::Sheet(sheet)) => {
                self.source_symbol = sheet.expected.content.as_ref().and_then(|c| c.symbols.values().next().map(|s| s.id));
                self.guard = Some(sheet);
            }
            Ok(Completed::Loaded(template)) => self.template = Some(template),
            Err(error) => self.message = Some(error),
        }
        replaced
    }
    fn start(&mut self, engine: EngineHandle, action: UiAction, input: serde_json::Value) {
        if self.pending.is_some() {
            return;
        }
        let Some(source) = self.context else { return };
        self.message = None;
        if action == UiAction::LoadSymbolTemplate {
            self.template = None;
        }
        let (sender, receiver) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-symbol-library".into()).spawn(move || {
            let result = action.run(&engine, input).map_err(|e| e.to_string()).and_then(|value| match action {
                UiAction::SnapshotSymbolSheet => decode(value).map(Completed::Sheet),
                UiAction::LoadSymbolTemplate => decode(value).map(Completed::Loaded),
                UiAction::ExportSymbolTemplate => decode(value).map(Completed::Exported),
                _ => Err("Unsupported symbol library worker action".into()),
            });
            let _ = sender.try_send(result);
        }) {
            Ok(_) => self.pending = Some(Pending { source, receiver }),
            Err(error) => self.message = Some(format!("Could not start symbol library worker: {error}")),
        }
    }
    fn proposal(&self) -> Result<PlaceSymbolTemplate, String> {
        let guard = self.guard.as_ref().ok_or("Refresh the selected sheet first")?;
        let template = self.template.as_ref().ok_or("Load a symbol template first")?;
        let position = Coordinate { x: millimetres(&self.x)?, y: millimetres(&self.y)? };
        // Validate the exact transformed body and pins for the local preview too.
        template.instantiate(&self.reference, position, self.rotation, self.mirrored)?;
        Ok(PlaceSymbolTemplate {
            expected_project: guard.expected_project,
            source_cursor: guard.source_cursor,
            expected: guard.expected.clone(),
            template: template.clone(),
            designator: self.reference.clone(),
            position,
            rotation_quarters: self.rotation,
            mirrored: self.mirrored,
        })
    }
}
impl ViamindApp {
    pub fn begin_symbol_library(&mut self) {
        self.set_schematic_mode(true);
        self.symbol_library.open = true;
    }
    pub fn symbol_library_window(&mut self, ui: &mut Ui) {
        let engine = self.design.as_ref().map(|d| d.engine.clone());
        let context = engine.as_ref().and_then(|e| {
            self.schematic
                .active
                .then(|| self.schematic.active_sheet_id())
                .flatten()
                .map(|sheet| e.with_engine(|e| (e.project().id, e.event_cursor(), sheet)))
        });
        let replaced = self.symbol_library.poll(context);
        if self.symbol_library.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        if !self.symbol_library.open {
            return;
        }
        let Some(engine) = engine else { return };
        if replaced
            && self.symbol_library.pending.is_none()
            && let Some((_, _, sheet)) = context
        {
            self.symbol_library.start(
                engine.clone(),
                UiAction::SnapshotSymbolSheet,
                serde_json::to_value(SymbolSheetQuery { sheet_id: sheet }).expect("sheet query"),
            );
        }
        let blocked = self.files.busy() || self.schematic_object_draft_active() || self.schematic_wires.active() || !self.schematic.active;
        let mut open = true;
        let mut refresh = false;
        let mut load = false;
        let mut export = false;
        let mut place = false;
        egui::Window::new("Native symbol library").open(&mut open).default_width(500.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Reuse one rectangular single-unit symbol. Library files are detached; placing creates fresh pins and one undoable instance.");
            if blocked {
                ui.label("Finish or cancel the schematic draft or wire tool; select a sheet to place a library symbol.");
            }
            let ready = self.symbol_library.pending.is_none();
            refresh = ui.add_enabled(ready && context.is_some() && !self.files.busy(), egui::Button::new("Refresh library target sheet")).clicked();
            let fresh = self.symbol_library.guard.as_ref().is_some_and(|g| Some((g.expected_project, g.source_cursor, g.expected.id)) == context);
            if let Some(guard) = &self.symbol_library.guard {
                ui.label(format!(
                    "Target: {} · source {}:{}",
                    guard.expected.name.chars().take(64).collect::<String>(),
                    guard.source_cursor.stream_id,
                    guard.source_cursor.sequence
                ));
                if !fresh {
                    ui.label("Document changed; refresh the library target sheet before placement or export.");
                }
            }
            ui.add_enabled_ui(ready, |ui| {
                let label = ui.label("Symbol library file path");
                if ui.add(egui::TextEdit::singleline(&mut self.symbol_library.path).char_limit(4096)).labelled_by(label.id).changed() {
                    self.symbol_library.template = None;
                    self.symbol_library.message = None;
                }
                load = ui.add_enabled(context.is_some(), egui::Button::new("Load symbol template")).clicked();
                if let Some(template) = &self.symbol_library.template {
                    ui.label(format!("Loaded {} · {} pins · {}", template.name, template.pins.len(), template.value));
                    ui.label("Original instances and PCB links remain unchanged.");
                }
                ui.separator();
                let label = ui.label("New symbol reference");
                ui.add(egui::TextEdit::singleline(&mut self.symbol_library.reference).char_limit(128)).labelled_by(label.id);
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label("Symbol X (mm)");
                    ui.add(egui::TextEdit::singleline(&mut self.symbol_library.x).desired_width(110.0).char_limit(32)).labelled_by(label.id);
                    let label = ui.label("Symbol Y (mm)");
                    ui.add(egui::TextEdit::singleline(&mut self.symbol_library.y).desired_width(110.0).char_limit(32)).labelled_by(label.id);
                });
                ui.horizontal_wrapped(|ui| {
                    for quarter in 0..4 {
                        ui.radio_value(&mut self.symbol_library.rotation, quarter, format!("{}°", u16::from(quarter) * 90));
                    }
                    ui.checkbox(&mut self.symbol_library.mirrored, "Mirror symbol locally");
                });
                place = ui
                    .add_enabled(fresh && self.symbol_library.template.is_some() && !blocked, egui::Button::new("Place new library symbol"))
                    .clicked();
                ui.separator();
                ui.label("Export a committed symbol from this sheet to a new library file:");
                if let Some(guard) = &self.symbol_library.guard {
                    let symbols = guard.expected.content.as_ref().map(|c| &c.symbols);
                    let selected = symbols.and_then(|s| self.symbol_library.source_symbol.and_then(|id| s.get(&id)));
                    let label = ui.label("Library source symbol");
                    egui::ComboBox::from_id_salt("library_source_symbol")
                        .selected_text(selected.map_or("Choose symbol".into(), |s| s.designator.chars().take(64).collect::<String>()))
                        .show_ui(ui, |ui| {
                            if let Some(symbols) = symbols {
                                egui::ScrollArea::vertical().max_height(150.0).show_rows(
                                    ui,
                                    ui.text_style_height(&egui::TextStyle::Body),
                                    symbols.len(),
                                    |ui, rows| {
                                        for (id, symbol) in symbols.iter().skip(rows.start).take(rows.len()) {
                                            ui.selectable_value(
                                                &mut self.symbol_library.source_symbol,
                                                Some(*id),
                                                symbol.designator.chars().take(64).collect::<String>(),
                                            );
                                        }
                                    },
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id);
                }
                let label = ui.label("Library template name");
                ui.add(egui::TextEdit::singleline(&mut self.symbol_library.name).char_limit(128)).labelled_by(label.id);
                export = ui
                    .add_enabled(
                        fresh && self.symbol_library.source_symbol.is_some() && !self.files.busy(),
                        egui::Button::new("Export new symbol template file"),
                    )
                    .clicked();
            });
            if !ready {
                ui.label("Working on bounded symbol library data…");
            }
            if let Some(message) = &self.symbol_library.message {
                ui.label(message);
            }
        });
        self.symbol_library.open = open;
        if refresh && let Some((_, _, sheet_id)) = context {
            self.symbol_library.start(
                engine.clone(),
                UiAction::SnapshotSymbolSheet,
                serde_json::to_value(SymbolSheetQuery { sheet_id }).expect("sheet query"),
            );
        } else if load {
            self.symbol_library.start(
                engine.clone(),
                UiAction::LoadSymbolTemplate,
                serde_json::to_value(LoadSymbolTemplate { path: self.symbol_library.path.clone().into() }).expect("load query"),
            );
        } else if export && let (Some(guard), Some(symbol_id)) = (&self.symbol_library.guard, self.symbol_library.source_symbol) {
            let input = ExportSymbolTemplate {
                expected_project: guard.expected_project,
                source_cursor: guard.source_cursor,
                sheet_id: guard.expected.id,
                symbol_id,
                name: self.symbol_library.name.clone(),
                path: self.symbol_library.path.clone().into(),
            };
            self.symbol_library.start(engine.clone(), UiAction::ExportSymbolTemplate, serde_json::to_value(input).expect("export query"));
        } else if place {
            self.symbol_library.message = Some(
                match self.symbol_library.proposal().and_then(|command| {
                    UiAction::PlaceSymbolTemplate.run(&engine, serde_json::to_value(command).expect("placement")).map_err(|e| e.to_string())
                }) {
                    Ok(_) => "Placed fresh library symbol; refresh the target sheet before another placement".into(),
                    Err(error) => error,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> SymbolSheetSnapshot {
        let mut project = viamind_doc::Project::new("library worker");
        let sheet = viamind_doc::Sheet::new("Main");
        let sheet_id = sheet.id;
        project.schematic.sheets.insert(sheet_id, sheet);
        EngineHandle::new(project).snapshot_symbol_sheet(&SymbolSheetQuery { sheet_id }).unwrap()
    }
    #[test]
    fn exact_decimal_coordinates_reject_rounding_and_overflow() {
        for (text, expected) in [("-0.000001", -1), ("40.000003", 40_000_003), ("10000", 10_000_000_000), ("-10000.000000", -10_000_000_000)] {
            assert_eq!(millimetres(text).unwrap(), Nm(expected));
        }
        for text in ["", " 1", "+1", "1e2", "1.0000001", "10000.000001", "99999999999999999999999999999999", ".1", "NaN"] {
            assert!(millimetres(text).is_err(), "{text}");
        }
    }
    #[test]
    fn retained_hidden_worker_replacement_staleness_and_disconnect_do_not_publish_old_previews() {
        for replacement in 0..4 {
            let sheet = snapshot();
            let context = (sheet.expected_project, sheet.source_cursor, sheet.expected.id);
            let (sender, receiver) = sync_channel(1);
            let mut state = SymbolLibraryUi { context: Some(context), pending: Some(Pending { source: context, receiver }), ..Default::default() };
            let mut changed = context;
            match replacement {
                0 => changed.1.sequence += 1,
                1 => changed.1.stream_id = ObjectId::new(),
                2 => changed.2 = ObjectId::new(),
                _ => changed.0 = ObjectId::new(),
            }
            state.poll(Some(changed));
            assert!(state.pending.is_some(), "hidden/replaced worker stays retained");
            sender.try_send(Ok(Completed::Sheet(sheet))).unwrap();
            state.poll(Some(changed));
            assert!(state.guard.is_none());
            assert!(state.pending.is_none());
        }
        let sheet = snapshot();
        let context = (sheet.expected_project, sheet.source_cursor, sheet.expected.id);
        let (sender, receiver) = sync_channel(1);
        let mut state = SymbolLibraryUi { context: Some(context), pending: Some(Pending { source: context, receiver }), ..Default::default() };
        drop(sender);
        state.poll(Some(context));
        assert!(state.pending.is_none());
        assert!(state.message.unwrap().contains("worker stopped"));
    }
    #[test]
    fn duplicate_start_retains_worker_and_export_reports_complete_old_cursor() {
        let sheet = snapshot();
        let context = (sheet.expected_project, sheet.source_cursor, sheet.expected.id);
        let (sender, receiver) = sync_channel(1);
        let mut state = SymbolLibraryUi { context: Some(context), pending: Some(Pending { source: context, receiver }), ..Default::default() };
        state.start(EngineHandle::new(viamind_doc::Project::new("unused")), UiAction::LoadSymbolTemplate, serde_json::json!({"path":"/missing"}));
        sender
            .try_send(Ok(Completed::Exported(SymbolTemplateExportResult {
                path: "original.symbol.json".into(),
                exported_cursor: context.1,
                name: "Original".into(),
                pin_count: 3,
            })))
            .unwrap();
        let mut newer = context;
        newer.1.sequence += 1;
        state.poll(Some(newer));
        let message = state.message.unwrap();
        assert!(message.contains("document has newer edits"));
        assert!(message.contains(&context.1.stream_id.to_string()));
    }
}
