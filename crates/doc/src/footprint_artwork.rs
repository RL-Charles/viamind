//! Explicit mounted-side noncopper footprint geometry, in local integer nanometres.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::fmt;
use viamind_geom::{Nm, Point};

pub const MAX_SILK_SEGMENTS: usize = 256;
const MAX_COORD: i128 = 10_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SilkscreenSegment {
    #[serde(deserialize_with = "point")]
    pub start: Point,
    #[serde(deserialize_with = "point")]
    pub end: Point,
    pub width: Nm,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RectangularCourtyard {
    #[serde(deserialize_with = "point")]
    pub min: Point,
    #[serde(deserialize_with = "point")]
    pub max: Point,
    pub width: Nm,
}
impl RectangularCourtyard {
    pub fn corners(&self) -> [Point; 4] {
        [self.min, Point::new(self.max.x, self.min.y), self.max, Point::new(self.min.x, self.max.y)]
    }
}

/// Presence is meaningful: an empty object intentionally suppresses legacy artwork.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintArtwork {
    pub silkscreen: Vec<SilkscreenSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courtyard: Option<RectangularCourtyard>,
}
impl FootprintArtwork {
    /// Bound borrowed rows before visiting geometry or allocating copies.
    pub fn validate(&self) -> Result<(), String> {
        if self.silkscreen.len() > MAX_SILK_SEGMENTS {
            return Err("footprint artwork supports at most256 silkscreen segments".into());
        }
        let stroke = |points: &[Point], width: Nm| -> Result<(), String> {
            let width = i128::from(width.0);
            if width <= 0 || width > MAX_COORD {
                return Err("artwork stroke width must be positive and at most10000mm".into());
            }
            for point in points {
                for n in [point.x.0, point.y.0] {
                    if i128::from(n).abs() * 2 + width > MAX_COORD * 2 {
                        return Err("stroked footprint artwork must stay within +/-10000mm locally".into());
                    }
                }
            }
            Ok(())
        };
        for line in &self.silkscreen {
            if line.start == line.end {
                return Err("silkscreen segment endpoints must differ".into());
            }
            stroke(&[line.start, line.end], line.width)?;
        }
        if let Some(courtyard) = &self.courtyard {
            if courtyard.min.x >= courtyard.max.x || courtyard.min.y >= courtyard.max.y {
                return Err("rectangular courtyard requires min < max on both axes".into());
            }
            stroke(&[courtyard.min, courtyard.max], courtyard.width)?;
        }
        Ok(())
    }

    pub fn structural_rows(&self) -> usize {
        self.silkscreen.len().saturating_add(usize::from(self.courtyard.is_some()))
    }
    pub fn primitive_count(&self) -> usize {
        self.silkscreen.len().saturating_add(usize::from(self.courtyard.is_some()) * 4)
    }
}

fn segments<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<SilkscreenSegment>, D::Error> {
    struct Rows;
    struct Excess;
    impl<'de> de::DeserializeSeed<'de> for Excess {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
            Err(de::Error::custom("footprint artwork supports at most256 silkscreen segments before decoding extra rows"))
        }
    }
    impl<'de> de::Visitor<'de> for Rows {
        type Value = Vec<SilkscreenSegment>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most256 silkscreen segments")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while rows.len() < MAX_SILK_SEGMENTS {
                let Some(row) = seq.next_element()? else { return Ok(rows) };
                rows.push(row);
            }
            seq.next_element_seed(Excess)?;
            Ok(rows)
        }
    }
    deserializer.deserialize_seq(Rows)
}
impl<'de> Deserialize<'de> for FootprintArtwork {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            #[serde(default, deserialize_with = "segments")]
            silkscreen: Vec<SilkscreenSegment>,
            #[serde(default)]
            courtyard: Option<RectangularCourtyard>,
        }
        let input = Input::deserialize(deserializer)?;
        let artwork = Self { silkscreen: input.silkscreen, courtyard: input.courtyard };
        artwork.validate().map_err(de::Error::custom)?;
        Ok(artwork)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn odd_stroke_extents_and_excess_before_poison() {
        let mut art = FootprintArtwork {
            silkscreen: vec![SilkscreenSegment { start: Point::new(Nm(-3), Nm(5)), end: Point::new(Nm(7), Nm(11)), width: Nm(5) }],
            courtyard: Some(RectangularCourtyard { min: Point::new(Nm(-13), Nm(-17)), max: Point::new(Nm(19), Nm(23)), width: Nm(3) }),
        };
        assert!(art.validate().is_ok());
        art.silkscreen[0].end.x = Nm(9_999_999_998);
        assert!(art.validate().unwrap_err().contains("stroked"));
        art.silkscreen[0].end.x = Nm(9_999_999_997);
        assert!(art.validate().is_ok());
        let line = serde_json::to_value(&art.silkscreen[0]).unwrap();
        let mut rows = vec![line; 256];
        rows.push(serde_json::json!({"poison": "not geometry"}));
        let error = serde_json::from_value::<FootprintArtwork>(serde_json::json!({"silkscreen":rows})).unwrap_err();
        assert!(error.to_string().contains("at most256"), "{error}");
        assert_eq!(serde_json::to_string(&FootprintArtwork::default()).unwrap(), "{\"silkscreen\":[]}");
    }
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
