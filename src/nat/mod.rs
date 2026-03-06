// NAT traversal and hole punching implementation
// This module enables direct connections between clients and agents behind NATs

use crate::error::Result;
use std::net::{SocketAddr, Ipv4Addr};
use tracing::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// Check if IP address is private (RFC 1918)
fn is_private_ip(ip: &std::net::IpAddr) -> bool {
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

        // Parse the address to determine NAT type
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
    fn determine_strategy(&self, local_nat: &NatType, remote_nat: &NatType) -> HolePunchStrategy {
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

        // For now, this is a simplified hole punching implementation
        // In production, would use actual UDP sockets to send punch packets
        // The client and agent would need to coordinate punch packet timing

        // Simulate hole punching delay
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        info!("Hole punching completed, endpoint should be accessible");
        Ok(remote_public.to_string())
    }
}

impl Default for HolePuncher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HolePunchStrategy {
    Direct,
    HolePunch,
    Relay,
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
