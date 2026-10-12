//! Guarded mechanical drill edits and bounded global identity admission.
use crate::{Command, CommandError, Delta, EventCursor};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;
use std::{borrow::Cow, collections::BTreeSet};
use viamind_doc::{ObjectId, Part, PartHole, Project};

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("holes", message)
}

pub(crate) fn preflight(value: &Value) -> Result<(), String> {
    if value.get("holes").and_then(Value::as_array).is_some_and(|h| h.len() > 256) {
        return Err("at most256 holes before typed decoding".into());
    }
    if value
        .get("holes")
        .and_then(Value::as_array)
        .is_some_and(|h| h.iter().any(|h| h.get("key").and_then(Value::as_str).is_some_and(|s| s.len() > 64)))
    {
        return Err("hole keys support at most64 bytes before typed decoding".into());
    }
    Ok(())
}

/// Source and prospective whole-project admission precedes identity indexes and copies.
pub(crate) fn validate_part(part: &Part) -> Result<(), CommandError> {
    if let Some(holes) = &part.holes {
        viamind_doc::footprint_holes::validate_part_holes(holes).map_err(invalid)?;
        for hole in holes {
            viamind_doc::resolve_hole(part, hole).map_err(invalid)?;
        }
    }
    Ok(())
}

pub(crate) fn admit(project: &Project, current: &Part, pads: usize, holes: Option<&[PartHole]>) -> Result<(), CommandError> {
    viamind_doc::footprint_holes::preflight_project(project, None).map_err(invalid)?;
    viamind_doc::footprint_holes::preflight_replacement(project, current.id, pads, holes).map_err(invalid)?;
    if let Some(holes) = holes {
        viamind_doc::footprint_holes::validate_part_holes(holes).map_err(invalid)?;
        for hole in holes {
            viamind_doc::resolve_hole(current, hole).map_err(invalid)?;
        }
    }
    Ok(())
}

/// Caller has already admitted the combined source/prospective identity count.
pub(crate) fn identities(project: &Project, omit_owner_holes: Option<ObjectId>) -> Result<BTreeSet<ObjectId>, CommandError> {
    viamind_doc::schematic::validate_project(project, None).map_err(invalid)?;
    let mut ids = BTreeSet::from([project.id]);
    for part in project.board.parts.values() {
        ids.insert(part.id);
        ids.extend(part.pads.iter().flat_map(|p| &p.0).map(|p| p.id));
        if Some(part.id) != omit_owner_holes {
            ids.extend(part.holes.iter().flatten().map(|h| h.id));
        }
    }
    ids.extend(project.board.tracks.keys().chain(project.board.vias.keys()).copied());
    for sheet in project.schematic.sheets.values() {
        ids.insert(sheet.id);
        if let Some(c) = &sheet.content {
            ids.extend(c.packages.keys().chain(c.symbols.keys()).chain(c.wires.keys()).chain(c.junctions.keys()).chain(c.labels.keys()).copied());
            ids.extend(c.symbols.values().flat_map(|s| &s.pins).map(|p| p.id));
        }
    }
    Ok(ids)
}

pub(crate) fn check_ids(project: &Project, current: &Part, holes: Option<&[PartHole]>) -> Result<(), CommandError> {
    let mut ids = identities(project, Some(current.id))?;
    for hole in holes.into_iter().flatten() {
        if !ids.insert(hole.id) {
            return Err(invalid(format!("hole identity {} collides with the project", hole.id)));
        }
    }
    Ok(())
}

pub(crate) fn check_part_ids(project: &Project, current: &Part, after: &Part) -> Result<(), CommandError> {
    let mut ids = identities(project, Some(current.id))?;
    for pad in current.pads.iter().flat_map(|p| &p.0) {
        ids.remove(&pad.id);
    }
    for id in after.pads.iter().flat_map(|p| &p.0).map(|p| p.id).chain(after.holes.iter().flatten().map(|h| h.id)) {
        if id.as_uuid().is_nil() || !ids.insert(id) {
            return Err(invalid(format!("replacement pad/hole identity {id} collides with the project")));
        }
    }
    Ok(())
}

