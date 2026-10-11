//! Detached complete-package definition editing and atomic guarded placement.
use crate::{ViamindApp, actions::UiAction, package_definition::PackageEditor};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, symbol_package::SymbolPackageTemplate};
use viamind_engine::{
    DeleteSymbolPackage, EngineHandle, EventCursor, LoadSymbolPackage, PlaceSymbolPackage, SaveSymbolPackage, SymbolPackageSaved, SymbolSheetQuery,
    SymbolSheetSnapshot,
};

type Context = (ObjectId, EventCursor, ObjectId);
enum Completed {
    Sheet(SymbolSheetSnapshot),
    Loaded(SymbolPackageTemplate),
    Saved(SymbolPackageSaved),
    Changed(String),
}
struct Pending {
    source: Option<Context>,
    detached: bool,
    generation: u64,
    receiver: Receiver<Result<Completed, String>>,
}
pub struct PackageLibraryUi {
    open: bool,
    context: Option<Context>,
    guard: Option<SymbolSheetSnapshot>,
    pending: Option<Pending>,
    editor: PackageEditor,
    path: String,
    reference: String,
    delete_id: Option<ObjectId>,
    generation: u64,
    message: Option<String>,
}
impl Default for PackageLibraryUi {
    fn default() -> Self {
        Self {
            open: false,
            context: None,
            guard: None,
            pending: None,
            editor: PackageEditor::new(),
            path: String::new(),
            reference: "U1".into(),
            delete_id: None,
            generation: 0,
            message: None,
        }
    }
}
fn identity(c: Context) -> (ObjectId, ObjectId, ObjectId) {
    (c.0, c.1.stream_id, c.2)
}
fn committed(value: serde_json::Value) -> Result<Completed, String> {
    let result: viamind_engine::ExecResult = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut message = "Complete package command applied in one undoable sheet transaction; refresh the target before another edit".to_string();
    for delta in &result.deltas {
        if let viamind_engine::Delta::Sheet { before, after: Some(after), .. } = delta {
            let before = before.as_ref().and_then(|s| s.content.as_ref());
            if let Some(content) = &after.content {
                for package in content.packages.values().filter(|p| !before.is_some_and(|c| c.packages.contains_key(&p.id))) {
                    use std::fmt::Write as _;
                    let _ = write!(message, "\nCommitted package {}", package.id);
                    for unit in &package.units {
                        let _ = write!(message, "\nUnit {}: {}", unit.key, unit.symbol_id);
                    }
                }
            }
        }
    }
    Ok(Completed::Changed(message))
}
impl PackageLibraryUi {
    /// Close this local form while retaining the draft and pending worker.
    pub fn close(&mut self) {
        self.open = false;
    }

