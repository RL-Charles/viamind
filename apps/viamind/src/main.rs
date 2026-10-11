//! Viamind desktop app: opens a design and serves it to AI agents over MCP while it is open.
//!
//! ```text
//! viamind              open a new design; agents connect to http://127.0.0.1:7878/mcp
//! viamind --demo       open the synthetic 10k-object test scene instead (no design, no MCP)
//! viamind --no-mcp     open a design without the agent endpoint
//! ```
//! `VIAMIND_MCP_PORT` changes the port.

use std::net::{Ipv4Addr, SocketAddr};

use viamind_doc::Project;
use viamind_engine::EngineHandle;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--demo") {
        return viamind_ui_egui::run();
    }

    let engine = EngineHandle::new(Project::new("Untitled"));
    let mut mcp_url = None;
    if !args.iter().any(|a| a == "--no-mcp") {
        let port = std::env::var("VIAMIND_MCP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(viamind_mcp::DEFAULT_PORT);
        match viamind_mcp::spawn_engine_server(engine.clone(), SocketAddr::from((Ipv4Addr::LOCALHOST, port))) {
            Ok(server) => {
                eprintln!("viamind: agents can connect at {}", server.url());
                eprintln!("viamind: e.g. claude mcp add --transport http viamind {}", server.url());
                mcp_url = Some(server.url());
            }
            Err(e) => eprintln!("viamind: MCP endpoint not started: {e:#}"),
        }
    }
    viamind_ui_egui::run_design(engine, mcp_url)
}
