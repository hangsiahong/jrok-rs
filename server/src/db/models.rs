use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ApiKey {
    pub id: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub name: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub http_host: String,
    pub tcp_host: String,
    pub started_at: i64,
    pub last_heartbeat: i64,
    pub is_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterState {
    pub leader_id: Option<String>,
    pub leader_term: i64,
    pub last_heartbeat: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Tunnel {
    pub id: String,
    pub subdomain: String,
    pub protocol: Protocol,
    pub tcp_port: Option<u16>,
    pub api_key_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub subdomain: String,
    pub server_id: String,
    pub tunnel_id: Option<String>,
    pub local_port: u16,
    pub local_host: String,
    pub protocol: Protocol,
    pub connection_token: String,
    pub connected_at: i64,
    pub last_heartbeat: i64,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Http,
    Tcp,
}

impl Default for Protocol {
    fn default() -> Self {
        Self::Http
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Http => write!(f, "http"),
            Protocol::Tcp => write!(f, "tcp"),
        }
    }
}

impl From<&str> for Protocol {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tcp" => Protocol::Tcp,
            _ => Protocol::Http,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TcpPort {
    pub port: u16,
    pub tunnel_id: String,
    pub server_id: String,
}
