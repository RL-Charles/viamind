//! Local counterpart highlights over the shared bounded stable-link query.
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

use egui::{Color32, Stroke, Ui};
use viamind_doc::{ObjectId, Side};
use viamind_engine::{ComponentLinkQuery, ComponentLinkResult, ComponentLinkStatus, EventCursor, LinkedComponent};

use crate::{ViamindApp, camera::WorldPos, layers::LayerId};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    project: ObjectId,
    cursor: EventCursor,
    schematic_source: bool,
    ids: Vec<ObjectId>,
}
struct Pending {
    context: Context,
    receiver: Receiver<Result<ComponentLinkResult, String>>,
}
#[derive(Default)]
pub struct CrossProbeUi {
    document: Option<(ObjectId, ObjectId)>,
    board_revision: Option<u64>,
    schematic_selection: Option<ObjectId>,
    source: Option<(bool, Vec<ObjectId>)>,
    current: Option<Context>,
    attempted: Option<Context>,
    pending: Option<Pending>,
    result: Option<ComponentLinkResult>,
    message: Option<String>,
}
impl CrossProbeUi {
    /// Derived local highlights, independent of shared edit selection.
    pub fn highlighted_symbols(&self) -> Vec<ObjectId> {
        if self.current.as_ref().is_some_and(|c| !c.schematic_source) {
            self.result
                .as_ref()
                .map(|r| r.components.iter().filter(|row| row.status == ComponentLinkStatus::Linked).filter_map(|row| row.symbol_id).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
    pub fn highlighted_parts(&self) -> Vec<ObjectId> {
        if self.current.as_ref().is_some_and(|c| c.schematic_source) {
            self.result
                .as_ref()
                .map(|r| r.components.iter().filter(|row| row.status == ComponentLinkStatus::Linked).filter_map(|row| row.part_id).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn poll(&mut self) {
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Component query worker stopped; change selection to retry.".into()),
        };
        let pending = self.pending.take().expect("completed query");
        if self.current.as_ref() != Some(&pending.context) {
            return;
        }
        match result {
            Ok(result) if result.project_id == pending.context.project && result.source_cursor == pending.context.cursor => {
                self.result = Some(result);
                self.message = None;
            }
            Ok(_) => self.attempted = None,
            Err(error) => self.message = Some(error),
        }
    }
    pub(crate) fn symbol_highlighted(&self, id: ObjectId) -> bool {
        self.current.as_ref().is_some_and(|c| !c.schematic_source)
            && self
                .result
                .as_ref()
                .is_some_and(|r| r.components.iter().any(|row| row.status == ComponentLinkStatus::Linked && row.symbol_id == Some(id)))
    }
}
impl ViamindApp {
    pub(crate) fn sync_cross_probe(&mut self, ui: &Ui) {
        let Some(design) = &self.design else {
            self.cross_probe.current = None;
            self.cross_probe.result = None;
            self.cross_probe.poll();
            return;
        };
        let engine = design.engine.clone();
        let (project, cursor, revision, board_ids) = engine.with_engine(|e| {
            (e.project().id, e.event_cursor(), e.selection().revision, e.selection().ids.iter().take(101).copied().collect::<Vec<_>>())
        });
        let selected = self.schematic_probe_selection(project, cursor.stream_id);
        let document = Some((project, cursor.stream_id));
        if self.cross_probe.document != document {
            self.cross_probe.document = document;
            self.cross_probe.board_revision = None;
            self.cross_probe.schematic_selection = None;
            self.cross_probe.source = None;
            self.cross_probe.result = None;
            self.cross_probe.message = None;
        }
        let board_changed = self.cross_probe.board_revision != Some(revision);
        let schematic_changed = self.cross_probe.schematic_selection != selected;
        if schematic_changed && self.schematic.active {
            self.cross_probe.source = Some((true, selected.into_iter().collect()));
        } else if board_changed {
            self.cross_probe.source = Some((false, board_ids));
        } else if self.cross_probe.source.is_none() {
            self.cross_probe.source = Some((self.schematic.active, selected.into_iter().collect()));
        }
        self.cross_probe.board_revision = Some(revision);
        self.cross_probe.schematic_selection = selected;
        let (schematic_source, ids) = self.cross_probe.source.clone().expect("source initialized");
        let context = Context { project, cursor, schematic_source, ids };
        if self.cross_probe.current.as_ref() != Some(&context) {
            self.cross_probe.current = Some(context.clone());
            self.cross_probe.attempted = None;
            self.cross_probe.result = None;
            self.cross_probe.message = None;
        }
        self.cross_probe.poll();
        if context.ids.len() > 100 {
            self.cross_probe.message = Some("Cross-probing supports at most 100 selected objects; reduce the selection.".into());
        } else if !context.ids.is_empty() && self.cross_probe.pending.is_none() && self.cross_probe.attempted.as_ref() != Some(&context) {
            let query = ComponentLinkQuery { object_ids: context.ids.clone(), ..Default::default() };
            let (sender, receiver) = sync_channel(1);
            match std::thread::Builder::new().name("viamind-cross-probe".into()).spawn(move || {
                let result = engine.query_linked_components(&query).map_err(|e| e.to_string());
                let _ = sender.try_send(result);
            }) {
                Ok(_) => {
                    self.cross_probe.attempted = Some(context.clone());
                    self.cross_probe.pending = Some(Pending { context, receiver });
                }
                Err(error) => self.cross_probe.message = Some(format!("Could not start component query: {error}")),
            }
        }
        if self.cross_probe.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
    }
    pub(crate) fn cross_probe_panel(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.heading("Linked component");
        ui.label("Counterpart highlights are local; shared PCB selection stays unchanged.");
        let mut navigate = None;
        if let Some(message) = &self.cross_probe.message {
            ui.label(message);
        } else if self.cross_probe.pending.is_some() {
            ui.label("Finding linked counterparts…");
        } else if let Some(result) = &self.cross_probe.result {
            let schematic_source = self.cross_probe.current.as_ref().is_some_and(|c| c.schematic_source);
            if !result.unknown_ids.is_empty() {
                ui.label(format!("{} selected objects are not components.", result.unknown_ids.len()));
            }
            egui::ScrollArea::vertical().id_salt("cross_probe_rows").max_height(240.0).show(ui, |ui| {
                for row in &result.components {
                    ui.push_id(row.part_id.or(row.symbol_id), |ui| {
                        let caption = format!(
                            "{} ↔ {}",
                            row.symbol_reference.as_deref().unwrap_or("missing symbol"),
                            row.part_reference.as_deref().unwrap_or("no PCB component")
                        );
                        ui.add(egui::Label::new(&caption).truncate())
                            .on_hover_text(format!("{caption}\nSymbol {:?}\nPCB {:?}", row.symbol_id, row.part_id));
                        if row.status == ComponentLinkStatus::Linked {
                            let enabled = if schematic_source { row.board_navigable } else { row.schematic_navigable };
                            if !enabled {
                                ui.label("Counterpart coordinates exceed supported ±10000 mm navigation.");
                            }
                            if schematic_source && !self.probe_board_visible(row) {
                                ui.label("PCB counterpart is on hidden layers; visibility is preserved.");
                            }
                            let label = if schematic_source { "Show PCB counterpart" } else { "Show schematic counterpart" };
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                navigate = Some((row.clone(), schematic_source));
                            }
                        } else {
                            ui.label(match row.status {
                                ComponentLinkStatus::UnresolvedSymbol => "Linked symbol is missing; resolve or unlink explicitly.",
                                _ => "Component is unlinked; establish a stable link through synchronization.",
                            });
                        }
                    });
                }
            });
        } else {
            ui.label("Select a schematic symbol or PCB component to highlight its linked counterpart.");
        }
        if let Some((row, schematic_source)) = navigate
            && let Err(error) = self.navigate_counterpart(&row, schematic_source)
        {
            self.cross_probe.message = Some(error);
        }
    }
    fn probe_board_visible(&self, row: &LinkedComponent) -> bool {
        let Some(design) = &self.design else { return false };
        let side = design.engine.read(|p| row.part_id.and_then(|id| p.board.parts.get(&id)).map(|p| p.side));
        let ids = match side {
            Some(Side::Top) => [LayerId::F_CU, LayerId::F_MASK, LayerId::F_SILKS],
            Some(Side::Bottom) => [LayerId::B_CU, LayerId::B_MASK, LayerId::B_SILKS],
            None => return false,
        };
        ids.into_iter().any(|id| self.layers.get(id).is_some_and(|layer| layer.visible && layer.opacity > 0.0))
    }
    fn navigate_counterpart(&mut self, row: &LinkedComponent, schematic_source: bool) -> Result<(), String> {
        let Some(context) = self.cross_probe.current.clone() else { return Err("Selection changed; select the source again.".into()) };
        let Some(design) = &self.design else { return Err("Document closed.".into()) };
        if self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
        {
            return Err("Finish the active edit before navigating to a counterpart.".into());
        }
        let current = design
            .engine
            .query_linked_components(&ComponentLinkQuery { object_ids: context.ids.clone(), ..Default::default() })
            .map_err(|e| e.to_string())?;
        if current.project_id != context.project || current.source_cursor != context.cursor || context.schematic_source != schematic_source {
            return Err("Document or selection changed; wait for current counterparts.".into());
        }
        let target = current
            .components
            .iter()
            .find(|r| r.part_id == row.part_id && r.symbol_id == row.symbol_id && r.status == ComponentLinkStatus::Linked)
            .ok_or("Component link changed; select the source again.")?;
        if schematic_source {
            if !target.board_navigable {
                return Err("PCB coordinates exceed supported navigation bounds.".into());
            }
            let at = target.board_position.ok_or("PCB counterpart is missing.")?;
            self.set_schematic_mode(false);
            self.canvas.focus_world(WorldPos::new(at.x.to_mm(), at.y.to_mm()));
        } else {
            if !target.schematic_navigable {
                return Err("Schematic coordinates exceed supported navigation bounds.".into());
            }
            self.navigate_probe_symbol(context.project, context.cursor, target)?;
        }
        // Explicit view navigation may clear an old-sheet local selection; it is not a new source gesture.
        self.cross_probe.schematic_selection = self.schematic_probe_selection(context.project, context.cursor.stream_id);
        Ok(())
    }
    pub(crate) fn paint_cross_probe(&self, ui: &Ui, viewport: egui::Rect) {
        if !self.cross_probe.current.as_ref().is_some_and(|c| c.schematic_source) {
            return;
        }
        let Some(result) = &self.cross_probe.result else { return };
        let painter = ui.painter_at(viewport);
        for row in &result.components {
            if row.status == ComponentLinkStatus::Linked
                && row.board_navigable
                && self.probe_board_visible(row)
                && let Some(at) = row.board_position
            {
                let at = self.canvas.camera.world_to_screen(WorldPos::new(at.x.to_mm(), at.y.to_mm()), viewport);
                painter.circle_stroke(at, 14.0, Stroke::new(3.0, Color32::from_rgb(255, 100, 220)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result() -> ComponentLinkResult {
        let e = viamind_engine::EngineHandle::new(
            viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap(),
        );
        e.query_linked_components(&ComponentLinkQuery::default()).unwrap()
    }
    fn context(r: &ComponentLinkResult) -> Context {
        Context { project: r.project_id, cursor: r.source_cursor, schematic_source: false, ids: vec![r.components[0].part_id.unwrap()] }
    }
    #[test]
    fn one_receiver_survives_replacement_and_discards_old_completion() {
        let result = result();
        let old = context(&result);
        let mut next = old.clone();
        next.cursor.stream_id = ObjectId::new();
        let (sender, receiver) = sync_channel(1);
        let mut ui = CrossProbeUi { current: Some(next.clone()), pending: Some(Pending { context: old, receiver }), ..Default::default() };
        ui.poll();
        assert!(ui.pending.is_some(), "replacement retains one outstanding receiver");
        sender.try_send(Ok(result)).unwrap();
        ui.poll();
        assert!(ui.pending.is_none());
        assert!(ui.result.is_none());
        assert_eq!(ui.current, Some(next));
    }
    #[test]
    fn changed_source_and_cursor_cannot_publish_old_highlights_and_disconnect_is_actionable() {
        for changed_source in [false, true] {
            let result = result();
            let old = context(&result);
            let mut next = old.clone();
            if changed_source {
                next.schematic_source = true;
            } else {
                next.cursor.sequence += 1;
            }
            let (sender, receiver) = sync_channel(1);
            sender.try_send(Ok(result)).unwrap();
            let mut ui = CrossProbeUi { current: Some(next), pending: Some(Pending { context: old, receiver }), ..Default::default() };
            ui.poll();
            assert!(ui.highlighted_symbols().is_empty());
            assert!(ui.highlighted_parts().is_empty());
        }
        let r = result();
        let context = context(&r);
        let (sender, receiver) = sync_channel(1);
        drop(sender);
        let mut ui = CrossProbeUi { current: Some(context.clone()), pending: Some(Pending { context, receiver }), ..Default::default() };
        ui.poll();
        assert!(ui.message.as_deref().unwrap().contains("worker stopped"));
    }
}
