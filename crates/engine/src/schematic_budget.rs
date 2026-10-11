//! Shared preflight bounds for selected-sheet snapshots and inherited global validation.
use crate::CommandError;
use viamind_doc::{Project, Sheet};
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_budget", message)
}
pub(crate) fn sheet(sheet: &Sheet, purpose: &str) -> Result<usize, CommandError> {
    let c = sheet.content.as_ref();
    if c.is_some_and(|c| c.object_count() > 10_000) {
        return Err(invalid(format!("{purpose} supports at most 10000 sheet objects")));
    }
    let mut pins = 0usize;
    let mut text = 0usize;
    let mut add = |s: &str| -> Result<(), CommandError> {
        text = text.saturating_add(s.len());
        if s.len() > 4096 || text > 2 * 1024 * 1024 {
            return Err(invalid(format!("{purpose} text exceeds 4096 bytes per string or 2 MiB per sheet")));
        }
        Ok(())
    };
    add(&sheet.name)?;
    if let Some(c) = c {
        for s in c.symbols.values() {
            pins = pins.saturating_add(s.pins.len());
            if pins > 10_000 {
                return Err(invalid(format!("{purpose} supports at most 10000 pins per sheet")));
            }
            for value in [s.designator.as_str(), &s.value, s.footprint.as_deref().unwrap_or("")] {
                add(value)?;
            }
            for p in &s.pins {
                add(&p.number)?;
                add(&p.name)?;
            }
        }
        for l in c.labels.values() {
            add(&l.name)?;
        }
    }
    if sheet.id.as_uuid().is_nil() {
        return Err(invalid("sheet identity must be nonnil"));
    }
    viamind_doc::schematic::validate_sheet(sheet).map_err(invalid)?;
    Ok(text)
}
// The inherited global identity/reference validator allocates a global set. Bound
// all data it visits before calling it; no board strings or geometry are copied.
pub(crate) fn project(project: &Project, added: usize, purpose: &str) -> Result<usize, CommandError> {
    let mut count = 1usize
        .saturating_add(added)
        .saturating_add(project.board.parts.len())
        .saturating_add(project.board.tracks.len())
        .saturating_add(project.board.vias.len())
        .saturating_add(project.schematic.sheets.len());
    let mut more = |n: usize| {
        count = count.saturating_add(n);
        if count > 100_000 {
            Err(invalid(format!(
                "{purpose} supports at most 100000 combined project/sheet/part/pad/copper/schematic/pin identities including additions"
            )))
        } else {
            Ok(())
        }
    };
    more(0)?;
    if project.schematic.sheets.len() > 1000 {
        return Err(invalid(format!("{purpose} supports at most 1000 sheets")));
    }
    for p in project.board.parts.values() {
        more(p.pads.as_ref().map_or(0, |p| p.0.len()))?;
    }
    let mut bytes = 0usize;
    let mut text = |s: &str| {
        bytes = bytes.saturating_add(s.len());
        if s.len() > 4096 || bytes > 4 * 1024 * 1024 {
            Err(invalid(format!("{purpose} text exceeds 4096 bytes per string or 4 MiB schematic text")))
        } else {
            Ok(())
        }
    };
    for sheet in project.schematic.sheets.values() {
        text(&sheet.name)?;
        if let Some(c) = &sheet.content {
            more(c.object_count())?;
            for s in c.symbols.values() {
                more(s.pins.len())?;
                for value in [s.designator.as_str(), &s.value, s.footprint.as_deref().unwrap_or("")] {
                    text(value)?;
                }
                for pin in &s.pins {
                    text(&pin.number)?;
                    text(&pin.name)?;
                }
            }
            for label in c.labels.values() {
                text(&label.name)?;
            }
        }
    }
    Ok(bytes)
}
