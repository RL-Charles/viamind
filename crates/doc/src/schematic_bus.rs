//! Explicit nonconducting bus declarations and owned scalar breakout associations.
use crate::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, LabelScope, SchematicContent},
};
use schemars::JsonSchema;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub const MAX_BUSES: usize = 256;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_MEMBERS: usize = 10_000;
pub const MAX_TEXT: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicBus {
    pub id: ObjectId,
    #[serde(deserialize_with = "expression_string")]
    pub expression: String,
    pub start: Coordinate,
    pub end: Coordinate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicBusEntry {
    pub id: ObjectId,
    pub bus_id: ObjectId,
    #[serde(deserialize_with = "member_string")]
    pub member: String,
    pub wire_id: ObjectId,
    pub label_id: ObjectId,
    pub bus_point: Coordinate,
    pub wire_point: Coordinate,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BusBudget {
    pub buses: usize,
    pub entries: usize,
    pub members: usize,
    pub text_bytes: usize,
}
fn add(target: &mut usize, n: usize, maximum: usize, message: &str) -> Result<(), String> {
    *target = target.checked_add(n).ok_or_else(|| message.to_owned())?;
    if *target > maximum {
        return Err(message.into());
    }
    Ok(())
}
fn name(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 512 || s.chars().count() > 128 || s.chars().any(|c| c.is_control() || c.is_whitespace() || "[]{}".contains(c)) {
        return Err("bus names require1..128 characters, at most512 bytes, without whitespace, controls or brackets/braces".into());
    }
    Ok(())
}
fn uint(s: &str) -> Result<u32, String> {
    if s.is_empty() || s.len() > 10 || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err("bus suffixes require canonical unsigned decimal u32 integers".into());
    }
    s.parse().map_err(|_| "bus suffix exceeds u32".into())
}
fn term<'a>(s: &'a str, vector_required: bool, mut f: impl FnMut(&'a str, Option<(u32, u32)>) -> Result<(), String>) -> Result<(), String> {
    if let Some((prefix, range)) = s.split_once('[') {
        name(prefix)?;
        let range = range.strip_suffix(']').ok_or("bus vector requires closing bracket")?;
        let (first, last) = range.split_once("..").ok_or("bus vector requires M..N")?;
        let (first, last) = (uint(first)?, uint(last)?);
        f(prefix, Some((first.min(last), first.max(last))))
    } else {
        if vector_required {
            return Err("bus expression must be a vector or flat group".into());
        }
        name(s)?;
        f(s, None)
    }
}
fn walk<'a>(expression: &'a str, mut f: impl FnMut(Option<&'a str>, &'a str, Option<(u32, u32)>) -> Result<(), String>) -> Result<(), String> {
    if expression.is_empty() || expression.len() > 4096 {
        return Err("bus expression supports1..4096 UTF8 bytes".into());
    }
    if let Some((group, rest)) = expression.split_once('{') {
        let group = if group.is_empty() {
            None
        } else {
            name(group)?;
            Some(group)
        };
        let rest = rest.strip_suffix('}').ok_or("bus group requires closing brace")?;
        if rest.is_empty() || rest.starts_with(' ') || rest.ends_with(' ') {
            return Err("bus group requires members without edge spaces".into());
        }
        for t in rest.split(' ').filter(|s| !s.is_empty()) {
            term(t, false, |n, r| f(group, n, r))?;
        }
        Ok(())
    } else {
        term(expression, true, |n, r| f(None, n, r))
    }
}
fn digits(n: u32) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}
/// Borrowed first pass: count all expansion rows/bytes before allocating names.
pub fn expression_budget(expression: &str) -> Result<BusBudget, String> {
    let mut b = BusBudget::default();
    walk(expression, |group, prefix, range| {
        let count = range.map_or(1, |(a, z)| u64::from(z) - u64::from(a) + 1);
        if count > 256 {
            return Err("at most256 expanded members per bus".into());
        }
        add(&mut b.members, count as usize, 256, "at most256 expanded members per bus")?;
        let group_bytes = group.map_or(0, |g| g.len() + 1);
        let group_chars = group.map_or(0, |g| g.chars().count() + 1);
        let mut row = |suffix: usize| {
            let bytes = group_bytes + prefix.len() + suffix;
            let chars = group_chars + prefix.chars().count() + suffix;
            if bytes > 512 || chars > 128 {
                return Err("expanded member exceeds128 characters or512 UTF8 bytes".into());
            }
            add(&mut b.text_bytes, bytes, MAX_TEXT, "bus expanded text exceeds2MiB")
        };
        if let Some((a, z)) = range {
            for n in a..=z {
                row(digits(n))?;
            }
        } else {
            row(0)?;
        }
        Ok(())
    })?;
    Ok(b)
}
/// Deterministic exact expansion; duplicate final names refuse the entire expression.
pub fn expand(expression: &str) -> Result<Vec<String>, String> {
    let b = expression_budget(expression)?;
    let mut result = Vec::with_capacity(b.members);
    let mut seen = BTreeSet::new();
    walk(expression, |group, prefix, range| {
        let mut push = |suffix: Option<u32>| {
            let mut s = String::with_capacity(group.map_or(0, |g| g.len() + 1) + prefix.len() + suffix.map_or(0, digits));
            if let Some(g) = group {
                s.push_str(g);
                s.push('.');
            }
            s.push_str(prefix);
            if let Some(n) = suffix {
                use std::fmt::Write;
                write!(s, "{n}").map_err(|e| e.to_string())?;
            }
            if !seen.insert(s.clone()) {
                return Err("duplicate expanded bus member".into());
            }
            result.push(s);
            Ok(())
        };
        if let Some((a, z)) = range {
            for n in a..=z {
                push(Some(n))?;
            }
        } else {
            push(None)?;
        }
        Ok(())
    })?;
    Ok(result)
}
fn bounded_string<'de, D: Deserializer<'de>>(d: D, check: impl Fn(&str) -> Result<(), String>) -> Result<String, D::Error> {
    struct Text<F>(F);
    impl<'de, F: Fn(&str) -> Result<(), String>> Visitor<'de> for Text<F> {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded bus text")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            (self.0)(v).map_err(E::custom)?;
            Ok(v.to_owned())
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            (self.0)(&v).map_err(E::custom)?;
            Ok(v)
        }
    }
    d.deserialize_str(Text(check))
}
fn expression_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    bounded_string(d, |s| expression_budget(s).map(|_| ()))
}
fn member_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    bounded_string(d, name)
}
fn bounded_map<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D, max: usize) -> Result<BTreeMap<ObjectId, T>, D::Error> {
    struct Rows<T>(usize, std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Rows<T> {
        type Value = BTreeMap<ObjectId, T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded unique bus/entry map")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut rows = BTreeMap::new();
            let mut count = 0usize;
            while let Some(key) = map.next_key::<ObjectId>()? {
                count += 1;
                if count > self.0 {
                    return Err(de::Error::custom(format!("bus/entry map exceeds{} raw rows", self.0)));
                }
                if rows.contains_key(&key) {
                    return Err(de::Error::custom("duplicate bus/entry map key"));
                }
                rows.insert(key, map.next_value()?);
            }
            Ok(rows)
        }
    }
    d.deserialize_map(Rows(max, std::marker::PhantomData))
}
pub fn bounded_buses<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<ObjectId, SchematicBus>, D::Error> {
    bounded_map(d, MAX_BUSES)
}
pub fn bounded_entries<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<ObjectId, SchematicBusEntry>, D::Error> {
    bounded_map(d, MAX_ENTRIES)
}

