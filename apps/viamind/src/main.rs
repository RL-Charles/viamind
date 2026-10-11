//! Viamind desktop and headless MCP entry points. See `viamind --help`.

mod cli;

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context;
use viamind_doc::{Project, file};
use viamind_engine::EngineHandle;

fn main() -> anyhow::Result<()> {
    let options = match cli::parse(std::env::args_os().skip(1)).map_err(anyhow::Error::msg)? {
        cli::Invocation::Help => {
            print!("{}", cli::HELP);
            return Ok(());
        }
        cli::Invocation::Version => {
            println!("viamind {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        cli::Invocation::Run(options) => options,
    };
    if options.demo {
        return viamind_ui_egui::run().map_err(|e| anyhow::anyhow!("{e}"));
    }
    let engine = match options.open {
        Some(path) => EngineHandle::load(&path).with_context(|| format!("could not open project {}", path.display()))?,
        None => EngineHandle::new(Project::new("Untitled")),
    };
    if options.export_gerber {
        let snapshot = engine.session_state();
        let output = options.output.as_deref().expect("CLI requires fabrication destination");
        let (path, manifest) = viamind_fab::write_fabrication(
            &snapshot.project,
            viamind_fab::FabricationOptions { mask_expansion_mm: options.mask_expansion_mm.unwrap_or(0.0) },
            output,
            engine.document_status().path.as_deref(),
        )
        .with_context(|| format!("could not export fabrication {}", output.display()))?;
        eprintln!("viamind: exported {} files to {}", manifest.files.len(), path.display());
        return Ok(());
    }
    if options.export_png {
        let project = engine.read(viamind_render::board_view_snapshot)?;
        let rendered = viamind_render::board_png(&project, &options.svg_options)?;
        let path =
            viamind_render::write_png(&rendered.png, options.output.as_deref().expect("validated output"), engine.document_status().path.as_deref())?;
        eprintln!(
            "Exported PNG to {} ({} × {} px, {} legacy previews omitted)",
            path.display(),
            options.svg_options.width_px,
            options.svg_options.height_px,
            rendered.metadata.omitted_legacy_parts
        );
        return Ok(());
    }
    if options.export_svg {
        let project = engine.read(viamind_render::board_view_snapshot)?;
        let rendered = viamind_render::board_svg(&project, &options.svg_options)?;
        let output = options.output.as_deref().expect("CLI requires a view destination");
        let path = viamind_render::write_svg(&rendered.svg, output, engine.document_status().path.as_deref())?;
        eprintln!(
            "viamind: rendered board SVG {} ({} pads, {} tracks, {} through vias; {} legacy previews omitted)",
            path.display(),
            rendered.metadata.pad_count,
            rendered.metadata.track_count,
            rendered.metadata.through_via_count,
            rendered.metadata.omitted_legacy_parts
        );
        return Ok(());
    }
    if let Some(kind) = options.export_csv {
        let snapshot = engine.session_state();
        let output = options.output.as_deref().expect("CLI requires an export destination");
        let path = viamind_fab::write_csv(&snapshot.project, kind, output, options.overwrite, engine.document_status().path.as_deref())
            .with_context(|| format!("could not export CSV {}", output.display()))?;
        eprintln!("viamind: exported {}", path.display());
        return Ok(());
    }
    if options.headless {
        let registry = viamind_mcp::engine_registry(engine.clone())?;
        eprintln!("viamind: headless design MCP on stdin/stdout; close stdin to finish");
        tokio::runtime::Runtime::new()?.block_on(viamind_mcp::serve_stdio(registry))?;
        if let Some(path) = options.save_as {
            engine.read(|project| file::save(project, &path)).with_context(|| format!("could not save project {}", path.display()))?;
            eprintln!("viamind: saved {}", path.display());
        }
        return Ok(());
    }

    let mut server = None;
    let mut mcp_url = None;
    if !options.no_mcp {
        let port = match std::env::var("VIAMIND_MCP_PORT") {
            Ok(value) => value.parse::<u16>().context("VIAMIND_MCP_PORT must be an integer from 0 to 65535")?,
            Err(std::env::VarError::NotPresent) => viamind_mcp::DEFAULT_PORT,
            Err(error) => anyhow::bail!("invalid VIAMIND_MCP_PORT: {error}"),
        };
        match viamind_mcp::spawn_engine_server(engine.clone(), SocketAddr::from((Ipv4Addr::LOCALHOST, port))) {
            Ok(endpoint) => {
                eprintln!("viamind: agents can connect at {}", endpoint.url());
                eprintln!("viamind: e.g. claude mcp add --transport http viamind {}", endpoint.url());
                mcp_url = Some(endpoint.url());
                server = Some(endpoint);
            }
            Err(e) => eprintln!("viamind: MCP endpoint not started: {e:#}"),
        }
    }
    let result = viamind_ui_egui::run_design(engine, mcp_url).map_err(|e| anyhow::anyhow!("{e}"));
    if let Some(server) = server {
        server.shutdown();
    }
    result
}
