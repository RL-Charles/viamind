//! Atomic per-part pad and net editing.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Pad, Pads, Part, Project};
use viamind_geom::Nm;

/// One rectangular SMD pad in footprint-local millimetres.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadInput {
    /// Omit to retain an existing pad's identity by number or allocate a new identity.
    pub id: Option<ObjectId>,
    pub number: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub net: Option<String>,
}
/// Replace a part's explicit pads. Empty list removes all electrical pads.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetPartPads {
    pub id: ObjectId,
    /// Exact snapshot required to prevent overwriting a concurrent edit.
    pub expected: Part,
    pub pads: Vec<PadInput>,
}
impl Command for SetPartPads {
    const NAME: &'static str = "set_part_pads";
    const DESCRIPTION: &'static str = "Replace rectangular SMD pads and named net memberships of a part, guarded by its exact expected snapshot. Coordinates are local millimetres. Omitted IDs preserve identities by number. Empty pads means no electrical pads.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let current = project.board.part(&self.id).ok_or_else(|| CommandError::not_found("part", self.id))?;
        if current != &self.expected {
            return Err(CommandError::conflict("part changed; reload pads before applying"));
        }
        if self.pads.len() > 1024 {
            return Err(CommandError::invalid("pads", "at most 1024 pads per part"));
        }
        let existing = current.pads.as_ref().map(|p| p.0.as_slice()).unwrap_or_default();
        let mut pads = Vec::new();
        for p in self.pads {
            let number = p.number.trim().to_owned();
            let id = match p.id {
                Some(id) if existing.iter().any(|p| p.id == id) => id,
                Some(_) => return Err(CommandError::invalid("pads.id", "must identify an existing pad on this part; omit IDs for new pads")),
                None => existing.iter().find(|p| p.number == number).map(|p| p.id).unwrap_or_default(),
            };
            let position = crate::commands::position_mm("pads.x_mm", p.x_mm, "pads.y_mm", p.y_mm)?;
            let length = |n: f64| -> Result<Nm, CommandError> {
                if !n.is_finite() || n <= 0.0 || n > 10000.0 {
                    return Err(CommandError::invalid("pads.size", "must be finite, greater than zero and at most 10000 mm"));
                }
                Ok(Nm::from_mm(n))
            };
            pads.push(Pad {
                id,
                number,
                position,
                width: length(p.width_mm)?,
                height: length(p.height_mm)?,
                net: p.net.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()),
                apertures: existing.iter().find(|p| p.id == id).and_then(|p| p.apertures.clone()),
            });
        }
        crate::footprint_artwork::validate_part_artwork(current)?;
        let mut after = current.clone();
        after.pads = Some(Pads::try_from(pads).map_err(|e| CommandError::invalid("pads", e))?);
        if after == *current {
            return Ok(vec![]);
        }
        Ok(vec![Delta::record_part(project, self.id, Some(after))])
    }
}
