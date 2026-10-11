//! Explicit stable links and inspectable guarded schematic/PCB synchronization.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, Project};
use viamind_engine::{EngineHandle, EventCursor, LinkSchematicSymbol, PcbSyncDirection, PcbSyncPreview, SchematicLinkTarget};
enum Completed {
    Links(Project, EventCursor),
    Preview(PcbSyncPreview),
}
struct Pending {
    stream: ObjectId,
    receiver: Receiver<Result<Completed, String>>,
}
pub struct PcbSyncUi {
    open: bool,
    stream: Option<ObjectId>,
    direction: PcbSyncDirection,
    snapshot: Option<(Project, EventCursor)>,
    part: Option<ObjectId>,
    symbol: Option<ObjectId>,
    preview: Option<PcbSyncPreview>,
    pending: Option<Pending>,
    message: Option<String>,
}
impl Default for PcbSyncUi {
    fn default() -> Self {
        Self {
            open: false,
            stream: None,
            direction: PcbSyncDirection::Forward,
            snapshot: None,
            part: None,
            symbol: None,
            preview: None,
            pending: None,
            message: None,
        }
    }
}
impl PcbSyncUi {
    fn poll(&mut self, stream: Option<ObjectId>) {
        if self.stream != stream {
            self.stream = stream;
            self.snapshot = None;
            self.preview = None;
            self.part = None;
            self.symbol = None;
            self.message = None;
            // Retain the one receiver until completion even when the document changes.
        }
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Synchronization worker stopped; refresh again".into()),
        };
        let pending = self.pending.take().expect("completed worker");
        if Some(pending.stream) != self.stream {
            return;
        }
        match result {
            Ok(Completed::Links(project, cursor)) if Some(cursor.stream_id) == self.stream => self.snapshot = Some((project, cursor)),
            Ok(Completed::Preview(preview)) if Some(preview.command.source_cursor.stream_id) == self.stream => {
                if preview.command.direction == self.direction {
                    self.preview = Some(preview);
                } else {
                    self.message = Some("Direction changed; prepare synchronization again".into());
                }
            }
            Ok(_) => self.message = Some("Document changed; prepare synchronization again".into()),
            Err(error) => self.message = Some(error),
        }
    }
    fn start(&mut self, engine: EngineHandle, preview: bool) {
        if self.pending.is_some() {
            return;
        }
        let stream = engine.event_cursor().stream_id;
        self.poll(Some(stream));
        self.message = None;
        if preview {
            self.preview = None;
        } else {
            self.snapshot = None;
        }
        let direction = self.direction;
        let (sender, receiver) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-pcb-sync".into()).spawn(move || {
            let result = if preview {
                engine.preview_schematic_pcb_sync(direction).map(Completed::Preview)
            } else {
                engine.schematic_pcb_snapshot().map(|(project, cursor)| Completed::Links(project, cursor))
            };
            let _ = sender.try_send(result.map_err(|e| e.to_string()));
        }) {
            Ok(_) => self.pending = Some(Pending { stream, receiver }),
            Err(error) => self.message = Some(format!("Could not start synchronization: {error}")),
        }
    }
}
fn short(text: &str) -> String {
    let mut result: String = text.chars().take(42).collect();
    if text.chars().count() > 42 {
        result.push('…');
    }
    result
}
impl ViamindApp {
    pub fn begin_pcb_sync(&mut self) {
        if let Some(design) = &self.design {
            self.pcb_sync.open = true;
            self.pcb_sync.start(design.engine.clone(), false);
        }
    }
    pub(crate) fn pcb_sync_window(&mut self, ui: &mut Ui) {
        let engine = self.design.as_ref().map(|d| d.engine.clone());
        let cursor = engine.as_ref().map(EngineHandle::event_cursor);
        self.pcb_sync.poll(cursor.map(|c| c.stream_id));
        if self.pcb_sync.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        let (Some(engine), Some(cursor)) = (engine, cursor) else { return };
        if !self.pcb_sync.open {
            return;
        }
        let mut open = self.pcb_sync.open;
        let mut create_component = false;
        let mut refresh = false;
        let mut prepare = false;
        let mut link = None;
        let mut apply = None;
        egui::Window::new("Schematic and PCB synchronization").open(&mut open).default_width(740.0).default_height(640.0).show(ui.ctx(), |ui| {
            ui.label("Link existing components by identity, then inspect proposed changes. Undo restores the complete update.");
            create_component = ui.button("Create PCB component from symbol…").clicked();
            ui.collapsing("Component links", |ui| {
                refresh = ui.add_enabled(self.pcb_sync.pending.is_none(), egui::Button::new("Refresh component links")).clicked();
                if let Some((project, source)) = &self.pcb_sync.snapshot {
                    let fresh = *source == cursor;
                    if !fresh { ui.label("Component list is stale; refresh component links."); }
                    let parts: Vec<_> = project.board.parts.values().collect();
                    let symbols: Vec<_> = project.schematic.sheets.values().flat_map(|s| s.content.iter().flat_map(move |c| c.symbols.values().map(move |symbol| (s, symbol)))).collect();
                    let selected_part = parts.iter().find(|p| Some(p.id) == self.pcb_sync.part).copied();
                    let selected_symbol = symbols.iter().find(|(_, s)| Some(s.id) == self.pcb_sync.symbol).map(|(_, s)| *s);
                    let label = ui.label("PCB component to link");
                    egui::ComboBox::from_id_salt("sync_part").selected_text(selected_part.map_or_else(|| "Choose PCB component".into(), |p| short(&p.designator))).width(320.0).show_ui(ui, |ui| {
                        for part in parts { ui.selectable_value(&mut self.pcb_sync.part, Some(part.id), format!("{} · {}", short(&part.designator), &part.id.to_string()[28..])).on_hover_text(format!("{}\n{}", part.designator, part.id)); }
                    }).response.labelled_by(label.id);
                    let label = ui.label("Schematic symbol to link");
                    egui::ComboBox::from_id_salt("sync_symbol").selected_text(selected_symbol.map_or_else(|| "Choose schematic symbol".into(), |s| short(&s.designator))).width(320.0).show_ui(ui, |ui| {
                        for (sheet, symbol) in symbols { ui.selectable_value(&mut self.pcb_sync.symbol, Some(symbol.id), format!("{} · {}", short(&symbol.designator), short(&sheet.name))).on_hover_text(format!("{}\nSheet {}\n{}", symbol.designator, sheet.name, symbol.id)); }
                    }).response.labelled_by(label.id);
                    if let Some(part) = selected_part {
                        ui.label(format!("Current symbol link: {}", part.schematic_symbol.map_or_else(|| "unlinked".into(), |id| id.to_string())));
                        let target = selected_symbol.map(|s| SchematicLinkTarget::Symbol { expected: s.clone() });
                        if ui.add_enabled(fresh && target.is_some() && !self.files.busy() && self.pcb_sync.pending.is_none(), egui::Button::new("Link selected components")).clicked() {
                            link = Some(LinkSchematicSymbol { expected_project: project.id, expected_part: part.clone(), target: target.expect("enabled symbol") });
                        }
                        if ui.add_enabled(fresh && part.schematic_symbol.is_some() && !self.files.busy() && self.pcb_sync.pending.is_none(), egui::Button::new("Unlink PCB component")).clicked() {
                            link = Some(LinkSchematicSymbol { expected_project: project.id, expected_part: part.clone(), target: SchematicLinkTarget::Unlink });
                        }
                    }
                }
            });
            let old_direction = self.pcb_sync.direction;
            ui.radio_value(&mut self.pcb_sync.direction, PcbSyncDirection::Forward, "Schematic to PCB: references, values and pad nets");
            ui.radio_value(&mut self.pcb_sync.direction, PcbSyncDirection::Back, "PCB to schematic: references, values and footprint assignments");
            if self.pcb_sync.direction != old_direction { self.pcb_sync.preview = None; }
            ui.label("Existing numbered pads must match symbol pins. Footprint geometry, copper, positions and unlinked objects stay unchanged. Routed net reassignment requires separate migration.");
            prepare = ui.add_enabled(self.pcb_sync.pending.is_none(), egui::Button::new("Prepare synchronization")).clicked();
            if self.pcb_sync.pending.is_some() { ui.label("Preparing bounded snapshot; editing remains available."); }
            if let Some(preview) = &self.pcb_sync.preview {
                let fresh = preview.command.source_cursor == cursor && preview.command.direction == self.pcb_sync.direction;
                if !fresh { ui.label("Document changed; prepare synchronization again."); }
                ui.label(format!("{} proposed changes · {} unlinked symbols · {} unlinked PCB components", preview.changes.len(), preview.unlinked_symbols.len(), preview.unlinked_parts.len()));
                egui::ScrollArea::vertical().id_salt("sync_changes").max_height(220.0).show_rows(ui, ui.text_style_height(&egui::TextStyle::Body), preview.changes.len(), |ui, rows| {
                    for row in rows { let change = &preview.changes[row]; let line = format!("{}: {} → {} · part {}", change.field, change.before.as_deref().unwrap_or("unassigned"), change.after.as_deref().unwrap_or("unassigned"), change.part_id); ui.add(egui::Label::new(&line).truncate()).on_hover_text(line); }
                });
                ui.collapsing("Net identities and label names", |ui| {
                    egui::ScrollArea::vertical().id_salt("sync_nets").max_height(140.0).show(ui, |ui| { for net in &preview.nets { let text = format!("{} · {}", net.canonical_name.as_deref().unwrap_or("unnamed"), net.board_net); ui.add(egui::Label::new(&text).truncate()).on_hover_text(text); } });
                });
                if ui.add_enabled(fresh && !preview.changes.is_empty() && !self.files.busy() && self.pcb_sync.pending.is_none(), egui::Button::new("Apply synchronization")).clicked() { apply = Some(preview.command.clone()); }
            }
            if let Some(message) = &self.pcb_sync.message { ui.label(message); }
        });
        self.pcb_sync.open = open;
        if create_component {
            self.pcb_sync.open = false;
            self.begin_schematic_component();
        }
        if refresh {
            self.pcb_sync.start(engine.clone(), false);
        }
        if prepare {
            self.pcb_sync.start(engine.clone(), true);
        }
        if let Some(command) = link {
            match UiAction::LinkSchematicSymbol.run(&engine, serde_json::to_value(command).expect("link input")) {
                Ok(_) => {
                    self.pcb_sync.preview = None;
                    self.pcb_sync.start(engine.clone(), false);
                }
                Err(error) => self.pcb_sync.message = Some(format!("{error}; refresh component links.")),
            }
        }
        if let Some(command) = apply {
            match UiAction::SynchronizeSchematicPcb.run(&engine, serde_json::to_value(command).expect("sync input")) {
                Ok(_) => {
                    self.pcb_sync.preview = None;
                    self.pcb_sync.snapshot = None;
                    self.pcb_sync.message = Some("Synchronization applied; Undo restores the complete update.".into());
                }
                Err(error) => self.pcb_sync.message = Some(format!("{error}; prepare synchronization again.")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn actual_direction_change_during_pending_worker_discards_old_policy_completion() {
        let engine =
            EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
        let before = engine.session_state();
        let preview = engine.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap();
        let (sender, receiver) = sync_channel(1);
        let mut app = ViamindApp::with_design(engine.clone(), None);
        app.pcb_sync.open = true;
        app.pcb_sync.stream = Some(before.cursor.stream_id);
        app.pcb_sync.pending = Some(Pending { stream: before.cursor.stream_id, receiver });
        let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1200.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
        h.run_steps(5);
        h.get_by_label("PCB to schematic: references, values and footprint assignments").click();
        h.run_steps(5);
        assert_eq!(h.state().pcb_sync.direction, PcbSyncDirection::Back);
        sender.try_send(Ok(Completed::Preview(preview))).unwrap();
        h.run_steps(5);
        assert!(h.state().pcb_sync.preview.is_none());
        assert!(h.query_by_label("Apply synchronization").is_none());
        assert!(h.query_by_label("Direction changed; prepare synchronization again").is_some());
        assert_eq!(engine.session_state(), before);
    }
}
