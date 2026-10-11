//! Viamind agent harness: MCP server exposing every engine command to agents.
//!
//! Tools live in a [`ToolRegistry`] (name, description, JSON input schema,
//! async JSON handler). [`ViamindServer`] serves a registry over MCP;
//! [`serve_stdio`] runs it on stdin/stdout, as the `viamind-mcp` binary does.

pub mod builtin;
pub mod registry;
pub mod server;

pub use registry::{CallError, RegistryError, ToolDef, ToolError, ToolFuture, ToolHandler, ToolRegistry};
pub use server::{ViamindServer, serve_stdio};

/// Name the server reports in `initialize`.
pub const SERVER_NAME: &str = "viamind";

/// Instructions sent to clients in `initialize`.
pub const INSTRUCTIONS: &str = "Viamind is an open-source, agent-driven PCB design suite. \
Agents are first-class users: every action available in the Viamind UI is exposed as an MCP \
tool that takes and returns JSON. Use `viamind_version` to identify the server and `echo` to \
check the connection. Design-editing tools generated from Viamind engine commands will appear \
in tools/list as they are added.";
