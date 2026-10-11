//! Atomic, guarded grouped part placement without changing local pad geometry.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use viamind_doc::{ObjectId, Part, Project, Side};
use viamind_geom::{Nm, Point};

/// Coordinate axis for arranging part origins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlacementAxis {
    X,
    Y,
}
/// Origin alignment reference within the current selection's coordinate range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OriginAlignment {
    Minimum,
    Centre,
    Maximum,
}
/// Group operation. All positions and deltas are integer nanometres.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartPlacement {
    Translate { delta: Point },
    Rotate { degrees: i32 },
    Flip,
    Align { axis: PlacementAxis, reference: OriginAlignment },
    Distribute { axis: PlacementAxis },
}
/// One guarded grouped operation, shared by pointer gestures, menus and MCP.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransformParts {
    pub expected_project: ObjectId,
    /// Exact Part records from query_parts/get_design; one to10000 unique IDs.
    pub expected: Vec<Part>,
    pub operation: PartPlacement,
}
fn coordinate(part: &Part, axis: PlacementAxis) -> i64 {
    match axis {
        PlacementAxis::X => part.position.x.0,
        PlacementAxis::Y => part.position.y.0,
    }
}
fn set_coordinate(part: &mut Part, axis: PlacementAxis, value: i64) {
    match axis {
        PlacementAxis::X => part.position.x = Nm(value),
        PlacementAxis::Y => part.position.y = Nm(value),
    }
}
impl Command for TransformParts {
    const NAME: &'static str = "transform_parts";
    const DESCRIPTION: &'static str = "Atomically move/rotate/flip/arrange a group of parts. Requires expected_project and exact expected Part snapshots for every target; any stale/missing/duplicate target rejects the whole edit. Translate delta uses integer nanometres. Rotation degrees are -360..360 around each part's own origin; flip changes side at the origin. Align/distribute use origins, not footprint edges; distribution keeps outermost origins fixed. IDs/local pads preserved; one undoable transaction.";
    fn apply(mut self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project {
            return Err(CommandError::conflict("project changed; reload the selected parts"));
        }
        if self.expected.is_empty() || self.expected.len() > 10000 {
            return Err(CommandError::invalid("expected", "must contain one to10000 unique Part records"));
        }
        let mut ids = BTreeSet::new();
        for expected in &self.expected {
            if !ids.insert(expected.id) {
                return Err(CommandError::invalid("expected", "duplicate part ID"));
            }
            let current = project.board.part(&expected.id).ok_or_else(|| CommandError::not_found("part", expected.id))?;
            if current != expected {
                return Err(CommandError::conflict(format!("{} changed; reload the selected parts before retrying", expected.designator)));
            }
        }
        self.expected.sort_by_key(|p| p.id);
        let mut after = self.expected.clone();
        match self.operation {
            PartPlacement::Translate { delta } => {
                for part in &mut after {
                    part.position = Point::new(
                        Nm(part.position.x.0.checked_add(delta.x.0).ok_or_else(|| CommandError::invalid("delta", "X coordinate overflow"))?),
                        Nm(part.position.y.0.checked_add(delta.y.0).ok_or_else(|| CommandError::invalid("delta", "Y coordinate overflow"))?),
                    );
                }
            }
            PartPlacement::Rotate { degrees } => {
                if !(-360..=360).contains(&degrees) {
                    return Err(CommandError::invalid("degrees", "must be within -360..360"));
                }
                for part in &mut after {
                    part.rotation_mdeg = (i64::from(part.rotation_mdeg) + i64::from(degrees) * 1000).rem_euclid(360000) as i32;
                }
            }
            PartPlacement::Flip => {
                for part in &mut after {
                    part.side = match part.side {
                        Side::Top => Side::Bottom,
                        Side::Bottom => Side::Top,
                    };
                }
            }
            PartPlacement::Align { axis, reference } => {
                let min = after.iter().map(|p| coordinate(p, axis)).min().expect("selection is nonempty");
                let max = after.iter().map(|p| coordinate(p, axis)).max().expect("selection is nonempty");
                let target = match reference {
                    OriginAlignment::Minimum => min,
                    OriginAlignment::Maximum => max,
                    OriginAlignment::Centre => (i128::from(min) + (i128::from(max) - i128::from(min)) / 2) as i64,
                };
                for part in &mut after {
                    set_coordinate(part, axis, target);
                }
            }
            PartPlacement::Distribute { axis } => {
                if after.len() > 1 {
                    let mut indices: Vec<_> = (0..after.len()).collect();
                    indices.sort_by_key(|&i| (coordinate(&after[i], axis), after[i].id));
                    let min = i128::from(coordinate(&after[indices[0]], axis));
                    let max = i128::from(coordinate(&after[*indices.last().expect("selection is nonempty")], axis));
                    let intervals = (indices.len() - 1) as i128;
                    for (i, index) in indices.into_iter().enumerate() {
                        set_coordinate(&mut after[index], axis, (min + (max - min) * i as i128 / intervals) as i64);
                    }
                }
            }
        }
        for part in &after {
            if [part.position.x.0, part.position.y.0].iter().any(|value| value.unsigned_abs() > 10_000_000_000) {
                return Err(CommandError::invalid("position", "resulting origins must stay within ±10000 mm"));
            }
        }
        Ok(self
            .expected
            .into_iter()
            .zip(after)
            .filter_map(|(before, after)| (before != after).then(|| Delta::record_part(project, after.id, Some(after))))
            .collect())
    }
}
