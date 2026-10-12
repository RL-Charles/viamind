//! Native polygon outline form, shared with the MCP commands.
//!
//! The curve-capable DXF form keeps one retained background worker (weak engine
//! reference, fallible spawn) for certified preview and guarded Apply. Its fields and
//! pending receiver survive Close/reopen; stale completions are discarded by local form
//! generation, project and full cursor, and a closed form never auto-applies.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::{Value, json};
use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use viamind_doc::ObjectId;
use viamind_doc::dxf::{DxfUnits, MAX_DXF_BYTES};
use viamind_doc::dxf_curves::DEFAULT_TOLERANCE_NM;
use viamind_engine::{DxfOutlinePreview, EngineHandle, EventCursor, ExpectedOutline, WeakEngineHandle};

struct OutlineDraft {
    outer: String,
    cutouts: String,
    expected: ExpectedOutline,
    message: Option<String>,
}

struct CurveWorker {
    receiver: Receiver<Result<Value, String>>,
    generation: u64,
    apply: bool,
}

/// Retained DXF import form: survives Close/reopen until explicitly reset.
struct CurveForm {
    path: String,
    units: DxfUnits,
    layer: String,
    tolerance: String,
    generation: u64,
    worker: Option<CurveWorker>,
    /// Fresh preview for `generation`, with the exact request it answered.
    preview: Option<(u64, DxfOutlinePreview, Value)>,
    message: Option<String>,
}
impl Default for CurveForm {
    fn default() -> Self {
        Self {
            path: String::new(),
            units: DxfUnits::Header,
            layer: String::new(),
            tolerance: DEFAULT_TOLERANCE_NM.to_string(),
            generation: 0,
            worker: None,
            preview: None,
            message: None,
        }
    }
}
impl CurveForm {
    fn busy(&self) -> bool {
        self.worker.is_some()
    }
    /// Any path/units/layer/tolerance change invalidates preview freshness.
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
    }
    fn reset(&mut self) {
        if self.busy() {
            return;
        }
        let generation = self.generation.wrapping_add(1);
        *self = Self { generation, ..Self::default() };
    }
    fn fresh(&self, project: ObjectId, cursor: EventCursor) -> Option<&(u64, DxfOutlinePreview, Value)> {
        self.preview.as_ref().filter(|(g, p, _)| *g == self.generation && p.expected.project_id == project && p.expected_cursor == cursor)
    }
    fn request(&self) -> Result<Value, String> {
        if self.path.is_empty() {
            return Err("Enter a DXF file path.".into());
        }
        let tolerance = self
            .tolerance
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|t| (10..=1_000_000).contains(t))
            .ok_or("Curve tolerance must be an integer number of nm in 10..1000000.")?;
        let mut request = json!({"path": self.path, "units": self.units, "tolerance_nm": tolerance});
        if !self.layer.is_empty() {
            request["layer"] = json!(self.layer);
        }
        Ok(request)
    }
    /// Returns true when an Apply completed successfully (the caller refreshes the draft).
    fn poll(&mut self, project: ObjectId, cursor: EventCursor, open: bool, ctx: &egui::Context) -> bool {
        let mut applied = false;
        if let Some(worker) = &self.worker {
            let value = match worker.receiver.try_recv() {
                Ok(v) => Some(v),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    None
                }
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("DXF worker disconnected; inspect the current outline before retrying.".into())),
            };
            if let Some(value) = value {
                let worker = self.worker.take().expect("worker present");
                if worker.apply {
                    // A guarded Apply already decided the design; always report its outcome.
                    match value {
                        Ok(value) => {
                            applied = true;
                            self.preview = None;
                            self.message = Some(format!(
                                "Applied curved DXF outline as one undoable change: {} vertices, {} cutouts, certified bound <= {} nm (polygon approximation).",
                                value["vertices"], value["cutouts"], value["achieved_bound_nm"]
                            ));
                        }
                        Err(error) => self.message = Some(format!("Apply failed; nothing changed: {error}")),
                    }
                } else if worker.generation != self.generation {
                    // A newer local draft supersedes this completion.
                } else if !open {
                    self.message = Some("A preview finished while the outline form was closed; preview again.".into());
                } else {
                    match value.and_then(|v| serde_json::from_value::<DxfOutlinePreview>(v).map_err(|e| e.to_string())) {
                        Ok(preview) if preview.expected.project_id == project && preview.expected_cursor == cursor => {
                            let request = self.request().unwrap_or(Value::Null);
                            self.preview = Some((worker.generation, preview, request));
                            self.message = None;
                        }
                        Ok(_) => self.message = Some("Document changed while previewing; stale preview discarded. Preview again.".into()),
                        Err(error) => {
                            self.preview = None;
                            self.message = Some(format!("Preview failed: {error}"));
                        }
                    }
                }
            }
        }
        if self.worker.is_none() && self.preview.is_some() && self.fresh(project, cursor).is_none() {
            self.preview = None;
            self.message = Some("Document or form changed; preview the DXF again.".into());
        }
        applied
    }
    fn start(&mut self, engine: &EngineHandle, project: ObjectId, cursor: EventCursor, apply: bool, ctx: egui::Context) {
        self.start_with(engine.downgrade(), project, cursor, apply, ctx, |task| {
            std::thread::Builder::new().name("viamind-dxf-outline".into()).spawn(task).map(|_| ())
        });
    }
    fn start_with(
        &mut self,
        engine: WeakEngineHandle,
        project: ObjectId,
        cursor: EventCursor,
        apply: bool,
        ctx: egui::Context,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        if self.busy() {
            return;
        }
        let (action, input) = if apply {
            let Some((_, preview, request)) = self.fresh(project, cursor) else {
                self.message = Some("A current preview for this form and document is required before Apply.".into());
                return;
            };
            let mut input = request.clone();
            input["expected_source_sha256"] = json!(preview.source_sha256);
            input["expected_cursor"] = json!(preview.expected_cursor);
            input["expected"] = json!(preview.expected);
            (UiAction::ApplyBoardOutlineDxf, input)
        } else {
            match self.request() {
                Ok(request) => (UiAction::PreviewBoardOutlineDxf, request),
                Err(error) => {
                    self.message = Some(error);
                    return;
                }
            }
        };
        let (tx, rx) = mpsc::channel();
        match spawn(Box::new(move || {
            let value = match engine.upgrade() {
                Some(engine) => action.run(&engine, input).map_err(|e| e.to_string()),
                None => Err("The design was closed before the DXF worker started.".into()),
            };
            let _ = tx.send(value);
            ctx.request_repaint();
        })) {
            Ok(()) => {
                self.worker = Some(CurveWorker { receiver: rx, generation: self.generation, apply });
                self.message = None;
            }
            Err(error) => self.message = Some(format!("Could not start DXF worker: {error}")),
        }
    }
}

