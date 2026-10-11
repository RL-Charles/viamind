//! The board and the parts placed on it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::Point;

use crate::id::ObjectId;

/// The physical board: placed parts (more object kinds follow).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Board {
    /// Enforced exact named-net minima; empty maps preserve legacy serialized bytes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "crate::net_rules::deserialize_named_net_rules")]
    pub net_rules: crate::NamedNetRules,
    /// Independent routed straight copper objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tracks: BTreeMap<ObjectId, crate::Track>,
    /// Through vias; other via types are not represented.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vias: BTreeMap<ObjectId, crate::Via>,
    /// Explicit substrate boundary; absent in legacy projects with an automatic preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<crate::BoardOutline>,
    /// Placed parts by id, in id order.
    pub parts: BTreeMap<ObjectId, Part>,
}

impl Board {
    /// Copy board geometry and existing headers without copying sourcing strings.
    /// Callers must validate geometry/text budgets and borrowed named-net rules before invoking this helper.
    pub fn clone_without_assembly(&self) -> Self {
        Self {
            net_rules: self.net_rules.clone(),
            tracks: self.tracks.clone(),
            vias: self.vias.clone(),
            outline: self.outline.clone(),
            parts: self.parts.iter().map(|(id, part)| (*id, part.clone_without_assembly())).collect(),
        }
    }
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
    /// Optional explicitly entered sourcing and population instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<Box<crate::AssemblyMetadata>>,
    /// Stable schematic symbol identity; missing symbols remain explicit unresolved links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schematic_symbol: Option<ObjectId>,
    /// Explicit electrical pads; None retains the legacy graphical preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pads: Option<crate::Pads>,
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

impl Part {
    /// Purpose-specific copy that never clones unused sourcing strings.
    pub fn clone_without_assembly(&self) -> Self {
        Self {
            assembly: None,
            schematic_symbol: self.schematic_symbol,
            pads: self.pads.clone(),
            id: self.id,
            designator: self.designator.clone(),
            value: self.value.clone(),
            footprint: self.footprint.clone(),
            position: self.position,
            rotation_mdeg: self.rotation_mdeg,
            side: self.side,
        }
    }
    /// Transform a footprint-local integer point exactly for quarter-turn rotations.
    /// Mirror bottom-side local X, rotate counterclockwise in y-down coordinates, then translate.
    /// Wider intermediates allow valid final coordinates even when mirroring alone overflows i64.
    pub fn local_to_world_nm_exact(&self, local: Point) -> Result<Point, &'static str> {
        let x = i128::from(local.x.0) * if self.side == Side::Bottom { -1 } else { 1 };
        let y = i128::from(local.y.0);
        let (x, y) = match self.rotation_mdeg.rem_euclid(360_000) {
            0 => (x, y),
            90_000 => (y, -x),
            180_000 => (-x, -y),
            270_000 => (-y, x),
            _ => return Err("exact pad attachment supports quarter-turn part rotations only"),
        };
        let x = i64::try_from(i128::from(self.position.x.0) + x).map_err(|_| "transformed pad center overflows integer coordinates")?;
        let y = i64::try_from(i128::from(self.position.y.0) + y).map_err(|_| "transformed pad center overflows integer coordinates")?;
        Ok(Point::new(viamind_geom::Nm(x), viamind_geom::Nm(y)))
    }

    /// Transform a footprint-local millimetre point at a render or query boundary.
    /// Mirrors local X for bottom-side parts, then rotates counterclockwise in y-down coordinates.
    /// Supports arbitrary angles without storing floating-point geometry in the design model.
    pub fn local_to_world_mm(&self, x_mm: f64, y_mm: f64) -> (f64, f64) {
        let x = if self.side == Side::Bottom { -x_mm } else { x_mm };
        let (s, c) = match self.rotation_mdeg {
            0 => (0.0, 1.0),
            90_000 => (1.0, 0.0),
            180_000 => (0.0, -1.0),
            270_000 => (-1.0, 0.0),
            angle => (f64::from(angle) / 1000.0).to_radians().sin_cos(),
        };
        (self.position.x.to_mm() + x * c + y_mm * s, self.position.y.to_mm() - x * s + y_mm * c)
    }
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
                assembly: None,
                schematic_symbol: None,
                pads: None,
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
    fn exact_integer_transforms_match_rendering_quadrants_and_check_overflow() {
        let project = sample();
        let mut part = project.board.parts.values().next().unwrap().clone();
        part.position = Point::new(Nm(1234567), Nm(-7654321));
        let local = Point::new(Nm(345), Nm(-789));
        for side in [Side::Top, Side::Bottom] {
            for angle in [0, 90000, 180000, 270000, -90000, 450000] {
                part.side = side;
                part.rotation_mdeg = angle;
                let exact = part.local_to_world_nm_exact(local).unwrap();
                let (x, y) = part.local_to_world_mm(local.x.to_mm(), local.y.to_mm());
                assert!((exact.x.to_mm() - x).abs() < 1e-12);
                assert!((exact.y.to_mm() - y).abs() < 1e-12);
            }
        }
        part.rotation_mdeg = 45000;
        assert!(part.local_to_world_nm_exact(local).is_err());
        part.rotation_mdeg = 0;
        part.side = Side::Bottom;
        part.position = Point::new(Nm(-1), Nm(0));
        assert_eq!(part.local_to_world_nm_exact(Point::new(Nm(i64::MIN), Nm(0))).unwrap().x, Nm(i64::MAX));
        part.position = Point::ORIGIN;
        assert!(part.local_to_world_nm_exact(Point::new(Nm(i64::MIN), Nm(0))).is_err());
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
