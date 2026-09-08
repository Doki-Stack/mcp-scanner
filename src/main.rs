use axum::{routing::get, Json, Router};
use serde::Serialize;
use tokio::signal;

mod config;
mod models;

use config::Config;

#[derive(Serialize)]
struct HealthStatus {
    status: &'static str,
}

async fn healthz() -> Json<HealthStatus> {
    Json(HealthStatus { status: "healthy" })
}

// No dependency checks registered yet — those are added as PG/MinIO/
// Dragonfly/RabbitMQ/Ollama clients land in later tasks (mirrors
// mcp-policy's A1: skeleton first, readiness checks as each dependency is
// actually wired in).
async fn readyz() -> Json<HealthStatus> {
    Json(HealthStatus { status: "healthy" })
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    doki_shared::tracing::init_tracing("mcp-scanner")?;

    let cfg = Config::from_env()?;

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!(%addr, "mcp-scanner listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
