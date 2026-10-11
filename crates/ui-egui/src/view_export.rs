//! Immutable view export: bounded capture, rendering and publication run on one worker.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};
use viamind_doc::ObjectId;
use viamind_engine::{EngineError, EngineHandle, EventCursor};
use viamind_render::{BoardViewRequest, SchematicLayer, SchematicViewOptions, SchematicViewRequest, SvgOptions, ViewBounds, ViewLayer};

/// Run the same bounded render/publication APIs as the matching MCP tools.
/// Desktop actions return metadata and path; MCP additionally transports image content.
pub(crate) fn export(engine: &EngineHandle, action: UiAction, input: Value) -> Result<Value, EngineError> {
    let invalid = |message: String| EngineError::InvalidInput { command: action.tool().into(), message };
    let schematic = matches!(action, UiAction::RenderSchematicSvg | UiAction::RenderSchematicPng);
    let png = matches!(action, UiAction::RenderBoardPng | UiAction::RenderSchematicPng);
    let result = (|| -> Result<Value, String> {
        if schematic {
            let request: SchematicViewRequest<EventCursor> = serde_json::from_value(input).map_err(|e| e.to_string())?;
            request.options.validate().map_err(|e| e.to_string())?;
            let (snapshot, cursor, native) = engine.with_engine(|e| {
                let cursor = e.event_cursor();
                if request.expected_cursor.is_some_and(|expected| expected != cursor) {
                    return Err("Design changed before snapshot capture. Retry the export.".to_owned());
                }
                let snapshot = viamind_render::schematic_view_snapshot(e.project(), request.sheet).map_err(|e| e.to_string())?;
                Ok((snapshot, cursor, e.document_status().path))
            })?;
            let (path, metadata) = if png {
                let rendered = viamind_render::schematic_png(&snapshot, &request.options).map_err(|e| e.to_string())?;
                let path =
                    request.path.map(|p| viamind_render::write_png(&rendered.png, &p, native.as_deref())).transpose().map_err(|e| e.to_string())?;
                (path, json!(rendered.metadata))
            } else {
                let rendered = viamind_render::schematic_svg(&snapshot, &request.options).map_err(|e| e.to_string())?;
                let path =
                    request.path.map(|p| viamind_render::write_svg(&rendered.svg, &p, native.as_deref())).transpose().map_err(|e| e.to_string())?;
                (path, json!(rendered.metadata))
            };
            Ok(
                json!({"path":path,"metadata":metadata,"source_cursor":cursor,"view":"schematic","sheet":request.sheet,"format":if png { "PNG" } else { "SVG" }}),
            )
        } else {
            let request: BoardViewRequest<EventCursor> = serde_json::from_value(input).map_err(|e| e.to_string())?;
            if png { viamind_render::validate_png_options(&request.options) } else { request.options.validate() }.map_err(|e| e.to_string())?;
            let (snapshot, cursor, native) = engine.with_engine(|e| {
                let cursor = e.event_cursor();
                if request.expected_cursor.is_some_and(|expected| expected != cursor) {
                    return Err("Design changed before snapshot capture. Retry the export.".to_owned());
                }
                let snapshot = viamind_render::board_view_snapshot(e.project()).map_err(|e| e.to_string())?;
                Ok((snapshot, cursor, e.document_status().path))
            })?;
            let (path, metadata) = if png {
                let rendered = viamind_render::board_png(&snapshot, &request.options).map_err(|e| e.to_string())?;
                let path =
                    request.path.map(|p| viamind_render::write_png(&rendered.png, &p, native.as_deref())).transpose().map_err(|e| e.to_string())?;
                (path, json!(rendered.metadata))
            } else {
                let rendered = viamind_render::board_svg(&snapshot, &request.options).map_err(|e| e.to_string())?;
                let path =
                    request.path.map(|p| viamind_render::write_svg(&rendered.svg, &p, native.as_deref())).transpose().map_err(|e| e.to_string())?;
                (path, json!(rendered.metadata))
            };
            Ok(json!({"path":path,"metadata":metadata,"source_cursor":cursor,"view":"board","format":if png { "PNG" } else { "SVG" }}))
        }
    })();
    result.map_err(invalid)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Board,
    Schematic,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Svg,
    Png,
}

