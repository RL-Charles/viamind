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
    const DESCRIPTION: &'static str = "Set explicit board-component manufacturer, MPN, supplier, supplier SKU and population in one undoable transaction. Null clears; all-default metadata normalizes to absent. Fields are 1–512 UTF-8 bytes, no controls/edge whitespace; project limit 2 MiB sourcing text/100000 metadata-bearing parts. Optional expected full Part and expected_project reject stale drafts. No identifier inference or lookup. Only sourcing_bom exports population; existing BOM/placement/job inventories remain unchanged.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let before = project.board.part(&self.id).ok_or_else(|| CommandError::not_found("part", self.id))?;
        if self.expected_project.is_some_and(|id| id != project.id) || self.expected.as_ref().is_some_and(|p| p != before) {
            return Err(CommandError::conflict("part or document changed; reload sourcing before applying"));
        }
        if let Some(m) = &self.assembly {
            m.validate().map_err(|e| CommandError::invalid("assembly", e))?;
        }
        let assembly = self.assembly.filter(|m| !m.is_default());
        viamind_doc::assembly::validate_metadata(
            project.board.parts.values().filter_map(|p| if p.id == self.id { assembly.as_ref() } else { p.assembly.as_ref() }),
        )
        .map_err(|e| CommandError::invalid("assembly", e))?;
        // Validate the original too before Delta::record_part copies it.
        before.assembly.as_ref().map(AssemblyMetadata::validate).transpose().map_err(|e| CommandError::invalid("assembly", e))?;
        if before.assembly == assembly {
            return Ok(vec![]);
        }
        let mut after = before.clone_without_assembly();
        after.assembly = assembly;
        Ok(vec![Delta::record_part(project, self.id, Some(after))])
    }
}
