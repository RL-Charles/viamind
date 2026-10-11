//! Shared read-only view requests; cursor is supplied by the engine-facing caller.
use crate::{SchematicViewOptions, SvgOptions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use viamind_doc::ObjectId;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, bound(deserialize = "C: Deserialize<'de>"))]
pub struct BoardViewRequest<C> {
    #[serde(default)]
    pub options: SvgOptions,
    /// Optional NEW file destination; existing destinations are always preserved.
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub expected_cursor: Option<C>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, bound(deserialize = "C: Deserialize<'de>"))]
pub struct SchematicViewRequest<C> {
    pub sheet: ObjectId,
    #[serde(default)]
    pub options: SchematicViewOptions,
    /// Optional NEW file destination; existing destinations are always preserved.
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub expected_cursor: Option<C>,
}