pub fn budget(content: &SchematicContent) -> Result<BusBudget, String> {
    let mut b = BusBudget { buses: content.buses.len(), entries: content.bus_entries.len(), ..Default::default() };
    if b.buses > MAX_BUSES || b.entries > MAX_ENTRIES {
        return Err("at most256 buses and4096 entries".into());
    }
    for bus in content.buses.values() {
        let x = expression_budget(&bus.expression)?;
        add(&mut b.members, x.members, MAX_MEMBERS, "at most10000 expanded bus members")?;
        add(&mut b.text_bytes, bus.expression.len(), MAX_TEXT, "bus text exceeds2MiB")?;
        add(&mut b.text_bytes, x.text_bytes, MAX_TEXT, "bus text exceeds2MiB")?;
    }
    for entry in content.bus_entries.values() {
        name(&entry.member)?;
        add(&mut b.text_bytes, entry.member.len(), MAX_TEXT, "bus text exceeds2MiB")?;
    }
    Ok(b)
}
/// Admit combined rows and every inherited identity before any native global index or source copy.
pub fn preflight_project(project: &Project, replacement: Option<&Sheet>) -> Result<BusBudget, String> {
    if project.schematic.sheets.len() > 1000 {
        return Err("at most1000 schematic sheets".into());
    }
    let replacing = replacement.map(|s| s.id);
    let sheets = || project.schematic.sheets.values().filter(|s| Some(s.id) != replacing).chain(replacement);
    let mut total = BusBudget::default();
    for s in sheets() {
        if let Some(c) = &s.content {
            add(&mut total.buses, c.buses.len(), MAX_BUSES, "at most256 project buses")?;
            add(&mut total.entries, c.bus_entries.len(), MAX_ENTRIES, "at most4096 project entries")?;
        }
    }
    if total.buses == 0 && total.entries == 0 {
        return Ok(total);
    }
    let mut identities = 1usize;
    let mut count = |n| add(&mut identities, n, 100_000, "buses require at most100000 combined project identities");
    count(project.board.parts.len())?;
    count(project.board.tracks.len())?;
    count(project.board.vias.len())?;
    for part in project.board.parts.values() {
        count(part.pads.as_ref().map_or(0, |p| p.0.len()))?;
        count(part.holes.as_ref().map_or(0, Vec::len))?;
    }
    for s in sheets() {
        count(1)?;
        if let Some(c) = &s.content {
            count(c.object_count())?;
            for symbol in c.symbols.values() {
                count(symbol.pins.len())?;
            }
        }
    }
    for s in sheets() {
        if let Some(c) = &s.content {
            let b = budget(c)?;
            add(&mut total.members, b.members, MAX_MEMBERS, "at most10000 project bus members")?;
            add(&mut total.text_bytes, b.text_bytes, MAX_TEXT, "project bus text exceeds2MiB")?;
        }
    }
    Ok(total)
}
fn on(p: Coordinate, a: Coordinate, b: Coordinate) -> bool {
    let (px, py, ax, ay, bx, by) = (i128::from(p.x.0), i128::from(p.y.0), i128::from(a.x.0), i128::from(a.y.0), i128::from(b.x.0), i128::from(b.y.0));
    (px - ax) * (by - ay) == (py - ay) * (bx - ax) && px >= ax.min(bx) && px <= ax.max(bx) && py >= ay.min(by) && py <= ay.max(by)
}
pub fn validate(content: &SchematicContent) -> Result<(), String> {
    budget(content)?;
    let mut names = BTreeMap::new();
    for bus in content.buses.values() {
        bus.start.validate()?;
        bus.end.validate()?;
        if bus.start == bus.end || (bus.start.x != bus.end.x && bus.start.y != bus.end.y) {
            return Err("bus segments must be nonzero and cardinal".into());
        }
        names.insert(bus.id, expand(&bus.expression)?);
    }
    let mut wires = BTreeSet::new();
    let mut labels = BTreeSet::new();
    for e in content.bus_entries.values() {
        e.bus_point.validate()?;
        e.wire_point.validate()?;
        let dx = i128::from(e.bus_point.x.0) - i128::from(e.wire_point.x.0);
        let dy = i128::from(e.bus_point.y.0) - i128::from(e.wire_point.y.0);
        if dx == 0 || dx.abs() != dy.abs() {
            return Err("bus entries must be nonzero exact45degree diagonals".into());
        }
        let b = content.buses.get(&e.bus_id).ok_or("entry bus is absent")?;
        if !on(e.bus_point, b.start, b.end) || !names.get(&e.bus_id).is_some_and(|n| n.contains(&e.member)) {
            return Err("entry requires an exact existing bus point and member".into());
        }
        let w = content.wires.get(&e.wire_id).ok_or("entry owned scalar wire is absent")?;
        let l = content.labels.get(&e.label_id).ok_or("entry owned scalar label is absent")?;
        if !wires.insert(e.wire_id) || !labels.insert(e.label_id) {
            return Err("bus entries require distinct owned wire and label pairs".into());
        }
        if w.start == w.end
            || (w.start.x != w.end.x && w.start.y != w.end.y)
            || (w.start != e.wire_point && w.end != e.wire_point)
            || l.position != e.wire_point
            || l.scope != LabelScope::Local
            || l.name != e.member
        {
            return Err("entry must own a cardinal wire endpoint and exact Local member label at that endpoint".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_declared_order_unicode_and_prefix_oracles() {
        for (s, expected) in [
            ("DATA[3..0]", vec!["DATA0", "DATA1", "DATA2", "DATA3"]),
            ("USB1{DP DM}", vec!["USB1.DP", "USB1.DM"]),
            ("MEM{A[2..0] OE}", vec!["MEM.A0", "MEM.A1", "MEM.A2", "MEM.OE"]),
            ("Δ{σ[1..0] n}", vec!["Δ.σ0", "Δ.σ1", "Δ.n"]),
            ("{A a}", vec!["A", "a"]),
        ] {
            assert_eq!(expand(s).unwrap(), expected);
        }
        assert_eq!(expand("DATA[0..3]").unwrap(), expand("DATA[3..0]").unwrap());
        assert_eq!(expand("{A  B}").unwrap(), ["A", "B"]);
    }
    #[test]
    fn malformed_duplicate_overflow_and_range_refuse_without_expansion() {
        for s in [
            "DATA[-1..2]",
            "DATA[0..4294967296]",
            "DATA[00..2]",
            "DATA[0..256]",
            "{A A}",
            "{A[0..1] A0}",
            "USB{A{B}}",
            " {A}",
            "{A} ",
            "{A\tB}",
            "DATA[1e2..200]",
            "DATA[0..1]X",
            "{}",
        ] {
            assert!(expand(s).is_err(), "{s}");
        }
        assert_eq!(expand("D[4294967295..4294967295]").unwrap(), ["D4294967295"]);
        assert!(expression_budget("D[0..4294967295]").unwrap_err().contains("256"));
        assert!(expression_budget(&format!("{{{}0}}", "A".repeat(128))).is_err());
    }
    #[test]
    fn native_raw_row_cap_precedes_poison_value_and_duplicates_refuse() {
        let id = ObjectId::new();
        let bus = serde_json::json!({"id":id,"expression":"D[0..1]","start":{"x":0,"y":0},"end":{"x":1,"y":0}});
        let row = format!("\"{id}\":{bus}");
        let text = format!("{{{row},{row}}}");
        #[derive(Deserialize)]
        struct Map(#[serde(deserialize_with = "bounded_buses")] BTreeMap<ObjectId, SchematicBus>);
        assert!(serde_json::from_str::<Map>(&text).err().unwrap().to_string().contains("duplicate"));
        let mut rows = Vec::new();
        for n in 1..=MAX_BUSES {
            let id = ObjectId::from_uuid(uuid::Uuid::from_u128(n as u128));
            let mut b = bus.clone();
            b["id"] = serde_json::json!(id);
            rows.push(format!("\"{id}\":{b}"));
        }
        rows.push(format!("\"{}\":null", ObjectId::new()));
        assert!(serde_json::from_str::<Map>(&format!("{{{}}}", rows.join(","))).err().unwrap().to_string().contains("256 raw rows"));
        let ok: Map = serde_json::from_str(&format!("{{{row}}}")).unwrap();
        assert_eq!(ok.0.len(), 1);
    }
    fn p(x: i64, y: i64) -> Coordinate {
        Coordinate { x: viamind_geom::Nm(x), y: viamind_geom::Nm(y) }
    }
    fn fixture() -> (Project, ObjectId, ObjectId) {
        let mut project = Project::new("explicit buses");
        let mut sheet = Sheet::new("Bus");
        let bus = SchematicBus { id: ObjectId::new(), expression: "DATA[1..0]".into(), start: p(0, 0), end: p(10, 0) };
        let id = bus.id;
        let sheet_id = sheet.id;
        let mut c = SchematicContent::default();
        c.buses.insert(id, bus);
        sheet.content = Some(c);
        project.schematic.sheets.insert(sheet.id, sheet);
        (project, sheet_id, id)
    }
    #[test]
    fn exact_breakout_association_and_nonconducting_ids_validate() {
        use crate::schematic::{SchematicLabel, SchematicWire};
        let (mut project, sheet, bus) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        let wire = SchematicWire { id: ObjectId::new(), start: p(4, 4), end: p(4, 8) };
        let label = SchematicLabel { id: ObjectId::new(), position: p(4, 4), name: "DATA0".into(), scope: LabelScope::Local };
        let entry = SchematicBusEntry {
            id: ObjectId::new(),
            bus_id: bus,
            member: "DATA0".into(),
            wire_id: wire.id,
            label_id: label.id,
            bus_point: p(0, 0),
            wire_point: p(4, 4),
        };
        c.wires.insert(wire.id, wire);
        c.labels.insert(label.id, label.clone());
        c.bus_entries.insert(entry.id, entry.clone());
        assert!(crate::schematic::validate_project(&project, None).is_ok());
        let valid = project.clone();
        for which in 0..5 {
            let mut bad = valid.clone();
            let c = bad.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
            match which {
                0 => c.labels.get_mut(&label.id).unwrap().scope = LabelScope::Global,
                1 => c.bus_entries.get_mut(&entry.id).unwrap().member = "DATA2".into(),
                2 => c.bus_entries.get_mut(&entry.id).unwrap().bus_point = p(0, 1),
                3 => c.bus_entries.get_mut(&entry.id).unwrap().wire_point = p(5, 4),
                _ => {
                    let mut duplicate = entry.clone();
                    duplicate.id = ObjectId::new();
                    c.bus_entries.insert(duplicate.id, duplicate);
                }
            }
            assert!(crate::schematic::validate_project(&bad, None).is_err(), "case{which}");
        }
    }
    #[test]
    fn version_sixteen_and_absent_legacy_bytes_and_global_namespace() {
        let (mut project, sheet, bus) = fixture();
        let text = crate::file::to_string(&project);
        assert_eq!(crate::file::minimum_version(&project), 16);
        assert_eq!(crate::file::from_str(&text).unwrap(), project);
        assert!(matches!(
            crate::file::from_str(&text.replacen("\"version\": 16", "\"version\": 15", 1)),
            Err(crate::file::FileError::FeatureVersion(16))
        ));
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        c.buses.clear();
        let legacy = crate::file::to_string(&project);
        assert!(!legacy.contains("buses"));
        assert!(!legacy.contains("bus_entries"));
        assert_eq!(crate::file::minimum_version(&project), 5);
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        c.buses.insert(project.id, SchematicBus { id: project.id, expression: "D[0..0]".into(), start: p(0, 0), end: p(1, 0) });
        assert!(crate::schematic::validate_project(&project, None).unwrap_err().contains("duplicate"));
        assert_ne!(bus, project.id);
    }
    #[test]
    fn borrowed_max_plus_one_precedes_field_poison_and_global_index() {
        let (mut project, sheet, _) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        c.buses.clear();
        for _ in 0..=MAX_BUSES {
            let id = ObjectId::new();
            c.buses.insert(id, SchematicBus { id, expression: "poison".into(), start: p(0, 0), end: p(0, 0) });
        }
        assert!(crate::schematic::validate_project(&project, None).unwrap_err().contains("256 project buses"));
        let mut c = SchematicContent::default();
        for _ in 0..=MAX_ENTRIES {
            let id = ObjectId::new();
            c.bus_entries.insert(
                id,
                SchematicBusEntry { id, bus_id: id, member: "poison\n".into(), wire_id: id, label_id: id, bus_point: p(0, 0), wire_point: p(0, 0) },
            );
        }
        assert!(budget(&c).unwrap_err().contains("4096 entries"));
    }
}
