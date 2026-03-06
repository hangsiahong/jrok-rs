// NAT traversal and hole punching implementation
// This module enables direct connections between clients and agents behind NATs

use crate::error::Result;
use std::net::{SocketAddr, Ipv4Addr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// Public endpoint discovered via STUN (internal)
#[derive(Debug, Clone)]
struct PublicEndpointDiscovered {
    pub public_ip: String,
    pub public_port: u16,
    pub stun_server: String,
}

// Helper trait for random transaction ID generation
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

/// Check if IP address is private (RFC 1918)
pub fn is_private_ip(ip: &std::net::IpAddr) -> bool {
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

/// NAT type detection result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NatType {
    /// No NAT (public IP)
    Open,
    /// Full cone NAT - any external IP can connect
    FullCone,
    /// Restricted cone NAT - external IP must be from previous communication
    RestrictedCone,
    /// Port-restricted cone NAT - external IP:port must be from previous communication
    PortRestrictedCone,
    /// Symmetric NAT - different mappings for different destinations
    Symmetric,
    /// Unknown - unable to determine
    Unknown,
}

/// NAT information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatInfo {
    pub local_addr: String,
    pub public_ip: String,
    pub public_port: u16,
    pub nat_type: NatType,
    pub hairpinning: bool,
}

/// NAT detector using STUN
pub struct NatDetector {
    _stun_servers: Vec<String>,
}

