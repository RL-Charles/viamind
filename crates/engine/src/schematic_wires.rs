//! Bounded exact sheet snapshots and atomic cardinal wire paths.
use crate::{Command, CommandError, Delta, EditSchematicSheet, EventCursor, SchematicEdit};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project, Sheet, schematic::Coordinate};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicWireSheetQuery {
    pub sheet_id: ObjectId,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicWireSnapshot {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub expected: Sheet,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawSchematicWirePolyline {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub expected: Sheet,
    pub vertices: Vec<Coordinate>,
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_wires", message)
}
fn budget(sheet: &Sheet) -> Result<(), CommandError> {
    let c = sheet.content.as_ref();
    if c.is_some_and(|c| c.object_count() > 10_000) {
        return Err(invalid("wire tool supports at most 10000 sheet objects"));
    }
    let mut pins = 0usize;
    let mut text = 0usize;
    let mut add = |s: &str| -> Result<(), CommandError> {
        text = text.saturating_add(s.len());
        if s.len() > 4096 || text > 2 * 1024 * 1024 {
            return Err(invalid("wire tool text exceeds 4096 bytes per string or 2 MiB per sheet"));
        }
        Ok(())
    };
    add(&sheet.name)?;
    if let Some(c) = c {
        for s in c.symbols.values() {
            pins = pins.saturating_add(s.pins.len());
            if pins > 10_000 {
                return Err(invalid("wire tool supports at most 10000 pins per sheet"));
            }
            for value in [s.designator.as_str(), &s.value, s.footprint.as_deref().unwrap_or("")] {
                add(value)?;
            }
            for p in &s.pins {
                add(&p.number)?;
                add(&p.name)?;
            }
        }
        for l in c.labels.values() {
            add(&l.name)?;
        }
    }
    if sheet.id.as_uuid().is_nil() {
        return Err(invalid("sheet identity must be nonnil"));
    }
    viamind_doc::schematic::validate_sheet(sheet).map_err(invalid)
}
// The inherited global identity/reference validator allocates a global set. Bound
// all data it visits before calling it; no board strings or geometry are copied.
fn global_validation_budget(project: &Project, added: usize) -> Result<(), CommandError> {
    let mut count = 1usize
        .saturating_add(added)
        .saturating_add(project.board.parts.len())
        .saturating_add(project.board.tracks.len())
        .saturating_add(project.board.vias.len())
        .saturating_add(project.schematic.sheets.len());
    let mut more = |n: usize| {
        count = count.saturating_add(n);
        if count > 100_000 {
            Err(invalid(
                "global wire validation supports at most 100000 combined project/sheet/part/pad/copper/schematic/pin identities including new wires",
            ))
        } else {
            Ok(())
        }
    };
    more(0)?;
    if project.schematic.sheets.len() > 1000 {
        return Err(invalid("global wire validation supports at most 1000 sheets"));
    }
    for p in project.board.parts.values() {
        more(p.pads.as_ref().map_or(0, |p| p.0.len()))?;
    }
    let mut bytes = 0usize;
    let mut text = |s: &str| {
        bytes = bytes.saturating_add(s.len());
        if s.len() > 4096 || bytes > 4 * 1024 * 1024 {
            Err(invalid("global wire validation text exceeds 4096 bytes per string or 4 MiB schematic text"))
        } else {
            Ok(())
        }
    };
    for sheet in project.schematic.sheets.values() {
        text(&sheet.name)?;
        if let Some(c) = &sheet.content {
            more(c.object_count())?;
            for s in c.symbols.values() {
                more(s.pins.len())?;
                for value in [s.designator.as_str(), &s.value, s.footprint.as_deref().unwrap_or("")] {
                    text(value)?;
                }
                for pin in &s.pins {
                    text(&pin.number)?;
                    text(&pin.name)?;
                }
            }
            for label in c.labels.values() {
                text(&label.name)?;
            }
        }
    }
    Ok(())
}
impl crate::EngineHandle {
    /// Validate limits before cloning just one sheet; never copy the whole project.
    pub fn snapshot_schematic_wire_sheet(&self, input: &SchematicWireSheetQuery) -> Result<SchematicWireSnapshot, CommandError> {
        self.with_engine(|engine| {
            let project = engine.project();
            if project.id.as_uuid().is_nil() {
                return Err(invalid("project identity must be nonnil"));
            }
            let sheet = project.schematic.sheets.get(&input.sheet_id).ok_or_else(|| CommandError::not_found("sheet", input.sheet_id))?;
            if sheet.id != input.sheet_id {
                return Err(invalid("sheet key does not match identity"));
            }
            budget(sheet)?;
            Ok(SchematicWireSnapshot { expected_project: project.id, source_cursor: engine.event_cursor(), expected: sheet.clone() })
        })
    }
}
impl Command for DrawSchematicWirePolyline {
    const NAME: &'static str = "draw_schematic_wire_polyline";
    const DESCRIPTION: &'static str = "Atomically add 1..255 horizontal/vertical schematic wire segments from 2..256 exact integer Nm vertices. Requires full initial event cursor, project UUID and exact expected sheet. Endpoint/T contacts join by native graph semantics; interior crossings do not join and no junctions are inserted. Snapshot with snapshot_schematic_wire_sheet first. One transaction/undo, no board changes. Selected sheet bounded to 10000 objects/10000 pins/2 MiB text. Commit global validation bounded to 100000 combined identities/1000 sheets/4 MiB schematic text; stale, malformed, noncardinal or oversized result rejects unchanged.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        budget(&self.expected)?;
        if !(2..=256).contains(&self.vertices.len()) {
            return Err(invalid("supply 2..256 confirmed vertices"));
        }
        global_validation_budget(project, self.vertices.len() - 1)?;
        if project.id != self.expected_project || project.schematic.sheets.get(&self.expected.id) != Some(&self.expected) {
            return Err(CommandError::conflict("project or sheet changed; cancel and restart wire drawing"));
        }
        let object_count = self.expected.content.as_ref().map_or(0, viamind_doc::schematic::SchematicContent::object_count);
        if object_count + self.vertices.len() - 1 > 10_000 {
            return Err(invalid("result would exceed 10000 sheet objects"));
        }
        for p in &self.vertices {
            p.validate().map_err(invalid)?;
        }
        let mut edits = Vec::with_capacity(self.vertices.len() - 1);
        for pair in self.vertices.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start == end || (start.x != end.x && start.y != end.y) {
                return Err(invalid("each segment must have distinct horizontal or vertical endpoints"));
            }
            edits.push(SchematicEdit::Wire { id: None, start, end });
        }
        EditSchematicSheet {
            expected_project: self.expected_project,
            sheet_id: Some(self.expected.id),
            name: self.expected.name.clone(),
            expected: Some(self.expected),
            edits,
        }
        .apply(project)
    }
}
