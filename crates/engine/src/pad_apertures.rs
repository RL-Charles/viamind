//! Exact guarded replacement of one existing pad's explicit opening policy.
use crate::{Command, CommandError, Delta, EventCursor};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;
use std::{borrow::Cow, fmt};
use viamind_doc::{ObjectId, PadApertures, Part, Project};

/// Initial JSON ownership with duplicate-key refusal retained at every raw object.
/// A caller that already supplied Value has already lost duplicate-token information.
pub(crate) struct StrictValue(pub Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::DeserializeSeed;
        struct Seed(usize);
        impl<'de> DeserializeSeed<'de> for Seed {
            type Value = Value;
            fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
                if self.0 > 64 {
                    return Err(de::Error::custom("JSON depth exceeds64 before typed decoding"));
                }
                struct V(usize);
                impl<'de> de::Visitor<'de> for V {
                    type Value = Value;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("JSON without duplicate object keys")
                    }
                    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
                        Ok(Value::Null)
                    }
                    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
                        Ok(Value::Null)
                    }
                    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
                        Ok(Value::Bool(v))
                    }
                    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
                        Ok(Value::Number(v.into()))
                    }
                    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
                        Ok(Value::Number(v.into()))
                    }
                    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
                        serde_json::Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("nonfinite JSON number"))
                    }
                    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
                        Ok(Value::String(v.into()))
                    }
                    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
                        Ok(Value::String(v))
                    }
                    fn visit_seq<A: de::SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
                        let mut out = Vec::new();
                        while let Some(v) = a.next_element_seed(Seed(self.0 + 1))? {
                            out.push(v);
                        }
                        Ok(Value::Array(out))
                    }
                    fn visit_map<M: de::MapAccess<'de>>(self, mut m: M) -> Result<Value, M::Error> {
                        let mut out = serde_json::Map::new();
                        while let Some(k) = m.next_key::<String>()? {
                            if out.contains_key(&k) {
                                return Err(de::Error::custom(format!("duplicate JSON field {k}")));
                            }
                            let v = m.next_value_seed(Seed(self.0 + 1))?;
                            out.insert(k, v);
                        }
                        Ok(Value::Object(out))
                    }
                }
                d.deserialize_any(V(self.0))
            }
        }
        Seed(0).deserialize(d).map(Self)
    }
}

/// Admission before secondary typed decoding or a serializer's owned pad copy.
pub(crate) fn preflight_part(value: &Value) -> Result<(), String> {
    crate::part_holes::preflight(value)?;
    if let Some(pads) = value.get("pads").and_then(Value::as_array) {
        if pads.len() > 1024 {
            return Err("expected part supports at most1024 pads before typed decoding".into());
        }
        for pad in pads {
            for key in ["number", "net"] {
                if pad.get(key).and_then(Value::as_str).is_some_and(|s| s.len() > 512) {
                    return Err("pad labels support at most512 UTF-8 bytes before typed decoding".into());
                }
            }
        }
    }
    if value.get("artwork").and_then(|a| a.get("silkscreen")).and_then(Value::as_array).is_some_and(|a| a.len() > 256) {
        return Err("at most256 silkscreen segments before typed decoding".into());
    }
    for key in ["designator", "value", "footprint"] {
        if value.get(key).and_then(Value::as_str).is_some_and(|s| s.len() > 4096) {
            return Err("part labels support at most4096 UTF-8 bytes before typed decoding".into());
        }
    }
    if let Some(assembly) = value.get("assembly") {
        for key in ["manufacturer", "mpn", "supplier", "supplier_sku"] {
            if assembly.get(key).and_then(Value::as_str).is_some_and(|s| s.len() > 512) {
                return Err("sourcing fields support at most512 UTF-8 bytes before typed decoding".into());
            }
        }
    }
    bounded_bytes(value)
}
pub(crate) fn bounded_bytes(value: &impl Serialize) -> Result<(), String> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(b.len());
            if self.0 > 2 * 1024 * 1024 {
                return Err(std::io::Error::other("pad aperture command exceeds2MiB serialized budget"));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter(0), value).map_err(|e| e.to_string())
}
pub(crate) fn validate_part(part: &Part) -> Result<(), CommandError> {
    if let Some(holes) = &part.holes {
        viamind_doc::footprint_holes::validate_part_holes(holes).map_err(|e| CommandError::invalid("holes", e))?;
        for hole in holes {
            viamind_doc::resolve_hole(part, hole).map_err(|e| CommandError::invalid("holes", e))?;
        }
    }
    crate::footprint_artwork::validate_part_artwork(part)?;
    if part.pads.as_ref().is_some_and(|p| p.0.len() > 1024) {
        return Err(CommandError::invalid("expected", "at most1024 pads before copies"));
    }
    for s in [&part.designator, &part.value, &part.footprint] {
        if s.len() > 4096 {
            return Err(CommandError::invalid("expected", "part labels exceed4096 bytes"));
        }
    }
    if let Some(a) = &part.assembly {
        a.validate().map_err(|e| CommandError::invalid("assembly", e))?;
    }
    if let Some(pads) = &part.pads {
        for pad in &pads.0 {
            if pad.number.len() > 512 || pad.net.as_ref().is_some_and(|s| s.len() > 512) {
                return Err(CommandError::invalid("expected.pads", "pad labels exceed512 bytes"));
            }
            if let Some(a) = &pad.apertures {
                a.validate_at(pad.position).map_err(|e| CommandError::invalid("apertures", e))?;
            }
        }
    }
    bounded_bytes(part).map_err(|e| CommandError::invalid("expected", e))
}

