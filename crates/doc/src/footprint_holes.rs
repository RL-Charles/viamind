//! Explicit circular nonplated mechanical drills, independent of electrical pads.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{collections::BTreeSet, fmt};
use viamind_geom::{Nm, Point};

pub const MAX_HOLES_PER_OWNER: usize = 256;
pub const MAX_PROJECT_IDENTITIES: usize = 100_000;
pub const MAX_HOLE_KEY_BYTES: usize = 2 * 1024 * 1024;
const MAX_LOCAL_NM: i128 = 10_000_000_000;
const MAX_DIAMETER_NM: i64 = 100_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintHole {
    pub key: String,
    #[serde(deserialize_with = "point")]
    pub center: Point,
    pub diameter: Nm,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartHole {
    pub id: crate::ObjectId,
    pub key: String,
    #[serde(deserialize_with = "point")]
    pub center: Point,
    pub diameter: Nm,
}

fn point<'de, D: Deserializer<'de>>(d: D) -> Result<Point, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Coordinates {
        x: Nm,
        y: Nm,
    }
    let p = Coordinates::deserialize(d)?;
    Ok(Point::new(p.x, p.y))
}

fn geometry(key: &str, center: Point, diameter: Nm) -> Result<(), String> {
    if key.is_empty() || key.len() > 64 || !key.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)) {
        return Err("hole keys require1..64 ASCII letters, digits, underscore, dot or hyphen".into());
    }
    if !(1..=MAX_DIAMETER_NM).contains(&diameter.0) {
        return Err("hole diameter must be positive and at most100mm".into());
    }
    for coordinate in [center.x.0, center.y.0] {
        if i128::from(coordinate).abs() * 2 + i128::from(diameter.0) > MAX_LOCAL_NM * 2 {
            return Err("hole doubled local extents must remain within +/-10000mm".into());
        }
    }
    Ok(())
}
impl FootprintHole {
    pub fn validate(&self) -> Result<(), String> {
        geometry(&self.key, self.center, self.diameter)
    }
}
impl PartHole {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.as_uuid().is_nil() {
            return Err("hole identity must be nonnil".into());
        }
        geometry(&self.key, self.center, self.diameter)
    }
    pub fn detached(&self) -> FootprintHole {
        FootprintHole { key: self.key.clone(), center: self.center, diameter: self.diameter }
    }
}

pub fn validate_part_holes(holes: &[PartHole]) -> Result<(), String> {
    if holes.len() > MAX_HOLES_PER_OWNER {
        return Err("at most256 holes per owner".into());
    }
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for hole in holes {
        hole.validate()?;
        if !keys.insert(&hole.key) || !ids.insert(hole.id) {
            return Err("duplicate hole key or identity within owner".into());
        }
    }
    Ok(())
}
pub fn validate_footprint_holes(holes: &[FootprintHole]) -> Result<(), String> {
    if holes.len() > MAX_HOLES_PER_OWNER {
        return Err("at most256 holes per owner".into());
    }
    let mut keys = BTreeSet::new();
    for hole in holes {
        hole.validate()?;
        if !keys.insert(&hole.key) {
            return Err("duplicate hole key within owner".into());
        }
    }
    Ok(())
}

/// Refuse max+1 before decoding that row, including an invalid/null poison row.
fn rows<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    struct Rows<T>(std::marker::PhantomData<T>);
    struct Excess;
    impl<'de> de::DeserializeSeed<'de> for Excess {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
            Err(de::Error::custom("at most256 holes per owner before decoding extra rows"))
        }
    }
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Rows<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most256 holes")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut result = Vec::new();
            while result.len() < MAX_HOLES_PER_OWNER {
                let Some(row) = seq.next_element()? else { return Ok(result) };
                result.push(row);
            }
            seq.next_element_seed(Excess)?;
            Ok(result)
        }
    }
    d.deserialize_seq(Rows(std::marker::PhantomData))
}
pub fn deserialize_holes<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Vec<T>>, D::Error> {
    struct Optional<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Optional<T> {
        type Value = Option<Vec<T>>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("null or at most256 holes")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            rows(d).map(Some)
        }
    }
    d.deserialize_option(Optional(std::marker::PhantomData))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedHole {
    pub center: Point,
    pub diameter: Nm,
}

/// Resolve once. Quarter turns are exact; other poses quantize the computed center to Nm.
pub fn resolve_hole(part: &crate::Part, hole: &PartHole) -> Result<ResolvedHole, String> {
    hole.validate()?;
    let center = if part.rotation_mdeg.rem_euclid(90_000) == 0 {
        part.local_to_world_nm_exact(hole.center).map_err(str::to_owned)?
    } else {
        let (x, y) = part.local_to_world_mm(hole.center.x.to_mm(), hole.center.y.to_mm());
        let quantize = |n: f64| -> Result<Nm, String> {
            let n = (n * 1_000_000.0).round();
            // The upper integer boundary is exclusive: i64::MAX rounds to2^63 in f64.
            if !n.is_finite() || n < i64::MIN as f64 || n >= -(i64::MIN as f64) {
                return Err("transformed hole center overflows integer coordinates".into());
            }
            Ok(Nm(n as i64))
        };
        Point::new(quantize(x)?, quantize(y)?)
    };
    for coordinate in [center.x.0, center.y.0] {
        let n = i128::from(coordinate) * 2;
        if n - i128::from(hole.diameter.0) < i128::from(i64::MIN) * 2 || n + i128::from(hole.diameter.0) > i128::from(i64::MAX) * 2 {
            return Err("transformed hole extent exceeds integer coordinates".into());
        }
    }
    Ok(ResolvedHole { center, diameter: hole.diameter })
}

