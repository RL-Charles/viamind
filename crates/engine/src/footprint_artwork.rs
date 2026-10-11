//! Guarded explicit artwork replacement; no electrical geometry changes.
use crate::{Command, CommandError, Delta, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use viamind_doc::{FootprintArtwork, ObjectId, Part, Project};

/// Validate only borrowed metadata before a live Part copy, retaining all fields.
pub(crate) fn validate_part_artwork(part: &Part) -> Result<(), CommandError> {
    if let Some(artwork) = &part.artwork {
        artwork.validate().map_err(|e| CommandError::invalid("artwork", e))?;
    }
    Ok(())
}

fn value_budget(value: &impl Serialize) -> Result<(), String> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            if self.0 > 2 * 1024 * 1024 {
                return Err(std::io::Error::other("artwork command exceeds2MiB serialized guard budget"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter(0), value).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetFootprintArtwork {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub expected: Part,
    pub artwork: Option<Box<FootprintArtwork>>,
}
impl<'de> Deserialize<'de> for SetFootprintArtwork {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        for art in [value.get("artwork"), value.get("expected").and_then(|p| p.get("artwork"))].into_iter().flatten() {
            if art.get("silkscreen").and_then(serde_json::Value::as_array).is_some_and(|r| r.len() > 256) {
                return Err(serde::de::Error::custom("footprint artwork supports at most256 silkscreen segments before typed decoding"));
            }
        }
        if value.get("expected").and_then(|p| p.get("pads")).and_then(serde_json::Value::as_array).is_some_and(|p| p.len() > 1024) {
            return Err(serde::de::Error::custom("expected part supports at most1024 pads before typed decoding"));
        }
        value_budget(&value).map_err(serde::de::Error::custom)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            expected_project: ObjectId,
            expected_cursor: EventCursor,
            expected: Part,
            artwork: Option<Box<FootprintArtwork>>,
        }
        let input: Input = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            expected_project: input.expected_project,
            expected_cursor: input.expected_cursor,
            expected: input.expected,
            artwork: input.artwork,
        })
    }
}
impl Command for SetFootprintArtwork {
    const NAME: &'static str = "set_footprint_artwork";
    const DESCRIPTION: &'static str = "Replace mounted-side footprint silkscreen line geometry and optional rectangular courtyard using integer local Nm. Full initial cursor, project and exact Part guards; one undoable Part edit, no-op emits nothing. At most256 segments, positive strokes and stroked extents within +/-10000mm. Explicit empty artwork suppresses legacy synthetic body; null restores it. Pads/nets/pose/links remain unchanged. Native feature13; no silk clipping or courtyard/clearance certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected_project != project.id {
            return Err(CommandError::conflict("project changed; reload artwork before applying"));
        }
        validate_part_artwork(&self.expected)?;
        if self.expected.pads.as_ref().is_some_and(|p| p.0.len() > 1024) {
            return Err(CommandError::invalid("expected", "at most1024 pads"));
        }
        for text in [&self.expected.designator, &self.expected.value, &self.expected.footprint] {
            if text.len() > 4096 {
                return Err(CommandError::invalid("expected", "part labels must not exceed4096 bytes"));
            }
        }
        if let Some(assembly) = &self.expected.assembly {
            assembly.validate().map_err(|e| CommandError::invalid("expected.assembly", e))?;
        }
        if let Some(pads) = &self.expected.pads {
            for pad in &pads.0 {
                if pad.number.len() > 512 || pad.net.as_ref().is_some_and(|n| n.len() > 512) {
                    return Err(CommandError::invalid("expected.pads", "pad labels must not exceed512 bytes"));
                }
            }
        }
        if let Some(artwork) = &self.artwork {
            artwork.validate().map_err(|e| CommandError::invalid("artwork", e))?;
        }
        value_budget(&self).map_err(|e| CommandError::invalid("expected", e))?;
        let current = project.board.part(&self.expected.id).ok_or_else(|| CommandError::not_found("part", self.expected.id))?;
        if current != &self.expected {
            return Err(CommandError::conflict("part changed; reload artwork before applying"));
        }
        if current.artwork == self.artwork {
            return Ok(vec![]);
        }
        let mut after = current.clone();
        after.artwork = self.artwork;
        Ok(vec![Delta::record_part(project, after.id, Some(after))])
    }
}