    fn fresh(&self) -> bool {
        self.guard.as_ref().is_some_and(|g| Some((g.expected_project, g.source_cursor, g.expected.id)) == self.context)
    }
    fn poll(&mut self, context: Option<Context>) {
        if self.context.map(identity) != context.map(identity) {
            self.guard = None;
            self.delete_id = None;
        }
        self.context = context;
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Package worker stopped; retry".into()),
        };
        let pending = self.pending.take().expect("completed package worker");
        if pending.generation != self.generation || (!pending.detached && pending.source.map(identity) != context.map(identity)) {
            return;
        }
        match result {
            Ok(Completed::Sheet(guard)) if pending.source == context => {
                self.guard = Some(guard);
                self.delete_id = None;
                self.message = Some("Refreshed complete package target".into());
            }
            Ok(Completed::Sheet(_)) => self.message = Some("Target changed during refresh; refresh again".into()),
            Ok(Completed::Loaded(template)) => {
                self.editor = PackageEditor::from_template(&template);
                self.message =
                    Some(format!("Loaded {} units; detached definition only. Existing project and drafts unchanged", template.units.len()));
            }
            Ok(Completed::Saved(saved)) => {
                self.message = Some(format!(
                    "Created {} · {} units / {} pins. Project/session unchanged; definition retained",
                    saved.path.display(),
                    saved.unit_count,
                    saved.pin_count
                ));
            }
            Ok(Completed::Changed(message)) => {
                self.guard = None;
                self.delete_id = None;
                self.message = Some(message);
            }
            Err(error) => self.message = Some(error),
        }
    }
    fn start(&mut self, engine: EngineHandle, action: UiAction, input: serde_json::Value) {
        self.start_with(engine, action, input, |job| std::thread::Builder::new().name("viamind-symbol-package".into()).spawn(job).map(|_| ()));
    }
    fn start_with(
        &mut self,
        engine: EngineHandle,
        action: UiAction,
        input: serde_json::Value,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        if self.pending.is_some() {
            return;
        }
        let detached = matches!(action, UiAction::LoadSymbolPackage | UiAction::SaveSymbolPackage);
        if self.context.is_none() && !detached {
            return;
        }
        let (sender, receiver) = sync_channel(1);
        let job = Box::new(move || {
            let result = action.run(&engine, input).map_err(|e| e.to_string()).and_then(|v| match action {
                UiAction::SnapshotSymbolSheet => serde_json::from_value(v).map(Completed::Sheet).map_err(|e| e.to_string()),
                UiAction::LoadSymbolPackage => serde_json::from_value(v).map(Completed::Loaded).map_err(|e| e.to_string()),
                UiAction::SaveSymbolPackage => serde_json::from_value(v).map(Completed::Saved).map_err(|e| e.to_string()),
                UiAction::PlaceSymbolPackage | UiAction::DeleteSymbolPackage => committed(v),
                _ => Err("Unsupported complete package action".into()),
            });
            let _ = sender.try_send(result);
        });
        match spawn(job) {
            Ok(()) => {
                self.message = None;
                self.pending = Some(Pending { source: self.context, detached, generation: self.generation, receiver });
            }
            Err(e) => self.message = Some(format!("Could not start package worker: {e}")),
        }
    }
    fn file_path(&self) -> Result<std::path::PathBuf, String> {
        if self.path.is_empty() || self.path.len() > 4096 {
            return Err("Package path requires1..4096 UTF8 bytes before copying".into());
        }
        Ok(self.path.clone().into())
    }
    fn proposal(&self) -> Result<PlaceSymbolPackage, String> {
        if !self.fresh() {
            return Err("Refresh the target sheet after document changes".into());
        }
        let guard = self.guard.as_ref().ok_or("Select and refresh a target sheet")?;
        let template = self.editor.template()?;
        let placements = self.editor.placements(&template)?;
        Ok(PlaceSymbolPackage {
            expected_project: guard.expected_project,
            source_cursor: guard.source_cursor,
            expected: guard.expected.clone(),
            template,
            designator: self.reference.clone(),
            placements,
        })
    }
    fn deletion(&self) -> Result<DeleteSymbolPackage, String> {
        if !self.fresh() {
            return Err("Refresh the target before deleting a complete package".into());
        }
        let g = self.guard.as_ref().ok_or("Refresh the target sheet")?;
        let package_id = self.delete_id.ok_or("Choose a complete package by its stable identity")?;
        if !g.expected.content.as_ref().is_some_and(|c| c.packages.contains_key(&package_id)) {
            return Err("Package is absent from the captured target".into());
        }
        Ok(DeleteSymbolPackage { expected_project: g.expected_project, source_cursor: g.source_cursor, expected: g.expected.clone(), package_id })
    }
}
impl ViamindApp {
    pub fn begin_package_library(&mut self) {
        self.set_schematic_mode(true);
        self.package_library.open = true;
    }
    pub fn package_library_window(&mut self, ui: &mut egui::Ui) {
        let engine = self.design.as_ref().map(|d| d.engine.clone());
        let context = engine.as_ref().and_then(|e| {
            self.schematic
                .active
                .then(|| self.schematic.active_sheet_id())
                .flatten()
                .map(|sheet| e.with_engine(|e| (e.project().id, e.event_cursor(), sheet)))
        });
        self.package_library.poll(context);
        if self.package_library.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        if !self.package_library.open {
            return;
        }
        let Some(engine) = engine else { return };
        let blocked = self.files.busy() || self.schematic_object_draft_active() || self.schematic_wires.active() || !self.schematic.active;
        let ready = self.package_library.pending.is_none();
        let fresh = self.package_library.fresh();
        let mut open = true;
        let mut action = None;
        let mut error = None;
        egui::Window::new("Native multi-unit package library").open(&mut open).default_width(560.).vscroll(true).show(ui.ctx(),|ui|{
            ui.label("Explicit complete package: all units share one reference/value/footprint. Pins retain separate identities; no implicit power or common connections.");
            ui.label("Package-aware annotation and package-to-PCB aggregation are separate follow-ups; legacy per-unit operations refuse grouped components.");
            if blocked{ui.label("Finish/cancel active schematic drafts or wires and choose a sheet before placement/deletion. Detached file editing remains available.");}
            ui.add_enabled_ui(ready,|ui|{
                let label=ui.label("Package file path (load or NEW save)");
                ui.add(egui::TextEdit::singleline(&mut self.package_library.path).id(egui::Id::new("package-file-path")).char_limit(4096)).labelled_by(label.id);
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Load complete package file").clicked(){
                        match self.package_library.file_path(){
                            Ok(path)=>action=Some((UiAction::LoadSymbolPackage,serde_json::to_value(LoadSymbolPackage{path}).expect("package load"))),
                            Err(e)=>error=Some(e),
                        }
                    }
                    if ui.button("Reset detached package").clicked(){self.package_library.editor=PackageEditor::new();self.package_library.generation=self.package_library.generation.wrapping_add(1);self.package_library.message=None;}
                });
            });
            self.package_library.editor.show(ui,ready);
            ui.add_enabled_ui(ready,|ui|{
                if ui.button("Save NEW complete package file").clicked(){match self.package_library.file_path().and_then(|path|self.package_library.editor.template().map(|template|SaveSymbolPackage{template,path})){Ok(input)=>action=Some((UiAction::SaveSymbolPackage,serde_json::to_value(input).expect("package save"))),Err(e)=>error=Some(e)}}
            });
            ui.separator();
            if ui.add_enabled(ready&&context.is_some()&&!self.files.busy(),egui::Button::new("Refresh complete package target")).clicked()&&let Some((_,_,sheet_id))=context {action=Some((UiAction::SnapshotSymbolSheet,serde_json::to_value(SymbolSheetQuery{sheet_id}).expect("sheet query")));}
            if let Some(g)=&self.package_library.guard{ui.label(format!("Target {} · {}:{}",g.expected.name,g.source_cursor.stream_id,g.source_cursor.sequence));}
            if !fresh{ui.label("Target is absent/stale. Refresh before Place or Delete.");}
            ui.add_enabled_ui(ready,|ui|{let l=ui.label("Shared new package reference");ui.add(egui::TextEdit::singleline(&mut self.package_library.reference).id(egui::Id::new("package-reference")).char_limit(128)).labelled_by(l.id);});
            self.package_library.editor.show_poses(ui,ready);
            if ui.add_enabled(ready&&fresh&&!blocked,egui::Button::new("Place complete package")).clicked(){match self.package_library.proposal(){Ok(p)=>action=Some((UiAction::PlaceSymbolPackage,serde_json::to_value(p).expect("package place"))),Err(e)=>error=Some(e)}}
            ui.separator();ui.label("Delete every member of one existing package atomically. Explicit wires/endpoints remain unchanged.");
            ui.add_enabled_ui(ready&&fresh&&!blocked,|ui|{
                let selected=self.package_library.delete_id.map_or("Choose package UUID".into(),|id|id.to_string());
                egui::ComboBox::from_id_salt("complete-package-delete").selected_text(selected).show_ui(ui,|ui|{
                    if let Some(c)=self.package_library.guard.as_ref().and_then(|g|g.expected.content.as_ref()){
                        let rows:Vec<_>=c.packages.values().collect();
                        egui::ScrollArea::vertical().max_height(160.).show_rows(ui,ui.text_style_height(&egui::TextStyle::Body),rows.len(),|ui,range|{for p in &rows[range]{ui.selectable_value(&mut self.package_library.delete_id,Some(p.id),format!("{} · {} units · {}",p.template_name,p.units.len(),p.id));}});
                    }
                });
                if ui.add_enabled(self.package_library.delete_id.is_some(),egui::Button::new("Delete complete package")).clicked(){match self.package_library.deletion(){Ok(p)=>action=Some((UiAction::DeleteSymbolPackage,serde_json::to_value(p).expect("package delete"))),Err(e)=>error=Some(e)}}
            });
            if !ready{ui.label("Working on bounded complete-package data; closing retains the worker and draft.");}
            if let Some(message)=&self.package_library.message{ui.label(message);}
        });
        self.package_library.open = open;
        if let Some(error) = error {
            self.package_library.message = Some(error);
        } else if let Some((action, input)) = action {
            self.package_library.start(engine, action, input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> (EngineHandle, ObjectId) {
        let mut p = viamind_doc::Project::new("Packages");
        let sheet = viamind_doc::Sheet::new("Main");
        let id = sheet.id;
        p.schematic.sheets.insert(id, sheet);
        (EngineHandle::new(p), id)
    }
    fn template() -> SymbolPackageTemplate {
        viamind_doc::symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json"))
            .unwrap()
    }
    #[test]
    fn guarded_desktop_proposal_places_and_deletes_complete_package_one_undo() {
        let (engine, sheet_id) = engine();
        let guard = engine.snapshot_symbol_sheet(&SymbolSheetQuery { sheet_id }).unwrap();
        let context = Some((guard.expected_project, guard.source_cursor, sheet_id));
        let mut state = PackageLibraryUi { context, guard: Some(guard), editor: PackageEditor::from_template(&template()), ..Default::default() };
        let original = engine.session_state();
        let input = serde_json::to_value(state.proposal().unwrap()).unwrap();
        UiAction::PlaceSymbolPackage.run(&engine, input).unwrap();
        let content = engine.read(|p| p.schematic.sheets[&sheet_id].content.clone().unwrap());
        assert_eq!(content.packages.len(), 1);
        assert_eq!(content.symbols.len(), 5);
        assert_eq!(content.symbols.values().map(|s| s.pins.len()).sum::<usize>(), 14);
        assert!(state.proposal().is_ok(), "local cached context is explicitly updated by poll");
        state.poll(Some((original.project.id, engine.event_cursor(), sheet_id)));
        assert!(state.proposal().is_err());
        UiAction::Undo.run(&engine, serde_json::json!({})).unwrap();
        assert_eq!(engine.session_state().project, original.project);
        UiAction::Redo.run(&engine, serde_json::json!({})).unwrap();
        let guard = engine.snapshot_symbol_sheet(&SymbolSheetQuery { sheet_id }).unwrap();
        state.context = Some((guard.expected_project, guard.source_cursor, sheet_id));
        state.delete_id = Some(*guard.expected.content.as_ref().unwrap().packages.keys().next().unwrap());
        state.guard = Some(guard);
        UiAction::DeleteSymbolPackage.run(&engine, serde_json::to_value(state.deletion().unwrap()).unwrap()).unwrap();
        assert!(engine.read(|p| p.schematic.sheets[&sheet_id].content.as_ref().unwrap().symbols.is_empty()));
        UiAction::Undo.run(&engine, serde_json::json!({})).unwrap();
        assert_eq!(engine.read(|p| p.schematic.sheets[&sheet_id].content.clone().unwrap()), content);
    }
    #[test]
    fn retained_workers_reject_old_stream_sheet_cursor_and_spawn_failure() {
        for index in 0..4 {
            let (engine, sheet_id) = engine();
            let guard = engine.snapshot_symbol_sheet(&SymbolSheetQuery { sheet_id }).unwrap();
            let context = (guard.expected_project, guard.source_cursor, sheet_id);
            let (sender, receiver) = sync_channel(1);
            let mut state = PackageLibraryUi {
                context: Some(context),
                pending: Some(Pending { source: Some(context), detached: false, generation: 0, receiver }),
                ..Default::default()
            };
            let mut changed = context;
            match index {
                0 => changed.0 = ObjectId::new(),
                1 => changed.1.stream_id = ObjectId::new(),
                2 => changed.1.sequence += 1,
                _ => changed.2 = ObjectId::new(),
            };
            state.poll(Some(changed));
            assert!(state.pending.is_some());
            sender.try_send(Ok(Completed::Sheet(guard))).ok().unwrap();
            state.poll(Some(changed));
            assert!(state.guard.is_none());
            assert!(state.pending.is_none());
        }
        let (engine, _) = engine();
        let mut state = PackageLibraryUi::default();
        state.start_with(engine, UiAction::LoadSymbolPackage, serde_json::json!({}), |_| Err(std::io::Error::other("controlled spawn refusal")));
        assert!(state.pending.is_none());
        assert!(state.message.unwrap().contains("controlled spawn refusal"));
    }
    #[test]
    fn actual_busy_editor_disables_reset_paths_and_rows_without_discarding_hidden_load() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let (engine, _) = engine();
        let original = engine.session_state();
        let mut app = ViamindApp::with_design(engine.clone(), None);
        app.begin_package_library();
        let (sender, receiver) = sync_channel(1);
        app.package_library.pending = Some(Pending { source: None, detached: true, generation: 0, receiver });
        let mut h = Harness::builder().with_size(egui::vec2(1600., 1800.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
        h.run_steps(5);
        for label in [
            "Package file path (load or NEW save)",
            "Reset detached package",
            "Unit key",
            "Definition width (mm)",
            "Add definition pin",
            "Save NEW complete package file",
        ] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        }
        h.state_mut().package_library.open = false;
        sender.try_send(Ok(Completed::Loaded(template()))).ok().unwrap();
        h.run_steps(3);
        assert!(h.state().package_library.pending.is_none());
        assert_eq!(h.state().package_library.editor.template().unwrap(), template());
        assert_eq!(engine.session_state(), original);
        h.state_mut().begin_package_library();
        h.run_steps(3);
        assert!(!h.get_by_label("Unit key").accesskit_node().is_disabled());
    }
    #[test]
    fn path_byte_bound_precedes_copy_and_typed_definition() {
        let mut state = PackageLibraryUi { path: "é".repeat(2049), ..Default::default() };
        assert!(state.file_path().unwrap_err().contains("4096"));
        state.path = "x".repeat(4096);
        assert_eq!(state.file_path().unwrap().as_os_str().len(), 4096);
        state.path.clear();
        assert!(state.file_path().is_err());
    }
}
