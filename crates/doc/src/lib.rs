//! Viamind design model: projects, schematic sheets, boards and rules.
//!
//! The model is plain data. Every object that can be referenced carries a stable
//! [`ObjectId`], collections are [`BTreeMap`](std::collections::BTreeMap)s keyed by
//! that id so iteration and serialisation order are deterministic, and every type
//! is serde-serialisable. Changes go through `viamind-engine` commands, never by
//! writing to these fields directly from UI or agent code.

pub mod board;
pub mod id;
pub mod project;

pub use board::{Board, Part, Side};
pub use id::ObjectId;
pub use project::{Project, Schematic, Sheet};
