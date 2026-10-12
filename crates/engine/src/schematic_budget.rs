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
    let bus_budget = c.map(viamind_doc::schematic_bus::budget).transpose().map_err(invalid)?.unwrap_or_default();
    let (membership, mut text) = c.map(viamind_doc::symbol_package::membership_budget).transpose().map_err(invalid)?.unwrap_or_default();
    if c.is_some_and(|c| c.object_count().saturating_add(membership.saturating_sub(c.packages.len())) > 10_000) {
        return Err(invalid(format!("{purpose} supports at most 10000 objects and package membership records")));
    }
    text = text.saturating_add(bus_budget.text_bytes);
    if text > 2 * 1024 * 1024 {
        return Err(invalid(format!("{purpose} package text exceeds 2 MiB")));
    }
    let mut pins = 0usize;
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
    viamind_doc::schematic_bus::preflight_project(project, None).map_err(invalid)?;
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
                "{purpose} supports at most 100000 combined project/sheet/part/pad/hole/copper/schematic/pin identities including additions"
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
        more(p.pads.as_ref().map_or(0, |p| p.0.len()).saturating_add(p.holes.as_ref().map_or(0, Vec::len)))?;
    }
    let mut bytes = 0usize;
    for sheet in project.schematic.sheets.values() {
        if let Some(c) = &sheet.content {
            let (records, text) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            more(records.saturating_sub(c.packages.len()))?;
            bytes = bytes.saturating_add(text).saturating_add(viamind_doc::schematic_bus::budget(c).map_err(invalid)?.text_bytes);
            if bytes > 4 * 1024 * 1024 {
                return Err(invalid("global package text exceeds 4 MiB"));
            }
        }
    }
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

/// Admit a borrowed transport sheet guard before allocating its typed objects/pins.
/// The original transport Value is outside this secondary allocation boundary.
pub(crate) fn predecode_sheet(value: &serde_json::Value, purpose: &str) -> Result<(), CommandError> {
    use serde_json::Value;
    let count = |v: Option<&Value>| v.and_then(Value::as_object).map_or(0, serde_json::Map::len);
    fn values(v: Option<&Value>) -> impl Iterator<Item = &Value> {
        v.and_then(Value::as_object).into_iter().flat_map(serde_json::Map::values)
    }
    fn items(v: Option<&Value>) -> impl Iterator<Item = &Value> {
        v.and_then(Value::as_array).into_iter().flatten()
    }
    let c = value.get("content");
    let mut rows = c.map_or(0, |c| {
        ["symbols", "wires", "junctions", "labels", "packages", "buses", "bus_entries"]
            .into_iter()
            .fold(0usize, |n, k| n.saturating_add(count(c.get(k))))
    });
    if rows > 10_000 {
        return Err(invalid(format!("{purpose} supports at most10000 sheet objects before typed decoding")));
    }
    if c.is_some_and(|c| count(c.get("buses")) > 256 || count(c.get("bus_entries")) > 4096) {
        return Err(invalid("at most256 buses and4096 entries before typed decoding"));
    }
    let mut pins = 0usize;
    if let Some(c) = c {
        for s in values(c.get("symbols")) {
            pins = pins.saturating_add(s.get("pins").and_then(Value::as_array).map_or(0, Vec::len));
            if pins > 10_000 {
                return Err(invalid(format!("{purpose} supports at most10000 sheet pins before typed decoding")));
            }
        }
        for p in values(c.get("packages")) {
            rows = rows.saturating_add(p.get("units").and_then(Value::as_array).map_or(0, Vec::len));
            if rows > 10_000 {
                return Err(invalid(format!("{purpose} supports at most10000 objects/package members before typed decoding")));
            }
        }
    }
    let mut bytes = 0usize;
    let mut text = |v: Option<&Value>| -> Result<(), CommandError> {
        if let Some(s) = v.and_then(Value::as_str) {
            bytes = bytes.saturating_add(s.len());
            if s.len() > 4096 || bytes > 2 * 1024 * 1024 {
                return Err(invalid(format!("{purpose} text exceeds4096bytes/string or2MiB before typed decoding")));
            }
        }
        Ok(())
    };
    text(value.get("name"))?;
    if let Some(c) = c {
        for s in values(c.get("symbols")) {
            for key in ["designator", "value", "footprint"] {
                text(s.get(key))?;
            }
            for p in items(s.get("pins")) {
                text(p.get("number"))?;
                text(p.get("name"))?;
            }
        }
        for bus in values(c.get("buses")) {
            text(bus.get("expression"))?;
        }
        for entry in values(c.get("bus_entries")) {
            text(entry.get("member"))?;
        }
        for label in values(c.get("labels")) {
            text(label.get("name"))?;
        }
        for p in values(c.get("packages")) {
            text(p.get("template_name"))?;
            for u in items(p.get("units")) {
                text(u.get("key"))?;
                text(u.get("name"))?;
            }
        }
    }
    Ok(())
}
