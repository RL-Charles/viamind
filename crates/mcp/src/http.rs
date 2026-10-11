//! MCP over streamable HTTP on localhost, so agents can connect to a running Viamind app.
//!
//! The desktop app owns the design, so agents cannot start it over stdio the way they start
//! `viamind-mcp`; instead the app serves `http://127.0.0.1:<port>/mcp` and an agent is pointed at it,
//! e.g. `claude mcp add --transport http viamind http://127.0.0.1:7878/mcp`.

use std::{net::SocketAddr, sync::Arc, thread};

use anyhow::Context as _;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager};
use tokio_util::sync::CancellationToken;
use viamind_engine::EngineHandle;

use crate::{engine_host::EngineHost, host::register_host, registry::ToolRegistry, server::ViamindServer};

/// Default port for the app's MCP endpoint; override with `VIAMIND_MCP_PORT`.
pub const DEFAULT_PORT: u16 = 7878;

/// Path the MCP endpoint is served under.
pub const MCP_PATH: &str = "/mcp";

/// A running HTTP MCP endpoint. Dropping it does not stop the server; call [`HttpServer::shutdown`].
#[derive(Debug, Clone)]
pub struct HttpServer {
    /// The bound address (useful when binding port 0).
    pub addr: SocketAddr,
    cancel: CancellationToken,
}

impl HttpServer {
    /// The endpoint URL agents connect to.
    pub fn url(&self) -> String {
        format!("http://{}{MCP_PATH}", self.addr)
    }

    /// Stops accepting requests and ends all sessions.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Serves `registry` over streamable HTTP at `addr` on the current tokio runtime.
///
/// Each MCP session gets its own [`ViamindServer`], so each connected agent has its own identity.
/// Only loopback `Host` headers are accepted (rmcp's DNS-rebinding protection).
pub async fn serve_http(registry: impl Into<Arc<ToolRegistry>>, addr: SocketAddr) -> anyhow::Result<HttpServer> {
    let registry: Arc<ToolRegistry> = registry.into();
    let cancel = CancellationToken::new();
    let config = StreamableHttpServerConfig::default().with_cancellation_token(cancel.child_token());
    let service = StreamableHttpService::new(move || Ok(ViamindServer::new(registry.clone())), Arc::new(LocalSessionManager::default()), config);
    let router = axum::Router::new().nest_service(MCP_PATH, service);
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("could not bind MCP endpoint on {addr}"))?;
    let addr = listener.local_addr()?;
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).with_graceful_shutdown(async move { shutdown.cancelled_owned().await }).await;
    });
    Ok(HttpServer { addr, cancel })
}

/// The tool registry for a live design: the built-in tools plus one tool per engine command and
/// `undo`, `redo` and `get_design`.
pub fn engine_registry(engine: EngineHandle) -> anyhow::Result<ToolRegistry> {
    let mut registry = ToolRegistry::with_builtins();
    crate::assembly::register(&mut registry, engine.clone())?;
    crate::fabrication::register(&mut registry, engine.clone())?;
    crate::views::register(&mut registry, engine.clone())?;
    crate::png_views::register(&mut registry, engine.clone())?;
    crate::schematic_views::register(&mut registry, engine.clone())?;
    register_host(&mut registry, Arc::new(EngineHost::new(engine)))?;
    Ok(registry)
}

/// Starts an HTTP MCP endpoint for `engine` on its own thread and tokio runtime, for apps (like the
/// egui UI) that do not run tokio themselves. Returns once the port is bound.
pub fn spawn_engine_server(engine: EngineHandle, addr: SocketAddr) -> anyhow::Result<HttpServer> {
    let registry = engine_registry(engine)?;
    let (tx, rx) = std::sync::mpsc::channel();
    thread::Builder::new().name("viamind-mcp".into()).spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(Err(anyhow::Error::from(e)));
                return;
            }
        };
        runtime.block_on(async move {
            match serve_http(registry, addr).await {
                Ok(server) => {
                    let cancel = server.cancel.clone();
                    let _ = tx.send(Ok(server));
                    cancel.cancelled_owned().await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });
    })?;
    rx.recv().context("MCP server thread exited before starting")?
}
