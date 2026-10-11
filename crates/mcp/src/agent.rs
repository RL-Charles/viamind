//! Who is calling a tool.

use std::fmt;

/// Identity of the agent behind a tool call.
///
/// [`crate::ViamindServer`] derives it per connection as
/// `<clientInfo.name>#<connection number>`, e.g. `claude-code#3`: the name the
/// MCP client reported (`unknown-agent` if it sent none) plus a number unique
/// to the connection within this process, so two sessions of the same client
/// are still told apart (for per-agent undo, attribution in the event log, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(pub String);

impl AgentId {
    /// The identity used when a tool is called outside any MCP connection,
    /// e.g. directly through [`crate::ToolRegistry::call`] in tests.
    pub const LOCAL: &str = "local";

    /// The id as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self(Self::LOCAL.to_owned())
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for AgentId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl From<String> for AgentId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

/// Per-call context passed to tool handlers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallContext {
    /// The agent making the call.
    pub agent: AgentId,
}

impl CallContext {
    /// Context for a call made by `agent`.
    pub fn new(agent: impl Into<AgentId>) -> Self {
        Self { agent: agent.into() }
    }
}
