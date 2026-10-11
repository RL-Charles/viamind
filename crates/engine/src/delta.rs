//! Before/after records of model changes.
//!
//! Every command reports what it changed as a list of [`Delta`]s. A delta holds a
//! full snapshot of one object before and after the change (`None` meaning the
//! object does not exist), so any change can be undone generically with
//! [`invert`] and the same list doubles as the change notification sent to the UI
//! and to agents.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use viamind_doc::{BoardOutline, ObjectId, Part, Project, Sheet};

/// The change to one model object: its full state before and after.
///
/// `before: None` means the object was created; `after: None` means it was
/// deleted. Serialises with a `kind` tag, e.g.
/// `{"kind":"part","id":"…","before":null,"after":{…}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Delta {
    /// Independent straight copper segment.
    Track { id: ObjectId, before: Option<viamind_doc::Track>, after: Option<viamind_doc::Track> },
    /// Circular through via.
    Via { id: ObjectId, before: Option<viamind_doc::Via>, after: Option<viamind_doc::Via> },
    /// The substrate geometry of the project changed.
    BoardOutline { id: ObjectId, before: Option<BoardOutline>, after: Option<BoardOutline> },
    /// A part on the board changed.
    Part {
        /// Id of the part.
        id: ObjectId,
        /// The part before the change, `None` if it was just created.
        before: Option<Part>,
        /// The part after the change, `None` if it was deleted.
        after: Option<Part>,
    },
    /// A schematic sheet changed.
    Sheet {
        /// Id of the sheet.
        id: ObjectId,
        /// The sheet before the change, `None` if it was just created.
        before: Option<Sheet>,
        /// The sheet after the change, `None` if it was deleted.
        after: Option<Sheet>,
    },
}

impl Delta {
    /// Id of the object this delta changes.
    pub fn id(&self) -> ObjectId {
        match self {
            Delta::Part { id, .. } | Delta::Sheet { id, .. } | Delta::BoardOutline { id, .. } | Delta::Track { id, .. } | Delta::Via { id, .. } => {
                *id
            }
        }
    }

    /// Whether the object was created (it did not exist before).
    pub fn is_create(&self) -> bool {
        match self {
            Delta::Track { before, after, .. } => before.is_none() && after.is_some(),
            Delta::Via { before, after, .. } => before.is_none() && after.is_some(),
            Delta::BoardOutline { before, after, .. } => before.is_none() && after.is_some(),
            Delta::Part { before, after, .. } => before.is_none() && after.is_some(),
            Delta::Sheet { before, after, .. } => before.is_none() && after.is_some(),
        }
    }

    /// Whether the object was deleted (it does not exist after).
    pub fn is_delete(&self) -> bool {
        match self {
            Delta::Track { before, after, .. } => before.is_some() && after.is_none(),
            Delta::Via { before, after, .. } => before.is_some() && after.is_none(),
            Delta::BoardOutline { before, after, .. } => before.is_some() && after.is_none(),
            Delta::Part { before, after, .. } => before.is_some() && after.is_none(),
            Delta::Sheet { before, after, .. } => before.is_some() && after.is_none(),
        }
    }

    /// Whether before and after are identical, i.e. nothing changed.
    pub fn is_noop(&self) -> bool {
        match self {
            Delta::Track { before, after, .. } => before == after,
            Delta::Via { before, after, .. } => before == after,
            Delta::BoardOutline { before, after, .. } => before == after,
            Delta::Part { before, after, .. } => before == after,
            Delta::Sheet { before, after, .. } => before == after,
        }
    }

    /// The opposite change: before and after swapped.
    pub fn inverted(&self) -> Delta {
        match self.clone() {
            Delta::Track { id, before, after } => Delta::Track { id, before: after, after: before },
            Delta::Via { id, before, after } => Delta::Via { id, before: after, after: before },
            Delta::BoardOutline { id, before, after } => Delta::BoardOutline { id, before: after, after: before },
            Delta::Part { id, before, after } => Delta::Part { id, before: after, after: before },
            Delta::Sheet { id, before, after } => Delta::Sheet { id, before: after, after: before },
        }
    }

