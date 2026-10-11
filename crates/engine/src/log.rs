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
/// New logs use version 8 to preserve guarded named-net policy deltas.
pub const LOG_VERSION: u32 = 8;

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
            || viamind_doc::file::validate_net_rules(&header.base).is_err()
            || viamind_doc::file::validate_assembly(&header.base).is_err()
            || viamind_doc::file::validate_pads(&header.base).is_err()
            || viamind_doc::file::validate_copper(&header.base).is_err()
            || viamind_doc::schematic::validate_project(&header.base, None).is_err()
        {
            return Err(LogError::BadHeader);
        }
        let mut budget =
            MetadataBudget::new(header.base.board.parts.values().filter_map(|p| p.assembly.as_deref())).map_err(|_| LogError::BadHeader)?;
        let mut current = header.base.clone();
        let mut log = EventLog::new(header.base);
        for (index, line) in lines {
            let event: ChangeEvent = serde_json::from_str(&line?).map_err(|source| LogError::Parse { line: index + 1, source })?;
            validate_event_assembly(&event).map_err(|_| LogError::BadHeader)?;
            if header.version < 3 && event.deltas.iter().any(|d| matches!(d, crate::Delta::Part { before, after, .. } if before.as_ref().is_some_and(|p| p.pads.is_some()) || after.as_ref().is_some_and(|p| p.pads.is_some()))) {
                return Err(LogError::BadHeader);
            }
            if header.version < 4 && event.deltas.iter().any(|d| matches!(d, crate::Delta::Track { .. } | crate::Delta::Via { .. })) {
                return Err(LogError::BadHeader);
            }
            for delta in &event.deltas {
                let valid_id = match delta {
                    crate::Delta::Part { id, before, after } => before.iter().chain(after).all(|part| {
                        part.id == *id
                            && (header.version >= 6 || part.schematic_symbol.is_none())
                            && (header.version >= 7 || part.assembly.is_none())
                            && part.assembly.as_ref().is_none_or(|m| m.validate().is_ok())
                    }),
                    crate::Delta::NamedNetRules { id, before, after } => {
                        header.version >= 8
                            && *id == current.id
                            && viamind_doc::named_net_rule_budget(before).is_ok()
                            && viamind_doc::named_net_rule_budget(after).is_ok()
                    }
                    crate::Delta::Track { id, before, after } => before.iter().chain(after).all(|v| v.id == *id),
                    crate::Delta::Via { id, before, after } => before.iter().chain(after).all(|v| v.id == *id),
                    crate::Delta::Sheet { id, before, after } => before.iter().chain(after).all(|sheet| {
                        sheet.id == *id && viamind_doc::schematic::validate_sheet(sheet).is_ok() && (header.version >= 5 || sheet.content.is_none())
                    }),
                    _ => true,
                };
                if !valid_id {
                    return Err(LogError::BadHeader);
                }
            }
            for delta in &event.deltas {
                if let crate::Delta::Part { id, before, after } = delta {
                    let before = before.as_ref().and_then(|p| p.assembly.as_deref());
                    if current.board.part(id).and_then(|p| p.assembly.as_deref()) != before {
                        return Err(LogError::BadHeader);
                    }
                    let after = after.as_ref().and_then(|p| p.assembly.as_deref());
                    budget.replace(before, after).map_err(|_| LogError::BadHeader)?;
                }
                if let crate::Delta::NamedNetRules { before, .. } = delta {
                    if *before != current.board.net_rules {
                        return Err(LogError::BadHeader);
                    }
                }
                delta.apply_to(&mut current);
            }
            log.append(event);
        }
        if viamind_doc::file::validate_copper(&log.replay()).is_err() {
            return Err(LogError::BadHeader);
        }
        if viamind_doc::schematic::validate_project(&log.replay(), None).is_err() {
            return Err(LogError::BadHeader);
        }
        Ok(log)
    }
}

/// Writes the header line of a log starting from `base`.
pub fn write_header(mut out: impl Write, base: &Project) -> io::Result<()> {
    viamind_doc::file::validate_net_rules(base).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    viamind_doc::file::validate_assembly(base).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    serde_json::to_writer(&mut out, &Header { format: LOG_FORMAT.to_owned(), version: LOG_VERSION, base })?;
    out.write_all(b"\n")
}

/// Appends one event as a single line.
pub fn write_entry(mut out: impl Write, event: &ChangeEvent) -> io::Result<()> {
    validate_event_assembly(event).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    serde_json::to_writer(&mut out, event)?;
    out.write_all(b"\n")
}

/// Reconstruct the touched portion's initial sourcing state without cloning strings.
/// The first before record supplies each ID; later records update it sequentially.
fn validate_event_assembly(event: &ChangeEvent) -> Result<(), &'static str> {
    for delta in &event.deltas {
        if let crate::Delta::NamedNetRules { before, after, .. } = delta {
            viamind_doc::named_net_rule_budget(before)?;
            viamind_doc::named_net_rule_budget(after)?;
        }
    }
    let mut state = std::collections::BTreeMap::new();
    for delta in &event.deltas {
        if let crate::Delta::Part { id, before, after } = delta {
            for p in before.iter().chain(after) {
                if let Some(m) = &p.assembly {
                    m.validate()?;
                }
            }
            state.entry(*id).or_insert_with(|| before.as_ref().and_then(|p| p.assembly.as_deref()));
        }
    }
    let mut budget = MetadataBudget::new(state.values().filter_map(|m| *m))?;
    for delta in &event.deltas {
        if let crate::Delta::Part { id, before, after } = delta {
            if state.get(id).copied().flatten() != before.as_ref().and_then(|p| p.assembly.as_deref()) {
                return Err("inconsistent assembly metadata in sequential event snapshots");
            }
            let before = before.as_ref().and_then(|p| p.assembly.as_deref());
            let after = after.as_ref().and_then(|p| p.assembly.as_deref());
            budget.replace(before, after)?;
            state.insert(*id, after);
        }
    }
    Ok(())
}

/// Each sequential transition checks only its borrowed records, not the entire board.
#[derive(Clone, Copy, Default)]
struct MetadataBudget {
    components: usize,
    text: usize,
}
impl MetadataBudget {
    fn new<'a>(records: impl IntoIterator<Item = &'a viamind_doc::AssemblyMetadata>) -> Result<Self, &'static str> {
        let mut budget = Self::default();
        for record in records {
            budget.replace(None, Some(record))?;
        }
        Ok(budget)
    }
    fn replace(&mut self, before: Option<&viamind_doc::AssemblyMetadata>, after: Option<&viamind_doc::AssemblyMetadata>) -> Result<(), &'static str> {
        let mut candidate = *self;
        if let Some(before) = before {
            candidate.components = candidate.components.checked_sub(1).ok_or("inconsistent assembly component budget")?;
            candidate.text = candidate.text.checked_sub(before.strings().map(str::len).sum()).ok_or("inconsistent assembly text budget")?;
        }
        if let Some(after) = after {
            after.validate()?;
            candidate.components = candidate.components.saturating_add(1);
            candidate.text = candidate.text.saturating_add(after.strings().map(str::len).sum());
        }
        if candidate.components > viamind_doc::assembly::MAX_SOURCING_COMPONENTS {
            return Err("at most 100000 components may carry sourcing metadata");
        }
        if candidate.text > viamind_doc::assembly::MAX_SOURCING_TEXT {
            return Err("sourcing metadata exceeds 2 MiB aggregate UTF-8 text");
        }
        *self = candidate;
        Ok(())
    }
}
