//! Command-line validation before any window, MCP endpoint or output file is opened.

use std::{collections::BTreeSet, ffi::OsString, path::PathBuf};

pub const HELP: &str = "Viamind — agent-driven PCB design

Usage: viamind [--open PATH] [--no-mcp]
       viamind --headless [--open PATH] [--save-as PATH]
       viamind --export-csv bom|placement --open PATH --output PATH [--overwrite]
       viamind --export-gerber --open PATH --output NEW_DIRECTORY [--mask-expansion-mm N]
       viamind --export-svg --open PATH --output NEW_FILE [--view-layers top_copper,bottom_copper,outline]
       viamind --demo

  --headless       Serve full design MCP over stdin/stdout without a window
  --open PATH      Load an existing native .viamind.json project
  --save-as PATH   Save the final headless design after the client closes stdin
  --no-mcp         Open the desktop without the HTTP MCP endpoint
  --export-csv KIND Export generic BOM or component-origin placement CSV and exit
  --export-gerber   Export scoped outer Gerber X2 and through-via XNC drill and exit
  --mask-expansion-mm N  Mask expansion for fabrication export (0–10 mm, default 0)
  --output PATH    Export destination (SVG needs a NEW file; fabrication a NEW directory)
  --overwrite      Allow replacing the CSV destination
  --export-svg     Export a read-only board SVG without a window
  --view-layers LIST Comma-separated top_copper,bottom_copper,outline (default all)
  --view-bounds MM  Explicit min_x,min_y,max_x,max_y in model-space mm (default auto-fit)
  --view-size PX    Viewport width,height in pixels, each 1–4096 (default 1600,1000)
  --view-padding-mm N Auto-fit padding 0–1000 mm (default 1); ignored with explicit bounds
  --demo           Open the synthetic rendering demo
  --help           Show this help
  --version        Show the build version

The desktop HTTP endpoint defaults to http://127.0.0.1:7878/mcp.
VIAMIND_MCP_PORT sets its port. Headless mode does not use a TCP port.
";

#[derive(Debug, Default)]
pub struct Options {
    pub headless: bool,
    pub demo: bool,
    pub no_mcp: bool,
    pub open: Option<PathBuf>,
    pub save_as: Option<PathBuf>,
    pub export_svg: bool,
    pub svg_options: viamind_render::SvgOptions,
    pub export_csv: Option<viamind_fab::AssemblyCsv>,
    pub output: Option<PathBuf>,
    pub overwrite: bool,
    pub export_gerber: bool,
    pub mask_expansion_mm: Option<f64>,
}

