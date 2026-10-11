//! Immutable queries over a consistent design snapshot.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Part, Project, Side};
use viamind_geom::Rect;

use crate::{CommandError, commands::position_mm};

const fn default_limit() -> usize {
    100
}

/// Inclusive search rectangle for part origins, in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartQueryBounds {
    /// Minimum X coordinate in millimetres.
    pub min_x_mm: f64,
    /// Minimum Y coordinate in millimetres.
    pub min_y_mm: f64,
    /// Maximum X coordinate in millimetres.
    pub max_x_mm: f64,
    /// Maximum Y coordinate in millimetres.
    pub max_y_mm: f64,
}

impl PartQueryBounds {
    fn rect(&self) -> Result<Rect, CommandError> {
        let min = position_mm("bounds.min_x_mm", self.min_x_mm, "bounds.min_y_mm", self.min_y_mm)?;
        let max = position_mm("bounds.max_x_mm", self.max_x_mm, "bounds.max_y_mm", self.max_y_mm)?;
        if self.min_x_mm > self.max_x_mm || self.min_y_mm > self.max_y_mm {
            return Err(CommandError::invalid("bounds", "minimum coordinates must be less than or equal to maximum coordinates"));
        }
        Ok(Rect::new(min, max))
    }
}

/// Filters and pagination for board parts. All supplied filters combine with AND.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartQuery {
    /// Restrict to these UUIDs. Empty list matches nothing; omitted matches any ID.
    pub ids: Option<Vec<ObjectId>>,
    /// Exact, case-sensitive reference designator.
    pub designator: Option<String>,
    /// Exact, case-sensitive component value.
    pub value: Option<String>,
    /// Exact, case-sensitive footprint name.
    pub footprint: Option<String>,
    /// Case-insensitive substring in designator, value or footprint. Empty matches all.
    pub text: Option<String>,
    /// Restrict to the top or bottom board side.
    pub side: Option<Side>,
    /// Inclusive rectangle containing the part origin (not its footprint extents).
    pub bounds: Option<PartQueryBounds>,
    /// Maximum returned parts: 1–1000, default 100.
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: usize,
    /// Exclusive UUID cursor from the preceding page's next_after; repeat the filters.
    pub after: Option<ObjectId>,
}

impl Default for PartQuery {
    fn default() -> Self {
        Self { ids: None, designator: None, value: None, footprint: None, text: None, side: None, bounds: None, limit: default_limit(), after: None }
    }
}

/// A bounded page of parts in UUID order. Coordinates remain integer nanometres.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartQueryResult {
    /// Full records of the returned parts.
    pub parts: Vec<Part>,
    /// Number of matches before pagination (ignores the cursor and limit).
    pub total: usize,
    /// Last returned UUID when more matches remain, otherwise None.
    pub next_after: Option<ObjectId>,
}

impl PartQuery {
    /// Queries a consistent snapshot without changing the project or session state.
    pub fn run(&self, project: &Project) -> Result<PartQueryResult, CommandError> {
        if !(1..=1000).contains(&self.limit) {
            return Err(CommandError::invalid("limit", "must be between 1 and 1000"));
        }
        let bounds = self.bounds.as_ref().map(PartQueryBounds::rect).transpose()?;
        let ids: Option<BTreeSet<_>> = self.ids.as_ref().map(|ids| ids.iter().copied().collect());
        let text = self.text.as_ref().map(|text| text.to_lowercase());
        let mut page = Vec::new();
        let mut result = PartQueryResult { parts: Vec::new(), total: 0, next_after: None };
        for part in project.board.parts.values() {
            if ids.as_ref().is_some_and(|ids| !ids.contains(&part.id))
                || self.designator.as_ref().is_some_and(|value| value != &part.designator)
                || self.value.as_ref().is_some_and(|value| value != &part.value)
                || self.footprint.as_ref().is_some_and(|value| value != &part.footprint)
                || self.side.is_some_and(|side| side != part.side)
                || bounds.is_some_and(|rect| !rect.contains(part.position))
                || text
                    .as_ref()
                    .is_some_and(|text| ![&part.designator, &part.value, &part.footprint].iter().any(|field| field.to_lowercase().contains(text)))
            {
                continue;
            }
            result.total += 1;
            if self.after.is_some_and(|after| part.id <= after) {
                continue;
            }
            if page.len() < self.limit {
                page.push(part);
            } else {
                result.next_after = page.last().map(|part| part.id);
            }
        }
        viamind_doc::assembly::validate_metadata(page.iter().filter_map(|p| p.assembly.as_deref()))
            .map_err(|e| CommandError::invalid("assembly", e))?;
        result.parts = page.into_iter().cloned().collect();
        Ok(result)
    }
}
