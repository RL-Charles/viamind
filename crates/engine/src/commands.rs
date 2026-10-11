//! Built-in commands: placing, moving and deleting parts.
//!
//! Coordinates are given in millimetres (what agents and users think in) and
//! stored as integer nanometres, rounded to the nearest nanometre.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Part, Project, Side};
use viamind_geom::Point;

use crate::command::{Command, CommandError};
use crate::delta::Delta;

/// Largest accepted coordinate magnitude, in millimetres (10 m).
pub const MAX_COORD_MM: f64 = 10_000.0;

/// Places a new part on the board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddPart {
    /// Optional project identity guard for a placement draft opened before switching documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_project: Option<ObjectId>,
    /// Reference designator, e.g. "R1". Must be non-empty and unique on the board.
    pub designator: String,
    /// Component value, e.g. "10k" or "100nF". May be empty.
    pub value: String,
    /// Footprint name, e.g. "R_0603". Must be non-empty.
    pub footprint: String,
    /// X position of the part origin, in millimetres.
    pub x_mm: f64,
    /// Y position of the part origin, in millimetres.
    pub y_mm: f64,
    /// Counter-clockwise rotation in degrees (default 0). Normalised to [0, 360) with 0.001° resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_deg: Option<f64>,
    /// Board side, "top" or "bottom" (default "top").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
}

impl Command for AddPart {
    const NAME: &'static str = "add_part";
    const DESCRIPTION: &'static str = "Place a new part on the board at (x_mm, y_mm). The new part's id is returned in the \
        result (`created`) and in the `after` snapshot of the part delta. Fails if the designator is already used.";

    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected_project.is_some_and(|id| id != project.id) {
            return Err(CommandError::conflict("document changed; open a new placement form"));
        }
        let designator = self.designator.trim();
        if designator.is_empty() {
            return Err(CommandError::invalid("designator", "must not be empty, e.g. \"R1\""));
        }
        if let Some(existing) = project.board.part_by_designator(designator) {
            return Err(CommandError::conflict(format!(
                "designator `{designator}` is already used by part {}; choose another designator or move that part instead",
                existing.id
            )));
        }
        let footprint = self.footprint.trim();
        if footprint.is_empty() {
            return Err(CommandError::invalid("footprint", "must not be empty, e.g. \"R_0603\""));
        }
        let position = position_mm("x_mm", self.x_mm, "y_mm", self.y_mm)?;
        let rotation_mdeg = rotation_mdeg(self.rotation_deg.unwrap_or(0.0))?;

        let part = Part {
            schematic_symbol: None,
            pads: None,
            id: ObjectId::new(),
            designator: designator.to_owned(),
            value: self.value.trim().to_owned(),
            footprint: footprint.to_owned(),
            position,
            rotation_mdeg,
            side: self.side.unwrap_or_default(),
        };
        Ok(vec![Delta::record_part(project, part.id, Some(part))])
    }
}

/// Moves an existing part so its origin is at the absolute position (x_mm, y_mm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MovePart {
    /// Id of the part to move.
    pub id: ObjectId,
    /// New absolute X position of the part origin, in millimetres.
    pub x_mm: f64,
    /// New absolute Y position of the part origin, in millimetres.
    pub y_mm: f64,
}

impl Command for MovePart {
    const NAME: &'static str = "move_part";
    const DESCRIPTION: &'static str = "Move an existing part so its origin is at the absolute board position (x_mm, y_mm). \
        Moving a part to where it already is changes nothing.";

    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let part = project.board.part(&self.id).ok_or_else(|| CommandError::not_found("part", self.id))?;
        let position = position_mm("x_mm", self.x_mm, "y_mm", self.y_mm)?;
        if part.position == position {
            return Ok(Vec::new());
        }
        let mut moved = part.clone();
        moved.position = position;
        Ok(vec![Delta::record_part(project, self.id, Some(moved))])
    }
}

/// Updates selected properties atomically, optionally guarded by an exact part snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePart {
    /// Stable part ID to edit.
    pub id: ObjectId,
    /// New unique reference designator; omitted keeps the current value.
    pub designator: Option<String>,
    /// New component value; empty is allowed.
    pub value: Option<String>,
    /// New nonempty footprint name.
    pub footprint: Option<String>,
    /// New absolute X in millimetres.
    pub x_mm: Option<f64>,
    /// New absolute Y in millimetres.
    pub y_mm: Option<f64>,
    /// New counter-clockwise degrees, normalized with millidegree resolution.
    pub rotation_deg: Option<f64>,
    /// New board side.
    pub side: Option<Side>,
    /// Exact previously read model part; mismatch rejects without changing anything.
    /// Coordinates in this snapshot are integer nanometres.
    pub expected: Option<Part>,
}

