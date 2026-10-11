//! Stable object identifiers.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier of a model object (project, sheet, part, ...).
///
/// A random (version 4) UUID, assigned once when the object is created and kept
/// for its whole life, across moves, edits, undo and save/load. Serialises as the
/// hyphenated UUID string, e.g. `"67e55044-10b1-426f-9247-bb680e5fe0c8"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ObjectId(pub Uuid);

impl ObjectId {
    /// Creates a fresh, random id.
    pub fn new() -> ObjectId {
        ObjectId(Uuid::new_v4())
    }

    /// Wraps an existing UUID, e.g. one read from a file.
    pub const fn from_uuid(uuid: Uuid) -> ObjectId {
        ObjectId(uuid)
    }

    /// The underlying UUID.
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

/// Same as [`ObjectId::new`]: a fresh, random id (never the nil UUID).
impl Default for ObjectId {
    fn default() -> ObjectId {
        ObjectId::new()
    }
}

/// Formats as the hyphenated lowercase UUID, the same text serde produces.
impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Parses any UUID text form accepted by [`Uuid::parse_str`].
impl FromStr for ObjectId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<ObjectId, uuid::Error> {
        Uuid::parse_str(s).map(ObjectId)
    }
}

impl From<Uuid> for ObjectId {
    fn from(uuid: Uuid) -> ObjectId {
        ObjectId(uuid)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn new_ids_are_unique_v4() {
        let ids: BTreeSet<ObjectId> = (0..1000).map(|_| ObjectId::new()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| id.as_uuid().get_version_num() == 4));
    }

    #[test]
    fn display_parse_and_serde_agree() {
        let id = ObjectId::new();
        let text = id.to_string();
        assert_eq!(text.len(), 36);
        assert_eq!(text.parse::<ObjectId>().unwrap(), id);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
        assert_eq!(serde_json::from_str::<ObjectId>(&format!("\"{text}\"")).unwrap(), id);
        assert!("not-a-uuid".parse::<ObjectId>().is_err());
    }

    #[test]
    fn schema_is_a_uuid_string() {
        let schema = schemars::schema_for!(ObjectId);
        assert_eq!(schema.get("type").and_then(|t| t.as_str()), Some("string"));
        assert_eq!(schema.get("format").and_then(|t| t.as_str()), Some("uuid"));
    }
}
