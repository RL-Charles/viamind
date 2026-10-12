//! Command-line validation before any window, MCP endpoint or output file is opened.

use std::{collections::BTreeSet, ffi::OsString, path::PathBuf};

pub const HELP: &str = "Viamind — agent-driven PCB design

Usage: viamind [--open PATH] [--no-mcp | --agent-read-only | --agent-approval]
       viamind --headless [--open PATH] [--save-as PATH | --agent-read-only | --agent-approval]
       viamind --export-csv bom|placement|sourcing_bom|catalog_bom|assembly_bom|assembly_placement --open PATH --output PATH [--overwrite]
       viamind --run-output-job JOB --open PATH --output NEW_RELEASE_DIRECTORY
       viamind --inspect-output-release DIRECTORY
       viamind --export-gerber --open PATH --output NEW_DIRECTORY [--mask-expansion-mm N] [--include-silkscreen] [--include-paste] [--include-npth]
       viamind --export-svg --open PATH --output NEW_FILE [--view-layers top_copper,bottom_copper,outline]
       viamind --export-png --open PATH --output NEW_FILE [--view-size 1600,1000]
       viamind --export-schematic-svg|--export-schematic-png --sheet UUID --open PATH --output NEW_FILE
       viamind --demo

  --headless       Serve full design MCP over stdin/stdout without a window
  --agent-approval  Agents prepare bounded PCB proposals for trusted human accept/reject
  --agent-read-only Agents can observe but cannot edit or write files; human editing stays live
  --open PATH      Load an existing native .viamind.json project
  --save-as PATH   Save the final headless design after the client closes stdin
  --no-mcp         Open the desktop without the HTTP MCP endpoint
  --export-csv KIND Export BOM, component-origin or explicit calibrated placement CSV and exit
  --run-output-job JOB Run saved manufacturing settings into a NEW release directory
  --inspect-output-release DIRECTORY Read-only bounded saved-release consistency JSON
  --export-gerber   Export scoped outer Gerber X2 and through-via XNC drill and exit
  --mask-expansion-mm N  Mask expansion for fabrication export (0–10 mm, default 0)
  --include-npth  Include explicit circular nonplated through-hole XNC with --export-gerber
  --include-silkscreen  Include stored straight-stroke Legend files with --export-gerber
  --output PATH    Export destination (SVG/PNG need a NEW file; fabrication a NEW directory)
  --overwrite      Allow replacing the CSV destination
  --export-svg     Export a read-only board SVG without a window
  --export-png     Export transparent board PNG (at most 4194304 pixels, 8 MiB)
  --export-schematic-svg Export a selected native schematic sheet with outlined text
  --export-schematic-png Export the same selected sheet as transparent PNG
  --sheet UUID     Required selected sheet for schematic export
  --schematic-layers LIST wires,symbols,pins,junctions,labels (default all)
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
    pub agent_read_only: bool,
    pub agent_approval: bool,
    pub demo: bool,
    pub no_mcp: bool,
    pub open: Option<PathBuf>,
    pub save_as: Option<PathBuf>,
    pub export_svg: bool,
    pub export_png: bool,
    pub export_schematic_svg: bool,
    pub export_schematic_png: bool,
    pub sheet: Option<viamind_doc::ObjectId>,
    pub schematic_options: viamind_render::SchematicViewOptions,
    pub svg_options: viamind_render::SvgOptions,
    pub export_csv: Option<viamind_fab::AssemblyCsv>,
    pub output: Option<PathBuf>,
    pub overwrite: bool,
    pub export_gerber: bool,
    pub include_silkscreen: bool,
    pub include_paste: bool,
    pub include_npth: bool,
    pub run_output_job: Option<PathBuf>,
    pub inspect_output_release: Option<PathBuf>,
    pub mask_expansion_mm: Option<f64>,
}

