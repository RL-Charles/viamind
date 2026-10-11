//! Explicit symbol/footprint creation with one retained background preview worker.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, Project, Side};
use viamind_engine::{EngineHandle, EventCursor, SchematicComponentPlacement, SchematicComponentPreview, SchematicComponentRequest};
use viamind_geom::{Nm, Point};

enum Completed {
    Symbols(Project, EventCursor),
    Preview(SchematicComponentPreview),
}
struct Pending {
    stream: ObjectId,
    receiver: Receiver<Result<Completed, String>>,
}
pub struct SchematicComponentUi {
    open: bool,
    stream: Option<ObjectId>,
    snapshot: Option<(Project, EventCursor)>,
    symbol: Option<ObjectId>,
    path: String,
    x: String,
    y: String,
    side: Side,
    rotation: i32,
    preview: Option<SchematicComponentPreview>,
    pending: Option<Pending>,
    message: Option<String>,
}
impl Default for SchematicComponentUi {
    fn default() -> Self {
        Self {
            open: false,
            stream: None,
            snapshot: None,
            symbol: None,
            path: String::new(),
            x: "40000000".into(),
            y: "40000000".into(),
            side: Side::Top,
            rotation: 0,
            preview: None,
            pending: None,
            message: None,
        }
    }
}
impl SchematicComponentUi {
    fn poll(&mut self, stream: Option<ObjectId>) {
        if self.stream != stream {
            self.stream = stream;
            self.snapshot = None;
            self.symbol = None;
            self.preview = None;
            self.message = None;
            // Retain a running receiver even across close/replacement.
        }
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(value) => value,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Component preview worker stopped; prepare again".into()),
        };
        let pending = self.pending.take().expect("completed worker");
        if Some(pending.stream) != self.stream {
            return;
        }
        match result {
            Ok(Completed::Symbols(project, cursor)) if Some(cursor.stream_id) == self.stream => self.snapshot = Some((project, cursor)),
            Ok(Completed::Preview(preview)) if Some(preview.command.source_cursor.stream_id) == self.stream => self.preview = Some(preview),
            Ok(_) => self.message = Some("Document changed; refresh symbols".into()),
            Err(error) => self.message = Some(error),
        }
    }
    fn start(&mut self, engine: EngineHandle, input: Option<SchematicComponentRequest>) {
        if self.pending.is_some() {
            return;
        }
        let stream = engine.event_cursor().stream_id;
        self.poll(Some(stream));
        self.preview = None;
        self.message = None;
        let path = self.path.clone();
        let (sender, receiver) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-component-preview".into()).spawn(move || {
            let result = if let Some(mut input) = input {
                viamind_doc::footprint::load(std::path::Path::new(&path)).and_then(|footprint| {
                    input.placement.footprint = footprint;
                    engine.preview_schematic_pcb_component(input).map(Completed::Preview).map_err(|e| e.to_string())
                })
            } else {
                engine.schematic_pcb_snapshot().map(|(p, c)| Completed::Symbols(p, c)).map_err(|e| e.to_string())
            };
            let _ = sender.try_send(result);
        }) {
            Ok(_) => self.pending = Some(Pending { stream, receiver }),
            Err(error) => self.message = Some(format!("Could not start preview worker: {error}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_schematic_component(&mut self) {
        self.schematic_component.open = true;
        if let Some(design) = &self.design {
            self.schematic_component.start(design.engine.clone(), None);
        }
    }
    pub fn schematic_component_window(&mut self, ui: &mut Ui) {
        let engine = self.design.as_ref().map(|d| d.engine.clone());
        let cursor = engine.as_ref().map(EngineHandle::event_cursor);
        self.schematic_component.poll(cursor.map(|c| c.stream_id));
        if self.schematic_component.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        if !self.schematic_component.open {
            return;
        }
        let Some(engine) = engine else {
            return;
        };
        let cursor = cursor.expect("live document");
        let mut open = true;
        let mut refresh = false;
        let mut prepare = false;
        let mut apply = None;
        egui::Window::new("Create linked PCB component").open(&mut open).default_width(630.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Choose one unlinked symbol and an explicit native rectangular SMD footprint. 1 mm = 1000000 nm.");
            refresh = ui.add_enabled(self.schematic_component.pending.is_none(), egui::Button::new("Refresh symbols")).clicked();
            let mut changed = false;
            ui.add_enabled_ui(self.schematic_component.pending.is_none(), |ui| {
                if let Some((project, source)) = &self.schematic_component.snapshot {
                    if *source != cursor {
                        ui.label("Document changed; refresh symbols before preparing.");
                    }
                    let linked: std::collections::BTreeSet<_> = project.board.parts.values().filter_map(|p| p.schematic_symbol).collect();
                    let symbols: Vec<_> = project
                        .schematic
                        .sheets
                        .values()
                        .flat_map(|sheet| sheet.content.iter().flat_map(move |c| c.symbols.values().map(move |s| (sheet, s))))
                        .filter(|(_, s)| !linked.contains(&s.id))
                        .collect();
                    let selected = symbols.iter().find(|(_, s)| Some(s.id) == self.schematic_component.symbol);
                    let label = ui.label("Schematic symbol for new PCB component");
                    let old = self.schematic_component.symbol;
                    egui::ComboBox::from_id_salt("component_symbol")
                        .selected_text(
                            selected.map_or_else(|| "Choose unlinked symbol".into(), |(_, s)| s.designator.chars().take(80).collect::<String>()),
                        )
                        .width(400.0)
                        .show_ui(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(240.0).show_rows(
                                ui,
                                ui.text_style_height(&egui::TextStyle::Body),
                                symbols.len(),
                                |ui, rows| {
                                    for row in rows {
                                        let (sheet, symbol) = symbols[row];
                                        ui.selectable_value(
                                            &mut self.schematic_component.symbol,
                                            Some(symbol.id),
                                            format!(
                                                "{} · {}",
                                                symbol.designator.chars().take(80).collect::<String>(),
                                                sheet.name.chars().take(80).collect::<String>()
                                            ),
                                        )
                                        .on_hover_text(format!("{}\n{}\n{}", symbol.designator, sheet.name, symbol.id));
                                    }
                                },
                            );
                        })
                        .response
                        .labelled_by(label.id);
                    changed |= old != self.schematic_component.symbol;
                }
                let label = ui.label("Native footprint library path");
                changed |= ui.text_edit_singleline(&mut self.schematic_component.path).labelled_by(label.id).changed();
                ui.horizontal(|ui| {
                    let label = ui.label("X (integer nm)");
                    changed |=
                        ui.add(egui::TextEdit::singleline(&mut self.schematic_component.x).desired_width(150.0)).labelled_by(label.id).changed();
                    let label = ui.label("Y (integer nm)");
                    changed |=
                        ui.add(egui::TextEdit::singleline(&mut self.schematic_component.y).desired_width(150.0)).labelled_by(label.id).changed();
                });
                ui.horizontal(|ui| {
                    changed |= ui.radio_value(&mut self.schematic_component.side, Side::Top, "Top").changed();
                    changed |= ui.radio_value(&mut self.schematic_component.side, Side::Bottom, "Bottom").changed();
                    for angle in [0, 90_000, 180_000, 270_000] {
                        changed |= ui.radio_value(&mut self.schematic_component.rotation, angle, format!("{}°", angle / 1000)).changed();
                    }
                });
            });
            if changed {
                self.schematic_component.preview = None;
                self.schematic_component.message = None;
            }
            let fresh = self.schematic_component.snapshot.as_ref().is_some_and(|(_, source)| *source == cursor);
            prepare = ui
                .add_enabled(
                    fresh && self.schematic_component.symbol.is_some() && self.schematic_component.pending.is_none() && !self.files.busy(),
                    egui::Button::new("Prepare new PCB component"),
                )
                .clicked();
            if self.schematic_component.pending.is_some() {
                ui.label("Loading bounded footprint and preparing component…");
            }
            if let Some(preview) = &self.schematic_component.preview {
                let fresh = preview.command.source_cursor == cursor;
                if !fresh {
                    ui.label("Document changed; refresh symbols and prepare again.");
                }
                let p = &preview.command.placement;
                let line = format!("Create {} · {} · {}", preview.designator, preview.value, p.footprint.name);
                ui.add(egui::Label::new(&line).truncate()).on_hover_text(line);
                ui.label(format!(
                    "Origin ({}, {}) nm · {:?} · {}° · {} pads",
                    p.position.x.0,
                    p.position.y.0,
                    p.side,
                    p.rotation_mdeg / 1000,
                    preview.pads.len()
                ));
                egui::ScrollArea::vertical().id_salt("new_component_nets").max_height(150.0).show_rows(
                    ui,
                    ui.text_style_height(&egui::TextStyle::Body),
                    preview.pads.len(),
                    |ui, rows| {
                        for row in rows {
                            let pad = &preview.pads[row];
                            let line = format!("Pad {} → {}", pad.number, pad.net);
                            ui.add(egui::Label::new(&line).truncate()).on_hover_text(line);
                        }
                    },
                );
                ui.label("Placement and copper clearances require DRC after creation.");
                if ui
                    .add_enabled(fresh && self.schematic_component.pending.is_none() && !self.files.busy(), egui::Button::new("Create PCB component"))
                    .clicked()
                {
                    apply = Some(preview.command.clone());
                }
            }
            if let Some(message) = &self.schematic_component.message {
                ui.label(message);
            }
        });
        self.schematic_component.open = open;
        if refresh {
            self.schematic_component.start(engine.clone(), None);
        }
        if prepare {
            match (self.schematic_component.x.parse::<i64>(), self.schematic_component.y.parse::<i64>()) {
                (Ok(x), Ok(y)) => {
                    let (project, source) = self.schematic_component.snapshot.as_ref().expect("enabled snapshot");
                    let input = SchematicComponentRequest {
                        expected_project: project.id,
                        source_cursor: *source,
                        placement: SchematicComponentPlacement {
                            symbol_id: self.schematic_component.symbol.expect("enabled symbol"),
                            position: Point::new(Nm(x), Nm(y)),
                            side: self.schematic_component.side,
                            rotation_mdeg: self.schematic_component.rotation,
                            footprint: viamind_doc::footprint::Footprint { name: String::new(), pads: vec![] },
                        },
                    };
                    self.schematic_component.start(engine.clone(), Some(input));
                }
                _ => self.schematic_component.message = Some("Coordinates must be signed integer nanometres".into()),
            }
        }
        if let Some(command) = apply {
            match UiAction::CreateSchematicPcbComponent.run(&engine, serde_json::to_value(command).expect("creation input")) {
                Ok(_) => {
                    self.schematic_component.preview = None;
                    self.schematic_component.snapshot = None;
                    self.schematic_component.symbol = None;
                    self.schematic_component.message =
                        Some("PCB component created. Undo removes the whole component; refresh to create another.".into());
                }
                Err(error) => self.schematic_component.message = Some(format!("{error}; refresh symbols and prepare again")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_worker_is_retained_and_old_stream_completion_is_discarded() {
        let old = EngineHandle::new(Project::new("old"));
        let replacement = EngineHandle::new(Project::new("replacement"));
        let (sender, receiver) = sync_channel(1);
        let mut state = SchematicComponentUi {
            stream: Some(old.event_cursor().stream_id),
            pending: Some(Pending { stream: old.event_cursor().stream_id, receiver }),
            ..Default::default()
        };
        assert!(!state.open);
        state.poll(Some(replacement.event_cursor().stream_id));
        assert!(state.pending.is_some(), "replacement retains running worker");
        state.start(replacement.clone(), None);
        assert!(state.pending.as_ref().unwrap().stream == old.event_cursor().stream_id, "cannot overlap workers");
        sender.try_send(Ok(Completed::Symbols(old.read(Clone::clone), old.event_cursor()))).unwrap();
        state.poll(Some(replacement.event_cursor().stream_id));
        assert!(state.pending.is_none());
        assert!(state.snapshot.is_none());
        assert!(state.preview.is_none());
    }
    #[test]
    fn same_stream_completion_keeps_literal_cursor_for_stale_ui_guard() {
        let e = EngineHandle::new(Project::new("same"));
        let captured = e.event_cursor();
        let project = e.read(Clone::clone);
        let (sender, receiver) = sync_channel(1);
        let mut state = SchematicComponentUi {
            stream: Some(captured.stream_id),
            pending: Some(Pending { stream: captured.stream_id, receiver }),
            ..Default::default()
        };
        e.execute(
            "add_part",
            serde_json::json!({"designator":"R1","value":"concurrent","footprint":"other","x_mm":1,"y_mm":1}),
            viamind_engine::Origin::agent("other"),
        )
        .unwrap();
        sender.try_send(Ok(Completed::Symbols(project, captured))).unwrap();
        state.poll(Some(e.event_cursor().stream_id));
        assert_eq!(state.snapshot.as_ref().unwrap().1, captured);
        assert_ne!(captured, e.event_cursor());
    }
}
