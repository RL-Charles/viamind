//! Append-only event log: the base project plus every change event, replayable.
//!
//! The engine appends each [`ChangeEvent`] it sends (applied, undone, redone), so
//! replaying the log onto its base rebuilds the current project exactly. On disk
//! it is JSON Lines: a header line `{"format":"viamind-log","version":1,"base":{…}}`
//! followed by one compact [`ChangeEvent`] per line, so it can be appended to as
//! events arrive ([`write_entry`]).

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use viamind_doc::Project;

use crate::delta::apply_all;
use crate::event::ChangeEvent;

/// Value of the header's `format` field.
pub const LOG_FORMAT: &str = "viamind-log";
/// New logs use version 4 to preserve independent copper in appended snapshots.
pub const LOG_VERSION: u32 = 4;

/// Why a log could not be read.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// Reading failed.
    #[error("reading the event log failed: {0}")]
    Io(#[from] io::Error),
    /// A line is not valid JSON or not the expected shape.
    #[error("event log line {line}: {source}")]
    Parse {
        /// One-based line number.
        line: usize,
        /// The JSON error.
        source: serde_json::Error,
    },
    /// The header is missing or not a Viamind event log of a supported version.
    #[error("not a supported Viamind event log (expected format \"viamind-log\" version ≤ {LOG_VERSION})")]
    BadHeader,
}

#[derive(Serialize, Deserialize)]
struct Header<P> {
    format: String,
    version: u32,
    base: P,
}

/// The project an engine started from plus every change event since, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventLog {
    base: Project,
    entries: Vec<ChangeEvent>,
}

impl EventLog {
    /// An empty log starting from `base`.
    pub fn new(base: Project) -> EventLog {
        EventLog { base, entries: Vec::new() }
    }

    /// The project the log starts from.
    pub fn base(&self) -> &Project {
        &self.base
    }

    /// Every logged event, oldest first.
    pub fn entries(&self) -> &[ChangeEvent] {
        &self.entries
    }

    /// Number of logged events.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no events are logged.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Appends one event.
    pub fn append(&mut self, event: ChangeEvent) {
        self.entries.push(event);
    }

    /// Rebuilds the project by applying every event's deltas to the base.
    pub fn replay(&self) -> Project {
        self.replay_until(self.entries.len())
    }

    /// The project after the first `count` events (clamped to the log length).
    pub fn replay_until(&self, count: usize) -> Project {
        let mut project = self.base.clone();
        for event in self.entries.iter().take(count) {
            apply_all(&mut project, &event.deltas);
        }
        project
    }

    /// Writes the whole log as JSON Lines.
    pub fn write_jsonl(&self, mut out: impl Write) -> io::Result<()> {
        write_header(&mut out, &self.base)?;
        for event in &self.entries {
            write_entry(&mut out, event)?;
        }
        Ok(())
    }

    /// Reads a log written by [`write_jsonl`](EventLog::write_jsonl) (or by
    /// [`write_header`] followed by [`write_entry`] calls). Blank lines are ignored.
    pub fn read_jsonl(input: impl BufRead) -> Result<EventLog, LogError> {
        let mut lines = input.lines().enumerate().filter(|(_, line)| line.as_ref().map_or(true, |l| !l.trim().is_empty()));
        let (index, header) = lines.next().ok_or(LogError::BadHeader)?;
        let header: Header<Project> = serde_json::from_str(&header?).map_err(|source| LogError::Parse { line: index + 1, source })?;
        if header.format != LOG_FORMAT
            || header.version > LOG_VERSION
            || (header.version < viamind_doc::file::minimum_version(&header.base))
            || viamind_doc::file::validate_pads(&header.base).is_err()
            || viamind_doc::file::validate_copper(&header.base).is_err()
        {
            return Err(LogError::BadHeader);
        }
        let mut log = EventLog::new(header.base);
        for (index, line) in lines {
            let event: ChangeEvent = serde_json::from_str(&line?).map_err(|source| LogError::Parse { line: index + 1, source })?;
            if header.version < 3 && event.deltas.iter().any(|d| matches!(d, crate::Delta::Part { before, after, .. } if before.as_ref().is_some_and(|p| p.pads.is_some()) || after.as_ref().is_some_and(|p| p.pads.is_some()))) {
                return Err(LogError::BadHeader);
            }
            if header.version < 4 && event.deltas.iter().any(|d| matches!(d, crate::Delta::Track { .. } | crate::Delta::Via { .. })) {
                return Err(LogError::BadHeader);
            }
            for delta in &event.deltas {
                let valid_id = match delta {
                    crate::Delta::Track { id, before, after } => before.iter().chain(after).all(|v| v.id == *id),
                    crate::Delta::Via { id, before, after } => before.iter().chain(after).all(|v| v.id == *id),
                    _ => true,
                };
                if !valid_id {
                    return Err(LogError::BadHeader);
                }
            }
            log.append(event);
        }
        if viamind_doc::file::validate_copper(&log.replay()).is_err() {
            return Err(LogError::BadHeader);
        }
        Ok(log)
    }
}

/// Writes the header line of a log starting from `base`.
pub fn write_header(mut out: impl Write, base: &Project) -> io::Result<()> {
    serde_json::to_writer(&mut out, &Header { format: LOG_FORMAT.to_owned(), version: LOG_VERSION, base })?;
    out.write_all(b"\n")
}

/// Appends one event as a single line.
pub fn write_entry(mut out: impl Write, event: &ChangeEvent) -> io::Result<()> {
    serde_json::to_writer(&mut out, event)?;
    out.write_all(b"\n")
}