/// Borrowed admission before any project validator creates a global identity set.
/// A replacement sheet uses the same semantics as schematic validation.
pub fn preflight_project(project: &crate::Project, replacement: Option<&crate::Sheet>) -> Result<(), String> {
    preflight(project, replacement, None)
}

/// Admit a prospective Part edit without cloning the Part or allocating identities.
pub fn preflight_replacement(project: &crate::Project, owner: crate::ObjectId, pads: usize, holes: Option<&[PartHole]>) -> Result<(), String> {
    preflight(project, None, Some((owner, pads, holes.map(HoleRows::Placed))))
}

pub fn preflight_footprint_replacement(
    project: &crate::Project,
    owner: crate::ObjectId,
    pads: usize,
    holes: Option<&[FootprintHole]>,
) -> Result<(), String> {
    preflight(project, None, Some((owner, pads, holes.map(HoleRows::Detached))))
}
#[derive(Clone, Copy)]
enum HoleRows<'a> {
    Placed(&'a [PartHole]),
    Detached(&'a [FootprintHole]),
}
impl HoleRows<'_> {
    fn len(self) -> usize {
        match self {
            Self::Placed(h) => h.len(),
            Self::Detached(h) => h.len(),
        }
    }
    fn key_bytes(self) -> Result<usize, String> {
        let mut bytes = 0usize;
        let mut add = |key: &str| {
            bytes = bytes.checked_add(key.len()).ok_or("hole key budget overflow")?;
            Ok::<(), String>(())
        };
        match self {
            Self::Placed(h) => {
                for h in h {
                    add(&h.key)?;
                }
            }
            Self::Detached(h) => {
                for h in h {
                    add(&h.key)?;
                }
            }
        }
        Ok(bytes)
    }
}
type Replacement<'a> = (crate::ObjectId, usize, Option<HoleRows<'a>>);
fn preflight(project: &crate::Project, replacement: Option<&crate::Sheet>, part_replacement: Option<Replacement<'_>>) -> Result<(), String> {
    if !project
        .board
        .parts
        .values()
        .any(|p| part_replacement.filter(|(id, _, _)| *id == p.id).map_or(p.holes.is_some(), |(_, _, holes)| holes.is_some()))
    {
        return Ok(());
    }
    let mut identities = 1usize;
    let mut hole_rows = 0usize;
    let mut key_bytes = 0usize;
    let mut add = |n: usize| -> Result<(), String> {
        identities = identities.checked_add(n).ok_or("hole project identity budget overflow")?;
        if identities > MAX_PROJECT_IDENTITIES {
            return Err("authored holes require at most100000 combined project identities".into());
        }
        Ok(())
    };
    add(project.board.parts.len())?;
    add(project.board.tracks.len())?;
    add(project.board.vias.len())?;
    for part in project.board.parts.values() {
        let (_, pads, holes) = part_replacement.filter(|(id, _, _)| *id == part.id).unwrap_or((
            part.id,
            part.pads.as_ref().map_or(0, |p| p.0.len()),
            part.holes.as_deref().map(HoleRows::Placed),
        ));
        add(pads)?;
        if let Some(holes) = holes {
            if holes.len() > MAX_HOLES_PER_OWNER {
                return Err("at most256 holes per owner".into());
            }
            add(holes.len())?;
            hole_rows = hole_rows.checked_add(holes.len()).ok_or("hole row budget overflow")?;
            if hole_rows > MAX_PROJECT_IDENTITIES {
                return Err("at most100000 component holes".into());
            }
            key_bytes = key_bytes.checked_add(holes.key_bytes()?).ok_or("hole key budget overflow")?;
            if key_bytes > MAX_HOLE_KEY_BYTES {
                return Err("aggregate hole keys exceed2MiB".into());
            }
        }
    }
    let replacing = replacement.map(|s| s.id);
    for sheet in project.schematic.sheets.values().filter(|s| Some(s.id) != replacing).chain(replacement) {
        add(1)?;
        if let Some(content) = &sheet.content {
            add(content.packages.len())?;
            add(content.symbols.len())?;
            add(content.wires.len())?;
            add(content.junctions.len())?;
            add(content.labels.len())?;
            for symbol in content.symbols.values() {
                add(symbol.pins.len())?;
            }
        }
    }
    Ok(())
}