    /// Writes this delta's `after` state into `project` (inserting, replacing or
    /// removing the object). The engine uses this to undo, redo and replay; commands
    /// use [`Delta::record_part`] and friends instead.
    pub fn apply_to(&self, project: &mut Project) {
        match self {
            Delta::Track { id, after, .. } => write(&mut project.board.tracks, *id, after.clone()),
            Delta::Via { id, after, .. } => write(&mut project.board.vias, *id, after.clone()),
            Delta::BoardOutline { after, .. } => project.board.outline = after.clone(),
            Delta::Part { id, after, .. } => write(&mut project.board.parts, *id, after.clone()),
            Delta::Sheet { id, after, .. } => write(&mut project.schematic.sheets, *id, after.clone()),
        }
    }

    /// Sets the part `id` to `after` (`None` deletes it) and returns the delta
    /// recording the change. This is how commands change parts.
    pub fn record_part(project: &mut Project, id: ObjectId, after: Option<Part>) -> Delta {
        debug_assert!(after.as_ref().is_none_or(|part| part.id == id), "part id must match its key");
        let before = project.board.parts.get(&id).cloned();
        let delta = Delta::Part { id, before, after };
        delta.apply_to(project);
        delta
    }

    /// Sets the sheet `id` to `after` (`None` deletes it) and returns the delta
    /// recording the change. This is how commands change sheets.
    pub fn record_sheet(project: &mut Project, id: ObjectId, after: Option<Sheet>) -> Delta {
        debug_assert!(after.as_ref().is_none_or(|sheet| sheet.id == id), "sheet id must match its key");
        let before = project.schematic.sheets.get(&id).cloned();
        let delta = Delta::Sheet { id, before, after };
        delta.apply_to(project);
        delta
    }
}

fn write<T>(map: &mut std::collections::BTreeMap<ObjectId, T>, id: ObjectId, value: Option<T>) {
    match value {
        Some(value) => {
            map.insert(id, value);
        }
        None => {
            map.remove(&id);
        }
    }
}

/// The deltas that undo `deltas`: each one inverted, in reverse order.
pub fn invert(deltas: &[Delta]) -> Vec<Delta> {
    deltas.iter().rev().map(Delta::inverted).collect()
}

/// Applies `deltas` to `project` in order (see [`Delta::apply_to`]).
pub fn apply_all(project: &mut Project, deltas: &[Delta]) {
    for delta in deltas {
        delta.apply_to(project);
    }
}

/// Merges deltas that touch the same object into one (its first `before` and
/// last `after`), in the order objects were first touched, and drops objects
/// that end as they started. Applying the result has the same effect as
/// applying `deltas`, because each delta is a full snapshot of one object.
pub fn coalesce(deltas: Vec<Delta>) -> Vec<Delta> {
    let mut merged: Vec<Delta> = Vec::with_capacity(deltas.len());
    let mut index: HashMap<(u8, ObjectId), usize> = HashMap::with_capacity(deltas.len());
    for delta in deltas {
        let key = (delta.kind_index(), delta.id());
        match index.get(&key) {
            Some(&i) => merged[i].absorb(delta),
            None => {
                index.insert(key, merged.len());
                merged.push(delta);
            }
        }
    }
    merged.retain(|delta| !delta.is_noop());
    merged
}

impl Delta {
    fn kind_index(&self) -> u8 {
        match self {
            Delta::Part { .. } => 0,
            Delta::Sheet { .. } => 1,
            Delta::BoardOutline { .. } => 2,
            Delta::Track { .. } => 3,
            Delta::Via { .. } => 4,
        }
    }