#[derive(Default)]
pub struct OutlineUi {
    draft: Option<OutlineDraft>,
    curve: CurveForm,
}
impl OutlineUi {
    pub(crate) fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        self.draft = None;
    }
    /// A retained DXF worker is still preparing a preview or Apply.
    pub fn dxf_busy(&self) -> bool {
        self.curve.busy()
    }
    pub fn dxf_preview(&self) -> Option<&DxfOutlinePreview> {
        self.curve.preview.as_ref().map(|(_, p, _)| p)
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
fn draft_from(engine: &EngineHandle) -> OutlineDraft {
    let (expected, outer, cutouts) = engine.read(|project| {
        let expected = ExpectedOutline { project_id: project.id, outline: project.board.outline.clone() };
        if let Some(outline) = &project.board.outline {
            (expected, text(&outline.outer), outline.cutouts.iter().map(text).collect::<Vec<_>>().join("\n\n"))
        } else {
            let b = crate::board_scene::board_outline(project);
            (expected, format!("{},{}\n{},{}\n{},{}\n{},{}", b.min.x, b.min.y, b.max.x, b.min.y, b.max.x, b.max.y, b.min.x, b.max.y), String::new())
        }
    });
    OutlineDraft { outer, cutouts, expected, message: None }
}
fn show_curve_report(ui: &mut Ui, preview: &DxfOutlinePreview) {
    let r = &preview.report;
    ui.label(format!(
        "Polygon approximation: {} vertices, {} cutouts; certified bound <= {} nm (requested {} nm).",
        r.vertices, r.cutouts, r.achieved_bound_nm, r.tolerance_nm
    ));
    ui.label(format!(
        "{} selected entities, {} segments ({} curved), {} ignored; {} nm per unit{}.",
        r.selected_entities,
        r.selected_segments,
        r.curved_segments,
        r.ignored_entities,
        r.nm_per_unit,
        if r.unit_override { " (explicit override)" } else { "" }
    ));
    if !preview.changes_outline {
        ui.label("Geometry equals the current outline; Apply is a no-op.");
    }
    ui.small(format!("Source SHA256 {}", preview.source_sha256));
    egui::CollapsingHeader::new("Curve fidelity rows").id_salt("dxf-curve-rows").default_open(true).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("dxf-curve-row-scroll").max_height(140.0).show_rows(ui, 18.0, r.rows.len(), |ui, rows| {
            for index in rows {
                let row = &r.rows[index];
                ui.add(
                    egui::Label::new(format!(
                        "Entity {} {:?} seg {} · {:?} · {} chords · bound {} nm (sagitta {}, endpoint {})",
                        row.entity, row.entity_type, row.segment, row.kind, row.chords, row.bound_nm, row.sagitta_nm, row.quantization_nm
                    ))
                    .truncate(),
                );
            }
        });
    });
}
impl ViamindApp {
    pub fn begin_board_outline(&mut self) {
        if self.routing.editing()
            || self.track_cleanup.editing()
            || self.net_rules.editing()
            || self.files.busy()
            || self.copper.editing()
            || self.parts.placing()
            || self.pads.editing()
            || self.outlines.editing()
        {
            return;
        }
        let Some(design) = &self.design else { return };
        self.outlines.draft = Some(draft_from(&design.engine));
    }
    pub(crate) fn outline_dialog(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let (project, cursor) = engine.with_engine(|e| (e.project().id, e.event_cursor()));
        let open = self.outlines.draft.is_some();
        if self.outlines.curve.poll(project, cursor, open, ui.ctx())
            && let Some(draft) = &mut self.outlines.draft
        {
            *draft = draft_from(&engine);
        }
        let Some(draft) = &mut self.outlines.draft else { return };
        let curve = &mut self.outlines.curve;
        let busy = curve.busy();
        let fresh = curve.fresh(project, cursor).is_some();
        let mut apply = false;
        let mut cancel = false;
        let mut remove = false;
        let mut import = false;
        let mut preview_curve = false;
        let mut apply_curve = false;
        let mut reset_curve = false;
        let mut changed = false;
        let width = (ui.ctx().content_rect().width() - 32.0).clamp(240.0, 600.0);
        let height = (ui.ctx().content_rect().height() - 150.0).max(80.0);
        let modal = egui::Modal::new(egui::Id::new("board_outline_dialog")).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.heading("Board outline");
            ui.horizontal_wrapped(|ui| {
                apply = ui.button("Apply outline").clicked();
                remove = ui.button("Remove outline").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if let Some(message) = &draft.message {
                ui.label(message);
            }
            egui::ScrollArea::vertical().id_salt("board-outline-form").max_height(height).show(ui, |ui| {
                ui.label("Vertices: one X,Y pair in mm per line. Rings close automatically.");
                let outer = ui.label("Outer vertices");
                ui.add(egui::TextEdit::multiline(&mut draft.outer).desired_width(ui.available_width().min(380.0)).desired_rows(5))
                    .labelled_by(outer.id);
                let cutouts = ui.label("Cutout vertices");
                ui.add(egui::TextEdit::multiline(&mut draft.cutouts).desired_width(ui.available_width().min(380.0)).desired_rows(5))
                    .labelled_by(cutouts.id);
                ui.label("Separate cutout rings with an empty line.");
                ui.collapsing("Import ASCII DXF", |ui| {
                    let path = ui.label("DXF file path");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut curve.path).desired_width(ui.available_width()).char_limit(4096))
                        .labelled_by(path.id)
                        .changed();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("DXF units");
                        changed |= ui.selectable_value(&mut curve.units, DxfUnits::Header, "Drawing header").changed();
                        changed |= ui.selectable_value(&mut curve.units, DxfUnits::Mm, "Millimetres").changed();
                        changed |= ui.selectable_value(&mut curve.units, DxfUnits::Inches, "Inches").changed();
                    });
                    let layer = ui.label("DXF layer (empty = all)");
                    changed |= ui.add(egui::TextEdit::singleline(&mut curve.layer).char_limit(512)).labelled_by(layer.id).changed();
                    ui.label("Straight closed polylines or joined LINE loops. Curves require conversion.");
                    import = ui.add_enabled(!busy, egui::Button::new("Import DXF outline")).clicked();
                    ui.separator();
                    ui.label("Curve conversion: closed polylines with |bulge| <= 1, ARC+LINE loops and CIRCLE rings become a polygon approximation.");
                    let tolerance = ui.label("Curve tolerance (nm)");
                    changed |= ui.add(egui::TextEdit::singleline(&mut curve.tolerance).char_limit(16)).labelled_by(tolerance.id).changed();
                    ui.label(
                        "Native outlines stay polygonal. Bounds are conservative analytic upper bounds, not measured error; no topology, clearance or fabrication guarantee.",
                    );
                    ui.horizontal_wrapped(|ui| {
                        preview_curve = ui.add_enabled(!busy, egui::Button::new("Preview curved DXF")).clicked();
                        apply_curve = ui.add_enabled(!busy && fresh, egui::Button::new("Apply curved DXF outline")).clicked();
                        reset_curve = ui.add_enabled(!busy, egui::Button::new("Reset DXF form")).clicked();
                    });
                    if busy {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reading and certifying the bounded DXF source…");
                        });
                    }
                    if let Some(message) = &curve.message {
                        ui.label(message);
                    }
                    if let Some((_, preview, _)) = curve.fresh(project, cursor) {
                        show_curve_report(ui, preview);
                    }
                });
            });
        });
        if changed {
            curve.changed();
        }
        if cancel || modal.should_close() {
            self.outlines.cancel();
            return;
        }
        if reset_curve {
            curve.reset();
            return;
        }
        if preview_curve && !changed {
            curve.start(&engine, project, cursor, false, ui.ctx().clone());
            return;
        }
        if apply_curve && !changed {
            curve.start(&engine, project, cursor, true, ui.ctx().clone());
            return;
        }
        if apply || remove || import {
            let result = (|| {
                if import {
                    let file = std::fs::File::open(&curve.path).map_err(|e| format!("Could not open DXF: {e}"))?;
                    let mut text = String::new();
                    file.take((MAX_DXF_BYTES + 1) as u64).read_to_string(&mut text).map_err(|e| format!("Could not read ASCII DXF: {e}"))?;
                    let layer = if curve.layer.is_empty() { None } else { Some(&curve.layer) };
                    return UiAction::ImportBoardOutlineDxf
                        .run(&engine, json!({"text":text,"units":curve.units,"layer":layer,"expected":draft.expected}))
                        .map_err(|e| e.to_string());
                }
                if remove {
                    return UiAction::SetBoardOutline.run(&engine, json!({ "outline": null, "expected": draft.expected })).map_err(|e| e.to_string());
                }
                let outer = draft.outer.lines().filter(|line| !line.trim().is_empty()).collect::<Vec<_>>();
                let lines = draft.cutouts.lines().collect::<Vec<_>>();
                let cutouts =
                    lines.split(|line| line.trim().is_empty()).filter(|ring| !ring.is_empty()).map(parse_ring).collect::<Result<Vec<_>, _>>()?;
                let input = json!({ "outline": { "outer": parse_ring(&outer)?, "cutouts": cutouts }, "expected": draft.expected });
                UiAction::SetBoardOutline.run(&engine, input).map_err(|e| e.to_string())
            })();
            match result {
                Ok(_) => self.outlines.cancel(),
                Err(error) => draft.message = Some(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> EngineHandle {
        EngineHandle::new(viamind_doc::Project::new("keep"))
    }
    fn form() -> CurveForm {
        CurveForm { path: "outline.dxf".into(), ..Default::default() }
    }
    fn pending(form: &mut CurveForm, apply: bool) -> mpsc::Sender<Result<Value, String>> {
        let (tx, rx) = mpsc::channel();
        form.worker = Some(CurveWorker { receiver: rx, generation: form.generation, apply });
        tx
    }
    fn preview(engine: &EngineHandle) -> Value {
        let text = include_str!("../../doc/tests/fixtures/outline-rounded.dxf");
        let input = viamind_engine::PreviewBoardOutlineDxf {
            text: Some(text.into()),
            path: None,
            units: DxfUnits::Header,
            layer: Some("Edge".into()),
            tolerance_nm: 10_000,
        };
        serde_json::to_value(engine.preview_board_outline_dxf(&input).unwrap()).unwrap()
    }

    #[test]
    fn field_change_close_and_stream_change_discard_retained_preview_completions() {
        let engine = engine();
        let state = engine.session_state();
        let (project, cursor) = (state.project.id, state.cursor);
        let ctx = egui::Context::default();
        // Newer local draft: completion of the older generation is discarded silently.
        let mut f = form();
        let tx = pending(&mut f, false);
        f.path = "other.dxf".into();
        f.changed();
        assert!(f.busy());
        tx.send(Ok(preview(&engine))).unwrap();
        assert!(!f.poll(project, cursor, true, &ctx));
        assert!(!f.busy() && f.preview.is_none() && f.message.is_none());
        // Closed form: retained receiver completes, but its preview is never kept or applied.
        let mut f = form();
        let tx = pending(&mut f, false);
        tx.send(Ok(preview(&engine))).unwrap();
        f.poll(project, cursor, false, &ctx);
        assert!(f.preview.is_none() && f.message.as_ref().unwrap().contains("closed"));
        assert_eq!(engine.session_state(), state);
        // New stream: stale preview discarded with an actionable message.
        let mut f = form();
        let tx = pending(&mut f, false);
        tx.send(Ok(preview(&engine))).unwrap();
        let other = EventCursor { stream_id: ObjectId::new(), sequence: 0 };
        f.poll(project, other, true, &ctx);
        assert!(f.preview.is_none() && f.message.as_ref().unwrap().contains("stale"));
        // Matching completion is fresh until the document cursor moves.
        let mut f = form();
        let tx = pending(&mut f, false);
        tx.send(Ok(preview(&engine))).unwrap();
        f.poll(project, cursor, true, &ctx);
        assert!(f.fresh(project, cursor).is_some());
        f.poll(project, EventCursor { sequence: 1, ..cursor }, true, &ctx);
        assert!(f.preview.is_none() && f.message.as_ref().unwrap().contains("preview the DXF again"));
        // Disconnected worker is actionable.
        let mut f = form();
        drop(pending(&mut f, false));
        f.poll(project, cursor, true, &ctx);
        assert!(f.message.as_ref().unwrap().contains("disconnected"));
    }

    #[test]
    fn failed_spawn_busy_duplicate_start_reset_and_dropped_design_never_mutate() {
        let engine = engine();
        let before = engine.session_state();
        let (project, cursor) = (before.project.id, before.cursor);
        let mut f = form();
        f.start_with(engine.downgrade(), project, cursor, false, egui::Context::default(), |_| {
            Err(std::io::Error::other("controlled spawn failure"))
        });
        assert!(!f.busy() && f.message.as_ref().unwrap().contains("controlled spawn failure"));
        let _tx = pending(&mut f, false);
        f.start_with(engine.downgrade(), project, cursor, false, egui::Context::default(), |_| panic!("must retain the existing worker"));
        f.reset();
        assert!(f.busy() && f.path == "outline.dxf", "busy controls cannot reset the form");
        // Apply without a fresh preview never spawns.
        let mut f = form();
        f.start_with(engine.downgrade(), project, cursor, true, egui::Context::default(), |_| panic!("no apply without fresh preview"));
        assert!(f.message.as_ref().unwrap().contains("current preview"));
        // Invalid tolerance is refused locally.
        f.tolerance = "9".into();
        f.start_with(engine.downgrade(), project, cursor, false, egui::Context::default(), |_| panic!("no preview with invalid tolerance"));
        assert!(f.message.as_ref().unwrap().contains("10..1000000"));
        // A worker whose design was dropped reports instead of keeping the design alive.
        let weak = EngineHandle::new(viamind_doc::Project::new("gone")).downgrade();
        let mut f = form();
        f.start_with(weak, project, cursor, false, egui::Context::default(), |task| {
            task();
            Ok(())
        });
        f.poll(project, cursor, true, &egui::Context::default());
        assert!(f.message.as_ref().unwrap().contains("closed"));
        assert_eq!(engine.session_state(), before);
    }
}
