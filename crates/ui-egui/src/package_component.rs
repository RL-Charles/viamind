//! Complete explicit packages share one physical component; previews use one retained worker.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, Project, Side};
use viamind_engine::{EngineHandle, EventCursor, LinkSchematicPackage, PackageComponentPlacement, PackageComponentPreview, PackageComponentRequest};
use viamind_geom::{Nm, Point};

enum Completed {
    Snapshot(Project, EventCursor),
    Preview(PackageComponentPreview),
}
struct Pending {
    stream: ObjectId,
    receiver: Receiver<Result<Completed, String>>,
}
pub struct PackageComponentUi {
    open: bool,
    stream: Option<ObjectId>,
    snapshot: Option<(Project, EventCursor)>,
    package: Option<ObjectId>,
    part: Option<ObjectId>,
    path: String,
    x: String,
    y: String,
    side: Side,
    rotation: i32,
    preview: Option<PackageComponentPreview>,
    pending: Option<Pending>,
    message: Option<String>,
}
impl Default for PackageComponentUi {
    fn default() -> Self {
        Self {
            open: false,
            stream: None,
            snapshot: None,
            package: None,
            part: None,
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
fn short(text: &str) -> String {
    text.chars().take(72).collect()
}
impl PackageComponentUi {
    fn poll(&mut self, stream: Option<ObjectId>) {
        if self.stream != stream {
            self.stream = stream;
            self.snapshot = None;
            self.package = None;
            self.part = None;
            self.preview = None;
            self.message = None;
        }
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Package worker stopped; refresh and prepare again".into()),
        };
        let pending = self.pending.take().expect("completed worker");
        if Some(pending.stream) != stream {
            return;
        }
        match result {
            Ok(Completed::Snapshot(p, c)) if Some(c.stream_id) == stream => self.snapshot = Some((p, c)),
            Ok(Completed::Preview(p)) if Some(p.command.source_cursor.stream_id) == stream => self.preview = Some(p),
            Ok(_) => self.message = Some("Document changed; refresh complete packages".into()),
            Err(e) => self.message = Some(e),
        }
    }
    fn start(&mut self, engine: EngineHandle, input: Option<PackageComponentRequest>) {
        if self.pending.is_some() {
            return;
        }
        if input.is_some() && (self.path.is_empty() || self.path.len() > 4096) {
            self.message = Some("Footprint path must contain 1..4096 encoded bytes".into());
            return;
        }
        let stream = engine.event_cursor().stream_id;
        self.poll(Some(stream));
        self.preview = None;
        self.message = None;
        let path = self.path.clone();
        let (sender, receiver) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-package-component".into()).spawn(move || {
            let result = if let Some(mut input) = input {
                viamind_doc::footprint::load(std::path::Path::new(&path)).and_then(|footprint| {
                    input.placement.footprint = footprint;
                    engine.preview_package_pcb_component(input).map(Completed::Preview).map_err(|e| e.to_string())
                })
            } else {
                engine.schematic_pcb_snapshot().map(|(p, c)| Completed::Snapshot(p, c)).map_err(|e| e.to_string())
            };
            let _ = sender.try_send(result);
        }) {
            Ok(_) => self.pending = Some(Pending { stream, receiver }),
            Err(e) => self.message = Some(format!("Could not start package worker: {e}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_package_component(&mut self) {
        self.package_component.open = true;
        if let Some(d) = &self.design {
            self.package_component.start(d.engine.clone(), None);
        }
    }
    pub(crate) fn package_component_window(&mut self, ui: &mut Ui) {
        let engine = self.design.as_ref().map(|d| d.engine.clone());
        let cursor = engine.as_ref().map(EngineHandle::event_cursor);
        self.package_component.poll(cursor.map(|c| c.stream_id));
        if self.package_component.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        if !self.package_component.open {
            return;
        }
        let (Some(engine), Some(cursor)) = (engine, cursor) else { return };
        let mut open = true;
        let mut refresh = false;
        let mut prepare = false;
        let mut create = None;
        let mut link = None;
        egui::Window::new("Complete package to one PCB component").open(&mut open).default_width(630.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Choose an explicit complete package. All units share one physical component; every pin needs one matching pad.");
            let busy = self.package_component.pending.is_some();
            refresh = ui.add_enabled(!busy, egui::Button::new("Refresh complete packages")).clicked();
            let mut changed = false;
            ui.add_enabled_ui(!busy, |ui| {
                if let Some((project, source)) = &self.package_component.snapshot {
                    let fresh = *source == cursor;
                    if !fresh {
                        ui.label("Document changed; refresh complete packages before preparing or linking.");
                    }
                    let packages: Vec<_> = project
                        .schematic
                        .sheets
                        .values()
                        .flat_map(|sheet| sheet.content.iter().flat_map(move |c| c.packages.values().map(move |g| (sheet, c, g))))
                        .collect();
                    let selected = packages.iter().find(|(_, _, g)| Some(g.id) == self.package_component.package);
                    let old = self.package_component.package;
                    let label = ui.label("Complete schematic package");
                    egui::ComboBox::from_id_salt("physical_package")
                        .width(350.0)
                        .selected_text(selected.map_or_else(
                            || "Choose complete package".into(),
                            |(_, c, g)| format!("{} · {} units", short(&c.symbols[&g.units[0].symbol_id].designator), g.units.len()),
                        ))
                        .show_ui(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(220.0).show_rows(
                                ui,
                                ui.text_style_height(&egui::TextStyle::Body),
                                packages.len(),
                                |ui, rows| {
                                    for i in rows {
                                        let (sheet, c, g) = packages[i];
                                        ui.selectable_value(
                                            &mut self.package_component.package,
                                            Some(g.id),
                                            format!(
                                                "{} · {} units · {}",
                                                short(&c.symbols[&g.units[0].symbol_id].designator),
                                                g.units.len(),
                                                short(&sheet.name)
                                            ),
                                        )
                                        .on_hover_text(g.id.to_string());
                                    }
                                },
                            );
                        })
                        .response
                        .labelled_by(label.id);
                    changed |= old != self.package_component.package;
                    if let Some((_, c, g)) = packages.iter().find(|(_, _, g)| Some(g.id) == self.package_component.package) {
                        ui.label(format!(
                            "{} units · {} explicit pins · {}",
                            g.units.len(),
                            g.units.iter().map(|u| c.symbols[&u.symbol_id].pins.len()).sum::<usize>(),
                            g.id
                        ));
                    }
                    ui.collapsing("Link or unlink an existing physical component", |ui| {
                        let selected_part = self.package_component.part.and_then(|id| project.board.parts.get(&id));
                        let label = ui.label("Existing physical component");
                        egui::ComboBox::from_id_salt("package_existing_part")
                            .width(350.0)
                            .selected_text(selected_part.map_or_else(|| "Choose physical component".into(), |p| short(&p.designator)))
                            .show_ui(ui, |ui| {
                                egui::ScrollArea::vertical().max_height(180.0).show_rows(
                                    ui,
                                    ui.text_style_height(&egui::TextStyle::Body),
                                    project.board.parts.len(),
                                    |ui, rows| {
                                        for part in project.board.parts.values().skip(rows.start).take(rows.len()) {
                                            ui.selectable_value(&mut self.package_component.part, Some(part.id), short(&part.designator))
                                                .on_hover_text(part.id.to_string());
                                        }
                                    },
                                );
                            })
                            .response
                            .labelled_by(label.id);
                        if let Some(part) = selected_part {
                            ui.label(format!("Current package: {}", part.schematic_package.map_or_else(|| "unlinked".into(), |id| id.to_string())));
                            let enabled = fresh && !self.files.busy() && part.schematic_symbol.is_none();
                            if ui
                                .add_enabled(enabled && self.package_component.package.is_some(), egui::Button::new("Link complete package"))
                                .clicked()
                            {
                                link = Some(LinkSchematicPackage {
                                    expected: project.clone(),
                                    source_cursor: *source,
                                    part_id: part.id,
                                    package_id: self.package_component.package,
                                });
                            }
                            if ui.add_enabled(enabled && part.schematic_package.is_some(), egui::Button::new("Unlink complete package")).clicked() {
                                link = Some(LinkSchematicPackage {
                                    expected: project.clone(),
                                    source_cursor: *source,
                                    part_id: part.id,
                                    package_id: None,
                                });
                            }
                            ui.label("Linking preserves geometry and existing nets. Prepare synchronization afterward to inspect changes.");
                        }
                    });
                }
                let label = ui.label("Native package footprint path");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut self.package_component.path).id(egui::Id::new("package_component_path")).char_limit(4096))
                    .labelled_by(label.id)
                    .changed();
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label("Package X (integer nm)");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut self.package_component.x).desired_width(130.0).char_limit(24))
                        .labelled_by(label.id)
                        .changed();
                    let label = ui.label("Package Y (integer nm)");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut self.package_component.y).desired_width(130.0).char_limit(24))
                        .labelled_by(label.id)
                        .changed();
                });
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.radio_value(&mut self.package_component.side, Side::Top, "Package top").changed();
                    changed |= ui.radio_value(&mut self.package_component.side, Side::Bottom, "Package bottom").changed();
                    for angle in [0, 90_000, 180_000, 270_000] {
                        changed |= ui.radio_value(&mut self.package_component.rotation, angle, format!("Package {}°", angle / 1000)).changed();
                    }
                });
            });
            if changed {
                self.package_component.preview = None;
                self.package_component.message = None;
            }
            let fresh = self.package_component.snapshot.as_ref().is_some_and(|(_, source)| *source == cursor);
            prepare = ui
                .add_enabled(
                    fresh && self.package_component.package.is_some() && !busy && !self.files.busy(),
                    egui::Button::new("Prepare complete package component"),
                )
                .clicked();
            if busy {
                ui.label("Preparing bounded complete package; editing remains available.");
            }
            if let Some(preview) = &self.package_component.preview {
                let fresh = preview.command.source_cursor == cursor;
                if !fresh {
                    ui.label("Document changed; refresh complete packages and prepare again.");
                }
                let p = &preview.command.placement;
                ui.label(format!("One {} · {} units · {} pads", short(&preview.designator), preview.unit_count, preview.pins.len()));
                ui.label(format!("Origin ({}, {}) nm · {:?} · {}°", p.position.x.0, p.position.y.0, p.side, p.rotation_mdeg / 1000));
                egui::ScrollArea::vertical().id_salt("package_complete_pins").max_height(140.0).show_rows(
                    ui,
                    ui.text_style_height(&egui::TextStyle::Body),
                    preview.pins.len(),
                    |ui, rows| {
                        for i in rows {
                            let pin = &preview.pins[i];
                            let line = format!("Unit {} · pin/pad {} → {}", pin.unit_key, pin.number, pin.net);
                            ui.add(egui::Label::new(&line).truncate()).on_hover_text(format!("{line}\nSymbol {}\nPin {}", pin.symbol_id, pin.pin_id));
                        }
                    },
                );
                ui.label("Placement and copper clearances require DRC after creation. No vendor acceptance is inferred.");
                if ui.add_enabled(fresh && !busy && !self.files.busy(), egui::Button::new("Create one package PCB component")).clicked() {
                    create = Some(preview.command.clone());
                }
            }
            if let Some(message) = &self.package_component.message {
                ui.label(message);
            }
        });
        self.package_component.open = open;
        if refresh {
            self.package_component.start(engine.clone(), None);
        }
        if prepare {
            match (self.package_component.x.parse::<i64>(), self.package_component.y.parse::<i64>()) {
                (Ok(x), Ok(y)) => {
                    let (p, c) = self.package_component.snapshot.as_ref().expect("enabled snapshot");
                    self.package_component.start(
                        engine.clone(),
                        Some(PackageComponentRequest {
                            expected_project: p.id,
                            source_cursor: *c,
                            placement: PackageComponentPlacement {
                                package_id: self.package_component.package.expect("enabled package"),
                                footprint: viamind_doc::footprint::Footprint { artwork: None, name: String::new(), pads: vec![] },
                                position: Point::new(Nm(x), Nm(y)),
                                side: self.package_component.side,
                                rotation_mdeg: self.package_component.rotation,
                            },
                        }),
                    );
                }
                _ => self.package_component.message = Some("Package coordinates must be signed integer nanometres".into()),
            }
        }
        if let Some(command) = create {
            match UiAction::CreatePackagePcbComponent.run(&engine, serde_json::to_value(command).expect("package creation input")) {
                Ok(_) => {
                    self.package_component.preview = None;
                    self.package_component.snapshot = None;
                    self.package_component.message =
                        Some("Created one linked physical component. Undo removes it; run DRC to inspect placement.".into());
                }
                Err(e) => self.package_component.message = Some(format!("{e}; refresh complete packages")),
            }
        }
        if let Some(command) = link {
            match UiAction::LinkSchematicPackage.run(&engine, serde_json::to_value(command).expect("package link input")) {
                Ok(_) => self.package_component.start(engine, None),
                Err(e) => self.package_component.message = Some(format!("{e}; refresh complete packages")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    use viamind_doc::{
        Sheet,
        footprint::{Footprint, FootprintPad, LocalPosition},
        schematic::{Coordinate, SchematicContent},
    };
    use viamind_engine::{PlaceSymbolPackage, UnitPose};
    fn engine() -> EngineHandle {
        let mut p = Project::new("Complete package desktop");
        let mut sheet = Sheet::new("Package sheet");
        sheet.content = Some(SchematicContent::default());
        p.schematic.sheets.insert(sheet.id, sheet.clone());
        let e = EngineHandle::new(p);
        let template =
            viamind_doc::symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json"))
                .unwrap();
        let placements = template
            .units
            .iter()
            .enumerate()
            .map(|(i, u)| UnitPose {
                key: u.key.clone(),
                position: Coordinate { x: Nm(i as i64 * 20_000_000), y: Nm(0) },
                rotation_quarters: 0,
                mirrored: false,
            })
            .collect();
        UiAction::PlaceSymbolPackage
            .run(
                &e,
                serde_json::to_value(PlaceSymbolPackage {
                    expected_project: e.read(|p| p.id),
                    source_cursor: e.event_cursor(),
                    expected: sheet,
                    template,
                    designator: "U1".into(),
                    placements,
                })
                .unwrap(),
            )
            .unwrap();
        e
    }
    fn footprint(path: &std::path::Path) {
        let f = Footprint {
            name: "Authored-14pad".into(),
            pads: (1..=14)
                .map(|n| FootprintPad {
                    number: n.to_string(),
                    position: LocalPosition { x: Nm(n * 2_000_000), y: Nm(257) },
                    width: Nm(800_001),
                    height: Nm(600_003),
                })
                .collect(),
        };
        viamind_doc::footprint::save_new(&f, path, None).unwrap();
    }
    fn wait(h: &mut Harness<'_, ViamindApp>) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while h.state().package_component.pending.is_some() {
            assert!(std::time::Instant::now() < end, "worker timeout");
            std::thread::sleep(std::time::Duration::from_millis(5));
            h.run_steps(3);
        }
        h.run_steps(3);
    }
    fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) {
        h.get_by_label(label).scroll_to_me();
        h.run_steps(8);
        let rect = h.get_by_label(label).rect();
        h.hover_at(rect.center());
        h.run_steps(2);
        let rect = h.get_by_label(label).rect();
        if label == "Link or unlink an existing physical component" {
            // Collapsing headers paint text and an arrow; they have no rectangular
            // background matching the accessibility label's bounds.
            let (text_rect, clip) = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| {
                    let egui::Shape::Text(text) = &shape.shape else { return None };
                    (text.galley.job.text == label).then_some((text.visual_bounding_rect(), shape.clip_rect.intersect(h.ctx.content_rect())))
                })
                .expect("painted collapsing header text");
            assert!(clip.contains_rect(text_rect), "{label}: {text_rect:?} clipped by {clip:?}");
            return;
        }
        let clip = h
            .output()
            .shapes
            .iter()
            .filter_map(|shape| {
                let egui::Shape::Rect(background) = &shape.shape else { return None };
                (background.rect.contains(rect.center())
                    && (background.rect.width() - rect.width()).abs() <= 4.0
                    && (background.rect.height() - rect.height()).abs() <= 4.0)
                    .then_some(shape.clip_rect.intersect(h.ctx.content_rect()))
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap_or_else(|| panic!("actual painted control missing for {label}: {rect:?}"));
        assert!(clip.contains_rect(rect), "{label}: {rect:?} clipped by {clip:?}");
    }
    fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
        reveal(h, label);
        h.get_by_label(label).click();
        h.run_steps(5);
    }
    fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
        reveal(h, label);
        h.get_by_label(label).focus();
        for pressed in [true, false] {
            h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
            h.run_steps(2);
        }
        h.get_by_label(label).type_text(text);
        h.run_steps(5);
    }
    #[test]
    fn compact_actual_complete_selection_footprint_failure_preview_create_undo_and_guarded_unlink() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("explicit.json");
        footprint(&path);
        let e = engine();
        let before = e.session_state();
        let mut app = ViamindApp::with_design(e.clone(), None);
        app.begin_package_component();
        let mut h = Harness::builder().with_size(egui::vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
        h.run_steps(5);
        wait(&mut h);
        click(&mut h, "Complete schematic package");
        h.get_by_label("U1 · 5 units · Package sheet").click();
        h.run_steps(5);
        input(&mut h, "Native package footprint path", temp.path().join("absent").to_str().unwrap());
        click(&mut h, "Prepare complete package component");
        wait(&mut h);
        assert!(h.state().package_component.preview.is_none());
        assert!(h.state().package_component.message.is_some());
        assert_eq!(e.session_state(), before);
        input(&mut h, "Native package footprint path", path.to_str().unwrap());
        input(&mut h, "Package X (integer nm)", "40000001");
        input(&mut h, "Package Y (integer nm)", "30000003");
        click(&mut h, "Package bottom");
        click(&mut h, "Package 90°");
        click(&mut h, "Prepare complete package component");
        wait(&mut h);
        assert_eq!(e.session_state(), before);
        assert_eq!(h.state().package_component.preview.as_ref().unwrap().pins.len(), 14);
        reveal(&mut h, "Create one package PCB component");
        click(&mut h, "Create one package PCB component");
        let after = e.read(Clone::clone);
        assert_eq!(after.board.parts.len(), 1);
        let part = after.board.parts.values().next().unwrap();
        assert_eq!((part.position, part.side, part.rotation_mdeg), (Point::new(Nm(40000001), Nm(30000003)), Side::Bottom, 90000));
        assert!(part.schematic_package.is_some());
        assert_eq!(after.schematic, before.project.schematic);
        UiAction::Undo.run(&e, serde_json::json!({})).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        UiAction::Redo.run(&e, serde_json::json!({})).unwrap();
        assert_eq!(e.read(Clone::clone), after);
        click(&mut h, "Refresh complete packages");
        wait(&mut h);
        // Existing physical owner is explicitly selected and guarded as a whole Project/cursor.
        click(&mut h, "Link or unlink an existing physical component");
        click(&mut h, "Existing physical component");
        h.get_by_label("U1").click();
        h.run_steps(5);
        click(&mut h, "Unlink complete package");
        wait(&mut h);
        let unlinked = e.read(Clone::clone);
        let mut normalized = after.clone();
        normalized.board.parts.values_mut().next().unwrap().schematic_package = None;
        assert_eq!(unlinked, normalized);
        UiAction::Undo.run(&e, serde_json::json!({})).unwrap();
        assert_eq!(e.read(Clone::clone), after);
    }
    #[test]
    fn retained_hidden_worker_cannot_overlap_and_replaced_stream_drops_completion() {
        let old = engine();
        let next = EngineHandle::new(Project::new("replacement"));
        let (sender, receiver) = sync_channel(1);
        let mut state = PackageComponentUi {
            stream: Some(old.event_cursor().stream_id),
            pending: Some(Pending { stream: old.event_cursor().stream_id, receiver }),
            ..Default::default()
        };
        state.poll(Some(next.event_cursor().stream_id));
        assert!(state.pending.is_some());
        assert!(!state.open);
        state.start(next.clone(), None);
        assert_eq!(state.pending.as_ref().unwrap().stream, old.event_cursor().stream_id);
        sender.try_send(Ok(Completed::Snapshot(old.read(Clone::clone), old.event_cursor()))).unwrap();
        state.poll(Some(next.event_cursor().stream_id));
        assert!(state.pending.is_none());
        assert!(state.snapshot.is_none());
        assert!(state.preview.is_none());
    }
    #[tokio::test]
    async fn actual_peer_edit_undo_equal_design_still_disables_complete_package_creation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("explicit.json");
        footprint(&path);
        let e = engine();
        let before = e.read(Clone::clone);
        let mut app = ViamindApp::with_design(e.clone(), None);
        app.begin_package_component();
        let mut h = Harness::builder().with_size(egui::vec2(1000., 800.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
        h.run_steps(5);
        wait(&mut h);
        click(&mut h, "Complete schematic package");
        h.get_by_label("U1 · 5 units · Package sheet").click();
        h.run_steps(5);
        input(&mut h, "Native package footprint path", path.to_str().unwrap());
        click(&mut h, "Prepare complete package component");
        wait(&mut h);
        let registry = viamind_mcp::engine_registry(e.clone()).unwrap();
        let peer = viamind_mcp::CallContext::new("peer");
        registry
            .call("add_part", serde_json::json!({"designator":"PEER1","value":"peer","footprint":"F","x_mm":1,"y_mm":1}), peer.clone())
            .await
            .unwrap();
        registry.call("undo", serde_json::json!({}), peer).await.unwrap();
        assert_eq!(e.read(Clone::clone), before);
        h.run_steps(5);
        let current = e.session_state();
        assert!(h.get_by_label("Create one package PCB component").accesskit_node().is_disabled());
        click(&mut h, "Create one package PCB component");
        assert_eq!(e.session_state(), current);
        click(&mut h, "Refresh complete packages");
        wait(&mut h);
        assert!(h.state().package_component.preview.is_none());
    }
}
