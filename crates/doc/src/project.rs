//! Projects and schematic sheets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::Board;
use crate::id::ObjectId;

/// A whole design: its schematic and its board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// Stable id of the project.
    pub id: ObjectId,
    /// Human-readable project name.
    pub name: String,
    /// Schematic sheets.
    pub schematic: Schematic,
    /// The physical board.
    pub board: Board,
}

impl Project {
    /// Creates an empty project (no sheets, no parts) with a fresh id.
    pub fn new(name: impl Into<String>) -> Project {
        Project { id: ObjectId::new(), name: name.into(), schematic: Schematic::default(), board: Board::default() }
    }
}

/// The schematic side of a project: a set of sheets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schematic {
    /// Sheets by id, in id order.
    pub sheets: BTreeMap<ObjectId, Sheet>,
}

impl Schematic {
    /// The sheet with `id`, if any.
    pub fn sheet(&self, id: &ObjectId) -> Option<&Sheet> {
        self.sheets.get(id)
    }
}

/// One schematic sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sheet {
    /// Stable id of the sheet.
    pub id: ObjectId,
    /// Sheet title, e.g. `"Power"`.
    pub name: String,
}

impl Sheet {
    /// Creates a sheet with a fresh id.
    pub fn new(name: impl Into<String>) -> Sheet {
        Sheet { id: ObjectId::new(), name: name.into() }
    }
}