pub struct ViewExportUi {
    open: bool,
    target: Target,
    format: Format,
    sheet: Option<ObjectId>,
    path: String,
    width: String,
    height: String,
    padding: String,
    crop: bool,
    region: [String; 4],
    board_layers: [bool; 3],
    schematic_layers: [bool; 5],
    worker: Option<Receiver<Result<Value, String>>>,
    message: Option<String>,
    result: Option<Value>,
}
impl Default for ViewExportUi {
    fn default() -> Self {
        Self {
            open: false,
            target: Target::Board,
            format: Format::Svg,
            sheet: None,
            path: String::new(),
            width: "1600".into(),
            height: "1000".into(),
            padding: "1".into(),
            crop: false,
            region: ["0".into(), "0".into(), "30".into(), "20".into()],
            board_layers: [true; 3],
            schematic_layers: [true; 5],
            worker: None,
            message: None,
            result: None,
        }
    }
}
fn field(ui: &mut Ui, label: &str, value: &mut String, width: f32, limit: usize) {
    let label = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(width).char_limit(limit)).labelled_by(label.id);
}
/// Exact decimal millimetres to integer nm; rejects exponent notation and sub-nm input.
fn mm(text: &str) -> Result<i64, String> {
    let text = text.trim();
    let negative = text.starts_with('-');
    let text = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() || !whole.bytes().all(|c| c.is_ascii_digit()) || fraction.len() > 6 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Use decimal millimetres with at most six decimal places (for example 1.25).".into());
    }
    let whole: i64 = whole.parse().map_err(|_| "Millimetre value is too large.")?;
    let digits = u32::try_from(fraction.len()).map_err(|_| "Invalid millimetres.")?;
    let fraction: i64 = if fraction.is_empty() { 0 } else { fraction.parse().map_err(|_| "Invalid millimetres.")? };
    let value = whole.checked_mul(1_000_000).and_then(|n| n.checked_add(fraction * 10i64.pow(6 - digits))).ok_or("Millimetre value is too large.")?;
    Ok(if negative { -value } else { value })
}
impl ViewExportUi {
    /// Includes work continuing while the export window is closed.
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn request(&self, cursor: EventCursor) -> Result<(UiAction, Value), String> {
        let path = PathBuf::from(self.path.trim());
        if self.path.trim().is_empty() {
            return Err("Enter a NEW output file path in an existing directory.".into());
        }
        let extension = if self.format == Format::Svg { "svg" } else { "png" };
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension)) {
            return Err(format!("Output path must end in .{extension} for the selected format."));
        }
        let width_px = self.width.trim().parse().map_err(|_| "Width needs a whole number of pixels.")?;
        let height_px = self.height.trim().parse().map_err(|_| "Height needs a whole number of pixels.")?;
        let padding_nm = if self.crop { 0 } else { mm(&self.padding)? };
        let region_nm = if self.crop {
            Some(ViewBounds { min_x: mm(&self.region[0])?, min_y: mm(&self.region[1])?, max_x: mm(&self.region[2])?, max_y: mm(&self.region[3])? })
        } else {
            None
        };
        let (action, input) = if self.target == Target::Board {
            let layers = [ViewLayer::BottomCopper, ViewLayer::TopCopper, ViewLayer::Outline]
                .into_iter()
                .zip(self.board_layers)
                .filter_map(|(layer, on)| on.then_some(layer))
                .collect();
            let options = SvgOptions { layers, width_px, height_px, padding_nm, region_nm };
            if self.format == Format::Png { viamind_render::validate_png_options(&options) } else { options.validate() }
                .map_err(|e| e.to_string())?;
            (
                if self.format == Format::Svg { UiAction::RenderBoardSvg } else { UiAction::RenderBoardPng },
                json!(BoardViewRequest { options, path: Some(path), expected_cursor: Some(cursor) }),
            )
        } else {
            let sheet = self.sheet.ok_or("Select the schematic sheet to export.")?;
            let layers = [SchematicLayer::Wires, SchematicLayer::Symbols, SchematicLayer::Pins, SchematicLayer::Junctions, SchematicLayer::Labels]
                .into_iter()
                .zip(self.schematic_layers)
                .filter_map(|(layer, on)| on.then_some(layer))
                .collect();
            let options = SchematicViewOptions { layers, width_px, height_px, padding_nm, region_nm };
            options.validate().map_err(|e| e.to_string())?;
            (
                if self.format == Format::Svg { UiAction::RenderSchematicSvg } else { UiAction::RenderSchematicPng },
                json!(SchematicViewRequest { sheet, options, path: Some(path), expected_cursor: Some(cursor) }),
            )
        };
        Ok((action, input))
    }
    fn start(&mut self, engine: EngineHandle, ctx: egui::Context) {
        if self.busy() {
            return;
        }
        self.result = None;
        let (action, input) = match self.request(engine.event_cursor()) {
            Ok(request) => request,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new().name("viamind-view-export".into()).spawn(move || {
            let _ = sender.send(action.run(&engine, input).map_err(|e| e.to_string()));
            ctx.request_repaint();
        }) {
            Ok(_) => {
                self.worker = Some(receiver);
                self.message = Some("Exporting immutable snapshot… Closing this window keeps the export running.".into());
            }
            Err(e) => self.message = Some(format!("Could not start export worker: {e}")),
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    match result {
                        Ok(value) => {
                            self.message = None;
                            self.result = Some(value);
                        }
                        Err(e) => self.message = Some(format!("Export failed: {e}")),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.message = Some("Export worker disconnected. Check the destination before retrying.".into());
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            }
        }
    }
}
impl ViamindApp {
    pub fn begin_view_export(&mut self) {
        if self.design.is_some() {
            self.view_export.open = true;
        }
    }
    pub(crate) fn view_export_window(&mut self, ui: &mut Ui) {
        self.view_export.poll(ui.ctx());
        if !self.view_export.open {
            return;
        }
        let Some(design) = &self.design else {
            return;
        };
        let engine = design.engine.clone();
        // Bound chooser work independently of selected-sheet render capture. Never clone contents.
        let (sheets, more, current) = engine.with_engine(|e| {
            let sheets =
                e.project().schematic.sheets.values().take(128).map(|s| (s.id, s.name.chars().take(64).collect::<String>())).collect::<Vec<_>>();
            (sheets, e.project().schematic.sheets.len() > 128, e.event_cursor())
        });
        let state = &mut self.view_export;
        let mut open = state.open;
        let mut start = false;
        egui::Window::new("Export rendered view").open(&mut open).default_width(620.0).show(ui.ctx(), |ui| {
            ui.label("Read-only inspection image • NEW file required • existing files are preserved.");
            ui.add_enabled_ui(!state.busy(), |ui| {
                ui.horizontal(|ui| { ui.selectable_value(&mut state.target, Target::Board, "Board view"); ui.selectable_value(&mut state.target, Target::Schematic, "Schematic sheet"); });
                if state.target == Target::Schematic {
                    let label = ui.label("Schematic sheet to export");
                    egui::ComboBox::from_id_salt("view_export_sheet").selected_text(state.sheet.map_or("Choose a sheet".into(), |id| sheets.iter().find(|(key, _)| *key == id).map_or_else(|| format!("Missing sheet {id}"), |(_, name)| name.clone()))).show_ui(ui, |ui| {
                        for (id, name) in &sheets { ui.selectable_value(&mut state.sheet, Some(*id), format!("{name} ({id})")); }
                    }).response.labelled_by(label.id);
                    if more { ui.label("Chooser shows the first 128 sheets; use MCP or CLI for other sheet IDs."); }
                    ui.label("Selected sheet only: symbol bodies, pin stems, wires, junctions and scoped labels. Bundled outlined text; no hierarchy or electrical certification.");
                } else { ui.label("Explicit outer copper pads, tracks, through vias and polygon outline/cutouts. Legacy previews omitted; no inner copper, pours or fabrication certification."); }
                ui.horizontal(|ui| { ui.selectable_value(&mut state.format, Format::Svg, "SVG"); ui.selectable_value(&mut state.format, Format::Png, "PNG"); });
                field(ui, "New output file", &mut state.path, 540.0, 4096);
                ui.horizontal(|ui| { field(ui, "Width (px)", &mut state.width, 65.0, 5); field(ui, "Height (px)", &mut state.height, 65.0, 5); });
                ui.label("Dimensions: 1–4096 each; PNG and schematic views: at most 4,194,304 pixels.");
                ui.horizontal(|ui| {
                    if state.target == Target::Board { for (value, name) in state.board_layers.iter_mut().zip(["Bottom copper", "Top copper", "Outline"]) { ui.checkbox(value, name); } }
                    else { for (value, name) in state.schematic_layers.iter_mut().zip(["Wires", "Symbols", "Pins", "Junctions", "Labels"]) { ui.checkbox(value, name); } }
                });
                ui.checkbox(&mut state.crop, "Explicit crop (mm, X right / Y down)");
                if state.crop {
                    ui.horizontal(|ui| { field(ui, "Minimum X (mm)", &mut state.region[0], 75.0, 24); field(ui, "Minimum Y (mm)", &mut state.region[1], 75.0, 24); });
                    ui.horizontal(|ui| { field(ui, "Maximum X (mm)", &mut state.region[2], 75.0, 24); field(ui, "Maximum Y (mm)", &mut state.region[3], 75.0, 24); });
                } else { field(ui, "Auto-fit padding (mm)", &mut state.padding, 90.0, 24); }
                start = ui.button("Export snapshot to NEW file").clicked();
            });
            if let Some(message) = &state.message { ui.label(message); }
            if let Some(value) = &state.result {
                ui.label(format!("Exported {}", value["path"].as_str().unwrap_or("")));
                ui.label(format!("{} {} view{}", value["format"].as_str().unwrap_or(""), value["view"].as_str().unwrap_or(""), value["sheet"].as_str().map_or(String::new(), |id| format!(" · sheet {id}"))));
                ui.label(format!("Source snapshot {} · {}", value["source_cursor"]["stream_id"].as_str().unwrap_or(""), value["source_cursor"]["sequence"]));
                if serde_json::from_value::<EventCursor>(value["source_cursor"].clone()).ok() != Some(current) { ui.label("The design has changed since this exported snapshot."); }
                if let Some(count) = value["metadata"]["omitted_legacy_parts"].as_u64().filter(|n| *n > 0) { ui.label(format!("Omitted {count} legacy footprint previews.")); }
            }
        });
        state.open = open;
        if start {
            state.start(engine, ui.ctx().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_millimetres_are_exact_and_invalid_precision_is_rejected() {
        for (text, expected) in [("-0.000001", -1), ("+12.345678", 12_345_678), ("0", 0), ("1.2", 1_200_000)] {
            assert_eq!(mm(text).unwrap(), expected);
        }
        for text in ["", ".1", "1e2", "NaN", "0.0000001", "1.2.3", "9999999999999999999", "9223372036854.775808"] {
            assert!(mm(text).is_err(), "{text}");
        }
    }
    #[test]
    fn pending_job_prevents_duplicates_and_hidden_poll_handles_completion_and_disconnect() {
        let ctx = egui::Context::default();
        let (sender, receiver) = mpsc::channel();
        let mut state = ViewExportUi { worker: Some(receiver), ..Default::default() };
        state.start(EngineHandle::new(viamind_doc::Project::new("pending")), ctx.clone());
        assert!(state.busy());
        assert!(state.message.is_none(), "duplicate start must not validate or start a second job");
        state.poll(&ctx);
        assert!(state.busy());
        sender.send(Ok(json!({"path":"hidden.svg"}))).unwrap();
        state.poll(&ctx);
        assert!(!state.busy());
        assert_eq!(state.result.as_ref().unwrap()["path"], "hidden.svg");
        let (sender, receiver) = mpsc::channel();
        state.worker = Some(receiver);
        drop(sender);
        state.poll(&ctx);
        assert!(!state.busy());
        assert!(state.message.unwrap().contains("disconnected"));
    }
}
