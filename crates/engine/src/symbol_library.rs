//! Bounded detached symbol capture and guarded creation of new schematic instances.
use crate::{Command, CommandError, Delta, EngineError, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, SchematicContent},
    symbol_library::SymbolTemplate,
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolSheetQuery {
    pub sheet_id: ObjectId,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolSheetSnapshot {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub expected: Sheet,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceSymbolTemplate {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub expected: Sheet,
    pub template: SymbolTemplate,
    pub designator: String,
    pub position: Coordinate,
    pub rotation_quarters: u8,
    pub mirrored: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadSymbolTemplate {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveSymbolTemplate {
    pub template: SymbolTemplate,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolTemplateSaveResult {
    pub path: PathBuf,
    pub name: String,
    pub pin_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportSymbolTemplate {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub sheet_id: ObjectId,
    pub symbol_id: ObjectId,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolTemplateExportResult {
    pub path: PathBuf,
    pub exported_cursor: EventCursor,
    pub name: String,
    pub pin_count: usize,
}

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("symbol_library", message)
}
impl Command for PlaceSymbolTemplate {
    const NAME: &'static str = "place_symbol_template";
    const DESCRIPTION: &'static str = "Place one fresh rectangular single-unit native symbol from a strict detached template. Requires exact project UUID/full source cursor/expected sheet; obtains target guard from snapshot_symbol_sheet. Generates fresh symbol/pin UUIDs, retains library/source/existing symbols/PCB unchanged, one sheet transaction/undo. Explicit reference must be unique globally; exact integer Nm position, rotation_quarters0..3 and local X mirror. Template at most1024 pins/256KiB text; selected sheet at most10000 objects/10000 pins/2MiB text; global validation at most100000 identities/1000 sheets/4MiB text. Stale, malformed or excessive results reject atomically. No existing-instance updates or multi-unit support.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let sheet_bytes = crate::schematic_budget::sheet(&self.expected, "symbol library")?;
        self.template.validate().map_err(invalid)?;
        if project.id.as_uuid().is_nil()
            || project.id != self.expected_project
            || project.schematic.sheets.get(&self.expected.id) != Some(&self.expected)
        {
            return Err(CommandError::conflict("project or target sheet changed; reload the symbol library preview"));
        }
        let project_bytes = crate::schematic_budget::project(project, self.template.pins.len() + 1, "global symbol validation")?;
        viamind_doc::schematic::name(&self.designator, false, 128).map_err(invalid)?;
        let added_bytes = [self.designator.as_str(), self.template.value.as_str(), self.template.footprint.as_deref().unwrap_or("")]
            .into_iter()
            .chain(self.template.pins.iter().flat_map(|p| [p.number.as_str(), p.name.as_str()]))
            .fold(0usize, |n, s| n.saturating_add(s.len()));
        if sheet_bytes.saturating_add(added_bytes) > 2 * 1024 * 1024 || project_bytes.saturating_add(added_bytes) > 4 * 1024 * 1024 {
            return Err(invalid("result exceeds 2 MiB sheet or 4 MiB global schematic text"));
        }
        let old = self.expected.content.as_ref();
        if old.map_or(0, SchematicContent::object_count) >= 10000
            || old.map_or(0, SchematicContent::pin_count).saturating_add(self.template.pins.len()) > 10000
        {
            return Err(invalid("result would exceed 10000 target sheet objects or pins"));
        }
        if project
            .schematic
            .sheets
            .values()
            .filter_map(|s| s.content.as_ref())
            .flat_map(|c| c.symbols.values())
            .any(|s| s.designator == self.designator)
        {
            return Err(invalid("symbol reference already exists in the project"));
        }
        let symbol = self.template.instantiate(&self.designator, self.position, self.rotation_quarters, self.mirrored).map_err(invalid)?;
        let mut sheet = self.expected;
        sheet.content.get_or_insert_with(SchematicContent::default).symbols.insert(symbol.id, symbol);
        viamind_doc::schematic::validate_project(project, Some(&sheet)).map_err(invalid)?;
        Ok(vec![Delta::record_sheet(project, sheet.id, Some(sheet))])
    }
}

impl EngineHandle {
    /// Stage outside locks; publish under the current native-file lifecycle guard.
    pub fn save_symbol_template(&self, input: SaveSymbolTemplate) -> Result<SymbolTemplateSaveResult, EngineError> {
        self.save_symbol_before_publish(input, || {})
    }
    fn save_symbol_before_publish(&self, input: SaveSymbolTemplate, before_publish: impl FnOnce()) -> Result<SymbolTemplateSaveResult, EngineError> {
        let staged = viamind_doc::symbol_library::stage_new(&input.template, &input.path).map_err(|message| EngineError::File { message })?;
        before_publish();
        let path = self.with_native_file_guard(|native| staged.publish(native)).map_err(|message| EngineError::File { message })?;
        Ok(SymbolTemplateSaveResult { path, name: input.template.name, pin_count: input.template.pins.len() })
    }
    pub fn snapshot_symbol_sheet(&self, input: &SymbolSheetQuery) -> Result<SymbolSheetSnapshot, CommandError> {
        self.with_engine(|engine| {
            let project = engine.project();
            if project.id.as_uuid().is_nil() {
                return Err(invalid("project identity must be nonnil"));
            }
            let sheet = project.schematic.sheets.get(&input.sheet_id).ok_or_else(|| CommandError::not_found("sheet", input.sheet_id))?;
            if sheet.id != input.sheet_id || sheet.id.as_uuid().is_nil() {
                return Err(invalid("target sheet identity is invalid"));
            }
            crate::schematic_budget::sheet(sheet, "symbol library")?;
            Ok(SymbolSheetSnapshot { expected_project: project.id, source_cursor: engine.event_cursor(), expected: sheet.clone() })
        })
    }
    /// File I/O is outside the engine lock. Callers use a background/blocking worker.
    pub fn load_symbol_template(&self, input: LoadSymbolTemplate) -> Result<SymbolTemplate, EngineError> {
        viamind_doc::symbol_library::load(&input.path).map_err(|message| EngineError::File { message })
    }
    /// Capture only one bounded symbol definition; never clone a project or sheet.
    pub fn export_symbol_template(&self, input: ExportSymbolTemplate) -> Result<SymbolTemplateExportResult, EngineError> {
        self.export_symbol_before_publish(input, || {})
    }
    fn export_symbol_before_publish(
        &self,
        input: ExportSymbolTemplate,
        before_publish: impl FnOnce(),
    ) -> Result<SymbolTemplateExportResult, EngineError> {
        let template = self.with_engine(|engine| {
            let project = engine.project();
            if project.id.as_uuid().is_nil() || project.id != input.expected_project || engine.event_cursor() != input.source_cursor {
                return Err(EngineError::InvalidInput {
                    command: "export_symbol_template".into(),
                    message: "document changed; refresh before exporting a symbol template".into(),
                });
            }
            let sheet = project
                .schematic
                .sheets
                .get(&input.sheet_id)
                .filter(|s| s.id == input.sheet_id && !s.id.as_uuid().is_nil())
                .ok_or_else(|| EngineError::File { message: "source sheet identity is invalid".into() })?;
            let symbol = sheet
                .content
                .as_ref()
                .and_then(|c| c.symbols.get(&input.symbol_id))
                .filter(|s| s.id == input.symbol_id && !s.id.as_uuid().is_nil())
                .ok_or_else(|| EngineError::File { message: "source symbol no longer exists".into() })?;
            let template = SymbolTemplate::from_symbol(&input.name, symbol).map_err(|message| EngineError::File { message })?;
            Ok(template)
        })?;
        let pin_count = template.pins.len();
        let name = template.name.clone();
        let staged = viamind_doc::symbol_library::stage_new(&template, &input.path).map_err(|message| EngineError::File { message })?;
        before_publish();
        let path = self.with_native_file_guard(|native| staged.publish(native)).map_err(|message| EngineError::File { message })?;
        Ok(SymbolTemplateExportResult { path, exported_cursor: input.source_cursor, name, pin_count })
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use crate::{OpenProject, Origin, SaveProject};
    use viamind_doc::{file, symbol_library::load};

    fn fixture() -> EngineHandle {
        EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
    }
    fn export_input(engine: &EngineHandle, path: PathBuf) -> ExportSymbolTemplate {
        engine.with_engine(|e| {
            let sheet = e.project().schematic.sheets.values().next().unwrap();
            let symbol = sheet.content.as_ref().unwrap().symbols.values().find(|s| s.designator == "U1").unwrap();
            ExportSymbolTemplate {
                expected_project: e.project().id,
                source_cursor: e.event_cursor(),
                sheet_id: sheet.id,
                symbol_id: symbol.id,
                name: "Exact regulator".into(),
                path,
            }
        })
    }
    fn definition(engine: &EngineHandle, input: &ExportSymbolTemplate) -> SymbolTemplate {
        engine.read(|p| {
            SymbolTemplate::from_symbol(&input.name, &p.schematic.sheets[&input.sheet_id].content.as_ref().unwrap().symbols[&input.symbol_id])
                .unwrap()
        })
    }

    #[test]
    fn both_symbol_writers_protect_latest_peer_saved_and_opened_native_path() {
        for exporting in [false, true] {
            for opening in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("became-native.json");
                let engine = fixture();
                let input = export_input(&engine, path.clone());
                let template = definition(&engine, &input);
                let peer = engine.clone();
                let mut after = None;
                let before_publish = || {
                    if opening {
                        file::save_new(&viamind_doc::Project::new("Peer opened native"), &path).unwrap();
                        peer.open_project(
                            OpenProject { path: path.clone(), discard_dirty: true, expected_cursor: Some(peer.event_cursor()) },
                            Origin::agent("peer"),
                        )
                        .unwrap();
                    } else {
                        peer.save_project(SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
                    }
                    after = Some((peer.session_state(), peer.document_status(), std::fs::read(&path).unwrap()));
                };
                let error = if exporting {
                    engine.export_symbol_before_publish(input, before_publish).unwrap_err()
                } else {
                    engine.save_symbol_before_publish(SaveSymbolTemplate { template, path: path.clone() }, before_publish).unwrap_err()
                };
                assert!(error.to_string().contains("active native"), "latest path guard must precede generic collision: {error}");
                let (session, status, bytes) = after.unwrap();
                assert_eq!(engine.session_state(), session);
                assert_eq!(engine.document_status(), status);
                assert_eq!(std::fs::read(path).unwrap(), bytes);
                assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1, "private stage cleaned");
            }
        }
    }

    #[test]
    fn both_writers_preserve_a_racing_destination_and_full_dirty_session() {
        for exporting in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("racing.json");
            let engine = fixture();
            engine
                .execute("add_part", serde_json::json!({"designator":"KEEP1","value":"dirty","footprint":"F","x_mm":1,"y_mm":2}), Origin::User)
                .unwrap();
            let before = engine.session_state();
            let status = engine.document_status();
            let input = export_input(&engine, path.clone());
            let template = definition(&engine, &input);
            let racing = || std::fs::write(&path, b"racing destination bytes").unwrap();
            if exporting {
                assert!(engine.export_symbol_before_publish(input, racing).is_err());
            } else {
                assert!(engine.save_symbol_before_publish(SaveSymbolTemplate { template, path: path.clone() }, racing).is_err());
            }
            assert_eq!(engine.session_state(), before);
            assert_eq!(engine.document_status(), status);
            assert_eq!(std::fs::read(path).unwrap(), b"racing destination bytes");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn ordinary_peer_edits_during_staging_remain_live_and_export_retains_capture_cursor() {
        for exporting in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("valid.json");
            let engine = fixture();
            let input = export_input(&engine, path.clone());
            let captured_cursor = input.source_cursor;
            let template = definition(&engine, &input);
            let expected_bytes = viamind_doc::symbol_library::to_vec(&template).unwrap();
            let mut after = None;
            let edit = || {
                engine
                    .execute(
                        "add_part",
                        serde_json::json!({"designator":"PEER1","value":"keep","footprint":"F","x_mm":3,"y_mm":4}),
                        Origin::agent("peer"),
                    )
                    .unwrap();
                after = Some((engine.session_state(), engine.document_status()));
            };
            if exporting {
                assert_eq!(engine.export_symbol_before_publish(input, edit).unwrap().exported_cursor, captured_cursor);
            } else {
                engine.save_symbol_before_publish(SaveSymbolTemplate { template: template.clone(), path: path.clone() }, edit).unwrap();
            }
            let (session, status) = after.unwrap();
            assert_eq!(engine.session_state(), session);
            assert_eq!(engine.document_status(), status);
            assert_ne!(session.cursor, captured_cursor);
            assert_eq!(std::fs::read(&path).unwrap(), expected_bytes);
            assert_eq!(load(&path).unwrap(), template);
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }
}
