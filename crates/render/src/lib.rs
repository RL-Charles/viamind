//! Viamind canvas rendering of schematics and boards

pub mod svg;
pub use svg::{BoardSvg, SvgMetadata, SvgOptions, ViewBounds, ViewLayer, board_svg, write_svg};
