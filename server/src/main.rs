mod agent;
mod cluster;
mod db;
mod tunnel;
mod proto;
mod error;
mod config;

use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .finish();
    
    let config = config::Config::from_env();
    
    info!("Starting jrok server {} on {}", config.server_id, config.http_host);
    
    let db = db::Db::new(&config.turso_url, &config.turso_token).await.expect("Failed to connect to database");
    info!("Database connected");
    
    let cluster = cluster::Cluster::new(db.clone(), config.clone());
    cluster.start().await;
    info!("Cluster started");
    
    let agent_registry = agent::AgentRegistry::new(db.clone(), config.server_id.clone(), config.http_host.clone());
    
    let tunnel_router = tunnel::TunnelRouter::new(
        agent_registry.clone(),
        db.clone(),
        config.clone(),
    );
    
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws/agent", get(agent::handle_agent_ws))
        .route("/:subdomain/*path", any(tunnel::proxy_http))
        .layer(CorsLayer::permissive())
        .with_state(tunnel_router);
    
    let addr = format!("{}:{}", config.http_host, config.http_port);
    
    info!("Server listening on {}", addr);
    
    let shutdown_cluster = cluster.clone();
    let shutdown_db = db.clone();
    
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Shutting down...");
        
        let _ = shutdown_cluster.shutdown().await;
        let _ = shutdown_db.close().await;
    });
    
    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn health() -> &'static str {
    "ok"
}
