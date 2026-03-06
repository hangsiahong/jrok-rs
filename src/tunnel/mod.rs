use crate::agent::AgentRegistry;
use crate::cluster::Cluster;
use crate::db::Db;
use crate::proto::Message;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

pub async fn proxy_http(
    Path((subdomain, path)): Path<(String, String)>,
    State((registry, db, cluster)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Response {
    let path = format!("/{}", path);
    
    debug!("Proxy request: {} {} (is_leader: {})", method, path, cluster.is_leader());

    // Check if agent exists locally
    let Some((agent_id, _agent)) = registry.get_by_subdomain(&subdomain).await else {
        // Check if agent exists on remote server
        if let Ok(Some(remote_server)) = db.get_agent_server(&subdomain).await {
            // Agent exists on remote server, redirect client
            let server_host = remote_server;
            let redirect_url = format!("http://{}/{}", server_host, path.trim_start_matches('/'));
            debug!("Redirecting to remote server: {}", redirect_url);

            return (
                StatusCode::TEMPORARY_REDIRECT,
                [("Location", &redirect_url)],
                "Redirecting to agent server".to_string()
            ).into_response();
        }

        return (StatusCode::NOT_FOUND, "Tunnel not found").into_response();
    };
    
    let request_id = uuid::Uuid::new_v4().to_string();
    
    let mut headers_map = HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            headers_map.insert(name.to_string(), v.to_string());
        }
    }
    
    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(_) => Vec::new(),
    };
    
    let message = Message::HttpRequest {
        request_id: request_id.clone(),
        method: method.to_string(),
        path,
        headers: headers_map,
        body: Some(body_bytes.to_vec()),
    };
    
    let pending = registry.create_pending_request(&request_id).await;

    if let Err(e) = registry.send_message(&agent_id, message).await {
        registry.remove_pending_request(&request_id).await;
        return (StatusCode::BAD_GATEWAY, format!("Agent error: {}", e)).into_response();
    }
    
    match tokio::time::timeout(std::time::Duration::from_secs(30), pending).await {
        Ok(Ok(response)) => response.into_response(),
        Ok(Err(_)) | Err(_) => {
            registry.remove_pending_request(&request_id).await;
            (StatusCode::GATEWAY_TIMEOUT, "Request timeout").into_response()
        }
    }
}
