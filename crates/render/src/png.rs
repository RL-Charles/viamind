//! Bounded PNG rasterization of the same supported board SVG geometry.
use crate::{SvgMetadata, SvgOptions};
use resvg::{tiny_skia, usvg};
use std::{
    io,
    path::{Path, PathBuf},
};
use viamind_doc::Project;

pub const MAX_PNG_PIXELS: u64 = 4_194_304;
pub const MAX_PNG_BYTES: usize = 8 * 1024 * 1024;
#[derive(Debug)]
pub struct BoardPng {
    pub png: Vec<u8>,
    pub metadata: SvgMetadata,
}
pub fn validate_png_options(options: &SvgOptions) -> io::Result<()> {
    options.validate()?;
    if u64::from(options.width_px) * u64::from(options.height_px) > MAX_PNG_PIXELS {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "PNG viewport must not exceed 4194304 pixels"));
    }
    Ok(())
}
/// Render trusted internally generated geometry, without fonts or external resources.
pub fn board_png(project: &Project, options: &SvgOptions) -> io::Result<BoardPng> {
    validate_png_options(options)?;
    let svg = crate::svg::board_svg_for_pixels(project, options)?;
    let parsing = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver { resolve_data: Box::new(|_, _, _| None), resolve_string: Box::new(|_, _| None) },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg.svg, &parsing).map_err(io::Error::other)?;
    let mut pixels =
        tiny_skia::Pixmap::new(options.width_px, options.height_px).ok_or_else(|| io::Error::other("could not allocate bounded PNG viewport"))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixels.as_mut());
    let png = pixels.encode_png().map_err(io::Error::other)?;
    if png.len() > MAX_PNG_BYTES {
        return Err(io::Error::other("encoded PNG exceeds 8 MiB"));
    }
    Ok(BoardPng { png, metadata: svg.metadata })
}
/// Publish a NEW PNG file atomically with the same protection as SVG output.
pub fn write_png(png: &[u8], path: &Path, native: Option<&Path>) -> io::Result<PathBuf> {
    crate::svg::write_view_bytes(png, path, native, MAX_PNG_BYTES, "PNG", ".viamind-png-")
}
