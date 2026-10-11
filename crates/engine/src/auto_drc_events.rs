//! Small independent history of automatic DRC state transitions.
use std::collections::VecDeque;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::ObjectId;

use crate::{AutoDrcPhase, AutoDrcStatus, EventCursor};

/// Maximum retained summaries; detailed reports exist only in current status.
pub const AUTO_DRC_EVENT_CAPACITY: usize = 128;

/// Position in one automatic DRC controller, independent of document cursors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutoDrcEventCursor {
    pub stream_id: ObjectId,
    pub sequence: u64,
}

/// Exact scalar facts from a scoped report, without its violation arrays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AutoDrcReportSummary {
    pub violation_count: usize,
    pub truncated: bool,
    pub has_incomplete_reason: bool,
    pub excluded_part_count: usize,
    pub explicit_outline_checked: bool,
}

/// One observed controller transition. Superseded work has no terminal record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AutoDrcEvent {
    pub sequence: u64,
    pub generation: u64,
    pub phase: AutoDrcPhase,
    pub source_cursor: Option<EventCursor>,
    pub report: Option<AutoDrcReportSummary>,
    /// At most 512 UTF-8 bytes, truncated on a character boundary.
    pub message: Option<String>,
}

/// Current complete status and its atomic event baseline.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AutoDrcEventState {
    pub status: AutoDrcStatus,
    pub cursor: AutoDrcEventCursor,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchAutoDrcEvents {
    #[serde(default)]
    pub after: Option<AutoDrcEventCursor>,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 128))]
    pub limit: usize,
    #[serde(default)]
    #[schemars(range(min = 0, max = 30000))]
    pub timeout_ms: u64,
}
fn default_limit() -> usize {
    100
}
impl Default for WatchAutoDrcEvents {
    fn default() -> Self {
        Self { after: None, limit: default_limit(), timeout_ms: 0 }
    }
}
impl WatchAutoDrcEvents {
    pub(crate) fn validate(&self) -> Result<(), AutoDrcEventError> {
        if !(1..=AUTO_DRC_EVENT_CAPACITY).contains(&self.limit) {
            return Err(AutoDrcEventError::InvalidInput("limit must be between 1 and 128"));
        }
        if self.timeout_ms > 30_000 {
            return Err(AutoDrcEventError::InvalidInput("timeout_ms must be between 0 and 30000"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AutoDrcEventBatch {
    pub events: Vec<AutoDrcEvent>,
    pub cursor: AutoDrcEventCursor,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AutoDrcEventError {
    #[error("invalid watch_auto_drc_events input: {0}")]
    InvalidInput(&'static str),
    #[error("DRC event cursor belongs to another controller; call get_auto_drc_event_state to resynchronize")]
    WrongStream,
    #[error("DRC event cursor is in the future; call get_auto_drc_event_state to resynchronize")]
    FutureCursor,
    #[error("DRC event cursor has expired; call get_auto_drc_event_state to resynchronize")]
    ExpiredCursor,
}

#[derive(Debug)]
pub(crate) struct AutoDrcJournal {
    cursor: AutoDrcEventCursor,
    events: VecDeque<AutoDrcEvent>,
}
impl Default for AutoDrcJournal {
    fn default() -> Self {
        Self { cursor: AutoDrcEventCursor { stream_id: ObjectId::new(), sequence: 0 }, events: VecDeque::new() }
    }
}
impl AutoDrcJournal {
    pub(crate) fn cursor(&self) -> AutoDrcEventCursor {
        self.cursor
    }
    pub(crate) fn push(&mut self, status: &AutoDrcStatus) {
        self.cursor.sequence = self.cursor.sequence.checked_add(1).expect("DRC event sequence exhausted");
        if self.events.len() == AUTO_DRC_EVENT_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(AutoDrcEvent {
            sequence: self.cursor.sequence,
            generation: status.generation,
            phase: status.phase,
            source_cursor: status.source_cursor,
            report: status.report.as_ref().map(|report| AutoDrcReportSummary {
                violation_count: report.violations.len(),
                truncated: report.truncated,
                has_incomplete_reason: report.incomplete_reason.is_some(),
                excluded_part_count: report.coverage.excluded_part_count,
                explicit_outline_checked: report.coverage.explicit_outline_checked,
            }),
            message: status.message.as_ref().map(|message| {
                let mut end = message.len().min(512);
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message[..end].to_owned()
            }),
        });
    }
    pub(crate) fn read(&self, after: AutoDrcEventCursor, limit: usize) -> Result<AutoDrcEventBatch, AutoDrcEventError> {
        if after.stream_id != self.cursor.stream_id {
            return Err(AutoDrcEventError::WrongStream);
        }
        if after.sequence > self.cursor.sequence {
            return Err(AutoDrcEventError::FutureCursor);
        }
        if self.events.front().is_some_and(|first| after.sequence < first.sequence - 1) {
            return Err(AutoDrcEventError::ExpiredCursor);
        }
        let events: Vec<_> = self.events.iter().filter(|event| event.sequence > after.sequence).take(limit).cloned().collect();
        let cursor = AutoDrcEventCursor { sequence: events.last().map_or(after.sequence, |event| event.sequence), ..after };
        Ok(AutoDrcEventBatch { events, cursor, has_more: cursor.sequence < self.cursor.sequence })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_boundary_paging_and_utf8_messages_are_exact() {
        let mut journal = AutoDrcJournal::default();
        let initial = journal.cursor();
        let status = AutoDrcStatus { message: Some("€".repeat(200)), ..Default::default() };
        for _ in 0..128 {
            journal.push(&status);
        }
        let page = journal.read(initial, 127).unwrap();
        assert_eq!(page.events.len(), 127);
        assert!(page.has_more);
        assert_eq!(page.cursor.sequence, 127);
        assert_eq!(page.events[0].message.as_ref().unwrap().len(), 510);
        let last = journal.read(page.cursor, 128).unwrap();
        assert_eq!(last.events.len(), 1);
        assert!(!last.has_more);
        journal.push(&status);
        assert_eq!(journal.read(initial, 128).unwrap_err(), AutoDrcEventError::ExpiredCursor);
        assert_eq!(journal.events.len(), 128);
        let floor = AutoDrcEventCursor { sequence: 1, ..initial };
        assert_eq!(journal.read(floor, 128).unwrap().events.len(), 128);
        assert_eq!(journal.read(AutoDrcEventCursor { sequence: 130, ..initial }, 1).unwrap_err(), AutoDrcEventError::FutureCursor);
        assert_eq!(journal.read(AutoDrcEventCursor { stream_id: ObjectId::new(), ..initial }, 1).unwrap_err(), AutoDrcEventError::WrongStream);
    }
}
