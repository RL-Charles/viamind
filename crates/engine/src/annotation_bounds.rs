//! Borrowed annotation capture and secondary JSON guard limits.
use super::annotation::invalid;
use crate::CommandError;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
};
use viamind_doc::{ObjectId, Sheet};
const MAX_ROWS: usize = 10_000;
const MAX_TEXT: usize = 256 * 1024;
const MAX_GUARD: usize = 2 * 1024 * 1024;

pub(super) fn serialized(value: &impl Serialize) -> Result<(), CommandError> {
    serialized_limit(value, MAX_GUARD)
}
pub(super) fn envelope(value: &impl Serialize) -> Result<(), CommandError> {
    serialized_limit(value, MAX_GUARD + 8192)
}
fn serialized_limit(value: &impl Serialize, limit: usize) -> Result<(), CommandError> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(io::Error::other("annotation serialized limit exceeded"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter { bytes: 0, limit }, value).map_err(|e| invalid(e.to_string()))
}
#[derive(Default)]
struct Budget {
    rows: usize,
    symbols: usize,
    text: usize,
}
impl Budget {
    fn rows(&mut self, rows: usize, symbols: usize) -> Result<(), CommandError> {
        self.rows = self.rows.saturating_add(rows);
        self.symbols = self.symbols.saturating_add(symbols);
        if self.rows > MAX_ROWS || self.symbols > 2000 {
            return Err(invalid("annotation supports at most 2000 symbols and 10000 schematic objects/pins/package members"));
        }
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), CommandError> {
        self.text = self.text.saturating_add(text.len());
        if self.text > MAX_TEXT {
            return Err(invalid("annotation schematic text exceeds 256 KiB"));
        }
        Ok(())
    }
}
pub(super) fn budget(sheets: &BTreeMap<ObjectId, Sheet>) -> Result<usize, CommandError> {
    if sheets.len() > 100 {
        return Err(invalid("annotation supports at most 100 sheets"));
    }
    let mut b = Budget::default();
    b.rows(sheets.len(), 0)?;
    // Structural rows are charged before visiting per-row fields or allocating membership indices.
    for sheet in sheets.values() {
        if let Some(c) = &sheet.content {
            b.rows(c.object_count(), c.symbols.len())?;
            for symbol in c.symbols.values() {
                b.rows(symbol.pins.len(), 0)?;
            }
            for package in c.packages.values() {
                b.rows(package.units.len(), 0)?;
            }
        }
    }
    for sheet in sheets.values() {
        b.text(&sheet.name)?;
        if let Some(c) = &sheet.content {
            let (_, text) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            b.text = b.text.saturating_add(text).saturating_add(viamind_doc::schematic_bus::budget(c).map_err(invalid)?.text_bytes);
            if b.text > MAX_TEXT {
                return Err(invalid("annotation schematic text exceeds 256 KiB"));
            }
            for s in c.symbols.values() {
                for text in [Some(s.designator.as_str()), Some(s.value.as_str()), s.footprint.as_deref()].into_iter().flatten() {
                    b.text(text)?;
                }
                for p in &s.pins {
                    b.text(&p.number)?;
                    b.text(&p.name)?;
                }
            }
            for label in c.labels.values() {
                b.text(&label.name)?;
            }
        }
    }
    serialized(sheets)?;
    Ok(b.symbols)
}
fn map_rows(value: Option<&Value>) -> usize {
    value.and_then(Value::as_object).map_or(0, serde_json::Map::len)
}
fn array_rows(value: Option<&Value>) -> usize {
    value.and_then(Value::as_array).map_or(0, Vec::len)
}
fn values(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_object).into_iter().flat_map(serde_json::Map::values)
}
fn items(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_array).into_iter().flatten()
}
/// Bounds an already decoded transport Value before the secondary typed sheet guard allocation.
pub(super) fn predecode(value: &Value) -> Result<(), CommandError> {
    let sheets = value.get("expected_sheets").and_then(Value::as_object).ok_or_else(|| invalid("expected_sheets must be a map"))?;
    if sheets.len() > 100 {
        return Err(invalid("annotation supports at most 100 sheets"));
    }
    if array_rows(value.get("options").and_then(|v| v.get("sheet_order"))) > 100 {
        return Err(invalid("sheet_order supports at most 100 sheets"));
    }
    let mut b = Budget::default();
    b.rows(sheets.len(), 0)?;
    for sheet in sheets.values() {
        if let Some(c) = sheet.get("content") {
            if map_rows(c.get("buses")) > 256 || map_rows(c.get("bus_entries")) > 4096 {
                return Err(invalid("at most256 buses and4096 entries before typed decoding"));
            }
            let symbols = map_rows(c.get("symbols"));
            let rows = ["symbols", "wires", "junctions", "labels", "packages", "buses", "bus_entries"]
                .into_iter()
                .fold(0usize, |n, k| n.saturating_add(map_rows(c.get(k))));
            b.rows(rows, symbols)?;
            for symbol in values(c.get("symbols")) {
                b.rows(array_rows(symbol.get("pins")), 0)?;
            }
            for package in values(c.get("packages")) {
                b.rows(array_rows(package.get("units")), 0)?;
            }
        }
    }
    let mut text = |value: Option<&Value>| {
        if let Some(text) = value.and_then(Value::as_str) {
            b.text(text)?;
        }
        Ok::<_, CommandError>(())
    };
    for sheet in sheets.values() {
        text(sheet.get("name"))?;
        if let Some(c) = sheet.get("content") {
            for s in values(c.get("symbols")) {
                for k in ["designator", "value", "footprint"] {
                    text(s.get(k))?;
                }
                for p in items(s.get("pins")) {
                    for k in ["number", "name"] {
                        text(p.get(k))?;
                    }
                }
            }
            for l in values(c.get("labels")) {
                text(l.get("name"))?;
            }
            for p in values(c.get("packages")) {
                text(p.get("template_name"))?;
                for u in items(p.get("units")) {
                    for k in ["key", "name"] {
                        text(u.get(k))?;
                    }
                }
            }
        }
    }
    serialized(sheets)?;
    envelope(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_serialized_sheet_and_whole_envelope_limits_minus_equal_plus_one() {
        for delta in [-1_i64, 0, 1] {
            let target = (MAX_GUARD as i64 + delta) as usize;
            let mut sheets = json!({"00000000-0000-0000-0000-000000000001":{"unknown_padding":""}});
            let overhead = serde_json::to_vec(&sheets).unwrap().len();
            sheets.as_object_mut().unwrap().values_mut().next().unwrap()["unknown_padding"] = json!("x".repeat(target - overhead));
            assert_eq!(serde_json::to_vec(&sheets).unwrap().len(), target);
            assert_eq!(serialized(&sheets).is_ok(), delta <= 0);
            assert_eq!(predecode(&json!({"expected_sheets":sheets})).is_ok(), delta <= 0);
        }
        for delta in [-1_i64, 0, 1] {
            let target = (MAX_GUARD as i64 + 8192 + delta) as usize;
            let mut input = json!({"expected_sheets":{},"unknown_padding":""});
            let overhead = serde_json::to_vec(&input).unwrap().len();
            input["unknown_padding"] = json!("x".repeat(target - overhead));
            assert_eq!(serde_json::to_vec(&input).unwrap().len(), target);
            assert_eq!(envelope(&input).is_ok(), delta <= 0);
            assert_eq!(predecode(&input).is_ok(), delta <= 0);
        }
    }
    #[test]
    fn raw_package_members_pins_and_aggregate_text_charge_before_secondary_copy() {
        let cases = [
            json!({"symbols":{"bad":{"pins":vec![Value::Null;10_001]}}}),
            json!({"packages":{"bad":{"units":vec![Value::Null;10_001]}}}),
            json!({"symbols":{"bad":{"value":"x".repeat(MAX_TEXT+1)}}}),
            json!({"packages":{"bad":{"template_name":"x".repeat(MAX_TEXT+1)}}}),
        ];
        for c in cases {
            let result = predecode(&json!({"expected_sheets":{"poison":{"content":c}}}));
            assert!(result.is_err());
            let error = result.unwrap_err().to_string();
            assert!(error.contains("10000") || error.contains("256 KiB"), "{error}");
        }
    }
}
