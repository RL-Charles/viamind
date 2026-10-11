//! Explicit user-entered board assembly instructions; no sourcing inference.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_SOURCING_COMPONENTS: usize = 100_000;
pub const MAX_SOURCING_TEXT: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssemblyMetadata {
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    pub supplier: Option<String>,
    pub supplier_sku: Option<String>,
    #[serde(default = "populated")]
    pub populate: bool,
}
const fn populated() -> bool {
    true
}
impl Default for AssemblyMetadata {
    fn default() -> Self {
        Self { manufacturer: None, mpn: None, supplier: None, supplier_sku: None, populate: true }
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
    pub fn validate(&self) -> Result<(), &'static str> {
        for s in self.strings() {
            Self::validate_field(s)?;
        }
        Ok(())
    }
    pub fn is_default(&self) -> bool {
        self.populate && self.strings().next().is_none()
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
