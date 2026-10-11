//! Viamind command engine: typed commands, transactions, undo and the event log.
//!
//! Every change to a [`viamind_doc::Project`] is a [`Command`]: a serde +
//! schemars input type whose [`apply`](Command::apply) returns the [`Delta`]s it
//! made. Deltas are full before/after snapshots, so undo is generic
//! ([`invert`]) and the same deltas are the change notifications. A
//! [`CommandRegistry`] lists commands as [`CommandSpec`]s (name, description,
//! JSON Schema) and runs them from JSON, which is how agents call them over MCP.

pub mod command;
pub mod delta;
pub mod registry;

pub use command::{Command, CommandError, CommandSpec, input_schema};
pub use delta::{Delta, apply_all, invert};
pub use registry::{CommandRegistry, EngineError};
/// Re-exported so command input types can derive `JsonSchema` with a matching version.
pub use schemars;
