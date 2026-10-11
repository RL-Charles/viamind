//! Persistent surface-mount pads in footprint-local coordinates.
use crate::ObjectId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use viamind_geom::{Nm, Point};

/// A rectangular SMD pad. Net names are case-sensitive; None means unassigned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pad {
    pub id: ObjectId,
    pub number: String,
    pub position: Point,
    pub width: Nm,
    pub height: Nm,
    pub net: Option<String>,
    /// None preserves legacy openings; Some contains explicit nullable mask/paste shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apertures: Option<crate::PadApertures>,
}

/// A validated ordered pad list; order is preserved across save/load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "Vec<Pad>", into = "Vec<Pad>")]
pub struct Pads(pub Vec<Pad>);
impl TryFrom<Vec<Pad>> for Pads {
    type Error = String;
    fn try_from(pads: Vec<Pad>) -> Result<Self, String> {
        if pads.len() > 1024 {
            return Err("a part supports at most 1024 pads".into());
        }
        let mut ids = BTreeSet::new();
        let mut numbers = BTreeSet::new();
        for p in &pads {
            if let Some(apertures) = &p.apertures {
                apertures.validate_at(p.position)?;
            }
            if !ids.insert(p.id) || !numbers.insert(&p.number) {
                return Err("pad IDs and numbers must be unique within a part".into());
            }
            if !valid_name(&p.number) || p.net.as_ref().is_some_and(|n| !valid_name(n)) {
                return Err("pad numbers and net names must be trimmed, nonempty, at most 128 characters and contain no control characters".into());
            }
            if [p.position.x.0, p.position.y.0].iter().any(|n| n.unsigned_abs() > 10_000_000_000)
                || [p.width.0, p.height.0].iter().any(|n| *n <= 0 || *n > 10_000_000_000)
            {
                return Err("pad coordinates must be within ±10000 mm and dimensions greater than zero and at most 10000 mm".into());
            }
        }
        Ok(Self(pads))
    }
}
impl From<Pads> for Vec<Pad> {
    fn from(pads: Pads) -> Self {
        pads.0
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.trim() == name && name.chars().count() <= 128 && !name.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pad() -> Pad {
        Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: Point::ORIGIN,
            width: Nm(1),
            height: Nm(1),
            net: None,
            aper,
            apertures: None,
            tures: None,
        }
    }
    #[test]
    fn bounded_sizes_names_counts_and_duplicate_identities_are_validated() {
        let p = pad();
        assert!(Pads::try_from(vec![]).is_ok());
        assert!(Pads::try_from(vec![p.clone()]).is_ok());
        for alter in [
            |p: &mut Pad| p.width = Nm(0),
            |p: &mut Pad| p.height = Nm(-1),
            |p: &mut Pad| p.position.x = Nm(i64::MIN),
            |p: &mut Pad| p.net = Some(" GND".into()),
            |p: &mut Pad| p.number = "\n".into(),
            |p: &mut Pad| p.number = "a".repeat(129),
        ] {
            let mut bad = p.clone();
            alter(&mut bad);
            assert!(Pads::try_from(vec![bad]).is_err());
        }
        let mut other = p.clone();
        other.number = "2".into();
        assert!(Pads::try_from(vec![p.clone(), other]).is_err());
        assert!(Pads::try_from(vec![p; 1025]).is_err());
    }
}
