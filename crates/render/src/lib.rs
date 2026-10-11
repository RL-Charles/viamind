//! Viamind canvas rendering of schematics and boards

pub mod svg;
pub use svg::{BoardSvg, SvgMetadata, SvgOptions, ViewBounds, ViewLayer, board_svg, write_svg};

pub mod png;
pub use png::{BoardPng, MAX_PNG_BYTES, MAX_PNG_PIXELS, board_png, validate_png_options, write_png};
