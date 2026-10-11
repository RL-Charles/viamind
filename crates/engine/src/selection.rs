//! Transient selection shared by the desktop and agents.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::ObjectId;

use crate::Origin;

/// How supplied IDs modify the current selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[default]
    Replace,
    Add,
    Remove,
    Toggle,
}

/// Input shared by UI selection actions and the MCP tool.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSelection {
    /// IDs of existing board parts, straight tracks or through vias. Empty with replace clears selection.
    pub ids: Vec<ObjectId>,
    /// Defaults to replace; add, remove and toggle are also supported.
    #[serde(default)]
    pub mode: SelectionMode,
}

/// A selection snapshot, independent of document history and serialization.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SelectionSnapshot {
    /// Selected IDs in stable sorted order.
    pub ids: Vec<ObjectId>,
    /// Increases whenever the set changes, including removal of deleted objects.
    pub revision: u64,
    /// Who last changed the selection.
    pub origin: Option<Origin>,
}