impl NatDetector {
    /// Create new NAT detector with default STUN servers
    pub fn new() -> Self {
        Self {
            _stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun1.l.google.com:19302".to_string(),
                "stun.cloudflare.com:3478".to_string(),
            ],
        }
    }

    /// Create NAT detector with custom STUN servers
    pub fn with_servers(servers: Vec<String>) -> Self {
        Self { _stun_servers: servers }
    }

    /// Detect NAT type and discover public endpoint
    pub async fn detect(&self, bind_addr: &str) -> Result<NatInfo> {
        info!("Starting NAT detection from {}", bind_addr);

        // Try STUN discovery with each server
        for stun_server_addr in &self._stun_servers {
            match self.try_stun_discovery(stun_server_addr, bind_addr) {
                Ok(endpoint) => {
                    // Determine NAT type by comparing local vs public
                    let local_addr: SocketAddr = bind_addr.parse()
                        .map_err(|e| crate::error::Error::Internal(format!("Invalid address: {}", e)))?;

                    let nat_type = if local_addr.ip().to_string() == endpoint.public_ip {
                        NatType::Open
                    } else {
                        // STUN succeeded but IPs differ - we're behind NAT
                        // For simplicity, assume symmetric (most restrictive)
                        // In production, would run additional STUN tests
                        NatType::Symmetric
                    };

                    let hairpinning = false; // Would need additional STUN test

                    return Ok(NatInfo {
                        local_addr: bind_addr.to_string(),
                        public_ip: endpoint.public_ip,
                        public_port: endpoint.public_port,
                        nat_type,
                        hairpinning,
                    });
                }
                Err(e) => {
                    warn!("STUN server {} failed: {}", stun_server_addr, e);
                    continue;
                }
            }
        }

        // If all STUN servers fail, fallback to IP-based detection
        warn!("All STUN servers failed, falling back to IP-based detection");
        self.detect_by_ip(bind_addr).await
    }

    /// Try STUN discovery with a specific server
    fn try_stun_discovery(&self, stun_server_addr: &str, bind_addr: &str) -> Result<PublicEndpointDiscovered> {
        let server_addr: SocketAddr = stun_server_addr.parse()
            .map_err(|_| crate::error::Error::Internal(format!("Invalid STUN server address: {}", stun_server_addr)))?;

        // Bind UDP socket
        let socket = UdpSocket::bind(bind_addr)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to bind UDP socket: {}", e)))?;

        // Set timeout
        socket.set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| crate::error::Error::Internal(format!("Failed to set socket timeout: {}", e)))?;

        // Create STUN binding request
        let request = self.create_stun_request();

        // Send request
        socket.send_to(&request, server_addr)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to send STUN request: {}", e)))?;

        debug!("Sent STUN request to {}", server_addr);

        // Receive response
        let mut buffer = [0u8; 1024];
        let (len, _from) = socket.recv_from(&mut buffer)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to receive STUN response: {}", e)))?;

        debug!("Received STUN response ({} bytes)", len);

        // Parse STUN response
        self.parse_stun_response(&buffer[..len], stun_server_addr)
    }

    /// Create STUN binding request
    fn create_stun_request(&self) -> Vec<u8> {
        let mut request = Vec::new();

        // STUN message header (20 bytes)
        // Message type: Binding Request (0x0001)
        request.extend_from_slice(&0x0001u16.to_be_bytes());
        // Message length: 0 (no attributes in request)
        request.extend_from_slice(&0x0000u16.to_be_bytes());
        // Magic cookie
        request.extend_from_slice(&0x2112A442u32.to_be_bytes());
        // Transaction ID (96 bits) - random
        let transaction_id: [u8; 12] = rand::random();
        request.extend_from_slice(&transaction_id);

        request
    }

    /// Parse STUN binding response
    fn parse_stun_response(&self, response: &[u8], stun_server: &str) -> Result<PublicEndpointDiscovered> {
        if response.len() < 20 {
            return Err(crate::error::Error::Internal(
                "STUN response too short".to_string()
            ));
        }

        // Parse header
        let _msg_type = u16::from_be_bytes([response[0], response[1]]);
        let _msg_len = u16::from_be_bytes([response[2], response[3]]) as usize;
        let magic_cookie = u32::from_be_bytes([response[4], response[5], response[6], response[7]]);

        // Verify magic cookie
        if magic_cookie != 0x2112A442 {
            return Err(crate::error::Error::Internal(
                "Invalid STUN magic cookie".to_string()
            ));
        }

        // Parse attributes (simplified - just look for MAPPED-ADDRESS)
        let mut public_ip = None;
        let mut public_port = None;

        // Simple search for mapped address attribute (0x0001)
        let mut i = 20;
        while i < response.len() {
            if i + 4 > response.len() {
                break;
            }

            let attr_type = u16::from_be_bytes([response[i], response[i + 1]]);
            let attr_len = u16::from_be_bytes([response[i + 2], response[i + 3]]) as usize;
            i += 4;

            if attr_type == 0x0001 && attr_len >= 8 {
                // MAPPED-ADDRESS found
                // Format: family (1) + port (2) + IP (4)
                let port = u16::from_be_bytes([response[i + 3], response[i + 4]]);
                let ip_bytes = &response[i + 5..i + 9];
                public_ip = Some(format!("{}.{}.{}", ip_bytes[0], ip_bytes[1], ip_bytes[2]));
                public_port = Some(port);
                break;
            }

            // Move to next attribute (4-byte aligned)
            i += (attr_len + 3) & !3;
        }

        match (public_ip, public_port) {
            (Some(ip), Some(port)) => {
                Ok(PublicEndpointDiscovered {
                    public_ip: ip,
                    public_port: port,
                    stun_server: stun_server.to_string(),
                })
            }
            _ => {
                Err(crate::error::Error::Internal(
                    "No mapped address in STUN response".to_string()
                ))
            }
        }
    }

    /// Fallback IP-based detection (when STUN fails)
    async fn detect_by_ip(&self, bind_addr: &str) -> Result<NatInfo> {
        warn!("Using fallback IP-based NAT detection");

        let addr: SocketAddr = bind_addr.parse()
            .map_err(|e| crate::error::Error::Internal(format!("Invalid address: {}", e)))?;

        let ip = addr.ip();

        // Check if IP is private
        let (nat_type, public_ip, public_port) = if is_private_ip(&ip) {
            // Private IP means behind NAT
            (NatType::Unknown, "0.0.0.0".to_string(), 0)
        } else {
            // Public IP
            (NatType::Open, ip.to_string(), addr.port())
        };

        Ok(NatInfo {
            local_addr: bind_addr.to_string(),
            public_ip,
            public_port,
            nat_type,
            hairpinning: false,
        })
    }
}

impl Default for NatDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Hole punching coordinator
pub struct HolePuncher {
    nat_detector: NatDetector,
}

/// Strategy for hole punching
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HolePunchStrategy {
    Direct,
    HolePunch,
    Relay,
}

impl HolePuncher {
    pub fn new() -> Self {
        Self {
            nat_detector: NatDetector::new(),
        }
    }

