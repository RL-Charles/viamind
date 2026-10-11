//! Transaction records: ids, origins and results.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use viamind_doc::ObjectId;

use crate::delta::Delta;

/// Identifier of a transaction, unique and increasing within one engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxId(pub u64);

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tx{}", self.0)
    }
}

/// Who made a change. Recorded on every transaction and change event.
///
/// Serialises as `{"kind":"user"}` or `{"kind":"agent","id":"…"}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// The human, through the desktop UI.
    User,
    /// An agent, e.g. over MCP.
    Agent {
        /// Identifies the agent or its session (e.g. the MCP client name).
        id: String,
    },
}

impl Origin {
    /// Shorthand for [`Origin::Agent`].
    pub fn agent(id: impl Into<String>) -> Origin {
        Origin::Agent { id: id.into() }
    }
}

/// One command call by name, as used in a batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandCall {
    /// Command name, e.g. `"add_part"`.
    pub name: String,
    /// Command input (an object matching the command's input schema).
    #[serde(default)]
    pub input: Value,
}

impl CommandCall {
    /// Creates a call of command `name` with `input`.
    pub fn new(name: impl Into<String>, input: Value) -> CommandCall {
        CommandCall { name: name.into(), input }
    }
}

/// Result of running a command or a transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecResult {
    /// The transaction that was recorded (undo it with `undo`).
    pub tx: TxId,
    /// Net change per object, in the order objects were first touched. Empty if
    /// nothing changed, in which case nothing was added to the undo history.
    pub deltas: Vec<Delta>,
    /// Ids of objects this transaction created (e.g. the new part of `add_part`).
    pub created: Vec<ObjectId>,
}

impl ExecResult {
    pub(crate) fn new(tx: TxId, deltas: Vec<Delta>) -> ExecResult {
        let created = deltas.iter().filter(|delta| delta.is_create()).map(Delta::id).collect();
        ExecResult { tx, deltas, created }
    }
}

/// A committed transaction in the undo or redo history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction id.
    pub id: TxId,
    /// Who made it.
    pub origin: Origin,
    /// Short label for undo menus, e.g. `"add_part"`.
    pub label: String,
    /// Net change per object.
    pub deltas: Vec<Delta>,
}

/// Summary of a transaction for undo/redo menus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxInfo {
    /// Transaction id.
    pub id: TxId,
    /// Who made it.
    pub origin: Origin,
    /// Short label, e.g. `"add_part"`.
    pub label: String,
}

impl From<&Transaction> for TxInfo {
    fn from(tx: &Transaction) -> TxInfo {
        TxInfo { id: tx.id, origin: tx.origin.clone(), label: tx.label.clone() }
    }
}
