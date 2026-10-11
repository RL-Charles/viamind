//! Bounded immutable policy query and one guarded undoable exact-map replacement.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use viamind_doc::{NamedNetMinima, NamedNetRules, ObjectId, Project};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedNetRuleRow {
    #[serde(deserialize_with = "viamind_doc::net_rules::deserialize_net_rule_name")]
    pub net: String,
    pub minimum_copper_clearance: viamind_geom::Nm,
    pub minimum_track_width: viamind_geom::Nm,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetNamedNetRules {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    /// Complete replacement, empty to clear. Duplicate exact names refuse.
    #[serde(deserialize_with = "deserialize_rows")]
    #[schemars(length(max = 128))]
    pub rules: Vec<NamedNetRuleRow>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedNetRuleSnapshot {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub document_net_rules: NamedNetRules,
}
impl EngineHandle {
    pub fn query_named_net_rules(&self) -> Result<NamedNetRuleSnapshot, CommandError> {
        self.with_engine(|e| {
            viamind_doc::named_net_rule_budget(&e.project().board.net_rules).map_err(|e| CommandError::invalid("rules", e))?;
            Ok(NamedNetRuleSnapshot {
                project_id: e.project().id,
                source_cursor: e.event_cursor(),
                document_net_rules: e.project().board.net_rules.clone(),
            })
        })
    }
}
impl Command for SetNamedNetRules {
    const NAME: &'static str = "set_named_net_rules";
    const DESCRIPTION: &'static str = "Atomically replace the bounded persisted exact named-net minima map. Requires expected_project and full initial expected_cursor; stale peers/reopen reject all rows. At most 128 distinct trimmed names within 128 characters/256 UTF-8 bytes each and 32768 aggregate name bytes. Integer Nm copper minimum 0..100000000, track-width minimum 1..100000000. Empty rows clear policy and preserve legacy schema; nonempty rules require native 8. Shared DRC and guarded routing resolve max(request clearance baseline, both named minima); widths stay explicit and below-minimum requests refuse. No preferred width, widening, classes/patterns/regions or shove. Existing violations may remain. One undo restores exact policy; no-op emits no event.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected_project != project.id {
            return Err(CommandError::conflict("project changed; refresh named-net rules"));
        }
        viamind_doc::named_net_rule_budget(&project.board.net_rules).map_err(|e| CommandError::invalid("rules", e))?;
        if self.rules.len() > viamind_doc::net_rules::MAX_NAMED_NET_RULES {
            return Err(CommandError::invalid("rules", "at most 128 exact named-net rules"));
        }
        let mut names = BTreeSet::new();
        let mut text = 0_usize;
        for row in &self.rules {
            viamind_doc::net_rules::validate_net_rule_name(&row.net).map_err(|e| CommandError::invalid("net", e))?;
            NamedNetMinima { minimum_copper_clearance: row.minimum_copper_clearance, minimum_track_width: row.minimum_track_width }
                .validate()
                .map_err(|e| CommandError::invalid("rules", e))?;
            text = text.saturating_add(row.net.len());
            if text > viamind_doc::net_rules::MAX_NAMED_NET_RULE_TEXT {
                return Err(CommandError::invalid("rules", "rule names exceed 32768 bytes"));
            }
            if !names.insert(row.net.as_str()) {
                return Err(CommandError::invalid("rules", "duplicate exact named-net rule"));
            }
        }
        drop(names);
        let after = self
            .rules
            .into_iter()
            .map(|r| (r.net, NamedNetMinima { minimum_copper_clearance: r.minimum_copper_clearance, minimum_track_width: r.minimum_track_width }))
            .collect();
        if project.board.net_rules == after {
            return Ok(Vec::new());
        }
        let delta = Delta::NamedNetRules { id: project.id, before: project.board.net_rules.clone(), after };
        delta.apply_to(project);
        Ok(vec![delta])
    }
}

fn deserialize_rows<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<NamedNetRuleRow>, D::Error> {
    struct RowSeed {
        allow: bool,
    }
    impl<'de> serde::de::DeserializeSeed<'de> for RowSeed {
        type Value = NamedNetRuleRow;
        fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
            if !self.allow {
                return Err(serde::de::Error::custom("named-net rule count exceeds 128"));
            }
            NamedNetRuleRow::deserialize(deserializer)
        }
    }
    struct RowsVisitor;
    impl<'de> serde::de::Visitor<'de> for RowsVisitor {
        type Value = Vec<NamedNetRuleRow>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 128 exact named-net minimum rows")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while let Some(row) = seq.next_element_seed(RowSeed { allow: rows.len() < viamind_doc::net_rules::MAX_NAMED_NET_RULES })? {
                rows.push(row);
            }
            Ok(rows)
        }
    }
    deserializer.deserialize_seq(RowsVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use viamind_geom::Nm;
    #[test]
    fn borrowed_policy_rows_and_text_charge_shared_capture_before_copy() {
        let mut project = Project::new("Policy capture");
        project.board.net_rules.insert("A".into(), NamedNetMinima { minimum_copper_clearance: Nm(1), minimum_track_width: Nm(1) });
        assert!(crate::auto_drc::snapshot_budget(&project, 0).unwrap_err().contains("structural objects"));
        assert!(crate::auto_drc::snapshot_budget(&project, 1).is_ok());
        project.board.net_rules.insert("large".repeat(100000), NamedNetMinima { minimum_copper_clearance: Nm(1), minimum_track_width: Nm(1) });
        assert!(crate::auto_drc::snapshot_budget(&project, 100000).unwrap_err().contains("256"));
    }
}
