//! Integer straight copper segments and through vias on the supported outer layers.
use crate::ObjectId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::{Nm, Point};

pub const MAX_COPPER_OBJECTS: usize = 100_000;
const LIMIT_NM: u64 = 10_000_000_000;

/// Supported copper surfaces; inner layer routing is not represented yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CopperLayer {
    Top,
    Bottom,
}

/// One round-ended straight copper segment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "TrackFields")]
pub struct Track {
    pub id: ObjectId,
    pub start: Point,
    pub end: Point,
    pub width: Nm,
    pub layer: CopperLayer,
    pub net: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TrackFields {
    id: ObjectId,
    start: Point,
    end: Point,
    width: Nm,
    layer: CopperLayer,
    net: Option<String>,
}
impl TryFrom<TrackFields> for Track {
    type Error = String;
    fn try_from(v: TrackFields) -> Result<Self, String> {
        let result = Self { id: v.id, start: v.start, end: v.end, width: v.width, layer: v.layer, net: v.net };
        result.validate()?;
        Ok(result)
    }
}
impl Track {
    pub fn validate(&self) -> Result<(), String> {
        coordinate(self.start)?;
        coordinate(self.end)?;
        length(self.width)?;
        net(&self.net)?;
        if self.start == self.end {
            return Err("track endpoints must be distinct".into());
        }
        Ok(())
    }
}

/// A circular plated through via spanning the top and bottom copper surfaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ViaFields")]
pub struct Via {
    pub id: ObjectId,
    pub position: Point,
    pub diameter: Nm,
    pub drill: Nm,
    pub net: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ViaFields {
    id: ObjectId,
    position: Point,
    diameter: Nm,
    drill: Nm,
    net: Option<String>,
}
impl TryFrom<ViaFields> for Via {
    type Error = String;
    fn try_from(v: ViaFields) -> Result<Self, String> {
        let result = Self { id: v.id, position: v.position, diameter: v.diameter, drill: v.drill, net: v.net };
        result.validate()?;
        Ok(result)
    }
}
impl Via {
    pub fn validate(&self) -> Result<(), String> {
        coordinate(self.position)?;
        length(self.diameter)?;
        length(self.drill)?;
        net(&self.net)?;
        if self.drill >= self.diameter {
            return Err("via drill must be smaller than copper diameter".into());
        }
        Ok(())
    }
}
fn coordinate(point: Point) -> Result<(), String> {
    if [point.x.0, point.y.0].iter().any(|n| n.unsigned_abs() > LIMIT_NM) {
        return Err("copper coordinates must be within ±10000 mm".into());
    }
    Ok(())
}
fn length(value: Nm) -> Result<(), String> {
    if value.0 <= 0 || value.0.unsigned_abs() > LIMIT_NM {
        return Err("copper dimensions must be greater than zero and at most 10000 mm".into());
    }
    Ok(())
}
fn net(value: &Option<String>) -> Result<(), String> {
    if value.as_ref().is_some_and(|n| n.is_empty() || n.trim() != n || n.chars().count() > 128 || n.chars().any(char::is_control)) {
        return Err("net names must be trimmed, nonempty, control-free and at most 128 characters".into());
    }
    Ok(())
}
