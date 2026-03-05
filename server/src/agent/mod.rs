mod registry;

pub use registry::*;

use crate::db::Db;
use crate::error::Result;
use crate::proto::Message;
use axum::{
    extract::{ws::WebSocket, State, WebSocketUpgrade},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tracing::{debug, error, info, warn};

pub async fn handle_agent_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AgentState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_agent_connection(socket, state))
}

struct AgentState {
    registry: AgentRegistry,
    db: Db,
}

async fn handle_agent_connection(socket: WebSocket, state: Arc<AgentState>) {
    let (tx, rx) = socket.split();
    
    let mut agent_id: Option<String> = None;
    let heartbeat_timeout_ms = 30_000;
    
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
    
    let mut rx = rx.filter_map(|msg| async {
        match msg {
            axum::extract::ws::Message::Text(text) => Some(text),
            _ => None,
        }
    });
    
    while let Some(Ok(msg)) = rx.next().await {
        match serde_json::from_str(&msg) {
            Message::Register { subdomain, local_port, local_host, protocol } => {
                match state.registry.register(
                    subdomain.clone(),
                    local_port,
                    local_host,
                    protocol,
                    msg_tx.clone(),
                ).await {
                    Ok(id) => {
                        agent_id = Some(id.clone());
                        let welcome = Message::Welcome {
                            agent_id: id,
                            subdomain,
                            protocol: protocol.to_string(),
                        };
                        if let Ok(json) = serde_json::to_string(&welcome) {
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
            Message::Heartbeat => {
                if let Some(ref id) = agent_id {
                    let _ = state.registry.updateHeartbeat(&id).await;
                }
            }
            Message::HttpResponse { request_id, status, headers, body } => {
                if let Some(ref id) = agent_id {
                    let _ = state.registry.handle_response(&id, request_id, status, headers, body).await;
                }
            }
            Message::TcpConnect { connection_id, client_ip } => {
                if let Some(ref id) = agent_id {
                    debug!("Agent {} TCP connect: {} from {}", id, connection_id, client_ip);
                }
            }
            Message::TcpData { connection_id, data } => {
                if let Some(ref _id) = agent_id {
                    debug!("TCP data for connection {}", connection_id);
                }
            }
            Message::TcpDisconnect { connection_id } => {
                if let Some(ref _id) = agent_id {
                    debug!("TCP disconnect for connection {}", connection_id);
                }
            }
            _ => {
                warn!("Unknown message type");
            }
        }
    }
}
