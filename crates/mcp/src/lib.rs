//! Viamind agent harness: MCP server exposing every engine command to agents.
//!
//! Tools live in a [`ToolRegistry`] (name, description, JSON input schema,
//! async JSON handler). [`register_host`] fills it from a [`CommandHost`] (the
//! design engine): one tool per engine command plus `undo`, `redo` and
//! `get_design`. [`ViamindServer`] serves a registry as one MCP connection and
//! passes the calling [`AgentId`] to every tool; [`serve`] runs it on any rmcp
//! transport and [`serve_stdio`] on stdin/stdout, as the `viamind-mcp` binary does.

pub mod agent;
pub mod builtin;
pub mod engine_host;
pub mod host;
pub mod http;
pub mod registry;
pub mod server;

pub use agent::{AgentId, CallContext};
pub use engine_host::EngineHost;
pub use host::{CommandHost, GET_DESIGN_TOOL, HostCommand, REDO_TOOL, UNDO_TOOL, register_host};
pub use http::{DEFAULT_PORT, HttpServer, MCP_PATH, engine_registry, serve_http, spawn_engine_server};
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
