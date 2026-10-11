//! Borrowed bounds before a sandbox copy. Pads' native `into` serializer clones;
//! these private reference views preserve its bytes without that intermediate copy.
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeStruct},
};
use std::{
    collections::BTreeMap,
    io::{self, Write},
};
use viamind_doc::{Board, ObjectId, Part, Project};

use super::{BranchError, DATA_LIMIT, ROW_LIMIT};

struct Counter {
    used: usize,
    limit: usize,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.used) {
            return Err(io::Error::other("PCB branch serialized byte budget exceeded"));
        }
        self.used += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn count(value: &impl Serialize, limit: usize) -> Result<usize, BranchError> {
    let mut counter = Counter { used: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|e| BranchError::invalid(format!("bounded PCB branch data: {e}")))?;
    Ok(counter.used)
}
pub(super) fn project(project: &Project) -> Result<(), BranchError> {
    let mut rows = 0usize;
    let mut add = |n: usize| -> Result<(), BranchError> {
        rows = rows.saturating_add(n);
        if rows > ROW_LIMIT {
            return Err(BranchError::invalid("PCB branches support at most 5000 whole-project structural rows"));
        }
        Ok(())
    };
    add(project.board.parts.len())?;
    add(project.board.tracks.len())?;
    add(project.board.vias.len())?;
    add(project.board.net_rules.len())?;
    add(project.schematic.sheets.len())?;
    for part in project.board.parts.values() {
        if let Some(pads) = &part.pads {
            add(pads.0.len())?;
        }
    }
    for sheet in project.schematic.sheets.values() {
        if let Some(content) = &sheet.content {
            add(content.symbols.len())?;
            add(content.wires.len())?;
            add(content.junctions.len())?;
            add(content.labels.len())?;
            for symbol in content.symbols.values() {
                add(symbol.pins.len())?;
            }
        }
    }
    count(&BorrowedProject(project), DATA_LIMIT)?;
    Ok(())
}
struct BorrowedProject<'a>(&'a Project);
impl Serialize for BorrowedProject<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        let mut s = serializer.serialize_struct("Project", 4)?;
        s.serialize_field("id", &p.id)?;
        s.serialize_field("name", &p.name)?;
        s.serialize_field("schematic", &p.schematic)?;
        s.serialize_field("board", &BorrowedBoard(&p.board))?;
        s.end()
    }
}
struct BorrowedBoard<'a>(&'a Board);
impl Serialize for BorrowedBoard<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let b = self.0;
        let mut s = serializer.serialize_struct("Board", 5)?;
        if !b.net_rules.is_empty() {
            s.serialize_field("net_rules", &b.net_rules)?;
        }
        if !b.tracks.is_empty() {
            s.serialize_field("tracks", &b.tracks)?;
        }
        if !b.vias.is_empty() {
            s.serialize_field("vias", &b.vias)?;
        }
        if let Some(outline) = &b.outline {
            s.serialize_field("outline", outline)?;
        }
        s.serialize_field("parts", &BorrowedParts(&b.parts))?;
        s.end()
    }
}
struct BorrowedParts<'a>(&'a BTreeMap<ObjectId, Part>);
impl Serialize for BorrowedParts<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_map(Some(self.0.len()))?;
        for (id, part) in self.0 {
            s.serialize_entry(id, &BorrowedPart(part))?;
        }
        s.end()
    }
}
struct BorrowedPart<'a>(&'a Part);
impl Serialize for BorrowedPart<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        let mut s = serializer.serialize_struct("Part", 11)?;
        if let Some(assembly) = &p.assembly {
            s.serialize_field("assembly", assembly)?;
        }
        if let Some(symbol) = &p.schematic_symbol {
            s.serialize_field("schematic_symbol", symbol)?;
        }
        if let Some(pads) = &p.pads {
            s.serialize_field("pads", &pads.0)?;
        }
        s.serialize_field("id", &p.id)?;
        s.serialize_field("designator", &p.designator)?;
        s.serialize_field("value", &p.value)?;
        s.serialize_field("footprint", &p.footprint)?;
        s.serialize_field("position", &p.position)?;
        s.serialize_field("rotation_mdeg", &p.rotation_mdeg)?;
        s.serialize_field("side", &p.side)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viamind_doc::{Pad, Pads, Side};
    use viamind_geom::{Nm, Point};

    #[test]
    fn borrowed_count_matches_native_serialization_with_pad_arrays_and_escaped_text() {
        let mut p = Project::new("quotes\" newline\n unicodeλ");
        let part = Part {
            id: ObjectId::new(),
            assembly: None,
            schematic_symbol: Some(ObjectId::new()),
            designator: "U1".into(),
            value: "\"\\λ".into(),
            footprint: "original".into(),
            position: Point::new(Nm(-3), Nm(7)),
            rotation_mdeg: 90_000,
            side: Side::Bottom,
            pads: Some(
                Pads::try_from(vec![Pad {
                    id: ObjectId::new(),
                    number: "1".into(),
                    position: Point::new(Nm(1), Nm(3)),
                    width: Nm(5),
                    height: Nm(7),
                    net: Some("N\"λ".into()),
                }])
                .unwrap(),
            ),
        };
        p.board.parts.insert(part.id, part);
        let native = serde_json::to_vec(&p).unwrap();
        let borrowed = serde_json::to_vec(&BorrowedProject(&p)).unwrap();
        assert_eq!(borrowed, native);
        assert_eq!(count(&BorrowedProject(&p), native.len()).unwrap(), native.len());
        assert!(count(&BorrowedProject(&p), native.len() - 1).is_err());
    }
}
