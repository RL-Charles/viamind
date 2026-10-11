//! Stable schematic/PCB links and bounded guarded synchronization.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Part, Project, schematic::SchematicSymbol};

/// Explicitly establish or remove a symbol link without reference-based inference.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchematicLinkTarget {
    Unlink,
    Symbol { expected: SchematicSymbol },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkSchematicSymbol {
    pub expected_project: ObjectId,
    pub expected_part: Part,
    /// Explicit unlink or exact current symbol snapshot.
    pub target: SchematicLinkTarget,
}
impl Command for LinkSchematicSymbol {
    const NAME: &'static str = "link_schematic_symbol";
    const DESCRIPTION: &'static str = "Link an existing PCB part to an existing schematic symbol by stable UUID, or explicitly unlink with target.kind=unlink. Requires exact project, part and current symbol guards; one symbol can link to at most one part. No reference-based inference or geometry/net edits. One undoable transaction; bounded native synchronization scope.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        sync_budget(project)?;
        viamind_doc::schematic::validate_project(project, None).map_err(|e| CommandError::invalid("project", e))?;
        if self.expected_project != project.id || project.board.parts.get(&self.expected_part.id) != Some(&self.expected_part) {
            return Err(CommandError::conflict("project or part changed; reload before linking"));
        }
        let symbol_id = if let SchematicLinkTarget::Symbol { expected } = &self.target {
            let current = project.schematic.sheets.values().filter_map(|s| s.content.as_ref()).find_map(|c| c.symbols.get(&expected.id));
            if current != Some(expected) {
                return Err(CommandError::conflict("symbol changed or missing; reload before linking"));
            }
            if expected.id.as_uuid().is_nil() {
                return Err(CommandError::invalid("target", "symbol identity must be nonnil"));
            }
            if project.board.parts.values().any(|p| p.id != self.expected_part.id && p.schematic_symbol == Some(expected.id)) {
                return Err(CommandError::conflict("symbol already linked to another board part; unlink it explicitly first"));
            }
            Some(expected.id)
        } else {
            None
        };
        let mut after = self.expected_part;
        if after.schematic_symbol == symbol_id {
            return Ok(vec![]);
        }
        after.schematic_symbol = symbol_id;
        Ok(vec![Delta::record_part(project, after.id, Some(after))])
    }
}

/// Check counts and string sizes before any whole-project synchronization copy.
fn sync_budget(project: &Project) -> Result<(), CommandError> {
    const MAX_OBJECTS: usize = 10_000;
    const MAX_TEXT: usize = 4 * 1024 * 1024;
    if project.id.as_uuid().is_nil() {
        return Err(CommandError::invalid("project", "project identity must be nonnil"));
    }
    let mut count = project
        .board
        .parts
        .len()
        .saturating_add(project.board.tracks.len())
        .saturating_add(project.board.vias.len())
        .saturating_add(project.schematic.sheets.len());
    let mut count_more = |n: usize| -> Result<(), CommandError> {
        count = count.saturating_add(n);
        if count > MAX_OBJECTS {
            return Err(CommandError::invalid("project", "synchronization supports at most10000 objects including pins, pads and outline vertices"));
        }
        Ok(())
    };
    count_more(0)?;
    for p in project.board.parts.values() {
        count_more(p.pads.as_ref().map_or(0, |p| p.0.len()))?;
    }
    if let Some(outline) = &project.board.outline {
        for ring in outline.rings() {
            count_more(ring.points.len())?;
        }
    }
    for sheet in project.schematic.sheets.values() {
        if let Some(c) = &sheet.content {
            count_more(c.object_count())?;
            for symbol in c.symbols.values() {
                count_more(symbol.pins.len())?;
            }
        }
    }
    let mut text = 0usize;
    let mut add = |s: &str| -> Result<(), CommandError> {
        text = text.saturating_add(s.len());
        if s.len() > 4096 || text > MAX_TEXT {
            return Err(CommandError::invalid("project", "synchronization text exceeds4096 bytes per string or4 MiB aggregate"));
        }
        Ok(())
    };
    add(&project.name)?;
    for p in project.board.parts.values() {
        for s in [&p.designator, &p.value, &p.footprint] {
            add(s)?;
        }
        for pad in p.pads.iter().flat_map(|p| &p.0) {
            add(&pad.number)?;
            if let Some(net) = &pad.net {
                add(net)?;
            }
        }
    }
    for t in project.board.tracks.values() {
        if let Some(net) = &t.net {
            add(net)?;
        }
    }
    for v in project.board.vias.values() {
        if let Some(net) = &v.net {
            add(net)?;
        }
    }
    for sheet in project.schematic.sheets.values() {
        add(&sheet.name)?;
        if let Some(c) = &sheet.content {
            for symbol in c.symbols.values() {
                add(&symbol.designator)?;
                add(&symbol.value)?;
                if let Some(f) = &symbol.footprint {
                    add(f)?;
                }
                for pin in &symbol.pins {
                    add(&pin.name)?;
                    add(&pin.number)?;
                }
            }
            for label in c.labels.values() {
                add(&label.name)?;
            }
        }
    }
    Ok(())
}
