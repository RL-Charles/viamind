//! Viamind command engine: typed commands, transactions, undo and the event log.
//!
//! Every change to a [`viamind_doc::Project`] is a [`Command`]: a serde +
//! schemars input type whose [`apply`](Command::apply) returns the [`Delta`]s it
//! made. Deltas are full before/after snapshots, so undo is generic
//! ([`invert`]) and the same deltas are the change notifications. A
//! [`CommandRegistry`] lists commands as [`CommandSpec`]s (name, description,
//! JSON Schema) and runs them from JSON, which is how agents call them over MCP.
//!
//! An [`Engine`] owns the project and runs every edit as a transaction with
//! per-transaction undo/redo; an [`EngineHandle`] shares one engine between the
//! UI thread and the MCP server.
//!
//! ```
//! use serde_json::json;
//! use viamind_doc::Project;
//! use viamind_engine::{EngineHandle, Origin};
//!
//! let engine = EngineHandle::new(Project::new("demo"));
//! let input = json!({ "designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 10.0, "y_mm": 5.0 });
//! let result = engine.execute("add_part", input, Origin::agent("claude")).unwrap();
//! let id = result.created[0];
//! assert_eq!(engine.read(|p| p.board.part(&id).unwrap().designator.clone()), "R1");
//!
//! engine.undo(Origin::User).unwrap(); // the user undoes the agent's edit
//! assert!(engine.read(|p| p.board.parts.is_empty()));
//! ```

pub mod command;
pub mod commands;
pub mod connectivity;
pub mod delta;
pub mod document;
pub mod engine;
pub mod event;
pub mod handle;
pub mod log;
pub mod pads;
pub mod query;
pub mod registry;
pub mod selection;
pub mod session;
pub mod transaction;

pub use command::{Command, CommandError, CommandSpec, input_schema};
pub use commands::{AddPart, DeletePart, ExpectedOutline, ImportBoardOutlineDxf, MovePart, OutlineMm, OutlinePoint, SetBoardOutline, UpdatePart};
pub use connectivity::{ConnectionGuide, ConnectivityQuery, ConnectivityResult, NetQuery, NetQueryResult, NetSummary, Terminal, connection_guides};
pub use delta::{Delta, apply_all, coalesce, invert};
pub use document::{DocumentStatus, NewProject, OpenProject, ReplaceGuard, SaveProject, SaveResult};
pub use engine::{Engine, Tx};
pub use event::{ChangeEvent, ChangeKind, Waker};
pub use handle::EngineHandle;
pub use log::{EventLog, LogError};
pub use pads::{PadInput, SetPartPads};
pub use query::{PartQuery, PartQueryBounds, PartQueryResult};
pub use registry::{CommandRegistry, EngineError};
/// Re-exported so command input types can derive `JsonSchema` with a matching version.
pub use schemars;
pub use selection::{SelectionMode, SelectionSnapshot, SetSelection};
pub use session::{EventBatch, EventCursor, SESSION_EVENT_CAPACITY, SessionError, SessionEvent, SessionEventRecord, SessionSnapshot, WatchEvents};
pub use transaction::{CommandCall, ExecResult, Origin, Transaction, TxId, TxInfo};

pub mod copper;
pub use copper::{AddTrack, AddVia, DeleteTrack, DeleteVia};
