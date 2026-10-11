//! Explicit user-entered board assembly instructions; no sourcing inference.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::{Nm, Point};

pub const MAX_SOURCING_COMPONENTS: usize = 100_000;
pub const MAX_SOURCING_TEXT: usize = 2 * 1024 * 1024;

/// Explicit user-entered assembly technology; never inferred from pad geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MountingTechnology {
    SurfaceMount,
    ThroughHole,
    Hybrid,
}
impl MountingTechnology {
    pub const ALL: [Self; 3] = [Self::SurfaceMount, Self::ThroughHole, Self::Hybrid];
    pub fn name(self) -> &'static str {
        match self {
            Self::SurfaceMount => "surface_mount",
            Self::ThroughHole => "through_hole",
            Self::Hybrid => "hybrid",
        }
    }
}

/// Local centroid and orientation marker in the unmirrored footprint frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssemblyPlacement {
    #[serde(deserialize_with = "strict_centroid")]
    #[schemars(with = "CentroidCoordinates")]
    pub centroid_local: Point,
    /// 0 points +X; 90000 points -Y. This is not a universal vendor zero.
    pub zero_axis_mdeg: i32,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CentroidCoordinates {
    x: Nm,
    y: Nm,
}
fn strict_centroid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Point, D::Error> {
    let coordinates = CentroidCoordinates::deserialize(deserializer)?;
    Ok(Point::new(coordinates.x, coordinates.y))
}
impl AssemblyPlacement {
    pub fn validate(&self) -> Result<(), &'static str> {
        if [self.centroid_local.x.0, self.centroid_local.y.0].iter().any(|value| !(-10_000_000_000..=10_000_000_000).contains(value)) {
            return Err("assembly local centroid coordinates must lie within ±10000 mm");
        }
        if !(0..360_000).contains(&self.zero_axis_mdeg) {
            return Err("assembly local zero-axis marker must lie within 0..359999 millidegrees");
        }
        Ok(())
    }
}
impl Ord for AssemblyPlacement {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.centroid_local.x, self.centroid_local.y, self.zero_axis_mdeg).cmp(&(
            other.centroid_local.x,
            other.centroid_local.y,
            other.zero_axis_mdeg,
        ))
    }
}
impl PartialOrd for AssemblyPlacement {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssemblyMetadata {
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    pub supplier: Option<String>,
    pub supplier_sku: Option<String>,
    #[serde(default = "populated")]
    pub populate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mounting: Option<MountingTechnology>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<AssemblyPlacement>,
}
const fn populated() -> bool {
    true
}
impl Default for AssemblyMetadata {
    fn default() -> Self {
        Self { manufacturer: None, mpn: None, supplier: None, supplier_sku: None, populate: true, mounting: None, placement: None }
    }
}
impl AssemblyMetadata {
    pub fn validate_field(s: &str) -> Result<(), &'static str> {
        if s.is_empty() || s.len() > 512 || s.trim() != s || s.chars().any(char::is_control) {
            return Err("sourcing fields require 1–512 UTF-8 bytes without control characters or edge whitespace");
        }
        Ok(())
    }
    pub fn strings(&self) -> impl Iterator<Item = &str> {
        [&self.manufacturer, &self.mpn, &self.supplier, &self.supplier_sku].into_iter().filter_map(|s| s.as_deref())
    }
    pub fn validate_sourcing(&self) -> Result<(), &'static str> {
        for s in self.strings() {
            Self::validate_field(s)?;
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        self.validate_sourcing()?;
        if let Some(placement) = self.placement {
            placement.validate()?;
        }
        Ok(())
    }
    pub fn is_default(&self) -> bool {
        self.populate && self.strings().next().is_none() && self.mounting.is_none() && self.placement.is_none()
    }
    /// Minimum schema preserving this metadata, including historic snapshots.
    pub fn minimum_version(&self) -> u32 {
        if self.mounting.is_some() || self.placement.is_some() { 9 } else { 7 }
    }
}
/// Validate borrowed records before cloning. Metadata-free parts do not count.
pub fn validate_metadata<'a>(records: impl IntoIterator<Item = &'a AssemblyMetadata>) -> Result<(), &'static str> {
    let mut count = 0usize;
    let mut text = 0usize;
    for record in records {
        record.validate()?;
        count += 1;
        if count > MAX_SOURCING_COMPONENTS {
            return Err("at most 100000 components may carry sourcing metadata");
        }
        for s in record.strings() {
            text = text.saturating_add(s.len());
            if text > MAX_SOURCING_TEXT {
                return Err("sourcing metadata exceeds 2 MiB aggregate UTF-8 text");
            }
        }
    }
    Ok(())
}
