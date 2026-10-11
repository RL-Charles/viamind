//! Local terminal choice and bounded asynchronous stable-link probing.
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

use egui::{Color32, Stroke, Ui};
use viamind_doc::{ObjectId, Side};
use viamind_engine::{EventCursor, LinkedPinPad, PinPadLinkQuery, PinPadLinkResult};

use crate::{ViamindApp, camera::WorldPos, layers::LayerId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub project: ObjectId,
    pub cursor: EventCursor,
    pub schematic: bool,
    pub ids: Vec<ObjectId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    source: Source,
    component: ObjectId,
    offset: usize,
}
struct Pending {
    context: Context,
    receiver: Receiver<Result<PinPadLinkResult, String>>,
}
#[derive(Default)]
pub struct PinPadProbeUi {
    current: Option<Context>,
    attempted: Option<Context>,
    pending: Option<Pending>,
    result: Option<PinPadLinkResult>,
    selected: Option<usize>,
    message: Option<String>,
}
impl PinPadProbeUi {
    /// Current local exact physical pair; does not alter engine selection.
    pub fn selected_terminal(&self) -> Option<(ObjectId, ObjectId)> {
        self.selected().and_then(|(_, row)| Some((row.pin_id?, row.pad_id?)))
    }
    fn poll(&mut self) {
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Terminal query worker stopped; inspect terminals again.".into()),
        };
        let pending = self.pending.take().expect("completed terminal worker");
        if self.current.as_ref() != Some(&pending.context) {
            return;
        }
        match result {
            Ok(report) if report.project_id == pending.context.source.project && report.source_cursor == pending.context.source.cursor => {
                self.result = Some(report);
                self.message = None;
            }
            Ok(_) => {
                self.current = None;
                self.message = Some("Document changed during terminal inspection; inspect again.".into());
            }
            Err(error) => self.message = Some(error),
        }
    }
    fn selected(&self) -> Option<(&PinPadLinkResult, &LinkedPinPad)> {
        let report = self.result.as_ref()?;
        let row = report.terminals.get(self.selected?)?;
        row.navigable.then_some((report, row))
    }
    pub(crate) fn pin_highlighted(&self, sheet: Option<ObjectId>, pin: ObjectId) -> bool {
        self.selected().is_some_and(|(r, p)| r.component.sheet_id == sheet && p.pin_id == Some(pin))
    }
}
impl ViamindApp {
    pub(crate) fn inspect_pin_pads(&mut self, component: ObjectId, offset: usize) {
        let Some(source) = self.cross_probe_source() else { return };
        if !source.ids.contains(&component) {
            self.pin_pad_probe.message = Some("Source selection changed; select the component again.".into());
            return;
        }
        self.pin_pad_probe.current = Some(Context { source, component, offset });
        self.pin_pad_probe.result = None;
        self.pin_pad_probe.selected = None;
        self.pin_pad_probe.attempted = None;
        self.pin_pad_probe.message = None;
    }
    pub(crate) fn sync_pin_pad_probe(&mut self, ui: &Ui) {
        let source = self.cross_probe_source();
        if self.pin_pad_probe.current.as_ref().is_some_and(|c| source.as_ref() != Some(&c.source)) {
            self.pin_pad_probe.current = None;
            self.pin_pad_probe.result = None;
            self.pin_pad_probe.selected = None;
            self.pin_pad_probe.message = Some("Document or source selection changed; inspect terminals again.".into());
        }
        self.pin_pad_probe.poll();
        if let Some(context) = self.pin_pad_probe.current.clone()
            && self.pin_pad_probe.pending.is_none()
            && self.pin_pad_probe.attempted.as_ref() != Some(&context)
            && let Some(design) = &self.design
        {
            let engine = design.engine.clone();
            let query = PinPadLinkQuery { component_id: context.component, offset: context.offset, limit: Some(100) };
            let (sender, receiver) = sync_channel(1);
            match std::thread::Builder::new().name("viamind-terminal-probe".into()).spawn(move || {
                let _ = sender.try_send(engine.query_linked_pin_pads(&query).map_err(|e| e.to_string()));
            }) {
                Ok(_) => {
                    self.pin_pad_probe.attempted = Some(context.clone());
                    self.pin_pad_probe.pending = Some(Pending { context, receiver });
                }
                Err(error) => self.pin_pad_probe.message = Some(format!("Could not start terminal query: {error}")),
            }
        }
        if self.pin_pad_probe.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
    }
    pub(crate) fn pin_pad_probe_panel(&mut self, ui: &mut Ui) {
        if self.pin_pad_probe.current.is_none() && self.pin_pad_probe.message.is_none() {
            return;
        }
        ui.separator();
        ui.heading("Pins and pads");
        ui.label("Terminal choice and highlights are local. PCB component selection is preserved.");
        if let Some(message) = &self.pin_pad_probe.message {
            ui.label(message);
        }
        let mut page = None;
        let mut navigate = None;
        if let Some(report) = &self.pin_pad_probe.result {
            ui.label(format!("Coverage: {:?} · {} terminal numbers", report.coverage, report.total));
            egui::ScrollArea::vertical().id_salt("pin_pad_rows").max_height(180.0).show(ui, |ui| {
                for (index, row) in report.terminals.iter().enumerate() {
                    let caption = format!(
                        "{} {} · pin {} / pad {}",
                        row.number,
                        row.pin_name.as_deref().unwrap_or(""),
                        if row.pin_id.is_some() { "present" } else { "missing" },
                        if row.pad_id.is_some() { "present" } else { "missing" }
                    );
                    if ui
                        .selectable_label(self.pin_pad_probe.selected == Some(index), &caption)
                        .on_hover_text(format!("Pin {:?}\nPad {:?}", row.pin_id, row.pad_id))
                        .clicked()
                    {
                        self.pin_pad_probe.selected = Some(index);
                    }
                    for reason in &row.unsupported_reasons {
                        ui.add(egui::Label::new(reason).wrap());
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if let Some(context) = &self.pin_pad_probe.current {
                    if context.offset > 0 && ui.button("Previous terminals").clicked() {
                        page = Some((context.component, context.offset.saturating_sub(100)));
                    }
                    if let Some(offset) = report.next_offset
                        && ui.button("Next terminals").clicked()
                    {
                        page = Some((context.component, offset));
                    }
                }
            });
            let enabled = self.pin_pad_probe.selected().is_some();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(enabled, egui::Button::new("Show schematic pin")).clicked() {
                    navigate = Some(false);
                }
                if ui.add_enabled(enabled, egui::Button::new("Show PCB pad")).clicked() {
                    navigate = Some(true);
                }
            });
        } else if self.pin_pad_probe.pending.is_some() {
            ui.label("Reading terminal identities…");
        }
        if let Some((component, offset)) = page {
            self.inspect_pin_pads(component, offset);
        }
        if let Some(board) = navigate
            && let Err(error) = self.navigate_pin_pad(board)
        {
            self.pin_pad_probe.message = Some(error);
        }
    }
    fn terminal_board_visible(&self, side: Option<Side>) -> bool {
        let id = match side {
            Some(Side::Top) => LayerId::F_CU,
            Some(Side::Bottom) => LayerId::B_CU,
            None => return false,
        };
        self.layers.get(id).is_some_and(|layer| layer.visible && layer.opacity > 0.0)
    }
    fn navigate_pin_pad(&mut self, board: bool) -> Result<(), String> {
        let context = self.pin_pad_probe.current.clone().ok_or("Inspect current terminals again.")?;
        if self.cross_probe_source().as_ref() != Some(&context.source) {
            return Err("Source selection changed; inspect again.".into());
        }
        let (report, row) = self.pin_pad_probe.selected().ok_or("Select a supported paired terminal first.")?;
        let component = report.component.clone();
        let row = row.clone();
        if self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
            || self.net_rules.editing()
            || self.schematic_wires.active()
            || self.schematic_object_draft_active()
        {
            return Err("Finish or cancel the active edit before terminal navigation.".into());
        }
        let design = self.design.as_ref().ok_or("Document closed.")?;
        let fresh = design.engine.with_engine(|e| {
            e.project().id == context.source.project
                && e.event_cursor() == context.source.cursor
                && (context.source.schematic || e.selection().ids == context.source.ids)
        });
        if !fresh
            || (context.source.schematic
                && self.schematic_probe_selection(context.source.project, context.source.cursor.stream_id).into_iter().collect::<Vec<_>>()
                    != context.source.ids)
        {
            return Err("Document or source selection changed; inspect terminals again.".into());
        }
        if board {
            if !self.terminal_board_visible(row.board_side) {
                return Err("Destination copper layer is hidden; enable it explicitly before navigating.".into());
            }
            let at = row.board_position.ok_or("PCB center is unsupported.")?;
            self.set_schematic_mode(false);
            self.canvas.focus_world(WorldPos::new(at.x.to_mm(), at.y.to_mm()));
        } else {
            self.navigate_terminal_pin(context.source.project, context.source.cursor, &component, &row)?;
        }
        Ok(())
    }
    pub(crate) fn paint_pin_pad_probe(&self, ui: &Ui, viewport: egui::Rect) {
        let Some((_, row)) = self.pin_pad_probe.selected() else { return };
        if self.terminal_board_visible(row.board_side)
            && let Some(at) = row.board_position
        {
            let at = self.canvas.camera.world_to_screen(WorldPos::new(at.x.to_mm(), at.y.to_mm()), viewport);
            ui.painter_at(viewport).circle_stroke(at, 10.0, Stroke::new(3.0, Color32::from_rgb(80, 230, 255)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Context, PinPadLinkResult) {
        let e = viamind_engine::EngineHandle::new(
            viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap(),
        );
        let id = e.read(|p| *p.board.parts.keys().next().unwrap());
        let result = e.query_linked_pin_pads(&PinPadLinkQuery { component_id: id, offset: 0, limit: None }).unwrap();
        (
            Context {
                source: Source { project: result.project_id, cursor: result.source_cursor, schematic: false, ids: vec![id] },
                component: id,
                offset: 0,
            },
            result,
        )
    }
    #[test]
    fn one_pending_receiver_survives_replacement_and_rejects_old_stream_selection_and_page() {
        for case in 0..3 {
            let (context, result) = fixture();
            let mut next = context.clone();
            match case {
                0 => next.source.cursor.stream_id = ObjectId::new(),
                1 => next.source.ids.clear(),
                _ => next.offset = 100,
            }
            let (sender, receiver) = sync_channel(1);
            let mut ui = PinPadProbeUi { current: Some(next.clone()), pending: Some(Pending { context, receiver }), ..Default::default() };
            ui.poll();
            assert!(ui.pending.is_some());
            sender.try_send(Ok(result)).unwrap();
            ui.poll();
            assert!(ui.pending.is_none());
            assert!(ui.result.is_none());
            assert_eq!(ui.current, Some(next));
            assert!(ui.selected_terminal().is_none());
        }
    }
    #[test]
    fn completion_with_changed_cursor_or_disconnect_never_publishes_terminal_highlights() {
        let (context, mut result) = fixture();
        result.source_cursor.sequence += 1;
        let (sender, receiver) = sync_channel(1);
        sender.try_send(Ok(result)).unwrap();
        let mut ui =
            PinPadProbeUi { current: Some(context.clone()), pending: Some(Pending { context: context.clone(), receiver }), ..Default::default() };
        ui.poll();
        assert!(ui.current.is_none());
        assert!(ui.result.is_none());
        assert!(ui.message.unwrap().contains("changed"));
        let (sender, receiver) = sync_channel(1);
        drop(sender);
        let mut ui = PinPadProbeUi { current: Some(context.clone()), pending: Some(Pending { context, receiver }), ..Default::default() };
        ui.poll();
        assert!(ui.message.unwrap().contains("worker stopped"));
    }
}