    /// Takes `later`'s `after` state; both must be for the same object.
    fn absorb(&mut self, later: Delta) {
        match (self, later) {
            (Delta::Track { after, .. }, Delta::Track { after: a, .. }) => *after = a,
            (Delta::Via { after, .. }, Delta::Via { after: a, .. }) => *after = a,
            (Delta::Part { after, .. }, Delta::Part { after: later, .. }) => *after = later,
            (Delta::Sheet { after, .. }, Delta::Sheet { after: later, .. }) => *after = later,
            (Delta::BoardOutline { after, .. }, Delta::BoardOutline { after: later, .. }) => *after = later,
            _ => unreachable!("coalesce only merges deltas of the same kind"),
        }
    }
}

#[cfg(test)]
mod tests {
    use viamind_doc::Side;
    use viamind_geom::Point;

    use super::*;

    fn part(designator: &str) -> Part {
        Part {
            schematic_symbol: None,
            pads: None,
            id: ObjectId::new(),
            designator: designator.into(),
            value: "1k".into(),
            footprint: "R_0402".into(),
            position: Point::from_mm(1.0, 2.0),
            rotation_mdeg: 0,
            side: Side::Top,
        }
    }

    #[test]
    fn record_then_invert_restores() {
        let mut project = Project::new("t");
        let original = project.clone();
        let r1 = part("R1");
        let mut moved = r1.clone();
        moved.position = Point::from_mm(5.0, 5.0);
        let sheet = Sheet::new("Main");

        let deltas = vec![
            Delta::record_part(&mut project, r1.id, Some(r1.clone())),
            Delta::record_sheet(&mut project, sheet.id, Some(sheet.clone())),
            Delta::record_part(&mut project, r1.id, Some(moved.clone())),
            Delta::record_part(&mut project, r1.id, None),
        ];
        assert!(deltas[0].is_create() && deltas[1].is_create());
        assert!(!deltas[2].is_create() && !deltas[2].is_delete() && !deltas[2].is_noop());
        assert!(deltas[3].is_delete());
        assert!(project.board.parts.is_empty());
        let after = project.clone();

        apply_all(&mut project, &invert(&deltas));
        assert_eq!(project, original);
        apply_all(&mut project, &deltas);
        assert_eq!(project, after);
    }

    #[test]
    fn coalesce_keeps_first_before_and_last_after() {
        let mut project = Project::new("t");
        let r1 = part("R1");
        let r2 = part("R2");
        Delta::record_part(&mut project, r2.id, Some(r2.clone()));
        let start = project.clone();
        let mut moved = r2.clone();
        moved.position = Point::from_mm(9.0, 9.0);

        let deltas = vec![
            Delta::record_part(&mut project, r1.id, Some(r1.clone())),
            Delta::record_part(&mut project, r2.id, Some(moved.clone())),
            Delta::record_part(&mut project, r1.id, None),
            Delta::record_part(&mut project, r2.id, Some(r2.clone())),
            Delta::record_part(&mut project, r2.id, Some(moved.clone())),
        ];
        let end = project.clone();
        let merged = coalesce(deltas);
        // R1 was created and deleted again: no net change.
        assert_eq!(merged, vec![Delta::Part { id: r2.id, before: Some(r2), after: Some(moved) }]);

        let mut replay = start.clone();
        apply_all(&mut replay, &merged);
        assert_eq!(replay, end);
        apply_all(&mut replay, &invert(&merged));
        assert_eq!(replay, start);
    }

    #[test]
    fn serde_shape() {
        let r1 = part("R1");
        let delta = Delta::Part { id: r1.id, before: None, after: Some(r1.clone()) };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["kind"], "part");
        assert_eq!(json["id"], r1.id.to_string());
        assert!(json["before"].is_null());
        assert_eq!(json["after"]["designator"], "R1");
        assert_eq!(serde_json::from_value::<Delta>(json).unwrap(), delta);
    }
}