pub(crate) fn fresh(ids: &mut BTreeSet<ObjectId>, generate: &mut impl FnMut() -> ObjectId) -> Result<ObjectId, CommandError> {
    for _ in 0..1024 {
        let id = generate();
        if !id.as_uuid().is_nil() && ids.insert(id) {
            return Ok(id);
        }
    }
    Err(invalid("fresh hole identity exhausted1024 collision attempts"))
}

#[derive(Clone, Debug, Serialize)]
pub struct SetPartHoles {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub expected: Part,
    pub holes: Option<Vec<PartHole>>,
}
impl JsonSchema for SetPartHoles {
    fn schema_name() -> Cow<'static, str> {
        "SetPartHoles".into()
    }
    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"object","additionalProperties":false,"required":["expected_project","expected_cursor","expected","holes"],
            "properties":{"expected_project":g.subschema_for::<ObjectId>(),"expected_cursor":g.subschema_for::<EventCursor>(),
            "expected":g.subschema_for::<Part>(),"holes":g.subschema_for::<Option<Vec<PartHole>>>()}})
    }
}
impl<'de> Deserialize<'de> for SetPartHoles {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = crate::pad_apertures::StrictValue::deserialize(d)?.0;
        if !value.as_object().is_some_and(|v| v.contains_key("holes")) {
            return Err(de::Error::missing_field("holes"));
        }
        preflight(&value).map_err(de::Error::custom)?;
        if let Some(expected) = value.get("expected") {
            crate::pad_apertures::preflight_part(expected).map_err(de::Error::custom)?;
        }
        crate::pad_apertures::bounded_bytes(&value).map_err(de::Error::custom)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            expected_project: ObjectId,
            expected_cursor: EventCursor,
            expected: Part,
            #[serde(deserialize_with = "viamind_doc::footprint_holes::deserialize_holes")]
            holes: Option<Vec<PartHole>>,
        }
        let i: Input = serde_json::from_value(value).map_err(de::Error::custom)?;
        Ok(Self { expected_project: i.expected_project, expected_cursor: i.expected_cursor, expected: i.expected, holes: i.holes })
    }
}
impl Command for SetPartHoles {
    const NAME: &'static str = "set_part_holes";
    const DESCRIPTION: &'static str = "Replace one part's circular nonplated through holes with strict integer local Nm centers, positive diameters, stable mechanical keys and globally unique UUIDs. Requires project, full initial cursor, exact Part and present nullable holes. Null restores Legacy; empty array explicitly clears. One undoable Part delta or no-op; pads, nets, pose and all unrelated metadata unchanged. Native15. No electrical terminal or manufacturing certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project {
            return Err(CommandError::conflict("project changed; reload holes"));
        }
        let current = project.board.part(&self.expected.id).ok_or_else(|| CommandError::not_found("part", self.expected.id))?;
        if current != &self.expected {
            return Err(CommandError::conflict("part changed; reload holes"));
        }
        admit(project, current, current.pads.as_ref().map_or(0, |p| p.0.len()), self.holes.as_deref())?;
        crate::pad_apertures::validate_part(current)?;
        if current.holes == self.holes {
            return Ok(vec![]);
        }
        check_ids(project, current, self.holes.as_deref())?;
        let mut after = current.clone();
        after.holes = self.holes;
        Ok(vec![Delta::record_part(project, after.id, Some(after))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collision_generation_is_finite_and_never_accepts_nil_or_existing_ids() {
        let id = ObjectId::new();
        let mut ids = BTreeSet::from([id]);
        let mut attempts = 0;
        assert!(
            fresh(&mut ids, &mut || {
                attempts += 1;
                id
            })
            .is_err()
        );
        assert_eq!(attempts, 1024);
        assert_eq!(ids, BTreeSet::from([id]));
        let valid = ObjectId::new();
        let mut attempts = 0;
        assert_eq!(
            fresh(&mut ids, &mut || {
                attempts += 1;
                match attempts {
                    1 => "00000000-0000-0000-0000-000000000000".parse().unwrap(),
                    2 => id,
                    _ => valid,
                }
            })
            .unwrap(),
            valid
        );
        assert_eq!(attempts, 3);
        assert!(ids.contains(&valid));
    }
}
