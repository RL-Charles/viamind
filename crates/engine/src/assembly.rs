//! Guarded, undoable explicit component sourcing and population.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{AssemblyMetadata, ObjectId, Part, Project};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetPartAssembly {
    pub id: ObjectId,
    /// Complete explicitly entered metadata; null clears all metadata.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub assembly: Option<AssemblyMetadata>,
    pub expected: Option<Part>,
    pub expected_project: Option<ObjectId>,
}
impl Command for SetPartAssembly {
    const NAME: &'static str = "set_part_assembly";
    const DESCRIPTION: &'static str = "Set explicit board-component sourcing, population, optional mounting (surface_mount/through_hole/hybrid) and placement calibration in one undoable transaction. Placement has required centroid_local integer nm within ±10000mm and zero_axis_mdeg 0..359999 (local +X=0, -Y=90deg marker, not universal vendor zero). Null clears all; all-default metadata normalizes absent. Sourcing fields 1–512 UTF-8 bytes, no controls/edge whitespace; project limit 2MiB text/100000 metadata-bearing parts. Optional expected full Part and expected_project reject stale drafts. No inference/lookup. Calibrated placement and template BOMs apply population; existing BOM/placement/job inventory bytes remain unchanged.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let before = project.board.part(&self.id).ok_or_else(|| CommandError::not_found("part", self.id))?;
        if self.expected_project.is_some_and(|id| id != project.id) || self.expected.as_ref().is_some_and(|p| p != before) {
            return Err(CommandError::conflict("part or document changed; reload sourcing before applying"));
        }
        if let Some(m) = &self.assembly {
            m.validate().map_err(|e| CommandError::invalid("assembly", e))?;
        }
        let assembly = self.assembly.filter(|m| !m.is_default()).map(Box::new);
        viamind_doc::assembly::validate_metadata(
            project.board.parts.values().filter_map(|p| if p.id == self.id { assembly.as_deref() } else { p.assembly.as_deref() }),
        )
        .map_err(|e| CommandError::invalid("assembly", e))?;
        // Validate the original too before Delta::record_part copies it.
        before.assembly.as_deref().map(AssemblyMetadata::validate).transpose().map_err(|e| CommandError::invalid("assembly", e))?;
        if before.assembly == assembly {
            return Ok(vec![]);
        }
        let mut after = before.clone_without_assembly();
        after.assembly = assembly;
        Ok(vec![Delta::record_part(project, self.id, Some(after))])
    }
}
