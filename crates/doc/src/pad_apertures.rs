//! Explicit centered rectangular SMD openings. Absence retains legacy behavior.
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{borrow::Cow, fmt};
use viamind_geom::{Nm, Point};

pub const MAX_LOCAL_NM: i64 = 10_000_000_000;

/// Absolute dimensions in the pad's local axes; odd dimensions retain half-Nm edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApertureRect {
    pub width: Nm,
    pub height: Nm,
}
impl ApertureRect {
    pub fn validate_at(self, center: Point) -> Result<(), String> {
        for (coordinate, dimension) in [(center.x.0, self.width.0), (center.y.0, self.height.0)] {
            if dimension <= 0 || dimension > MAX_LOCAL_NM {
                return Err("aperture dimensions must be positive and at most10000mm".into());
            }
            if i128::from(coordinate).abs() * 2 + i128::from(dimension) > i128::from(MAX_LOCAL_NM) * 2 {
                return Err("centered aperture doubled extents must remain within +/-10000mm".into());
            }
        }
        Ok(())
    }
    /// Explicit calculator only. No stored defaults or implicit resize relationship.
    pub fn equal_margin(copper_width: Nm, copper_height: Nm, margin: Nm) -> Result<Self, String> {
        let dimension = |copper: Nm| {
            let n = i128::from(copper.0) + 2 * i128::from(margin.0);
            if copper.0 <= 0 || copper.0 > MAX_LOCAL_NM || !(1..=i128::from(MAX_LOCAL_NM)).contains(&n) {
                Err("equal margin would collapse or exceed the supported aperture dimensions".into())
            } else {
                Ok(Nm(n as i64))
            }
        };
        Ok(Self { width: dimension(copper_width)?, height: dimension(copper_height)? })
    }
}

/// Both members are required when this container is present. Null means intentional omission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PadApertures {
    pub mask: Option<ApertureRect>,
    pub paste: Option<ApertureRect>,
}
impl JsonSchema for PadApertures {
    fn schema_name() -> Cow<'static, str> {
        "PadApertures".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let nullable = generator.subschema_for::<Option<ApertureRect>>();
        json_schema!({"type":"object","required":["mask","paste"],"additionalProperties":false,
            "properties":{"mask":nullable.clone(),"paste":nullable}})
    }
}
impl<'de> Deserialize<'de> for PadApertures {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = PadApertures;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object with required nullable mask and paste rectangles")
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut mask = None;
                let mut paste = None;
                while let Some(key) = map.next_key::<String>()? {
                    let slot = match key.as_str() {
                        "mask" => &mut mask,
                        "paste" => &mut paste,
                        _ => return Err(de::Error::unknown_field(&key, &["mask", "paste"])),
                    };
                    if slot.is_some() {
                        return Err(de::Error::custom(format!("duplicate aperture field {key}")));
                    }
                    *slot = Some(map.next_value::<Option<ApertureRect>>()?);
                }
                Ok(PadApertures {
                    mask: mask.ok_or_else(|| de::Error::missing_field("mask"))?,
                    paste: paste.ok_or_else(|| de::Error::missing_field("paste"))?,
                })
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
impl PadApertures {
    pub fn validate_at(&self, center: Point) -> Result<(), String> {
        for rect in self.mask.iter().chain(&self.paste) {
            rect.validate_at(center)?;
        }
        Ok(())
    }
    pub fn structural_rows(&self) -> usize {
        1 + usize::from(self.mask.is_some()) + usize::from(self.paste.is_some())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApertureKind {
    Mask,
    Paste,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApertureResolution {
    Legacy,
    IntentionalOmit,
    Rectangle(ApertureRect),
}
/// Shared policy resolution; legacy consumers separately disclose their historic expansion.
pub fn resolve_aperture(policy: Option<&PadApertures>, kind: ApertureKind) -> ApertureResolution {
    match policy {
        None => ApertureResolution::Legacy,
        Some(policy) => match kind {
            ApertureKind::Mask => policy.mask,
            ApertureKind::Paste => policy.paste,
        }
        .map_or(ApertureResolution::IntentionalOmit, ApertureResolution::Rectangle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullable_members_are_required_and_raw_duplicates_unknown_nested_fields_refuse() {
        let empty: PadApertures = serde_json::from_str(r#"{"mask":null,"paste":null}"#).unwrap();
        assert_eq!(empty.structural_rows(), 1);
        assert_eq!(resolve_aperture(Some(&empty), ApertureKind::Mask), ApertureResolution::IntentionalOmit);
        assert_eq!(resolve_aperture(None, ApertureKind::Mask), ApertureResolution::Legacy);
        for raw in [
            r#"{}"#,
            r#"{"mask":null}"#,
            r#"{"mask":null,"paste":null,"mask":null}"#,
            r#"{"mask":null,"paste":null,"zone":{}}"#,
            r#"{"mask":{"width":1,"height":1,"zone":{}},"paste":null}"#,
        ] {
            assert!(serde_json::from_str::<PadApertures>(raw).is_err(), "{raw}");
        }
        let schema = serde_json::to_value(schemars::schema_for!(PadApertures)).unwrap();
        assert_eq!(schema["required"], serde_json::json!(["mask", "paste"]));
        assert!(schema["properties"]["mask"].to_string().contains("null"));
    }
    #[test]
    fn exact_doubled_extents_and_checked_margin_preserve_odd_dimensions() {
        let odd = ApertureRect { width: Nm(3), height: Nm(5) };
        assert!(odd.validate_at(Point::ORIGIN).is_ok());
        assert!(odd.validate_at(Point { x: Nm(MAX_LOCAL_NM - 1), y: Nm(0) }).is_err());
        assert!(odd.validate_at(Point { x: Nm(MAX_LOCAL_NM - 2), y: Nm(0) }).is_ok());
        assert!(odd.validate_at(Point { x: Nm(i64::MIN), y: Nm(0) }).is_err());
        assert_eq!(ApertureRect::equal_margin(Nm(5), Nm(7), Nm(-1)).unwrap(), odd);
        for margin in [Nm(-3), Nm(i64::MAX), Nm(i64::MIN)] {
            assert!(ApertureRect::equal_margin(Nm(5), Nm(7), margin).is_err());
        }
    }
}