pub enum Invocation {
    Help,
    Version,
    Run(Box<Options>),
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
            Some("--agent-read-only") => options.agent_read_only = true,
            Some("--agent-approval") => options.agent_approval = true,
            Some("--demo") => options.demo = true,
            Some("--no-mcp") => options.no_mcp = true,
            Some("--overwrite") => options.overwrite = true,
            Some("--export-gerber") => options.export_gerber = true,
            Some("--include-silkscreen") => options.include_silkscreen = true,
            Some("--include-paste") => options.include_paste = true,
            Some("--include-npth") => options.include_npth = true,
            Some("--mask-expansion-mm") => {
                options.mask_expansion_mm = Some(
                    args.next()
                        .and_then(|s| s.to_str().and_then(|s| s.parse::<f64>().ok()))
                        .filter(|v| v.is_finite() && (0.0..=10.0).contains(v))
                        .ok_or("--mask-expansion-mm needs a finite number between 0 and 10")?,
                );
            }
            Some("--export-svg") => options.export_svg = true,
            Some("--export-png") => options.export_png = true,
            Some("--export-schematic-svg") => options.export_schematic_svg = true,
            Some("--export-schematic-png") => options.export_schematic_png = true,
            Some("--sheet") => {
                options.sheet = Some(
                    args.next()
                        .and_then(|s| s.into_string().ok())
                        .ok_or("--sheet requires a UUID")?
                        .parse()
                        .map_err(|_| "--sheet requires a UUID")?,
                );
            }
            Some("--schematic-layers") => {
                let value = args.next().and_then(|s| s.into_string().ok()).ok_or("--schematic-layers requires a list")?;
                options.schematic_options.layers = value
                    .split(',')
                    .map(|s| match s {
                        "buses" => Ok(viamind_render::SchematicLayer::Buses),
                        "wires" => Ok(viamind_render::SchematicLayer::Wires),
                        "symbols" => Ok(viamind_render::SchematicLayer::Symbols),
                        "pins" => Ok(viamind_render::SchematicLayer::Pins),
                        "junctions" => Ok(viamind_render::SchematicLayer::Junctions),
                        "labels" => Ok(viamind_render::SchematicLayer::Labels),
                        _ => Err("unknown schematic layer"),
                    })
                    .collect::<Result<_, _>>()?;
            }
            Some("--view-layers" | "--view-bounds" | "--view-size" | "--view-padding-mm") => {
                let value = args.next().and_then(|s| s.into_string().ok()).ok_or_else(|| format!("{} requires a value", arg.to_string_lossy()))?;
                match arg.to_str().unwrap() {
                    "--view-layers" => {
                        options.svg_options.layers = value
                            .split(',')
                            .map(|s| match s {
                                "top_mask" => Ok(viamind_render::ViewLayer::TopMask),
                                "bottom_mask" => Ok(viamind_render::ViewLayer::BottomMask),
                                "top_paste" => Ok(viamind_render::ViewLayer::TopPaste),
                                "bottom_paste" => Ok(viamind_render::ViewLayer::BottomPaste),
                                "top_copper" => Ok(viamind_render::ViewLayer::TopCopper),
                                "bottom_copper" => Ok(viamind_render::ViewLayer::BottomCopper),
                                "top_silkscreen" => Ok(viamind_render::ViewLayer::TopSilkscreen),
                                "bottom_silkscreen" => Ok(viamind_render::ViewLayer::BottomSilkscreen),
                                "top_courtyard" => Ok(viamind_render::ViewLayer::TopCourtyard),
                                "bottom_courtyard" => Ok(viamind_render::ViewLayer::BottomCourtyard),
                                "outline" => Ok(viamind_render::ViewLayer::Outline),
                                _ => Err("unknown board view layer".to_string()),
                            })
                            .collect::<Result<_, _>>()?;
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
                options.export_csv = Some(
                    args.next()
                        .as_deref()
                        .and_then(|arg| arg.to_str())
                        .and_then(viamind_fab::AssemblyCsv::from_name)
                        .ok_or("--export-csv requires bom, placement, sourcing_bom, catalog_bom, assembly_bom or assembly_placement")?,
                );
            }
            Some("--open" | "--save-as" | "--output" | "--run-output-job" | "--inspect-output-release") => {
                let path = args
                    .next()
                    .filter(|path| !path.is_empty() && !path.to_string_lossy().starts_with("--"))
                    .ok_or_else(|| format!("{} requires a path", arg.to_string_lossy()))?;
                if arg == "--inspect-output-release" {
                    options.inspect_output_release = Some(path.into());
                } else if arg == "--run-output-job" {
                    options.run_output_job = Some(path.into());
                } else if arg == "--open" {
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
    if options.include_npth && !options.export_gerber {
        return Err("--include-npth requires --export-gerber".into());
    }
    if options.include_paste && !options.export_gerber {
        return Err("--include-paste requires --export-gerber".into());
    }
    if options.include_silkscreen && !options.export_gerber {
        return Err("--include-silkscreen requires --export-gerber".into());
    }
    if options.inspect_output_release.is_some() {
        if seen.len() != 1 {
            return Err("--inspect-output-release is a separate read-only invocation; pass only its directory".into());
        }
        return Ok(Invocation::Run(Box::new(options)));
    }
    if options.agent_read_only && options.agent_approval {
        return Err("choose only one of --agent-read-only and --agent-approval".into());
    }
    if (options.agent_read_only || options.agent_approval)
        && (options.no_mcp
            || options.demo
            || options.save_as.is_some()
            || options.run_output_job.is_some()
            || options.export_svg
            || options.export_png
            || options.export_schematic_svg
            || options.export_schematic_png
            || options.export_gerber
            || options.export_csv.is_some())
    {
        return Err(format!(
            "{} requires desktop HTTP or headless stdio; remove save/export/job/demo/no-MCP options",
            if options.agent_read_only { "--agent-read-only" } else { "--agent-approval" }
        ));
    }
    if options.run_output_job.is_some() {
        if options.open.is_none()
            || options.output.is_none()
            || seen.iter().any(|flag| !["--run-output-job", "--open", "--output"].iter().any(|allowed| flag == *allowed))
        {
            return Err("--run-output-job requires only --open and --output NEW_RELEASE_DIRECTORY".into());
        }
        return Ok(Invocation::Run(Box::new(options)));
    }
    let schematic_export = options.export_schematic_svg || options.export_schematic_png;
    if schematic_export {
        if options.open.is_none() || options.output.is_none() || options.sheet.is_none() {
            return Err("schematic export requires --open, --sheet UUID and --output NEW_FILE".into());
        }
        if options.export_schematic_svg && options.export_schematic_png
            || options.export_svg
            || options.export_png
            || options.export_gerber
            || options.export_csv.is_some()
            || options.overwrite
            || options.demo
            || options.headless
            || options.no_mcp
            || options.save_as.is_some()
            || seen.contains(&OsString::from("--view-layers"))
        {
            return Err("schematic export is a separate invocation; use --schematic-layers for its layers".into());
        }
        options.schematic_options.region_nm = options.svg_options.region_nm;
        options.schematic_options.padding_nm = options.svg_options.padding_nm;
        options.schematic_options.width_px = options.svg_options.width_px;
        options.schematic_options.height_px = options.svg_options.height_px;
        options.schematic_options.validate().map_err(|e| e.to_string())?;
        return Ok(Invocation::Run(Box::new(options)));
    }
    if options.sheet.is_some() || seen.contains(&OsString::from("--schematic-layers")) {
        return Err("--sheet and --schematic-layers require schematic export".into());
    }
    if options.export_gerber {
        if options.open.is_none()
            || options.output.is_none()
            || options.export_csv.is_some()
            || options.export_svg
            || options.export_png
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
    if has_view_options && !options.export_svg && !options.export_png {
        return Err("--view-* options require --export-svg or --export-png".into());
    }
    if options.export_svg || options.export_png {
        if options.open.is_none() || options.output.is_none() {
            return Err("SVG/PNG export requires --open and --output".into());
        }
        if (options.export_svg && options.export_png)
            || options.export_gerber
            || options.export_csv.is_some()
            || options.overwrite
            || options.demo
            || options.headless
            || options.no_mcp
            || options.save_as.is_some()
        {
            return Err("SVG/PNG export is a separate invocation with a NEW output file".into());
        }
        options.svg_options.validate().map_err(|e| e.to_string())?;
        if options.export_png {
            viamind_render::validate_png_options(&options.svg_options).map_err(|e| e.to_string())?;
        }
    }
    if options.export_csv.is_some() {
        if options.open.is_none() || options.output.is_none() {
            return Err("--export-csv requires --open and --output".into());
        }
        if options.demo || options.headless || options.no_mcp || options.save_as.is_some() {
            return Err("--export-csv is a separate invocation; use --open, --output and optional --overwrite".into());
        }
    } else if !options.export_gerber && !options.export_svg && !options.export_png && (options.output.is_some() || options.overwrite) {
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
    Ok(Invocation::Run(Box::new(options)))
}

fn view_nm(s: &str) -> Result<i64, String> {
    let n = s.parse::<f64>().map_err(|_| "view coordinates require finite millimetres".to_string())?;
    if !n.is_finite() || n.abs() > 100_000.0 {
        return Err("view coordinates require finite millimetres within ±100000 mm".into());
    }
    Ok((n * 1_000_000.0).round() as i64)
}
