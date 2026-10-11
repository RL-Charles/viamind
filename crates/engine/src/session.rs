//! Bounded, resumable session history independent of undo and document persistence.

use std::collections::VecDeque;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project};

use crate::{ChangeEvent, SelectionSnapshot};

/// Number of session records retained by each engine.
pub const SESSION_EVENT_CAPACITY: usize = 1024;

/// Last consumed event in one engine session. Reusable by any connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventCursor {
    /// Random engine session ID; changes on restart.
    pub stream_id: ObjectId,
    /// Last consumed sequence, zero before the first event.
    pub sequence: u64,
}

/// Session changes available to observers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SessionEvent {
    /// A committed, undone or redone document transaction.
    Design(ChangeEvent),
    /// Complete new shared selection.
    Selection(SelectionSnapshot),
}

/// A change in session order, separate from transaction IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionEventRecord {
    /// Strictly increasing session sequence.
    pub sequence: u64,
    /// Change payload.
    pub event: SessionEvent,
}

/// Input for bounded long polling. Unknown fields are rejected.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchEvents {
    /// Resume after this cursor; omitted/null starts at the current tail.
    #[serde(default)]
    pub after: Option<EventCursor>,
    /// Page size: 1–1000, defaults to 100.
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: usize,
    /// Wait at most 0–30000 milliseconds when no events are available. Defaults to 0.
    #[serde(default)]
    #[schemars(range(min = 0, max = 30000))]
    pub timeout_ms: u64,
}

fn default_limit() -> usize {
    100
}

impl Default for WatchEvents {
    fn default() -> Self {
        Self { after: None, limit: default_limit(), timeout_ms: 0 }
    }
}

impl WatchEvents {
    pub(crate) fn validate(&self) -> Result<(), SessionError> {
        if !(1..=1000).contains(&self.limit) {
            return Err(SessionError::InvalidInput("limit must be between 1 and 1000"));
        }
        if self.timeout_ms > 30000 {
            return Err(SessionError::InvalidInput("timeout_ms must be between 0 and 30000"));
        }
        Ok(())
    }
}

/// One ordered page, with the cursor for the next watch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventBatch {
    /// Records strictly after the requested cursor.
    pub events: Vec<SessionEventRecord>,
    /// Last returned record, or the requested cursor on timeout.
    pub cursor: EventCursor,
    /// More records were available when this page was read.
    pub has_more: bool,
}

/// An atomic baseline for initializing or resynchronizing an observer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionSnapshot {
    /// Current project.
    pub project: Project,
    /// Current transient selection.
    pub selection: SelectionSnapshot,
    /// Changes committed after this snapshot follow this cursor.
    pub cursor: EventCursor,
}

/// Why watching cannot continue.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// Invalid polling bounds.
    #[error("invalid watch_events input: {0}")]
    InvalidInput(&'static str),
    /// A different engine instance issued this cursor.
    #[error("event cursor belongs to another session (server may have restarted); call get_session_state to resynchronize")]
    WrongSession,
    /// Cursor is ahead of the journal.
    #[error("event cursor is in the future; call get_session_state to resynchronize")]
    FutureCursor,
    /// Records needed for this cursor have been evicted.
    #[error("event cursor has expired; call get_session_state to resynchronize")]
    ExpiredCursor,
}

#[derive(Debug)]
pub(crate) struct SessionJournal {
    cursor: EventCursor,
    events: VecDeque<SessionEventRecord>,
}

impl Default for SessionJournal {
    fn default() -> Self {
        Self { cursor: EventCursor { stream_id: ObjectId::new(), sequence: 0 }, events: VecDeque::new() }
    }
}

impl SessionJournal {
    pub(crate) fn cursor(&self) -> EventCursor {
        self.cursor
    }

    pub(crate) fn push(&mut self, event: SessionEvent) {
        self.cursor.sequence += 1;
        self.events.push_back(SessionEventRecord { sequence: self.cursor.sequence, event });
        if self.events.len() > SESSION_EVENT_CAPACITY {
            self.events.pop_front();
        }
    }

    pub(crate) fn read(&self, after: EventCursor, limit: usize) -> Result<EventBatch, SessionError> {
        if after.stream_id != self.cursor.stream_id {
            return Err(SessionError::WrongSession);
        }
        if after.sequence > self.cursor.sequence {
            return Err(SessionError::FutureCursor);
        }
        if self.events.front().is_some_and(|first| after.sequence < first.sequence - 1) {
            return Err(SessionError::ExpiredCursor);
        }
        let events: Vec<_> = self.events.iter().filter(|record| record.sequence > after.sequence).take(limit).cloned().collect();
        let cursor = EventCursor { sequence: events.last().map_or(after.sequence, |record| record.sequence), ..after };
        Ok(EventBatch { events, cursor, has_more: cursor.sequence < self.cursor.sequence })
    }
}
