// TCP tunneling via CONNECTION FACILITATION
// This is NOT a TCP proxy - it's a signaling service like WebRTC
//
// Architecture:
//   Client asks jrok → jrok finds agent → agent creates listener → agent reports public endpoint
//   → jrok tells client → client connects DIRECTLY to agent (not through jrok)
//
// This scales to 10,000+ concurrent services because jrok is NOT in the data path

use crate::agent::AgentRegistry;
use crate::cluster::Cluster;
use crate::db::Db;
use crate::error::Result;
use crate::proto::Message;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    http::StatusCode,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Connection session tracks facilitation state
#[derive(Debug, Clone)]
pub struct ConnectionSession {
    pub session_id: String,
    pub subdomain: String,
    pub client_addr: String,
    pub agent_id: String,
    pub agent_endpoint: Option<String>,
    pub status: ConnectionStatus,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Pending,        // Initial state
    FindingAgent,   // Locating agent
    Requesting,     // Asking agent to create listener
    Ready,          // Agent ready, endpoint provided
    Connected,      // Client connected directly to agent
    Failed,         // Connection failed
}

/// Connection facilitator (NOT a proxy!)
/// This helps clients and agents find each other, but doesn't proxy data
pub struct ConnectionFacilitator {
    registry: Arc<AgentRegistry>,
    sessions: Arc<RwLock<HashMap<String, ConnectionSession>>>,
}

impl ConnectionFacilitator {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Client requests connection to a service
    /// This does NOT proxy data - it just helps establish the connection
    pub async fn request_connection(
        &self,
        subdomain: String,
        client_addr: String,
    ) -> Result<ConnectionSession> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::UNIX_EPOCH
            .elapsed()
            .map_err(|e| crate::error::Error::Internal(format!("Time error: {}", e)))
            .unwrap()
            .as_millis() as i64;

        // Find agent
        let (agent_id, agent) = self.registry.get_by_subdomain(&subdomain)
            .await
            .ok_or_else(|| crate::error::Error::TunnelNotFound(subdomain.clone()))?;

        if agent.protocol != crate::db::Protocol::Tcp {
            return Err(crate::error::Error::BadRequest(
                "Agent is not configured for TCP".to_string()
            ));
        }

        let session = ConnectionSession {
            session_id: session_id.clone(),
            subdomain: subdomain.clone(),
            client_addr: client_addr.clone(),
            agent_id: agent_id.clone(),
            agent_endpoint: None,
            status: ConnectionStatus::FindingAgent,
            created_at: now,
        };

        self.sessions.write().await.insert(session_id.clone(), session.clone());

        // Ask agent to create listener and report endpoint
        let msg = Message::TcpListenRequest {
            session_id: session_id.clone(),
        };

        self.registry.send_message(&agent_id, msg).await?;

        info!(
            "Connection facilitation requested: {} -> {} (session: {})",
            subdomain, agent_id, session_id
        );

        Ok(session)
    }

    /// Agent reports its listening endpoint
    pub async fn agent_listening(
        &self,
        session_id: &str,
        agent_endpoint: String,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.agent_endpoint = Some(agent_endpoint.clone());
            session.status = ConnectionStatus::Ready;

            info!(
                "Agent listening for session {}: {}",
                session_id, agent_endpoint
            );
            Ok(())
        } else {
            Err(crate::error::Error::BadRequest(
                "Session not found".to_string()
            ))
        }
    }

    /// Client signals successful connection to agent
    pub async fn client_connected(
        &self,
        session_id: &str,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.status = ConnectionStatus::Connected;
            info!(
                "Direct connection established: {} -> {} (endpoint: {:?})",
                session.subdomain, session.agent_id, session.agent_endpoint
            );
            Ok(())
        } else {
            Err(crate::error::Error::BadRequest(
                "Session not found".to_string()
            ))
        }
    }

    /// Get session status
    pub async fn get_session(&self, session_id: &str) -> Option<ConnectionSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// Clean up old sessions
    pub async fn cleanup_sessions(&self, timeout_ms: i64) -> Result<()> {
        let now = std::time::SystemTime::UNIX_EPOCH
            .elapsed()
            .map_err(|e| crate::error::Error::Internal(format!("Time error: {}", e)))
            .unwrap()
            .as_millis() as i64;

        let cutoff = now - timeout_ms;

        let mut sessions = self.sessions.write().await;
        let before = sessions.len();

        sessions.retain(|_, session| session.created_at > cutoff);

        let cleaned = before - sessions.len();
        if cleaned > 0 {
            info!("Cleaned up {} connection sessions", cleaned);
        }

        Ok(())
    }

    /// Get active session count
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

