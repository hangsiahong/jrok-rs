// STUN client integration for accurate public endpoint discovery
// This module implements STUN protocol for NAT traversal

use crate::error::Result;
use std::net::{SocketAddr, Ipv4Addr, UdpSocket};
use std::time::Duration;
use tracing::{debug, info, warn};
use serde::{Deserialize, Serialize};

// STUN message types (RFC 5389)
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;

// STUN attributes
const STUN_ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const STUN_ATTR_ERROR_CODE: u16 = 0x0009;

/// STUN server configuration
#[derive(Debug, Clone)]
pub struct StunServer {
    pub addr: String,
    pub port: u16,
}

impl StunServer {
    pub fn new(addr: String, port: u16) -> Self {
        Self { addr, port }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(crate::error::Error::Internal(
                format!("Invalid STUN server address: {}", s)
            ));
        }

        let addr = parts[0].to_string();
        let port = parts[1].parse::<u16>()
            .map_err(|_| crate::error::Error::Internal(
                format!("Invalid STUN server port: {}", parts[1])
            ))?;

        Ok(Self { addr, port })
    }

    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        format!("{}:{}", self.addr, self.port).parse()
            .map_err(|_| crate::error::Error::Internal(
                format!("Invalid STUN server address: {}:{}", self.addr, self.port)
            ))
    }
}

/// Public endpoint discovered via STUN
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicEndpoint {
    pub public_ip: String,
    pub public_port: u16,
    pub local_addr: String,
    pub stun_server: String,
}

/// STUN client for public endpoint discovery
pub struct StunClient {
    servers: Vec<StunServer>,
    timeout: Duration,
}

impl StunClient {
    /// Create new STUN client with default servers
    pub fn new() -> Self {
        Self {
            servers: vec![
                StunServer::new("stun.l.google.com".to_string(), 19302),
                StunServer::new("stun1.l.google.com".to_string(), 19302),
                StunServer::new("stun2.l.google.com".to_string(), 19302),
                StunServer::new("stun.cloudflare.com".to_string(), 3478),
            ],
            timeout: Duration::from_secs(3),
        }
    }

    /// Create STUN client with custom servers
    pub fn with_servers(servers: Vec<StunServer>) -> Self {
        Self {
            servers,
            timeout: Duration::from_secs(3),
        }
    }

    /// Discover public endpoint using STUN
    pub fn discover_public_endpoint(&self, local_addr: &str) -> Result<PublicEndpoint> {
        info!("Starting STUN discovery from {}", local_addr);

        // Try each STUN server
        for stun_server in &self.servers {
            match self.try_stun_server(stun_server, local_addr) {
                Ok(endpoint) => {
                    info!("STUN discovery successful: {} (via {})", endpoint.public_ip, stun_server.addr);
                    return Ok(endpoint);
                }
                Err(e) => {
                    warn!("STUN server {} failed: {}", stun_server.addr, e);
                    continue;
                }
            }
        }

        // If all STUN servers fail, return endpoint with unknown public IP
        warn!("All STUN servers failed, returning unknown public endpoint");
        Ok(PublicEndpoint {
            public_ip: "0.0.0.0".to_string(),
            public_port: 0,
            local_addr: local_addr.to_string(),
            stun_server: "none".to_string(),
        })
    }

    /// Try specific STUN server
    fn try_stun_server(&self, stun_server: &StunServer, local_addr: &str) -> Result<PublicEndpoint> {
        let server_addr = stun_server.to_socket_addr()?;

        // Bind UDP socket
        let socket = UdpSocket::bind(local_addr)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to bind UDP socket: {}", e)))?;

        // Set timeout
        socket.set_read_timeout(Some(self.timeout))
            .map_err(|e| crate::error::Error::Internal(format!("Failed to set socket timeout: {}", e)))?;

        // Create STUN binding request
        let request = self.create_stun_binding_request();

        // Send request
        socket.send_to(&request, server_addr)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to send STUN request: {}", e)))?;

        debug!("Sent STUN request to {}", server_addr);

        // Receive response
        let mut buffer = [0u8; 1024];
        let (len, from) = socket.recv_from(&mut buffer)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to receive STUN response: {}", e)))?;

        debug!("Received STUN response from {} ({} bytes)", from, len);

        // Parse STUN response
        self.parse_stun_response(&buffer[..len], stun_server, local_addr)
    }

    /// Create STUN binding request
    fn create_stun_binding_request(&self) -> Vec<u8> {
        let mut request = Vec::new();

        // STUN message header (20 bytes)
        // Message type: Binding Request (0x0001)
        request.extend_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());
        // Message length: 0 (no attributes in request)
        request.extend_from_slice(&0u16.to_be_bytes());
        // Magic cookie
        request.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        // Transaction ID (96 bits) - random
        let transaction_id: [u8; 12] = rand::random();
        request.extend_from_slice(&transaction_id);

