//! Command-line validation before any window, MCP endpoint or output file is opened.

use std::{collections::BTreeSet, ffi::OsString, path::PathBuf};

pub const HELP: &str = "Viamind — agent-driven PCB design

Usage: viamind [--open PATH] [--no-mcp]
       viamind --headless [--open PATH] [--save-as PATH]
       viamind --export-csv bom|placement --open PATH --output PATH [--overwrite]
       viamind --demo

  --headless       Serve full design MCP over stdin/stdout without a window
  --open PATH      Load an existing native .viamind.json project
  --save-as PATH   Save the final headless design after the client closes stdin
  --no-mcp         Open the desktop without the HTTP MCP endpoint
  --export-csv KIND Export generic BOM or component-origin placement CSV and exit
  --output PATH    CSV destination (existing files are refused by default)
  --overwrite      Allow replacing the CSV destination
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
    pub export_csv: Option<viamind_fab::AssemblyCsv>,
    pub output: Option<PathBuf>,
    pub overwrite: bool,
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
    if options.export_csv.is_some() {
        if options.open.is_none() || options.output.is_none() {
            return Err("--export-csv requires --open and --output".into());
        }
        if options.demo || options.headless || options.no_mcp || options.save_as.is_some() {
            return Err("--export-csv is a separate invocation; use --open, --output and optional --overwrite".into());
        }
    } else if options.output.is_some() || options.overwrite {
        return Err("--output and --overwrite require --export-csv".into());
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