/// HTTP endpoint for connection facilitation
pub async fn handle_tcp_connection_request(
    Path(subdomain): Path<String>,
    State((registry, _db, _cluster, facilitator)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>, Arc<ConnectionFacilitator>)>,
    addr: axum::extract::connect_info::ConnectInfo<std::net::SocketAddr>,
) -> Response {
    let client_addr = addr.0.to_string();

    debug!(
        "TCP connection request from {} for service '{}'",
        client_addr, subdomain
    );

    // Find agent
    match facilitator.request_connection(subdomain, client_addr).await {
        Ok(session) => {
            // Return session info to client
            // The client will poll for agent endpoint
            let response = serde_json::json!({
                "session_id": session.session_id,
                "status": "pending",
                "message": "Agent is preparing connection. Poll /tcp/session/{session_id} for status.",
                "poll_url": format!("/tcp/session/{}", session.session_id)
            });

            (StatusCode::ACCEPTED, axum::Json(response)).into_response()
        }
        Err(e) => {
            (StatusCode::NOT_FOUND, format!("Service not found: {}", e)).into_response()
        }
    }
}

/// Get session status (client polls this)
pub async fn get_session_status(
    Path(session_id): Path<String>,
    State((_registry, _db, _cluster, facilitator)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>, Arc<ConnectionFacilitator>)>,
) -> Response {
    match facilitator.get_session(&session_id).await {
        Some(session) => {
            let response = match session.status {
                ConnectionStatus::Pending | ConnectionStatus::FindingAgent | ConnectionStatus::Requesting => {
                    serde_json::json!({
                        "session_id": session_id,
                        "status": "pending",
                        "message": "Waiting for agent..."
                    })
                }
                ConnectionStatus::Ready => {
                    if let Some(endpoint) = &session.agent_endpoint {
                        serde_json::json!({
                            "session_id": session_id,
                            "status": "ready",
                            "agent_endpoint": endpoint,
                            "message": "Connect directly to this endpoint",
                            "instruction": format!("Connect directly to: {}", endpoint)
                        })
                    } else {
                        serde_json::json!({
                            "session_id": session_id,
                            "status": "pending",
                            "message": "Agent endpoint not yet available"
                        })
                    }
                }
                ConnectionStatus::Connected => {
                    serde_json::json!({
                        "session_id": session_id,
                        "status": "connected",
                        "agent_endpoint": session.agent_endpoint,
                        "message": "Connection established successfully"
                    })
                }
                ConnectionStatus::Failed => {
                    serde_json::json!({
                        "session_id": session_id,
                        "status": "failed",
                        "message": "Connection failed"
                    })
                }
            };

            axum::Json(response).into_response()
        }
        None => {
            (StatusCode::NOT_FOUND, "Session not found or expired").into_response()
        }
    }
}

/// Agent reports it's listening (called by agent via WebSocket)
pub async fn agent_listening(
    State((_registry, _db, _cluster, facilitator)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>, Arc<ConnectionFacilitator>)>,
    axum::extract::Json(request): axum::Json<AgentListeningRequest>,
) -> Response {
    match facilitator.agent_listening(&request.session_id, request.endpoint.clone()).await {
        Ok(()) => {
            info!("Agent {} listening at {}", request.session_id, request.endpoint);
            (StatusCode::OK, "Listening recorded").into_response()
        }
        Err(e) => {
            error!("Failed to record agent listening: {}", e);
            (StatusCode::NOT_FOUND, format!("Session not found: {}", e)).into_response()
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct AgentListeningRequest {
    pub session_id: String,
    pub endpoint: String,
}

/// Cleanup old sessions (called periodically)
pub async fn cleanup_old_sessions(
    State((_registry, _db, _cluster, facilitator)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>, Arc<ConnectionFacilitator>)>,
) -> String {
    match facilitator.cleanup_sessions(300_000).await {
        Ok(_) => {
            "Session cleanup completed".to_string()
        }
        Err(e) => {
            format!("Cleanup error: {}", e)
        }
    }
}
