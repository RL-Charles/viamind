//! Viamind design model: projects, schematic sheets, boards and rules.
//!
//! The model is plain data. Every object that can be referenced carries a stable
//! [`ObjectId`], collections are [`BTreeMap`](std::collections::BTreeMap)s keyed by
//! that id so iteration and serialisation order are deterministic, and every type
//! is serde-serialisable. Changes go through `viamind-engine` commands, never by
//! writing to these fields directly from UI or agent code.

pub mod assembly;
pub mod board;
pub mod dxf;
pub mod fabrication_view;
pub mod file;
pub mod footprint;
pub mod footprint_artwork;
pub mod footprint_holes;
pub mod footprint_sexpr;
pub mod id;
pub mod net_rules;
pub mod outline;
pub mod pad;
pub mod pad_apertures;
pub mod pcb_sexpr;
pub mod project;
pub mod schematic;
pub mod schematic_bus;
pub mod symbol_library;
pub mod symbol_package;

pub use assembly::{AssemblyMetadata, AssemblyPlacement, MountingTechnology};
pub use board::{Board, Part, Side};
pub use footprint_artwork::{FootprintArtwork, RectangularCourtyard, SilkscreenSegment};
pub use footprint_holes::{FootprintHole, PartHole, ResolvedHole, resolve_hole};
pub use id::ObjectId;
pub use outline::{BoardOutline, OutlineError};
pub use pad::{Pad, Pads};
pub use pad_apertures::{ApertureKind, ApertureRect, ApertureResolution, PadApertures, resolve_aperture};
pub use project::{Project, Schematic, Sheet};
pub use schematic_bus::{SchematicBus, SchematicBusEntry};

pub mod copper;
pub use copper::{CopperLayer, MAX_COPPER_OBJECTS, Track, Via};

pub use net_rules::{
    NamedNetMinima, NamedNetRules, NetRuleBudget, maximum_named_clearance, named_copper_clearance, named_net_rule_budget, named_track_width,
};
