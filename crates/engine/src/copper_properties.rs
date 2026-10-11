//! Single-object properties: exact identities, guarded replacement and complete scoped DRC.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};
use std::fmt;
use viamind_doc::{CopperLayer, NamedNetRules, ObjectId, Project, Track, Via};
use viamind_geom::{Nm, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CopperPropertyId {
    Track { id: ObjectId },
    Via { id: ObjectId },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopperPropertyQuery {
    pub target: CopperPropertyId,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopperPropertyObject {
    Track { track: Track },
    Via { via: Via },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CopperPropertySnapshot {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub object: CopperPropertyObject,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub document_net_rules: NamedNetRules,
}
fn net_budget(net: &Option<String>) -> Result<(), CommandError> {
    if net.as_ref().is_some_and(|s| s.len() > 256) {
        return Err(CommandError::invalid("net", "property net must fit256UTF-8bytes"));
    }
    Ok(())
}
fn identity(id: ObjectId, stored: ObjectId) -> Result<(), CommandError> {
    if id.as_uuid().is_nil() || id != stored {
        return Err(CommandError::invalid("id", "property identity must be nonnil and match the stored object"));
    }
    Ok(())
}
impl EngineHandle {
    pub fn query_copper_properties(&self, query: &CopperPropertyQuery) -> Result<CopperPropertySnapshot, CommandError> {
        self.with_engine(|e| {
            let p = e.project();
            viamind_doc::named_net_rule_budget(&p.board.net_rules).map_err(|e| CommandError::invalid("rules", e))?;
            let object = match query.target {
                CopperPropertyId::Track { id } => {
                    let t = p.board.tracks.get(&id).ok_or_else(|| CommandError::not_found("track", id))?;
                    identity(id, t.id)?;
                    net_budget(&t.net)?;
                    t.validate().map_err(|e| CommandError::invalid("track", e))?;
                    CopperPropertyObject::Track { track: t.clone() }
                }
                CopperPropertyId::Via { id } => {
                    let v = p.board.vias.get(&id).ok_or_else(|| CommandError::not_found("via", id))?;
                    identity(id, v.id)?;
                    net_budget(&v.net)?;
                    v.validate().map_err(|e| CommandError::invalid("via", e))?;
                    CopperPropertyObject::Via { via: v.clone() }
                }
            };
            Ok(CopperPropertySnapshot { project_id: p.id, source_cursor: e.event_cursor(), object, document_net_rules: p.board.net_rules.clone() })
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTrackProperties {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub id: ObjectId,
    #[serde(deserialize_with = "deserialize_track")]
    pub expected: Track,
    pub width: Nm,
    pub layer: CopperLayer,
    #[serde(deserialize_with = "deserialize_net")]
    pub net: Option<String>,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetViaProperties {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub id: ObjectId,
    #[serde(deserialize_with = "deserialize_via")]
    pub expected: Via,
    pub diameter: Nm,
    pub drill: Nm,
    #[serde(deserialize_with = "deserialize_net")]
    pub net: Option<String>,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for SetTrackProperties {
    const NAME: &'static str = "set_track_properties";
    const DESCRIPTION: &'static str = "Change one existing track's explicit integer-Nm width, top/bottom layer and optional exact net while retaining ID/start/end. Requires expected_project, full initial expected_cursor and exact expected original Track. Net labels are null or trimmed/control-free,128characters/256UTF-8bytes. Exact document track-width minimum applies; clearance uses max(request baseline,document minima). Explicit outline and bounded complete shared candidate DRC required; candidate violations or incomplete reports reject unchanged. Legacy/scoped exclusions and existing unrelated violations are not certified. One stable-ID undo/redo; exact no-op refuses. No movement, implicit sizing, repair or shove.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        guard(project, self.expected_project)?;
        identity(self.id, self.expected.id)?;
        net_budget(&self.expected.net)?;
        net_budget(&self.net)?;
        self.expected.validate().map_err(|e| CommandError::invalid("expected", e))?;
        let current = project.board.tracks.get(&self.id).ok_or_else(|| CommandError::not_found("track", self.id))?;
        identity(self.id, current.id)?;
        net_budget(&current.net)?;
        if current != &self.expected {
            return Err(CommandError::conflict("track changed; Reload properties before applying"));
        }
        let after = Track { id: self.id, start: current.start, end: current.end, width: self.width, layer: self.layer, net: self.net };
        after.validate().map_err(|e| CommandError::invalid("track", e))?;
        viamind_doc::named_net_rule_budget(&project.board.net_rules).map_err(|e| CommandError::invalid("rules", e))?;
        if after.width < viamind_doc::named_track_width(&project.board.net_rules, after.net.as_deref()) {
            return Err(CommandError::invalid("width", "below exact document track-width minimum; no resizing"));
        }
        if current == &after {
            return Err(CommandError::invalid("properties", "no property changes"));
        }
        let growth = after.net.as_ref().map_or(0, String::len).saturating_sub(current.net.as_ref().map_or(0, String::len));
        let delta = Delta::Track { id: self.id, before: Some(self.expected), after: Some(after) };
        preflight(project, &delta, self.id, growth, &self.rules)?;
        delta.apply_to(project);
        Ok(vec![delta])
    }
}
impl Command for SetViaProperties {
    const NAME: &'static str = "set_via_properties";
    const DESCRIPTION: &'static str = "Change one existing through via's explicit integer-Nm copper diameter, drill diameter and optional exact net while retaining ID/position/through span. Requires expected_project, full initial expected_cursor and exact expected original Via. Net labels null or trimmed/control-free,128characters/256UTF-8bytes. Drill and exact doubled-annulus floors max request baselines with document minima; no resizing. Explicit outline and bounded complete shared candidate DRC required, including hole spacing and foreign copper/edge checks; candidate violations or incomplete reports reject unchanged. Legacy/scoped exclusions and existing unrelated violations are not certified. One stable-ID undo/redo; exact no-op refuses. No movement, new via, implicit sizing, repair or shove.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        guard(project, self.expected_project)?;
        identity(self.id, self.expected.id)?;
        net_budget(&self.expected.net)?;
        net_budget(&self.net)?;
        self.expected.validate().map_err(|e| CommandError::invalid("expected", e))?;
        let current = project.board.vias.get(&self.id).ok_or_else(|| CommandError::not_found("via", self.id))?;
        identity(self.id, current.id)?;
        net_budget(&current.net)?;
        if current != &self.expected {
            return Err(CommandError::conflict("via changed; Reload properties before applying"));
        }
        let after = Via { id: self.id, position: current.position, diameter: self.diameter, drill: self.drill, net: self.net };
        after.validate().map_err(|e| CommandError::invalid("via", e))?;
        crate::route_via::validate_via_minima(project, &after, &self.rules)?;
        if current == &after {
            return Err(CommandError::invalid("properties", "no property changes"));
        }
        let growth = after.net.as_ref().map_or(0, String::len).saturating_sub(current.net.as_ref().map_or(0, String::len));
        let delta = Delta::Via { id: self.id, before: Some(self.expected), after: Some(after) };
        preflight(project, &delta, self.id, growth, &self.rules)?;
        delta.apply_to(project);
        Ok(vec![delta])
    }
}
fn guard(project: &Project, expected: ObjectId) -> Result<(), CommandError> {
    if project.id != expected {
        return Err(CommandError::conflict("project changed; Reload copper properties"));
    }
    Ok(())
}
fn preflight(project: &Project, delta: &Delta, id: ObjectId, growth: usize, rules: &viamind_drc::DrcInput) -> Result<(), CommandError> {
    rules.validate().map_err(|e| CommandError::invalid("rules", e.to_string()))?;
    if project.board.outline.is_none() {
        return Err(CommandError::invalid("outline", "explicit board outline required for checked property edits"));
    }
    crate::auto_drc::snapshot_budget_with_text_growth(project, 100_000, growth)
        .map_err(|e| CommandError::conflict(format!("property preflight cannot capture bounded board: {e}")))?;
    let mut candidate = Project { id: project.id, name: String::new(), schematic: Default::default(), board: project.board.clone_without_assembly() };
    delta.apply_to(&mut candidate);
    let report = viamind_drc::run_drc(&candidate, rules).map_err(|e| CommandError::invalid("rules", e.to_string()))?;
    if report.truncated || report.incomplete_reason.is_some() {
        return Err(CommandError::conflict(format!(
            "property preflight incomplete; no properties changed: {}",
            report.incomplete_reason.as_deref().unwrap_or("DRC result truncated")
        )));
    }
    if let Some(v) = report.violations.iter().find(|v| v.objects.iter().any(|o| o.selection_id() == id)) {
        return Err(CommandError::conflict(format!("property preflight failed; no properties changed: {}", v.message)));
    }
    Ok(())
}
fn deserialize_net<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    struct Optional;
    struct Text;
    impl<'de> Visitor<'de> for Text {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("net within256UTF-8bytes")
        }
        fn visit_str<E: de::Error>(self, s: &str) -> Result<String, E> {
            if s.len() > 256 {
                return Err(E::custom("net exceeds256UTF-8bytes"));
            }
            Ok(s.to_owned())
        }
        fn visit_string<E: de::Error>(self, s: String) -> Result<String, E> {
            if s.len() > 256 {
                return Err(E::custom("net exceeds256UTF-8bytes"));
            }
            Ok(s)
        }
    }
    impl<'de> Visitor<'de> for Optional {
        type Value = Option<String>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("null or bounded net")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_string(Text).map(Some)
        }
    }
    d.deserialize_option(Optional)
}
fn deserialize_track<'de, D: Deserializer<'de>>(d: D) -> Result<Track, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        id: ObjectId,
        start: Point,
        end: Point,
        width: Nm,
        layer: CopperLayer,
        #[serde(deserialize_with = "deserialize_net")]
        net: Option<String>,
    }
    let f = Fields::deserialize(d)?;
    let t = Track { id: f.id, start: f.start, end: f.end, width: f.width, layer: f.layer, net: f.net };
    t.validate().map_err(de::Error::custom)?;
    Ok(t)
}
fn deserialize_via<'de, D: Deserializer<'de>>(d: D) -> Result<Via, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        id: ObjectId,
        position: Point,
        diameter: Nm,
        drill: Nm,
        #[serde(deserialize_with = "deserialize_net")]
        net: Option<String>,
    }
    let f = Fields::deserialize(d)?;
    let v = Via { id: f.id, position: f.position, diameter: f.diameter, drill: f.drill, net: f.net };
    v.validate().map_err(de::Error::custom)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_replacement_growth_is_reserved_before_clone_at_exact_text_boundary() {
        let mut p = Project::new("text boundary");
        let part = viamind_doc::Part {
            id: ObjectId::new(),
            assembly: None,
            schematic_symbol: None,
            designator: "L".into(),
            value: "x".repeat(2 * 1024 * 1024 - 2),
            footprint: String::new(),
            position: Point::new(Nm(0), Nm(0)),
            rotation_mdeg: 0,
            side: viamind_doc::Side::Top,
            pads: None,
        };
        p.board.parts.insert(part.id, part);
        assert!(crate::auto_drc::snapshot_budget_with_text_growth(&p, 100_000, 1).is_ok());
        assert!(crate::auto_drc::snapshot_budget_with_text_growth(&p, 100_000, 2).unwrap_err().contains("snapshot text"));
        // Only fixed geometry text is counted; huge unused metadata stays borrowed/excluded.
        p.name = "x".repeat(3 * 1024 * 1024);
        assert!(crate::auto_drc::snapshot_budget_with_text_growth(&p, 100_000, 1).is_ok());
    }
}