impl Command for UpdatePart {
    const NAME: &'static str = "update_part";
    const DESCRIPTION: &'static str = "Update part properties in one undoable transaction. Omitted fields keep their values. Supply expected as the exact part from get_design/query_parts to reject stale edits. Coordinates are mm, rotation degrees; expected.position is integer nanometres. Duplicate references and invalid fields reject atomically.";

    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let before = project.board.part(&self.id).ok_or_else(|| CommandError::not_found("part", self.id))?;
        if self.expected.as_ref().is_some_and(|expected| expected != before) {
            return Err(CommandError::conflict("part changed since it was read; reload its properties and review the edit"));
        }
        let mut after = before.clone();
        if let Some(designator) = self.designator {
            let designator = designator.trim();
            if designator.is_empty() {
                return Err(CommandError::invalid("designator", "must not be empty"));
            }
            if project.board.parts.values().any(|p| p.id != self.id && p.designator == designator) {
                return Err(CommandError::conflict(format!("designator `{designator}` is already used; choose another designator")));
            }
            after.designator = designator.into();
        }
        if let Some(value) = self.value {
            after.value = value.trim().into();
        }
        if let Some(footprint) = self.footprint {
            if footprint.trim().is_empty() {
                return Err(CommandError::invalid("footprint", "must not be empty"));
            }
            after.footprint = footprint.trim().into();
        }
        if let Some(x) = self.x_mm {
            after.position.x = position_mm("x_mm", x, "y_mm", 0.0)?.x;
        }
        if let Some(y) = self.y_mm {
            after.position.y = position_mm("x_mm", 0.0, "y_mm", y)?.y;
        }
        after.rotation_mdeg = self.rotation_deg.map(rotation_mdeg).transpose()?.unwrap_or(before.rotation_mdeg);
        after.side = self.side.unwrap_or(before.side);
        if &after == before {
            return Ok(Vec::new());
        }
        Ok(vec![Delta::record_part(project, self.id, Some(after))])
    }
}

/// An outline vertex at the millimetre input boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlinePoint {
    pub x_mm: f64,
    pub y_mm: f64,
}

/// Polygon board outline input, implicitly closed without a repeated end vertex.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineMm {
    pub outer: Vec<OutlinePoint>,
    #[serde(default)]
    pub cutouts: Vec<Vec<OutlinePoint>>,
}
/// Exact outline state observed when starting an edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpectedOutline {
    pub project_id: ObjectId,
    pub outline: Option<viamind_doc::BoardOutline>,
}

/// Replace the substrate outline in one undoable transaction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetBoardOutline {
    /// New polygons in millimetres, or explicit null to restore the legacy preview.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub outline: Option<OutlineMm>,
    /// Optional exact project/outline guard; model snapshot uses integer nanometres.
    pub expected: Option<ExpectedOutline>,
}
impl Command for SetBoardOutline {
    const NAME: &'static str = "set_board_outline";
    const DESCRIPTION: &'static str = "Set a fixed board outer polygon and internal cutouts. Vertices are x_mm/y_mm, rings close implicitly. Rejects crossings, overlaps, degenerate rings and cutouts outside the board. Max 2048 vertices/64 cutouts. Null outline restores legacy preview. Optional expected contains project_id and exact current model outline (integer nm), guarding stale drafts. One undoable change; native saves use version 2 when geometry is present.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected.is_some_and(|e| e.project_id != project.id || e.outline != project.board.outline) {
            return Err(CommandError::conflict("board outline or document changed; reload the outline before applying"));
        }
        let outline = self
            .outline
            .map(|input| {
                let ring = |points: Vec<OutlinePoint>| -> Result<viamind_geom::Polygon, CommandError> {
                    points
                        .into_iter()
                        .map(|p| position_mm("x_mm", p.x_mm, "y_mm", p.y_mm))
                        .collect::<Result<Vec<_>, _>>()
                        .map(viamind_geom::Polygon::new)
                };
                viamind_doc::BoardOutline::new(ring(input.outer)?, input.cutouts.into_iter().map(ring).collect::<Result<Vec<_>, _>>()?)
                    .map_err(|e| CommandError::invalid("outline", e.to_string()))
            })
            .transpose()?;
        if project.board.outline == outline {
            return Ok(Vec::new());
        }
        let before = std::mem::replace(&mut project.board.outline, outline.clone());
        Ok(vec![Delta::BoardOutline { id: project.id, before, after: outline }])
    }
}

