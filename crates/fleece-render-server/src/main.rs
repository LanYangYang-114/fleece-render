mod logging;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use fleece_render_server::{AppConfig, AppState, build_router};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[derive(Debug, Parser)]
#[command(
    name = "fleece-render",
    version,
    about = "Pure-Rust Minecraft skin rendering HTTP service"
)]
struct Cli {
    /// TOML configuration file. Defaults to ./fleece-render.toml when present.
    #[arg(long, env = "FLEECE_RENDER_CONFIG")]
    config: Option<PathBuf>,

    #[arg(long, env = "FLEECE_RENDER_HOST")]
    host: Option<String>,

    #[arg(long, env = "FLEECE_RENDER_PORT")]
    port: Option<u16>,

    #[arg(long, env = "FLEECE_RENDER_BACKEND")]
    backend: Option<String>,

    /// Log base path; each start adds -YYYY-MM-DD-N. Relative to the executable. Empty disables.
    #[arg(long, env = "FLEECE_RENDER_LOG_FILE")]
    log_file: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.or_else(|| {
        let default = PathBuf::from("fleece-render.toml");
        default.exists().then_some(default)
    });
    let mut config = AppConfig::load(config_path.as_deref())?;
    if let Some(host) = cli.host {
        config.server.host = host;
    }
    if let Some(port) = cli.port {
        config.server.port = port;
    }
    if let Some(backend) = cli.backend {
        config.render.backend = backend;
    }
    if let Some(log_file) = cli.log_file {
        config.server.log_file = (!log_file.trim().is_empty()).then_some(log_file);
    }
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| config.server.log_filter.clone());
    let env_filter = EnvFilter::try_new(&log_filter)
        .with_context(|| format!("invalid log filter {log_filter:?}"))?;
    if let Some((path, file)) = logging::open_log_file(
        config.server.log_file.as_deref(),
        config.server.log_max_files,
    )? {
        let writer = std::io::stderr.and(Arc::new(file));
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .compact()
            .with_writer(writer)
            .init();
        tracing::info!(log_file = %path.display(), log_max_files = config.server.log_max_files, "file logging enabled");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .compact()
            .with_writer(std::io::stderr)
            .init();
        tracing::info!("file logging disabled");
    }

    let state = AppState::new(config.clone()).context("failed to create application state")?;
    let app = build_router(state.clone());
    let address: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .with_context(|| {
            format!(
                "invalid listen address {}:{}",
                config.server.host, config.server.port
            )
        })?;
    state.warmup_network();
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind {address}"))?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        %address,
        backend = state.engine.backend_id(),
        styles = ?state.engine.styles().ids(),
        "fleece-render API started"
    );
    tracing::info!("root: http://127.0.0.1:{}/", config.server.port);
    tracing::info!("health: http://127.0.0.1:{}/health", config.server.port);
    tracing::info!("v1: http://127.0.0.1:{}/api/v1", config.server.port);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server failed")?;
    tracing::info!("server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
