//! Bounded exact named-net minima. Request baselines can strengthen these floors.
use std::{collections::BTreeMap, fmt};

use schemars::JsonSchema;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, Visitor},
};
use viamind_geom::Nm;

pub const MAX_NAMED_NET_RULES: usize = 128;
pub const MAX_NAMED_NET_RULE_TEXT: usize = 32_768;
const MAX_MINIMUM_NM: i64 = 100_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedNetMinima {
    pub minimum_copper_clearance: Nm,
    pub minimum_track_width: Nm,
}
pub type NamedNetRules = BTreeMap<String, NamedNetMinima>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetRuleBudget {
    pub rows: usize,
    pub text_bytes: usize,
}
impl NamedNetMinima {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(0..=MAX_MINIMUM_NM).contains(&self.minimum_copper_clearance.0) {
            return Err("minimum_copper_clearance must be within 0..100000000 Nm (100mm)");
        }
        if !(1..=MAX_MINIMUM_NM).contains(&self.minimum_track_width.0) {
            return Err("minimum_track_width must be within 1..100000000 Nm (100mm)");
        }
        Ok(())
    }
}
pub fn validate_net_rule_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > 256 || name.trim() != name || name.chars().count() > 128 || name.chars().any(char::is_control) {
        return Err("rule net must be trimmed, nonempty, control-free and within 128 characters/256 UTF-8 bytes");
    }
    Ok(())
}
/// Validate borrowed counts/labels/scalars before ANY owned snapshot or index.
pub fn named_net_rule_budget(rules: &NamedNetRules) -> Result<NetRuleBudget, &'static str> {
    if rules.len() > MAX_NAMED_NET_RULES {
        return Err("named-net rule count exceeds 128");
    }
    let mut result = NetRuleBudget { rows: rules.len(), text_bytes: 0 };
    for (name, minima) in rules {
        validate_net_rule_name(name)?;
        minima.validate()?;
        result.text_bytes = result.text_bytes.saturating_add(name.len());
        if result.text_bytes > MAX_NAMED_NET_RULE_TEXT {
            return Err("named-net rule names exceed 32768 aggregate UTF-8 bytes");
        }
    }
    Ok(result)
}
/// Caller validates the bounded map first. Same-net contact policy is the checker's concern.
pub fn named_copper_clearance(rules: &NamedNetRules, baseline: Nm, a: Option<&str>, b: Option<&str>) -> Nm {
    [a, b].into_iter().flatten().filter_map(|net| rules.get(net)).fold(baseline, |value, rule| value.max(rule.minimum_copper_clearance))
}
pub fn maximum_named_clearance(rules: &NamedNetRules, baseline: Nm) -> Nm {
    rules.values().fold(baseline, |value, rule| value.max(rule.minimum_copper_clearance))
}
/// No match contributes zero: existing positive track widths keep their old behavior.
pub fn named_track_width(rules: &NamedNetRules, net: Option<&str>) -> Nm {
    net.and_then(|net| rules.get(net)).map_or(Nm(0), |rule| rule.minimum_track_width)
}

/// Parse original JSON map tokens: duplicate keys must never silently overwrite.
pub fn deserialize_named_net_rules<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NamedNetRules, D::Error> {
    struct RuleName(String);
    impl<'de> Deserialize<'de> for RuleName {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct NameVisitor;
            impl Visitor<'_> for NameVisitor {
                type Value = RuleName;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a bounded exact net name")
                }
                fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                    validate_net_rule_name(value).map_err(E::custom)?;
                    Ok(RuleName(value.to_owned()))
                }
            }
            deserializer.deserialize_str(NameVisitor)
        }
    }
    struct RulesVisitor;
    impl<'de> Visitor<'de> for RulesVisitor {
        type Value = NamedNetRules;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded exact named-net minima map")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut rules = NamedNetRules::new();
            let mut bytes = 0_usize;
            while let Some(RuleName(name)) = map.next_key::<RuleName>()? {
                if rules.len() == MAX_NAMED_NET_RULES {
                    return Err(de::Error::custom("named-net rule count exceeds 128"));
                }
                validate_net_rule_name(&name).map_err(de::Error::custom)?;
                if rules.contains_key(&name) {
                    return Err(de::Error::custom("duplicate exact named-net rule key"));
                }
                bytes = bytes.saturating_add(name.len());
                if bytes > MAX_NAMED_NET_RULE_TEXT {
                    return Err(de::Error::custom("named-net rule names exceed 32768 aggregate UTF-8 bytes"));
                }
                let minima: NamedNetMinima = map.next_value()?;
                minima.validate().map_err(de::Error::custom)?;
                rules.insert(name, minima);
            }
            Ok(rules)
        }
    }
    deserializer.deserialize_map(RulesVisitor)
}