/// Removes a part from the board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeletePart {
    /// Id of the part to delete.
    pub id: ObjectId,
}

impl Command for DeletePart {
    const NAME: &'static str = "delete_part";
    const DESCRIPTION: &'static str =
        "Delete a part from the board. The full part is kept in the `before` snapshot of the delta, so undo restores it.";

    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.board.part(&self.id).is_none() {
            return Err(CommandError::not_found("part", self.id));
        }
        Ok(vec![Delta::record_part(project, self.id, None)])
    }
}

/// Converts a millimetre position to nanometres, rejecting non-finite or out-of-range values.
pub(crate) fn position_mm(x_field: &str, x: f64, y_field: &str, y: f64) -> Result<Point, CommandError> {
    for (field, value) in [(x_field, x), (y_field, y)] {
        if !value.is_finite() || value.abs() > MAX_COORD_MM {
            return Err(CommandError::invalid(field, format!("must be a finite number of millimetres within ±{MAX_COORD_MM}, got {value}")));
        }
    }
    Ok(Point::from_mm(x, y))
}

/// Converts degrees to millidegrees normalised to `0..360_000`.
fn rotation_mdeg(degrees: f64) -> Result<i32, CommandError> {
    if !degrees.is_finite() || degrees.abs() > 1_000_000.0 {
        return Err(CommandError::invalid("rotation_deg", format!("must be a finite number of degrees, got {degrees}")));
    }
    // |degrees| ≤ 1e6, so the millidegree value fits an i64 exactly and the remainder fits an i32.
    let mdeg = (degrees * 1000.0).round() as i64;
    Ok(mdeg.rem_euclid(360_000) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_normalised() {
        assert_eq!(rotation_mdeg(0.0), Ok(0));
        assert_eq!(rotation_mdeg(90.0), Ok(90_000));
        assert_eq!(rotation_mdeg(-90.0), Ok(270_000));
        assert_eq!(rotation_mdeg(360.0), Ok(0));
        assert_eq!(rotation_mdeg(725.5), Ok(5_500));
        assert_eq!(rotation_mdeg(45.0005), Ok(45_001));
        assert!(rotation_mdeg(f64::NAN).is_err());
        assert!(rotation_mdeg(1e7).is_err());
    }

    #[test]
    fn positions_are_validated() {
        assert_eq!(position_mm("x", 1.5, "y", -2.0), Ok(Point::from_mm(1.5, -2.0)));
        let err = position_mm("x_mm", f64::INFINITY, "y_mm", 0.0).unwrap_err();
        assert!(err.to_string().starts_with("invalid `x_mm`: must be a finite number"), "{err}");
        assert!(position_mm("x_mm", 0.0, "y_mm", -10_000.5).is_err());
    }
}

/// Import an ASCII DXF outline as a guarded, ordinary undoable outline change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportBoardOutlineDxf {
    #[schemars(length(max = 8_388_608))]
    pub text: String,
    #[serde(default)]
    pub units: viamind_doc::dxf::DxfUnits,
    pub layer: Option<String>,
    pub expected: ExpectedOutline,
}
impl Command for ImportBoardOutlineDxf {
    const NAME: &'static str = "import_board_outline_dxf";
    const DESCRIPTION: &'static str = "Import ASCII DXF straight closed LWPOLYLINE/POLYLINE and joined LINE loops as outer boundary/cutouts. Required expected guards exact project_id/outline (nm). Units header (default) requires INSUNITS, mm or inches explicitly overrides it. Optional exact layer filter. Rejects curves/bulges, 3D, open/branching/intersecting geometry, disjoint boards. Max 8 MiB, 2048 vertices/64 cutouts. One undoable change; errors preserve the document.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected.project_id != project.id || self.expected.outline != project.board.outline {
            return Err(CommandError::conflict("board outline or document changed; reload the outline before importing"));
        }
        let outline = viamind_doc::dxf::import_outline_dxf(&self.text, self.units, self.layer.as_deref())
            .map_err(|e| CommandError::invalid("text", e.to_string()))?;
        if project.board.outline.as_ref() == Some(&outline) {
            return Ok(Vec::new());
        }
        let before = project.board.outline.replace(outline.clone());
        Ok(vec![Delta::BoardOutline { id: project.id, before, after: Some(outline) }])
    }
}
