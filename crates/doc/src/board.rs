//! The board and the parts placed on it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::Point;

use crate::id::ObjectId;

/// The physical board: placed parts (more object kinds follow).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// Explicit substrate boundary; absent in legacy projects with an automatic preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<crate::BoardOutline>,
    /// Placed parts by id, in id order.
    pub parts: BTreeMap<ObjectId, Part>,
}

impl Board {
    /// The part with `id`, if any.
    pub fn part(&self, id: &ObjectId) -> Option<&Part> {
        self.parts.get(id)
    }

    /// The first part (in id order) whose designator equals `designator`.
    pub fn part_by_designator(&self, designator: &str) -> Option<&Part> {
        self.parts.values().find(|part| part.designator == designator)
    }
}

/// A component placed on the board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Part {
    /// Stable id of the part.
    pub id: ObjectId,
    /// Reference designator, e.g. `"R1"`.
    pub designator: String,
    /// Value, e.g. `"10k"`.
    pub value: String,
    /// Footprint name, e.g. `"R_0603"`.
    pub footprint: String,
    /// Position of the part origin on the board, in nanometres.
    pub position: Point,
    /// Rotation counter-clockwise in millidegrees, normalised to `0..360_000`.
    pub rotation_mdeg: i32,
    /// Which side of the board the part sits on.
    pub side: Side,
}

/// Board side a part is mounted on. Serialises as `"top"` or `"bottom"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Component (top) side.
    #[default]
    Top,
    /// Solder (bottom) side.
    Bottom,
}

#[cfg(test)]
mod tests {
    use viamind_geom::Nm;

    use super::*;
    use crate::project::{Project, Sheet};

    fn sample() -> Project {
        let mut project = Project::new("demo");
        let sheet = Sheet::new("Main");
        project.schematic.sheets.insert(sheet.id, sheet);
        for (i, side) in [Side::Top, Side::Bottom].into_iter().enumerate() {
            let part = Part {
                id: ObjectId::new(),
                designator: format!("R{}", i + 1),
                value: "10k".into(),
                footprint: "R_0603".into(),
                position: Point::new(Nm(1_000_000 * i as i64), Nm(-250_000)),
                rotation_mdeg: 90_000,
                side,
            };
            project.board.parts.insert(part.id, part);
        }
        project
    }

    #[test]
    fn every_object_has_a_distinct_id() {
        let project = sample();
        let mut ids = vec![project.id];
        ids.extend(project.schematic.sheets.keys().copied());
        ids.extend(project.board.parts.keys().copied());
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        for (key, part) in &project.board.parts {
            assert_eq!(*key, part.id);
        }
        for (key, sheet) in &project.schematic.sheets {
            assert_eq!(*key, sheet.id);
        }
    }

    #[test]
    fn serde_round_trip_keeps_ids_and_order() {
        let project = sample();
        let json = serde_json::to_string(&project).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, project);
        assert_eq!(serde_json::to_string(&back).unwrap(), json, "serialisation is deterministic");
    }

    #[test]
    fn serde_shape() {
        let project = sample();
        let value = serde_json::to_value(&project).unwrap();
        let part = project.board.part_by_designator("R2").unwrap();
        let json_part = &value["board"]["parts"][part.id.to_string()];
        assert_eq!(json_part["side"], "bottom");
        assert_eq!(json_part["position"]["x"], 1_000_000);
        assert_eq!(json_part["rotation_mdeg"], 90_000);
        assert_eq!(json_part["id"], part.id.to_string());
        assert_eq!(value["id"], project.id.to_string());
    }

    #[test]
    fn lookups() {
        let project = sample();
        let r1 = project.board.part_by_designator("R1").unwrap();
        assert_eq!(project.board.part(&r1.id), Some(r1));
        assert!(project.board.part_by_designator("C1").is_none());
        let sheet = project.schematic.sheets.values().next().unwrap();
        assert_eq!(project.schematic.sheet(&sheet.id).unwrap().name, "Main");
        assert_eq!(Side::default(), Side::Top);
    }
}
