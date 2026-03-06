use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Register {
        subdomain: String,
        local_port: u16,
        local_host: String,
        protocol: String,
        api_key: String,
    },

    Welcome {
        agent_id: String,
        subdomain: String,
        protocol: String,
    },

    Error {
        message: String,
    },

    HttpRequest {
        request_id: String,
        method: String,
        path: String,
        headers: std::collections::HashMap<String, String>,
        body: Option<Vec<u8>>,
    },

    HttpResponse {
        request_id: String,
        status: u16,
        headers: std::collections::HashMap<String, String>,
        body: Option<Vec<u8>>,
    },

    Heartbeat,

    HeartbeatAck,

    // Connection facilitation messages (WebRTC-like signaling)
    TcpListenRequest {
        session_id: String,
    },

    TcpListenResponse {
        session_id: String,
        endpoint: String,  // Agent's public endpoint (ip:port)
    },

    ConnectionEstablished {
        session_id: String,
    },

    // Legacy TCP proxying messages (deprecated, will be removed)
    TcpConnect {
        connection_id: String,
        client_ip: String,
    },

    TcpData {
        connection_id: String,
        data: String,
    },

    TcpDisconnect {
        connection_id: String,
    },
}
