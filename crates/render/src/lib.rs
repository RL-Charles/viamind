//! Viamind canvas rendering of schematics and boards

pub mod svg;
pub use svg::{BoardSvg, SvgMetadata, SvgOptions, ViewBounds, ViewLayer, board_svg, write_svg};

pub mod png;
pub use png::{BoardPng, MAX_PNG_BYTES, MAX_PNG_PIXELS, board_png, validate_png_options, write_png};

pub use svg::{MAX_BOARD_VIEW_TEXT, board_view_snapshot};

pub mod schematic;
pub mod schematic_labels;
pub use schematic::{
    SchematicLayer, SchematicMetadata, SchematicPng, SchematicSnapshot, SchematicSvg, SchematicViewOptions, schematic_png, schematic_svg,
    schematic_view_snapshot,
};

pub mod requests;
pub use requests::{BoardViewRequest, SchematicViewRequest};
