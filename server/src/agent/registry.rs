use crate::db::{Agent, Db, Protocol};
use crate::error::{Error, Result};
use crate::proto::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::info;

#[derive(Clone)]
pub struct AgentRegistry {
    db: Db,
    server_id: String,
    agents: Arc<RwLock<HashMap<String, AgentState>>>,
    pending: Arc<RwLock<HashMap<String, PendingRequest>>>,
}

pub struct AgentState {
    pub tx: mpsc::Sender<Message>,
    pub subdomain: String,
    pub local_port: u16,
    pub local_host: String,
    pub protocol: Protocol,
    pub last_heartbeat: i64,
}

pub struct PendingRequest {
    pub response_tx: oneshot::Sender<HttpResponse>,
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl axum::response::IntoResponse for HttpResponse {
    fn into_response(self) -> axum::response::Response {
        let mut response = axum::response::Response::builder()
            .status(axum::http::StatusCode::from_u16(self.status).unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR));

        for (key, value) in self.headers {
            if let Ok(key) = axum::http::HeaderName::try_from(key) {
                if let Ok(value) = axum::http::HeaderValue::try_from(value) {
                    response = response.header(key, value);
                }
            }
        }

        response
            .body(axum::body::Body::from(self.body))
            .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
            .into_response()
    }
}

impl AgentRegistry {
    pub fn new(db: Db, server_id: String) -> Self {
        Self {
            db,
            server_id,
            agents: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn register(
        &self,
        subdomain: String,
        local_port: u16,
        local_host: String,
        protocol: Protocol,
    ) -> Result<(String, mpsc::Sender<Message>)> {
        let agent_id = uuid::Uuid::new_v4().to_string();
        let connection_token = uuid::Uuid::new_v4().to_string();
        let (tx, _rx) = mpsc::channel(100);
        
        let existing = self.db.get_agent_by_subdomain(&subdomain).await?;
        
        if let Some(agent) = &existing {
            if agent.server_id != self.server_id {
                if let Some(server) = self.db.get_server(&agent.server_id).await? {
                    if server.is_healthy {
                        return Err(Error::SubdomainTaken(format!(
                            "Subdomain {} is on server {}",
                            subdomain, agent.server_id
                        )));
                    }
                }
            }
        }
        
        self.db.register_agent(&Agent {
            id: agent_id.clone(),
            subdomain: subdomain.to_string(),
            server_id: self.server_id.clone(),
            tunnel_id: None,
            local_port,
            local_host: local_host.clone(),
            protocol,
            connection_token,
            connected_at: current_time_ms(),
            last_heartbeat: current_time_ms(),
            active: true,
        }).await?;
        
        let state = AgentState {
            tx: tx.clone(),
            subdomain: subdomain.to_string(),
            local_port,
            local_host: local_host.to_string(),
            protocol,
            last_heartbeat: current_time_ms(),
        };
        
        self.agents.write().await.insert(agent_id.clone(), state);
        
        info!("Agent registered: {} -> {}:{} on {}", subdomain, local_host, local_port, self.server_id);
        
        Ok((agent_id, tx))
    }
    
    pub async fn unregister(&self, agent_id: &str) -> Result<()> {
        if let Some(state) = self.agents.write().await.remove(agent_id) {
            self.db.unregister_agent(agent_id).await?;
            info!("Agent unregistered: {}", state.subdomain);
        }
        Ok(())
    }
    
    pub async fn get_by_subdomain(&self, subdomain: &str) -> Option<(String, AgentState)> {
        for (id, state) in self.agents.read().await.iter() {
            if state.subdomain == subdomain {
                return Some((id.clone(), state.clone()));
            }
        }
        None
    }
    
    pub async fn get_agent_server(&self, subdomain: &str) -> Result<Option<String>> {
        if let Some(agent) = self.db.get_agent_by_subdomain(subdomain).await? {
            if agent.active {
                return Ok(Some(agent.server_id));
            }
        }
        Ok(None)
    }
    
    pub async fn update_heartbeat(&self, agent_id: &str) -> Result<()> {
        if let Some(_state) = self.agents.write().await.get_mut(agent_id) {
            // Update local state (already in memory)
        }
        self.db.send_agent_heartbeat(agent_id).await?;
        Ok(())
    }
    
    pub async fn send_message(&self, agent_id: &str, message: Message) -> Result<()> {
        if let Some(state) = self.agents.read().await.get(agent_id) {
            state.tx.send(message).await.map_err(|e| {
                Error::Internal(format!("Failed to send message: {}", e))
            })?;
        } else {
            return Err(Error::AgentNotFound(agent_id.to_string()));
        }
        Ok(())
    }
    
    pub async fn create_pending_request(&self, request_id: &str) -> oneshot::Receiver<HttpResponse> {
        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(request_id.to_string(), PendingRequest { response_tx: tx });
        rx
    }

    pub async fn remove_pending_request(&self, request_id: &str) {
        self.pending.write().await.remove(request_id);
    }
    
    pub async fn handle_response(
        &self,
        request_id: String,
        status: u16,
        headers: HashMap<String, String>,
        body: Option<String>,
    ) -> Result<()> {
        if let Some(pending) = self.pending.write().await.remove(&request_id) {
            let body_bytes = if let Some(b) = body {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(&b).unwrap_or_default()
            } else {
                Vec::new()
            };
            
            let response = HttpResponse {
                status,
                headers,
                body: body_bytes,
            };
            
            let _ = pending.response_tx.send(response);
        }
        Ok(())
    }
    
    pub async fn cleanup_stale(&self, timeout_ms: i64) -> Result<()> {
        let now = current_time_ms();
        let cutoff = now - timeout_ms;
        
        let stale: Vec<String> = self.agents.read().await.iter()
            .filter(|(_, state)| state.last_heartbeat < cutoff)
            .map(|(id, _)| id.to_string())
            .collect();
        
        for id in stale {
            self.unregister(&id).await?;
        }
        
        self.db.cleanup_stale_agents(&self.server_id, timeout_ms as u64).await?;
        Ok(())
    }
    
    pub async fn count(&self) -> usize {
        self.agents.read().await.len()
    }
}

fn current_time_ms() -> i64 {
    std::time::SystemTime::UNIX_EPOCH
        .elapsed()
        .map_err(|e| crate::error::Error::Internal(format!("Time error: {}", e)))
        .unwrap()
        .as_millis() as i64
}

impl Clone for AgentState {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            subdomain: self.subdomain.clone(),
            local_port: self.local_port,
            local_host: self.local_host.clone(),
            protocol: self.protocol,
            last_heartbeat: self.last_heartbeat,
        }
    }
}
