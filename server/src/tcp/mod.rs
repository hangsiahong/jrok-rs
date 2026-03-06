// TCP tunneling implementation using rexpose
// This module provides TCP forwarding capabilities through WebSocket agents

use crate::agent::AgentRegistry;
use crate::cluster::Cluster;
use crate::db::Db;
use crate::error::Result;
use crate::proto::Message;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};
use tracing::{debug, error, info, warn};

/// TCP forwarder that manages TCP tunnel connections
pub struct TcpForwarder {
    registry: Arc<AgentRegistry>,
    active_listeners: Arc<Mutex<HashMap<String, u16>>>,
}

impl TcpForwarder {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            active_listeners: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start listening for TCP connections on behalf of an agent
    pub async fn start_listener(&self, subdomain: String, port: u16) -> Result<()> {
        self.active_listeners
            .lock()
            .await
            .insert(subdomain.clone(), port);

        info!("Started TCP listener for '{}' on port {}", subdomain, port);

        Ok(())
    }

    /// Stop listening for TCP connections
    pub async fn stop_listener(&self, subdomain: &str) -> Result<()> {
        self.active_listeners.lock().await.remove(subdomain);
        info!("Stopped TCP listener for '{}'", subdomain);
        Ok(())
    }
}

/// TCP connection manager that tracks active connections
#[derive(Clone)]
pub struct TcpConnectionManager {
    connections: Arc<Mutex<HashMap<String, TcpConnection>>>,
    registry: Arc<AgentRegistry>,
}

struct TcpConnection {
    connection_id: String,
    agent_id: String,
    subdomain: String,
    client_addr: String,
}

impl TcpConnectionManager {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            registry,
        }
    }

    pub async fn create_connection(
        &self,
        connection_id: String,
        agent_id: String,
        subdomain: String,
        client_addr: String,
    ) {
        let connection = TcpConnection {
            connection_id: connection_id.clone(),
            agent_id,
            subdomain,
            client_addr,
        };

        self.connections
            .lock()
            .await
            .insert(connection_id.clone(), connection);

        debug!("TCP connection created: {}", connection_id);
    }

    pub async fn remove_connection(&self, connection_id: &str) {
        self.connections.lock().await.remove(connection_id);
        debug!("TCP connection removed: {}", connection_id);
    }

    pub async fn forward_data(&self, connection_id: &str, data: Vec<u8>) -> Result<()> {
        let connections = self.connections.lock().await;
        if let Some(conn) = connections.get(connection_id) {
            // Send data to agent via WebSocket
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
            let msg = Message::TcpData {
                connection_id: connection_id.to_string(),
                data: encoded,
            };

            self.registry.send_message(&conn.agent_id, msg).await?;
        }
        Ok(())
    }
}

/// Handle incoming TCP tunnel connection
pub async fn handle_tcp_tunnel(
    Path((subdomain, tcp_port)): Path<(String, u16)>,
    State((registry, db, cluster)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
) -> Response {
    debug!("TCP tunnel request for {} on port {}", subdomain, tcp_port);

    // Check if agent exists
    let Some((agent_id, agent)) = registry.get_by_subdomain(&subdomain).await else {
        // Check if agent exists on remote server
        if let Ok(Some(remote_server)) = db.get_agent_server(&subdomain).await {
            // Agent exists on remote server
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!(
                    "Agent '{}' is on remote server '{}'. Connect directly to that server.",
                    subdomain, remote_server
                ),
            )
                .into_response();
        }

        return (
            axum::http::StatusCode::NOT_FOUND,
            format!("Tunnel '{}' not found. No agent registered.", subdomain),
        )
            .into_response();
    };

    // Check if agent supports TCP
    if agent.protocol != crate::db::Protocol::Tcp {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Agent is not configured for TCP tunneling. Use protocol='tcp' in registration.",
        )
            .into_response();
    }

    info!(
        "TCP tunnel established: {} -> {}:{} (agent: {})",
        subdomain, agent.local_host, agent.local_port, agent_id
    );

    // The actual TCP forwarding will be handled by TcpForwarder
    // This is just the initial connection setup
    (
        axum::http::StatusCode::OK,
        format!(
            "TCP tunnel ready for '{}' on port {}. Agent: {}",
            subdomain, tcp_port, agent_id
        ),
    )
        .into_response()
}

/// TCP forwarder that handles bidirectional data flow
impl TcpForwarder {
    pub async fn start_tcp_listener(
        &self,
        port: u16,
        subdomain: String,
    ) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("TCP listener started on port {} for {}", port, subdomain);

        let connection_manager = TcpConnectionManager::new(self.registry.clone());
        let registry = self.registry.clone();

        tokio::spawn(async move {
            while let Ok((mut socket, addr)) = listener.accept().await {
                let connection_id = uuid::Uuid::new_v4().to_string();
                let subdomain = subdomain.clone();
                let registry = registry.clone();
                let conn_mgr = connection_manager.clone();

                debug!(
                    "TCP connection accepted: {} from {}",
                    connection_id, addr
                );

                tokio::spawn(async move {
                    // Find agent for this subdomain
                    if let Some((agent_id, _agent)) =
                        registry.get_by_subdomain(&subdomain).await
                    {
                        let agent_id_clone = agent_id.clone();

                        // Notify agent of new connection
                        let msg = Message::TcpConnect {
                            connection_id: connection_id.clone(),
                            client_ip: addr.to_string(),
                        };

                        if let Err(e) = registry.send_message(&agent_id, msg).await {
                            error!("Failed to send TCP connect message: {}", e);
                            return;
                        }

                        conn_mgr
                            .create_connection(
                                connection_id.clone(),
                                agent_id,
                                subdomain,
                                addr.to_string(),
                            )
                            .await;

                        // Handle data forwarding
                        let mut buffer = [0u8; 4096];
                        loop {
                            match socket.read(&mut buffer).await {
                                Ok(0) => {
                                    // Connection closed
                                    debug!("TCP connection closed: {}", connection_id);
                                    break;
                                }
                                Ok(n) => {
                                    let data = buffer[..n].to_vec();

                                    // Forward data to agent
                                    if let Err(e) = conn_mgr.forward_data(&connection_id, data).await {
                                        error!("Failed to forward TCP data: {}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("TCP read error: {}", e);
                                    break;
                                }
                            }
                        }

                        // Notify agent of disconnect
                        let msg = Message::TcpDisconnect {
                            connection_id: connection_id.clone(),
                        };

                        if let Err(e) = registry.send_message(&agent_id_clone, msg).await {
                            error!("Failed to send TCP disconnect message: {}", e);
                        }

                        conn_mgr.remove_connection(&connection_id).await;
                    } else {
                        warn!("No agent found for subdomain: {}", subdomain);
                    }
                });
            }
        });

        Ok(())
    }
}

/// Allocate a TCP port for tunneling
pub async fn allocate_tcp_port(
    db: &Db,
    server_id: &str,
    tunnel_id: &str,
    start_port: u16,
    end_port: u16,
) -> Result<u16> {
    for port in start_port..end_port {
        match db.allocate_tcp_port(tunnel_id, server_id, start_port, end_port).await {
            Ok(Some(tcp_port)) => return Ok(tcp_port.port),
            Ok(None) => continue,
            Err(_) => continue,
        }
    }
    Err(crate::error::Error::NoTcpPort)
}

/// Deallocate a TCP port
pub async fn deallocate_tcp_port(db: &Db, port: u16) -> Result<()> {
    db.deallocate_tcp_port(port).await
}