pub enum Invocation {
    Help,
    Version,
    Run(Options),
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut args = args.into_iter();
    let mut options = Options::default();
    let mut seen = BTreeSet::new();
    while let Some(arg) = args.next() {
        if !seen.insert(arg.clone()) {
            return Err(format!("duplicate option: {}", arg.to_string_lossy()));
        }
        match arg.to_str() {
            Some("--help" | "--version") => {
                if seen.len() != 1 || args.next().is_some() {
                    return Err("use --help or --version by itself".into());
                }
                return Ok(if arg == "--help" { Invocation::Help } else { Invocation::Version });
            }
            Some("--headless") => options.headless = true,
            Some("--demo") => options.demo = true,
            Some("--no-mcp") => options.no_mcp = true,
            Some("--overwrite") => options.overwrite = true,
            Some("--export-gerber") => options.export_gerber = true,
            Some("--mask-expansion-mm") => {
                options.mask_expansion_mm = Some(
                    args.next()
                        .and_then(|s| s.to_str().and_then(|s| s.parse::<f64>().ok()))
                        .filter(|v| v.is_finite() && (0.0..=10.0).contains(v))
                        .ok_or("--mask-expansion-mm needs a finite number between 0 and 10")?,
                );
            }
            Some("--export-svg") => options.export_svg = true,
            Some("--view-layers" | "--view-bounds" | "--view-size" | "--view-padding-mm") => {
                let value = args.next().and_then(|s| s.into_string().ok()).ok_or_else(|| format!("{} requires a value", arg.to_string_lossy()))?;
                match arg.to_str().unwrap() {
                    "--view-layers" => {
                        options.svg_options.layers = value
                            .split(',')
                            .map(|s| match s {
                                "top_copper" => Ok(viamind_render::ViewLayer::TopCopper),
                                "bottom_copper" => Ok(viamind_render::ViewLayer::BottomCopper),
                                "outline" => Ok(viamind_render::ViewLayer::Outline),
                                _ => Err("unknown SVG layer".to_string()),
                            })
                            .collect::<Result<_, _>>()?
                    }
                    "--view-bounds" => {
                        let v = value.split(',').map(view_nm).collect::<Result<Vec<_>, _>>()?;
                        if v.len() != 4 {
                            return Err("--view-bounds requires min_x,min_y,max_x,max_y in mm".into());
                        }
                        options.svg_options.region_nm = Some(viamind_render::ViewBounds { min_x: v[0], min_y: v[1], max_x: v[2], max_y: v[3] });
                    }
                    "--view-size" => {
                        let v = value
                            .split(',')
                            .map(|s| s.parse::<u32>().map_err(|_| "--view-size requires integer width,height".to_string()))
                            .collect::<Result<Vec<_>, _>>()?;
                        if v.len() != 2 {
                            return Err("--view-size requires width,height".into());
                        }
                        options.svg_options.width_px = v[0];
                        options.svg_options.height_px = v[1];
                    }
                    _ => options.svg_options.padding_nm = view_nm(&value)?,
                }
            }
            Some("--export-csv") => {
                options.export_csv = Some(match args.next().as_deref().and_then(|arg| arg.to_str()) {
                    Some("bom") => viamind_fab::AssemblyCsv::Bom,
                    Some("placement") => viamind_fab::AssemblyCsv::Placement,
                    _ => return Err("--export-csv requires bom or placement".into()),
                });
            }
            Some("--open" | "--save-as" | "--output") => {
                let path = args
                    .next()
                    .filter(|path| !path.is_empty() && !path.to_string_lossy().starts_with("--"))
                    .ok_or_else(|| format!("{} requires a path", arg.to_string_lossy()))?;
                if arg == "--open" {
                    options.open = Some(path.into());
                } else if arg == "--output" {
                    options.output = Some(path.into());
                } else {
                    options.save_as = Some(path.into());
                }
            }
            _ => return Err(format!("unknown option: {}; use --help for usage", arg.to_string_lossy())),
        }
    }
    if options.export_gerber {
        if options.open.is_none()
            || options.output.is_none()
            || options.export_csv.is_some()
            || options.export_svg
            || options.demo
            || options.headless
            || options.no_mcp
            || options.save_as.is_some()
            || options.overwrite
        {
            return Err("--export-gerber requires --open and --output NEW_DIRECTORY, with optional --mask-expansion-mm; existing directories cannot be overwritten".into());
        }
    } else if options.mask_expansion_mm.is_some() {
        return Err("--mask-expansion-mm requires --export-gerber".into());
    }
    let has_view_options = seen.iter().any(|s| s.to_str().is_some_and(|s| s.starts_with("--view-")));
    if has_view_options && !options.export_svg {
        return Err("--view-* options require --export-svg".into());
    }
    if options.export_svg {
        if options.open.is_none() || options.output.is_none() {
            return Err("--export-svg requires --open and --output".into());
        }
        if options.export_gerber || options.export_csv.is_some() || options.overwrite || options.demo || options.headless || options.no_mcp || options.save_as.is_some() {
            return Err("--export-svg is a separate invocation with a NEW output file".into());
        }
        options.svg_options.validate().map_err(|e| e.to_string())?;
    }
    if options.export_csv.is_some() {
        if options.open.is_none() || options.output.is_none() {
            return Err("--export-csv requires --open and --output".into());
        }
        if options.demo || options.headless || options.no_mcp || options.save_as.is_some() {
            return Err("--export-csv is a separate invocation; use --open, --output and optional --overwrite".into());
        }
    } else if !options.export_gerber && !options.export_svg && (options.output.is_some() || options.overwrite) {
        return Err("--output requires an export mode; --overwrite is only supported with --export-csv".into());
    }
    if options.demo && (options.headless || options.no_mcp || options.open.is_some() || options.save_as.is_some()) {
        return Err("--demo must be used by itself".into());
    }
    if options.headless && options.no_mcp {
        return Err("--headless serves MCP; remove --no-mcp".into());
    }
    if options.save_as.is_some() && !options.headless {
        return Err("--save-as requires --headless".into());
    }
    Ok(Invocation::Run(options))
}

fn view_nm(s: &str) -> Result<i64, String> {
    let n = s.parse::<f64>().map_err(|_| "view coordinates require finite millimetres".to_string())?;
    if !n.is_finite() || n.abs() > 100_000.0 {
        return Err("view coordinates require finite millimetres within ±100000 mm".into());
    }
    Ok((n * 1_000_000.0).round() as i64)
}
