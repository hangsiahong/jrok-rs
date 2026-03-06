use serde::{Deserialize, Serialize};
use crate::nat::NatInfo;

/// NAT type for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatTypeSer {
    Open,
    FullCone,
    RestrictedCone,
    PortRestrictedCone,
    Symmetric,
    Unknown,
}

impl From<crate::nat::NatType> for NatTypeSer {
    fn from(nat_type: crate::nat::NatType) -> Self {
        match nat_type {
            crate::nat::NatType::Open => NatTypeSer::Open,
            crate::nat::NatType::FullCone => NatTypeSer::FullCone,
            crate::nat::NatType::RestrictedCone => NatTypeSer::RestrictedCone,
            crate::nat::NatType::PortRestrictedCone => NatTypeSer::PortRestrictedCone,
            crate::nat::NatType::Symmetric => NatTypeSer::Symmetric,
            crate::nat::NatType::Unknown => NatTypeSer::Unknown,
        }
    }
}

impl From<NatTypeSer> for crate::nat::NatType {
    fn from(nat_type: NatTypeSer) -> Self {
        match nat_type {
            NatTypeSer::Open => crate::nat::NatType::Open,
            NatTypeSer::FullCone => crate::nat::NatType::FullCone,
            NatTypeSer::RestrictedCone => crate::nat::NatType::RestrictedCone,
            NatTypeSer::PortRestrictedCone => crate::nat::NatType::PortRestrictedCone,
            NatTypeSer::Symmetric => crate::nat::NatType::Symmetric,
            NatTypeSer::Unknown => crate::nat::NatType::Unknown,
        }
    }
}

/// Serializable NAT info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatInfoSer {
    pub local_addr: String,
    pub public_ip: String,
    pub public_port: u16,
    pub nat_type: NatTypeSer,
    pub hairpinning: bool,
}

impl From<crate::nat::NatInfo> for NatInfoSer {
    fn from(info: crate::nat::NatInfo) -> Self {
        Self {
            local_addr: info.local_addr,
            public_ip: info.public_ip,
            public_port: info.public_port,
            nat_type: info.nat_type.into(),
            hairpinning: info.hairpinning,
        }
    }
}

impl From<NatInfoSer> for crate::nat::NatInfo {
    fn from(info: NatInfoSer) -> Self {
        Self {
            local_addr: info.local_addr,
            public_ip: info.public_ip,
            public_port: info.public_port,
            nat_type: info.nat_type.into(),
            hairpinning: info.hairpinning,
        }
    }
}



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
        client_nat: Option<NatInfoSer>,  // Client's NAT info
    },

    TcpListenResponse {
        session_id: String,
        endpoint: String,  // Agent's public endpoint (ip:port)
        agent_nat: Option<NatInfoSer>,  // Agent's NAT info
    },

    ConnectionEstablished {
        session_id: String,
        direct: bool,  // true if direct connection, false if relayed
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
