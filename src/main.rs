mod app;
mod assets;
mod auth;
mod routes;
mod sse;
mod state;
mod zt;

/// Application version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use crate::state::{AppState, Config};

#[tokio::main]
async fn main() {
    // Load .env file (silently ignore if missing)
    dotenvy::dotenv().ok();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_sessions_core=error")),
        )
        .init();

    // Try to load existing config
    let config = Config::load();
    let is_configured = config.is_some();

    // Build app state
    let state = AppState::new(config);

    // If already configured, start ZT client + poller immediately
    if is_configured {
        state.start_zt().await;
        tracing::info!("Loaded existing configuration");
    } else {
        tracing::info!("No configuration found — setup wizard will be shown");
    }

    // Build router
    let app = app::build_router(state);

    // Bind and serve
    let bind_addr =
        std::env::var("TIERDROP_BIND").unwrap_or_else(|_| "127.0.0.1:8000".to_string());
    let addr: SocketAddr = bind_addr.parse().unwrap_or_else(|_| {
        eprintln!("Invalid bind address: {}", bind_addr);
        std::process::exit(1);
    });

    println!("TierDrop v{} listening on http://{}", VERSION, addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        });

    // Graceful shutdown handling
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| {
            eprintln!("Server error: {}", e);
            std::process::exit(1);
        });

    tracing::info!("Shutdown complete");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, shutting down...");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        }
    }
}