        request
    }

    /// Parse STUN binding response
    fn parse_stun_response(&self, response: &[u8], stun_server: &StunServer, local_addr: &str) -> Result<PublicEndpoint> {
        if response.len() < 20 {
            return Err(crate::error::Error::Internal(
                "STUN response too short".to_string()
            ));
        }

        // Parse header
        let msg_type = u16::from_be_bytes([response[0], response[1]]);
        let msg_len = u16::from_be_bytes([response[2], response[3]]) as usize;
        let magic_cookie = u32::from_be_bytes([response[4], response[5], response[6], response[7]]);

        // Verify magic cookie
        if magic_cookie != STUN_MAGIC_COOKIE {
            return Err(crate::error::Error::Internal(
                "Invalid STUN magic cookie".to_string()
            ));
        }

        // Check message type
        if msg_type != STUN_BINDING_RESPONSE {
            return Err(crate::error::Error::Internal(
                format!("Unexpected STUN message type: 0x{:04x}", msg_type)
            ));
        }

        // Extract transaction ID
        let _transaction_id = &response[8..20];

        // Parse attributes
        let mut pos = 20;
        let mut public_ip = None;
        let mut public_port = None;

        while pos < 20 + msg_len {
            if pos + 4 > response.len() {
                break;
            }

            let attr_type = u16::from_be_bytes([response[pos], response[pos + 1]]);
            let attr_len = u16::from_be_bytes([response[pos + 2], response[pos + 3]]) as usize;
            pos += 4;

            // Attribute value alignment to 4 bytes
            let attr_value_len = (attr_len + 3) & !3;

            if pos + attr_value_len > response.len() {
                break;
            }

            let attr_value = &response[pos..pos + attr_len];

            match attr_type {
                STUN_ATTR_MAPPED_ADDRESS | STUN_ATTR_XOR_MAPPED_ADDRESS => {
                    if let Ok((ip, port)) = self.parse_mapped_address(attr_value, attr_type) {
                        public_ip = Some(ip);
                        public_port = Some(port);
                    }
                }
                STUN_ATTR_ERROR_CODE => {
                    return Err(crate::error::Error::Internal(
                        format!("STUN error code attribute present")
                    ));
                }
                _ => {
                    debug!("Unknown STUN attribute type: 0x{:04x}", attr_type);
                }
            }

            pos += attr_value_len;
        }

        match (public_ip, public_port) {
            (Some(ip), Some(port)) => {
                Ok(PublicEndpoint {
                    public_ip: ip,
                    public_port: port,
                    local_addr: local_addr.to_string(),
                    stun_server: format!("{}:{}", stun_server.addr, stun_server.port),
                })
            }
            _ => {
                Err(crate::error::Error::Internal(
                    "No mapped address in STUN response".to_string()
                ))
            }
        }
    }

    /// Parse MAPPED-ADDRESS or XOR-MAPPED-ADDRESS attribute
    fn parse_mapped_address(&self, attr_value: &[u8], attr_type: u16) -> Result<(String, u16)> {
        if attr_value.len() < 8 {
            return Err(crate::error::Error::Internal(
                "Mapped address too short".to_string()
            ));
        }

        let family = attr_value[1];
        let port = u16::from_be_bytes([attr_value[2], attr_value[3]]);

        let ip = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
            // XOR mapped address - need to XOR with magic cookie
            let xored_ip = &attr_value[4..8];
            let mut ip_bytes = [0u8; 4];
            let magic_bytes = STUN_MAGIC_COOKIE.to_be_bytes();

            for i in 0..4 {
                ip_bytes[i] = xored_ip[i] ^ magic_bytes[i];
            }

            Ipv4Addr::from(ip_bytes)
        } else {
            // Regular mapped address
            Ipv4Addr::from([attr_value[4], attr_value[5], attr_value[6], attr_value[7]])
        };

        if family != 0x01 {
            return Err(crate::error::Error::Internal(
                "Only IPv4 addresses supported".to_string()
            ));
        }

        Ok((ip.to_string(), port))
    }
}

impl Default for StunClient {
    fn default() -> Self {
        Self::new()
    }
}

// Add random trait for transaction ID generation
trait Random: Sized {
    fn random() -> Self;
}

impl Random for [u8; 12] {
    fn random() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut arr = [0u8; 12];
        rng.fill(&mut arr);
        arr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stun_server_parsing() {
        let server = StunServer::from_str("stun.example.com:3478").unwrap();
        assert_eq!(server.addr, "stun.example.com");
        assert_eq!(server.port, 3478);
    }

    #[test]
    fn test_stun_server_to_socket_addr() {
        let server = StunServer::new("127.0.0.1".to_string(), 3478);
        let addr = server.to_socket_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:3478");
    }

    #[test]
    fn test_stun_request_creation() {
        let client = StunClient::new();
        let request = client.create_stun_binding_request();

        assert_eq!(request.len(), 20); // STUN header is 20 bytes
        assert_eq!(request[0], 0x00); // Message type high byte
        assert_eq!(request[1], 0x01); // Binding request
    }
}