#[derive(Clone, Debug, Serialize)]
pub struct SetPadApertures {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub expected: Part,
    pub pad_id: ObjectId,
    pub apertures: Option<PadApertures>,
}
impl JsonSchema for SetPadApertures {
    fn schema_name() -> Cow<'static, str> {
        "SetPadApertures".into()
    }
    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"object","additionalProperties":false,
            "required":["expected_project","expected_cursor","expected","pad_id","apertures"],
            "properties":{"expected_project":g.subschema_for::<ObjectId>(),"expected_cursor":g.subschema_for::<EventCursor>(),
              "expected":g.subschema_for::<Part>(),"pad_id":g.subschema_for::<ObjectId>(),"apertures":g.subschema_for::<Option<PadApertures>>()}})
    }
}
impl<'de> Deserialize<'de> for SetPadApertures {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = SetPadApertures;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a guarded pad edit with present nullable apertures")
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
                let (mut project, mut cursor, mut expected, mut pad, mut apertures) = (None, None, None, None, None);
                while let Some(k) = m.next_key::<String>()? {
                    match k.as_str() {
                        "expected_project" => {
                            if project.is_some() {
                                return Err(de::Error::duplicate_field("expected_project"));
                            }
                            project = Some(m.next_value()?);
                        }
                        "expected_cursor" => {
                            if cursor.is_some() {
                                return Err(de::Error::duplicate_field("expected_cursor"));
                            }
                            cursor = Some(m.next_value()?);
                        }
                        "expected" => {
                            if expected.is_some() {
                                return Err(de::Error::duplicate_field("expected"));
                            }
                            let v = m.next_value::<StrictValue>()?.0;
                            preflight_part(&v).map_err(de::Error::custom)?;
                            expected = Some(serde_json::from_value(v).map_err(de::Error::custom)?);
                        }
                        "pad_id" => {
                            if pad.is_some() {
                                return Err(de::Error::duplicate_field("pad_id"));
                            }
                            pad = Some(m.next_value()?);
                        }
                        "apertures" => {
                            if apertures.is_some() {
                                return Err(de::Error::duplicate_field("apertures"));
                            }
                            apertures = Some(m.next_value::<Option<PadApertures>>()?);
                        }
                        _ => return Err(de::Error::unknown_field(&k, &["expected_project", "expected_cursor", "expected", "pad_id", "apertures"])),
                    }
                }
                Ok(SetPadApertures {
                    expected_project: project.ok_or_else(|| de::Error::missing_field("expected_project"))?,
                    expected_cursor: cursor.ok_or_else(|| de::Error::missing_field("expected_cursor"))?,
                    expected: expected.ok_or_else(|| de::Error::missing_field("expected"))?,
                    pad_id: pad.ok_or_else(|| de::Error::missing_field("pad_id"))?,
                    apertures: apertures.ok_or_else(|| de::Error::missing_field("apertures"))?,
                })
            }
        }
        d.deserialize_map(Visitor)
    }
}
impl Command for SetPadApertures {
    const NAME: &'static str = "set_pad_apertures";
    const DESCRIPTION: &'static str = "Set one existing rectangular SMD pad's explicit centered absolute mask/paste openings in integer local Nm. Requires project, full initial cursor and exact Part snapshot. Present null restores legacy policy; object requires both nullable mask/paste members, null intentionally omits that opening. One undoable Part delta or no-op; IDs/nets/copper/pose unchanged. Native feature14. No inferred manufacturing defaults or certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, p: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if p.id != self.expected_project {
            return Err(CommandError::conflict("project changed; reload pad apertures"));
        }
        validate_part(&self.expected)?;
        let current = p.board.part(&self.expected.id).ok_or_else(|| CommandError::not_found("part", self.expected.id))?;
        if current != &self.expected {
            return Err(CommandError::conflict("part changed; reload pad apertures"));
        }
        let index = current
            .pads
            .as_ref()
            .and_then(|pads| pads.0.iter().position(|pad| pad.id == self.pad_id))
            .ok_or_else(|| CommandError::not_found("pad", self.pad_id))?;
        let pad = &current.pads.as_ref().unwrap().0[index];
        if let Some(a) = &self.apertures {
            a.validate_at(pad.position).map_err(|e| CommandError::invalid("apertures", e))?;
        }
        if pad.apertures == self.apertures {
            return Ok(vec![]);
        }
        let mut after = current.clone();
        after.pads.as_mut().unwrap().0[index].apertures = self.apertures;
        Ok(vec![Delta::record_part(p, after.id, Some(after))])
    }
}
