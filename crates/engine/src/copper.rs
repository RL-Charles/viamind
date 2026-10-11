//! Shared integer copper editing; all validation occurs before mutation.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{CopperLayer, MAX_COPPER_OBJECTS, ObjectId, Project, Track, Via};
use viamind_geom::Nm;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddTrack {
    #[serde(default)]
    pub expected_project: Option<ObjectId>,
    pub start_x_mm: f64,
    pub start_y_mm: f64,
    pub end_x_mm: f64,
    pub end_y_mm: f64,
    pub width_mm: f64,
    pub layer: CopperLayer,
    #[serde(default)]
    pub net: Option<String>,
}
impl Command for AddTrack {
    const NAME: &'static str = "add_track";
    const DESCRIPTION: &'static str = "Add one straight round-ended copper track on top or bottom. Coordinates and width are mm, net is an optional exact trimmed name. Does not check clearance or electrical continuity. Returns its stable ID; undoable.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        guard(project, self.expected_project)?;
        capacity(project.board.tracks.len())?;
        let track = Track {
            id: ObjectId::new(),
            start: crate::commands::position_mm("start_x_mm", self.start_x_mm, "start_y_mm", self.start_y_mm)?,
            end: crate::commands::position_mm("end_x_mm", self.end_x_mm, "end_y_mm", self.end_y_mm)?,
            width: size(self.width_mm)?,
            layer: self.layer,
            net: self.net,
        };
        track.validate().map_err(|e| CommandError::invalid("track", e))?;
        let delta = Delta::Track { id: track.id, before: None, after: Some(track) };
        delta.apply_to(project);
        Ok(vec![delta])
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddVia {
    #[serde(default)]
    pub expected_project: Option<ObjectId>,
    pub x_mm: f64,
    pub y_mm: f64,
    pub diameter_mm: f64,
    pub drill_mm: f64,
    #[serde(default)]
    pub net: Option<String>,
}
impl Command for AddVia {
    const NAME: &'static str = "add_via";
    const DESCRIPTION: &'static str = "Add a circular plated through via spanning top/bottom copper; diameter and drill are mm with 0 < drill < diameter. Blind, buried and microvias are unsupported. Optional exact trimmed net label. No clearance/continuity checks. Undoable.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        guard(project, self.expected_project)?;
        capacity(project.board.vias.len())?;
        let via = Via {
            id: ObjectId::new(),
            position: crate::commands::position_mm("x_mm", self.x_mm, "y_mm", self.y_mm)?,
            diameter: size(self.diameter_mm)?,
            drill: size(self.drill_mm)?,
            net: self.net,
        };
        via.validate().map_err(|e| CommandError::invalid("via", e))?;
        let delta = Delta::Via { id: via.id, before: None, after: Some(via) };
        delta.apply_to(project);
        Ok(vec![delta])
    }
}
macro_rules! delete {
    ($type:ident, $model:ty, $map:ident, $delta:ident, $name:literal, $kind:literal) => {
        #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $type {
            pub id: ObjectId,
            #[serde(default)]
            pub expected_project: Option<ObjectId>,
            #[serde(default)]
            pub expected: Option<$model>,
        }
        impl Command for $type {
            const NAME: &'static str = $name;
            const DESCRIPTION: &'static str = concat!(
                "Delete one ",
                $kind,
                " by ID. Optional expected snapshot and project identity guard stale edits. Undo restores exact geometry and identity."
            );
            fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
                guard(project, self.expected_project)?;
                let current = project.board.$map.get(&self.id).ok_or_else(|| CommandError::not_found($kind, self.id))?;
                if self.expected.as_ref().is_some_and(|expected| expected != current) {
                    return Err(CommandError::conflict("copper changed; reload before deleting"));
                }
                let delta = Delta::$delta { id: self.id, before: Some(current.clone()), after: None };
                delta.apply_to(project);
                Ok(vec![delta])
            }
        }
    };
}
delete!(DeleteTrack, Track, tracks, Track, "delete_track", "track");
delete!(DeleteVia, Via, vias, Via, "delete_via", "via");
fn guard(project: &Project, expected: Option<ObjectId>) -> Result<(), CommandError> {
    if expected.is_some_and(|id| id != project.id) {
        return Err(CommandError::conflict("document changed; reopen the copper form"));
    }
    Ok(())
}
fn capacity(count: usize) -> Result<(), CommandError> {
    if count >= MAX_COPPER_OBJECTS {
        return Err(CommandError::invalid("copper", "at most 100000 objects of each copper type"));
    }
    Ok(())
}
fn size(n: f64) -> Result<Nm, CommandError> {
    if !n.is_finite() || n <= 0.0 || n > 10000.0 {
        return Err(CommandError::invalid("size", "must be finite, positive and at most 10000 mm"));
    }
    Ok(Nm::from_mm(n))
}
