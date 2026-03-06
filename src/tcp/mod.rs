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
use crate::nat::{NatDetector, NatInfo, NatType};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    http::StatusCode,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Check if IP address is private (RFC 1918)
fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 10.0.0.0/8
            if octets[0] == 10 {
                return true;
            }
            // 172.16.0.0/12
            if octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31 {
                return true;
            }
            // 192.168.0.0/16
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            // 127.0.0.0/8 (loopback)
            if octets[0] == 127 {
                return true;
            }
            false
        }
        std::net::IpAddr::V6(_) => true, // Assume all IPv6 needs NAT traversal for now
    }
}

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
    pub client_nat: Option<NatInfo>,
    pub agent_nat: Option<NatInfo>,
    pub direct_connection: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Pending,        // Initial state
    FindingAgent,   // Locating agent
    Requesting,     // Asking agent to create listener
    HolePunching,   // NAT traversal in progress
    Ready,          // Agent ready, endpoint provided
    Connected,      // Client connected directly to agent
    Relayed,        // Fallback to relay mode
    Failed,         // Connection failed
}

/// Connection facilitator (NOT a proxy!)
/// This helps clients and agents find each other, but doesn't proxy data
pub struct ConnectionFacilitator {
    registry: Arc<AgentRegistry>,
    sessions: Arc<RwLock<HashMap<String, ConnectionSession>>>,
    nat_detector: NatDetector,
}

impl ConnectionFacilitator {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            nat_detector: NatDetector::new(),
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

        // Detect client NAT (if possible)
        let client_nat = self.detect_client_nat(&client_addr).await;

        // Determine connection strategy
        let direct_connection = self.can_use_direct_connection(&client_nat).await;

        let session = ConnectionSession {
            session_id: session_id.clone(),
            subdomain: subdomain.clone(),
            client_addr: client_addr.clone(),
            agent_id: agent_id.clone(),
            agent_endpoint: None,
            status: ConnectionStatus::FindingAgent,
            created_at: now,
            client_nat: client_nat.clone(),
            agent_nat: None,
            direct_connection,
        };

        self.sessions.write().await.insert(session_id.clone(), session.clone());

        // Ask agent to create listener and report endpoint
        // Include client NAT info so agent can prepare
        let nat_ser = client_nat.map(|nat| crate::proto::NatInfoSer::from(nat));
        let msg = Message::TcpListenRequest {
            session_id: session_id.clone(),
            client_nat: nat_ser,
        };

        self.registry.send_message(&agent_id, msg).await?;

        info!(
            "Connection facilitation requested: {} -> {} (session: {}, direct: {})",
            subdomain, agent_id, session_id, direct_connection
        );

        Ok(session)
    }

    /// Agent reports its listening endpoint
    pub async fn agent_listening(
        &self,
        session_id: &str,
        agent_endpoint: String,
        agent_nat: Option<crate::nat::NatInfo>,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.agent_endpoint = Some(agent_endpoint.clone());
            session.agent_nat = agent_nat.clone();

            // Determine if we need hole punching
            let needs_hole_punching = self.needs_hole_punching(session).await;

            if needs_hole_punching {
                session.status = ConnectionStatus::HolePunching;
                info!(
                    "Hole punching needed for session {}: {} (client: {:?}, agent: {:?})",
                    session_id,
                    agent_endpoint,
                    session.client_nat.as_ref().map(|n| &n.nat_type),
                    agent_nat.as_ref().map(|n| &n.nat_type)
                );
            } else {
                session.status = ConnectionStatus::Ready;
                info!(
                    "Direct connection ready for session {}: {}",
                    session_id, agent_endpoint
                );
            }

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

    /// Detect client NAT type
    async fn detect_client_nat(&self, client_addr: &str) -> Option<NatInfo> {
        // Try to detect NAT type
        // This is a simplified version - in production would use STUN
        // For now, we'll assume unknown NAT type
        debug!("Detecting NAT for client: {}", client_addr);

        // Parse the client address to see if it's local/public
        if let Ok(addr) = client_addr.parse::<std::net::SocketAddr>() {
            let ip = addr.ip();

            // Check if it's a private IP address
            if is_private_ip(&ip) {
                // Client is behind NAT
                return Some(NatInfo {
                    local_addr: client_addr.to_string(),
                    public_ip: "0.0.0.0".to_string(), // Unknown public IP
                    public_port: 0,
                    nat_type: NatType::Unknown,
                    hairpinning: false,
                });
            } else {
                // Client has public IP
                return Some(NatInfo {
                    local_addr: client_addr.to_string(),
                    public_ip: ip.to_string(),
                    public_port: addr.port(),
                    nat_type: NatType::Open,
                    hairpinning: false,
                });
            }
        }

        None
    }

    /// Determine if direct connection is possible
    async fn can_use_direct_connection(&self, client_nat: &Option<NatInfo>) -> bool {
        match client_nat {
            None => false, // Unknown NAT, assume relay needed
            Some(nat) => match nat.nat_type {
                NatType::Open => true, // Public IP, direct connection possible
                NatType::FullCone | NatType::RestrictedCone | NatType::PortRestrictedCone => true, // Hole punching possible
                NatType::Symmetric => false, // Symmetric NAT difficult, may need relay
                NatType::Unknown => false, // Unknown, assume relay needed
            },
        }
    }

    /// Determine if hole punching is needed
    async fn needs_hole_punching(&self, session: &ConnectionSession) -> bool {
        match (&session.client_nat, &session.agent_nat) {
            (Some(client), Some(agent)) => {
                // Both sides have NAT info
                match (&client.nat_type, &agent.nat_type) {
                    (NatType::Open, _) | (_, NatType::Open) => false,
                    (NatType::Symmetric, NatType::Symmetric) => true,
                    _ => true, // Hole punching beneficial for most NAT combinations
                }
            }
            _ => false, // Missing NAT info, assume direct connection
        }
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
                ConnectionStatus::HolePunching => {
                    serde_json::json!({
                        "session_id": session_id,
                        "status": "hole_punching",
                        "message": "NAT traversal in progress",
                        "client_nat": session.client_nat.as_ref().map(|n| serde_json::to_value(n).ok()).flatten(),
                        "agent_nat": session.agent_nat.as_ref().map(|n| serde_json::to_value(n).ok()).flatten(),
                    })
                }
                ConnectionStatus::Ready => {
                    if let Some(endpoint) = &session.agent_endpoint {
                        serde_json::json!({
                            "session_id": session_id,
                            "status": "ready",
                            "agent_endpoint": endpoint,
                            "message": "Connect directly to this endpoint",
                            "instruction": format!("Connect directly to: {}", endpoint),
                            "direct_connection": session.direct_connection
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
                        "message": "Connection established successfully",
                        "direct": session.direct_connection
                    })
                }
                ConnectionStatus::Relayed => {
                    serde_json::json!({
                        "session_id": session_id,
                        "status": "relayed",
                        "message": "Connection relayed through server"
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
    // Convert NAT info from serializable format
    let nat_info = request.agent_nat.map(|nat| crate::nat::NatInfo::from(nat));

    match facilitator.agent_listening(&request.session_id, request.endpoint.clone(), nat_info).await {
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
    pub agent_nat: Option<crate::proto::NatInfoSer>,
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
