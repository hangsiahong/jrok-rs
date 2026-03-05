use crate::agent::AgentRegistry;
use crate::config::Config;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::proto::Message;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tracing::{debug, error, info, warn};

pub struct TunnelRouter {
    registry: AgentRegistry,
    db: Db,
    config: Config,
}

impl TunnelRouter {
    pub fn new(registry: AgentRegistry, db: Db, config: Config) -> Self {
        Self { registry, db, config }
    }
}

pub async fn proxy_http(
    Path((subdomain, path)): (String, String),
    State(state): State<Arc<TunnelRouter>>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Response {
    let path = format!("/{}", path);
    
    debug!("Proxy request: {} {}", method, path);
    
    let Some(agent) = state.registry.get_by_subdomain(&subdomain).await else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Tunnel not found")
            .into_response();
    };
    
    let request_id = uuid::Uuid::new_v4().to_string();
    
    let mut headers_map = HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            headers_map.insert(name.to_string(), v.to_string());
        }
    }
    
    let body_bytes = body.collect().await.unwrap_or_default();
    
    let message = Message::HttpRequest {
        request_id: request_id.clone(),
        method: method.to_string(),
        path,
        headers: headers_map,
        body: Some(body_bytes.to_vec()),
    };
    
    let pending = state.registry.create_pending_request(&request_id);
    
    if let Err(e) = state.registry.send_message(&agent.agent_id, message).await {
        state.registry.remove_pending_request(&request_id);
        return Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(format!("Agent error: {}", e))
            .into_response();
    }
    
    match tokio::time::timeout(std::time::Duration::from_secs(30), async {
        pending.wait().await
    }).await {
        Ok(Ok(response)) => response.into_response(),
        Ok(Err(_)) | Err(_) => {
            state.registry.remove_pending_request(&request_id);
            Response::builder()
                .status(StatusCode::GATEWAY_TIMEOUT)
                .body("Request timeout")
                .into_response()
        }
    }
}