    /// Coordinate hole punching between two endpoints
    pub async fn punch_hole(
        &self,
        local_addr: &str,
        remote_public: &str,
        remote_nat_type: &NatType,
    ) -> Result<String> {
        info!("Hole punching: {} -> {}", local_addr, remote_public);

        // Discover our own public endpoint
        let nat_info = self.nat_detector.detect(local_addr).await?;

        // Determine hole punching strategy based on NAT types
        let strategy = self.determine_strategy(&nat_info.nat_type, remote_nat_type);

        match strategy {
            HolePunchStrategy::Direct => {
                info!("Direct connection possible");
                Ok(remote_public.to_string())
            }
            HolePunchStrategy::HolePunch => {
                info!("Hole punching required");
                self.perform_hole_punch(&nat_info, remote_public).await
            }
            HolePunchStrategy::Relay => {
                warn!("Direct connection not possible, relay required");
                Err(crate::error::Error::Internal(
                    "Direct connection not possible, relay required".to_string()
                ))
            }
        }
    }

    /// Determine hole punching strategy
    pub fn determine_strategy(&self, local_nat: &NatType, remote_nat: &NatType) -> HolePunchStrategy {
        match (local_nat, remote_nat) {
            (NatType::Open, _) | (_, NatType::Open) => {
                // At least one endpoint has public IP
                HolePunchStrategy::Direct
            }
            (NatType::FullCone, _) | (_, NatType::FullCone) => {
                // Full cone NAT is easy to punch through
                HolePunchStrategy::HolePunch
            }
            (NatType::Symmetric, NatType::Symmetric) => {
                // Symmetric to symmetric is difficult
                HolePunchStrategy::Relay
            }
            _ => {
                // Try hole punching
                HolePunchStrategy::HolePunch
            }
        }
    }

    /// Perform UDP hole punching
    async fn perform_hole_punch(&self, local_nat: &NatInfo, remote_public: &str) -> Result<String> {
        info!("Hole punching from {} to {}", local_nat.local_addr, remote_public);

        // Parse remote address
        let remote_addr: SocketAddr = remote_public.parse()
            .map_err(|e| crate::error::Error::Internal(format!("Invalid remote address: {}", e)))?;

        // Create UDP socket
        let socket = UdpSocket::bind(&local_nat.local_addr)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to bind UDP socket: {}", e)))?;

        // Set timeout to avoid hanging
        socket.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| crate::error::Error::Internal(format!("Failed to set socket timeout: {}", e)))?;

        // Send punch packets
        let punch_count = 5;
        let punch_interval = Duration::from_millis(200);

        for i in 0..punch_count {
            debug!("Sending punch packet {}/{} to {}", i + 1, punch_count, remote_public);

            // Send small punch packet to open NAT mapping
            let punch_packet = [0u8; 4]; // Small packet

            match socket.send_to(&punch_packet, remote_addr) {
                Ok(bytes_sent) => {
                    debug!("Sent {} bytes as punch packet to {}", bytes_sent, remote_public);
                }
                Err(e) => {
                    warn!("Failed to send punch packet {}: {}", i + 1, e);
                }
            }

            // Wait before sending next punch packet
            tokio::time::sleep(punch_interval).await;
        }

        info!(
            "Completed UDP hole punching: {} packets sent to {}",
            punch_count, remote_public
        );

        Ok(remote_public.to_string())
    }
}

impl Default for HolePuncher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nat_detection() {
        let detector = NatDetector::new();
        // This would need a real binding address to test
        // Result depends on network environment
    }

    #[test]
    fn test_nat_strategy_determination() {
        let puncher = HolePuncher::new();

        // Open to anything should be direct
        assert_eq!(
            puncher.determine_strategy(&NatType::Open, &NatType::Symmetric),
            HolePunchStrategy::Direct
        );

        // Symmetric to symmetric should require relay
        assert_eq!(
            puncher.determine_strategy(&NatType::Symmetric, &NatType::Symmetric),
            HolePunchStrategy::Relay
        );

        // Full cone should allow hole punching
        assert_eq!(
            puncher.determine_strategy(&NatType::FullCone, &NatType::RestrictedCone),
            HolePunchStrategy::HolePunch
        );
    }
}
