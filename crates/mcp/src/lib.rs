//! Viamind agent harness: MCP server exposing every engine command to agents.
//!
//! Tools live in a [`ToolRegistry`] (name, description, JSON input schema,
//! async JSON handler). [`register_host`] fills it from a [`CommandHost`] (the
//! design engine): one tool per engine command plus `undo`, `redo` and
//! `get_design`. [`ViamindServer`] serves a registry as one MCP connection and
//! passes the calling [`AgentId`] to every tool; [`serve`] runs it on any rmcp
//! transport and [`serve_stdio`] on stdin/stdout, as the `viamind-mcp` binary does.

pub mod agent;
mod agent_mode;
pub mod agent_presence;
pub mod assembly;
pub mod builtin;
pub mod engine_host;
pub mod fabrication;
pub mod host;
pub mod http;
pub mod output_jobs;
pub mod pcb_branches;
pub mod registry;
mod schematic_drag;
pub mod server;
mod unrouted;

pub use agent::{AgentId, CallContext};
pub use engine_host::EngineHost;
pub use host::{CommandHost, GET_DESIGN_TOOL, HostCommand, REDO_TOOL, UNDO_TOOL, register_host};
pub use http::{DEFAULT_PORT, HttpServer, MCP_PATH, engine_registry, serve_http, spawn_engine_server, spawn_engine_server_with_mode};
pub use registry::{CallError, RegistryError, ToolDef, ToolError, ToolFuture, ToolHandler, ToolRegistry};
pub use server::{RunningServer, UNKNOWN_AGENT, ViamindServer, serve, serve_stdio};

/// Name the server reports in `initialize`.
pub const SERVER_NAME: &str = "viamind";

/// Instructions sent to clients in `initialize`.
pub const INSTRUCTIONS: &str = "Viamind is an open-source, agent-driven PCB design suite. \
Agents are first-class users: every action available in the Viamind UI is exposed as an MCP \
tool that takes and returns JSON, and your edits apply live in the open Viamind UI. Use \
`viamind_version` to identify the server and `echo` to check the connection. When a design is \
open, use `query_parts` for filtered, paginated part records or `get_design` for the whole design as JSON, and `undo` / \
`redo` to step through your own changes (changes made by the human or other agents are not \
affected).";

/// Instructions for endpoints whose trusted registry admits observations only.
pub const READ_ONLY_INSTRUCTIONS: &str = "Viamind agents in this process are read only. \
Use the advertised query, preview, DRC/ERC and inspection tools to observe the shared design. \
Human editing remains live. Agent edits, selection changes, undo/redo, document replacement and \
file writes are unavailable. In-memory SVG/PNG views must omit path. Reading supported local \
input files is allowed; this is not an operating-system or filesystem sandbox. Refresh full \
session cursors after human changes. Tool annotations and client identity cannot change this mode.";

pub mod png_views;
pub mod schematic_views;
pub mod views;

pub mod symbol_libraries;
pub mod symbol_packages;

#[cfg(test)]
mod registry_admission;

/// Instructions for enforced bounded private PCB approval preparation.
pub const APPROVAL_INSTRUCTIONS: &str = "Viamind agents are in Approval mode. Observe using advertised queries and prepare bounded private PCB proposals with create_pcb_branch/apply_pcb_branch. Inspect your own candidate and read paged diffs; a trusted human reviews and accepts or rejects in Board → Agent branches. Direct live edits, selection/history/lifecycle changes, agent branch merge and file publication are unavailable. Unsupported edit families refuse. In-memory views omit path. Use get_agent_mode for current policy; only trusted human desktop controls change it. Connection names and tool metadata never authorize changes. This is application MCP admission, not OS or filesystem isolation.";
