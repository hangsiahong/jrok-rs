// TCP tunnel support - PARTIALLY IMPLEMENTED
// Core structure is ready but full TCP forwarding is not yet implemented
// This module will be completed in a future update

use axum::{
    extract::Path,
    extract::State,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::debug;

use crate::agent::AgentRegistry;

// TCP forwarder - RESERVED FOR FUTURE USE
/*
pub struct TcpForwarder {
    registry: AgentRegistry,
    active_connections: Arc<tokio::sync::Mutex<HashMap<String, tokio::net::TcpStream>>>,
}

impl TcpForwarder {
    pub fn new(registry: AgentRegistry) -> Self {
        Self {
            registry,
            active_connections: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    // RESERVED FOR FUTURE USE
    /*
    pub async fn new(&self, connection_id: String, agent_id: String) -> Result<()> {
        // Implementation will be added when full TCP tunneling is developed
        Ok(())
    }

    pub async fn handle_tcp_connection(&self, connection_id: String) -> Result<()> {
        // Implementation will be added when full TCP tunneling is developed
        Ok(())
    }

    pub async fn forward_tcp_data(&self, connection_id: String, data: Vec<u8>) -> Result<()> {
        // Implementation will be added when full TCP tunneling is developed
        Ok(())
    }

    pub async fn close_tcp_connection(&self, connection_id: String) -> Result<()> {
        // Implementation will be added when full TCP tunneling is developed
        Ok(())
    }
    */
}
*/

// TCP tunnel endpoint - RESERVED FOR FUTURE USE
pub async fn handle_tcp_tunnel(
    Path((subdomain, tcp_port)): Path<(String, u16)>,
    State(_registry): State<Arc<AgentRegistry>>,
) -> Response {
    // This would be called when a client connects to a TCP port
    // For now, return a simple response
    debug!("TCP tunnel request for {} on port {}", subdomain, tcp_port);

    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        "TCP tunneling not yet implemented",
    )
        .into_response()
}
