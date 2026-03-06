mod agent;
mod api;
mod cluster;
mod db;
mod tunnel;
mod proto;
mod tcp;
mod error;
mod config;

use axum::routing::{any, get};
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .init();
    
    let config = config::Config::from_env();

    info!("Starting jrok server {} on {}", config.server_id, config.http_host);

    let db = db::Db::new(&config.turso_url, &config.turso_token).await.expect("Failed to connect to database");
    info!("Database connected");

    let agent_registry = agent::AgentRegistry::new(db.clone(), config.server_id.clone());
    let agent_registry = Arc::new(agent_registry);
    let db = Arc::new(db);

    let cluster = cluster::Cluster::new(db.clone(), config.clone(), agent_registry.clone());
    let cluster = Arc::new(cluster);
    cluster.start().await;
    info!("Cluster started");

    // Note: API routes temporarily disabled due to axum version conflict
    // from libsql/tonic dependency chain. Core authentication works.
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws/agent", get(agent::handle_agent_ws))
        .route("/:subdomain/*path", any(tunnel::proxy_http))
        .layer(CorsLayer::permissive())
        .with_state((agent_registry, db, cluster.clone()));

    let addr = format!("{}:{}", config.http_host, config.http_port);
    let listener = TcpListener::bind(&addr).await
        .expect("Failed to bind to address");
    info!("Server listening on {}", listener.local_addr()
        .expect("Failed to get local address"));

    let shutdown_cluster = cluster.clone();

    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to listen for shutdown signal: {}", e);
        }
        info!("Shutting down...");

        let _ = shutdown_cluster.shutdown().await;
    });

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
    }
}

async fn health() -> &'static str {
    "ok"
}
