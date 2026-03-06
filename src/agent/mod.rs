mod registry;

pub use registry::*;

use crate::cluster::Cluster;
use crate::db::{Db, Protocol};
use crate::proto::Message;
use axum::{
    extract::{ws::{WebSocket, Message as WsMessage}, State, WebSocketUpgrade},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tracing::{debug, error, warn};
use std::pin::pin;

pub async fn handle_agent_ws(
    ws: WebSocketUpgrade,
    State((registry, db, _cluster)): State<(Arc<registry::AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
) -> Response {
    ws.on_upgrade(|socket| handle_agent_connection(socket, registry, db))
}

async fn handle_agent_connection(socket: WebSocket, registry: Arc<registry::AgentRegistry>, db: Arc<Db>) {
    let (mut tx, rx) = socket.split();
    
    let mut agent_id: Option<String> = None;
    let _heartbeat_timeout_ms = 30_000;
    
    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel::<Message>(100);
    
    tokio::spawn(async move {
        let mut msg_rx = msg_rx;
        while let Some(msg) = msg_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if tx.send(WsMessage::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    let rx = rx.filter_map(|msg| async {
        match msg {
            Ok(WsMessage::Text(text)) => Some(text),
            _ => None,
        }
    });
    let mut rx = pin!(rx);

    while let Some(msg) = rx.next().await {
        match serde_json::from_str::<Message>(&msg) {
            Ok(Message::Register { subdomain, local_port, local_host, protocol, api_key }) => {
                // Validate API key
                match db.validate_api_key(&api_key).await {
                    Ok(true) => {},
                    Ok(false) => {
                        error!("Invalid API key for agent registration");
                        let _ = msg_tx.send(Message::Error {
                            message: "Invalid API key".to_string(),
                        }).await;
                        return;
                    }
                    Err(e) => {
                        error!("API key validation error: {}", e);
                        let _ = msg_tx.send(Message::Error {
                            message: "Authentication failed".to_string(),
                        }).await;
                        return;
                    }
                }

                match registry.register(
                    subdomain.clone(),
                    local_port,
                    local_host,
                    Protocol::from(protocol.as_str()),
                ).await {
                    Ok((id, _tx)) => {
                        agent_id = Some(id.clone());
                        let welcome = Message::Welcome {
                            agent_id: id,
                            subdomain,
                            protocol: protocol.to_string(),
                        };
                        if let Ok(_json) = serde_json::to_string(&welcome) {
                            let _ = msg_tx.send(welcome).await;
                        }
                    }
                    Err(e) => {
                        error!("Failed to register agent: {}", e);
                        let _ = msg_tx.send(Message::Error {
                            message: e.to_string(),
                        }).await;
                        return;
                    }
                }
            }
            Ok(Message::Heartbeat) => {
                if let Some(ref id) = agent_id {
                    let _ = registry.update_heartbeat(id).await;
                }
            }
            Ok(Message::HttpResponse { request_id, status, headers, body }) => {
                if let Some(ref _id) = agent_id {
                    use base64::Engine;
                    let body_str = body.map(|b| base64::engine::general_purpose::STANDARD.encode(b));
                    let _ = registry.handle_response(request_id, status, headers, body_str).await;
                }
            }
            Ok(Message::TcpConnect { connection_id, client_ip }) => {
                if let Some(ref id) = agent_id {
                    debug!("Agent {} TCP connect: {} from {}", id, connection_id, client_ip);
                    // Forward connection request to agent via msg_tx
                    let connect_msg = Message::TcpConnect {
                        connection_id: connection_id.clone(),
                        client_ip,
                    };
                    let _ = msg_tx.send(connect_msg).await;
                }
            }
            Ok(Message::TcpData { connection_id, data }) => {
                if let Some(ref id) = agent_id {
                    debug!("Agent {} received TCP data: {} bytes", id, data.len());
                    // Forward data back to the waiting TCP connection
                    // This would typically be stored in a pending requests map
                    use base64::Engine;
                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&data) {
                        // TODO: Forward decoded data to the actual TCP connection
                        debug!("Forwarding {} bytes to TCP connection {}", decoded.len(), connection_id);
                    }
                }
            }
            Ok(Message::TcpDisconnect { connection_id }) => {
                if let Some(ref id) = agent_id {
                    debug!("Agent {} TCP disconnect: {}", id, connection_id);
                    // Forward disconnect message to agent
                    let disconnect_msg = Message::TcpDisconnect {
                        connection_id: connection_id.clone(),
                    };
                    let _ = msg_tx.send(disconnect_msg).await;
                }
            }
            Ok(_) => {
                warn!("Unknown message type");
            }
            Err(e) => {
                error!("Failed to parse message: {}", e);
            }
        }
    }
}
