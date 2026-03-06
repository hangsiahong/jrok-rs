mod agent;
mod api;
mod cluster;
pub mod db;
mod tunnel;
mod proto;
mod tcp;
mod error;
mod config;

use axum::routing::{any, get, post};
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

    // Create TCP connection facilitator
    let tcp_facilitator = Arc::new(tcp::ConnectionFacilitator::new(agent_registry.clone()));

    // API routes disabled due to axum/libsql conflict - use CLI tool instead
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws/agent", get(agent::handle_agent_ws))
        .route("/:subdomain/*path", any(tunnel::proxy_http))
        // TCP connection facilitation endpoints
        .route("/tcp/:subdomain", get(tcp::handle_tcp_connection_request))
        .route("/tcp/session/:session_id", get(tcp::get_session_status))
        .route("/agent/listening", post(tcp::agent_listening))
        .layer(CorsLayer::permissive())
        .with_state((agent_registry, db, cluster.clone(), tcp_facilitator));

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
