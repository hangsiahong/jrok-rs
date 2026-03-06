// Connection tracking and relay fallback implementation
// This module tracks connection success rates and provides TURN-style relay fallback

use crate::error::Result;
use crate::proto::Message;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tokio::net::UdpSocket;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

/// Connection attempt statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub session_id: String,
    pub subdomain: String,
    pub client_addr: String,
    pub agent_id: String,
    pub direct_attempt: bool,
    pub direct_success: bool,
    pub relay_used: bool,
    pub latency_ms: u64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

/// Connection tracker monitors success/failure rates
pub struct ConnectionTracker {
    stats: Arc<RwLock<HashMap<String, ConnectionStats>>>,
    total_attempts: Arc<AtomicUsize>,
    direct_successes: Arc<AtomicUsize>,
    relay_used: Arc<AtomicUsize>,
}

impl ConnectionTracker {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(HashMap::new())),
            total_attempts: Arc::new(AtomicUsize::new(0)),
            direct_successes: Arc::new(AtomicUsize::new(0)),
            relay_used: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Track a new connection attempt
    pub async fn track_attempt(&self, session_id: String, subdomain: String, client_addr: String, agent_id: String, direct_attempt: bool) {
        let now = current_time_ms();

        self.total_attempts.fetch_add(1, Ordering::Relaxed);

        let stats = ConnectionStats {
            session_id,
            subdomain,
            client_addr,
            agent_id,
            direct_attempt,
            direct_success: false,
            relay_used: false,
            latency_ms: 0,
            created_at: now,
            completed_at: None,
        };

        self.stats.write().await.insert(stats.session_id.clone(), stats);
    }

    /// Mark connection as successful
    pub async fn mark_success(&self, session_id: &str, latency_ms: u64, direct: bool) {
        let mut stats = self.stats.write().await;
        if let Some(stat) = stats.get_mut(session_id) {
            stat.direct_success = true;
            stat.latency_ms = latency_ms;
            stat.completed_at = Some(current_time_ms());

            if direct {
                self.direct_successes.fetch_add(1, Ordering::Relaxed);
            } else {
                self.relay_used.fetch_add(1, Ordering::Relaxed);
            }

            info!(
                "Connection succeeded: {} (direct: {}, latency: {}ms)",
                session_id, direct, latency_ms
            );
        }
    }

    /// Mark connection as failed and record relay usage
    pub async fn mark_failed(&self, session_id: &str, relay_used: bool) {
        let mut stats = self.stats.write().await;
        if let Some(stat) = stats.get_mut(session_id) {
            stat.completed_at = Some(current_time_ms());

            if relay_used {
                stat.relay_used = true;
                self.relay_used.fetch_add(1, Ordering::Relaxed);
            }

            warn!("Connection failed: {} (relay_used: {})", session_id, relay_used);
        }
    }

    /// Get success rate for direct connections
    pub async fn direct_success_rate(&self) -> f64 {
        let total = self.total_attempts.load(Ordering::Relaxed);
        let successes = self.direct_successes.load(Ordering::Relaxed);

        if total == 0 {
            return 0.0;
        }

        (successes as f64) / (total as f64)
    }

    /// Get statistics
    pub async fn get_stats(&self) -> ConnectionTrackerStats {
        let total = self.total_attempts.load(Ordering::Relaxed);
        let direct_successes = self.direct_successes.load(Ordering::Relaxed);
        let relay_used = self.relay_used.load(Ordering::Relaxed);

        ConnectionTrackerStats {
            total_attempts: total,
            direct_successes,
            relay_used,
            success_rate: if total > 0 {
                (direct_successes as f64) / (total as f64)
            } else {
                0.0
            },
        }
    }

    /// Clean up old statistics
    pub async fn cleanup_old(&self, max_age_ms: i64) {
        let now = current_time_ms();
        let cutoff = now - max_age_ms;

        let mut stats = self.stats.write().await;
        let before = stats.len();

        stats.retain(|_, stat| stat.created_at > cutoff);

        let cleaned = before - stats.len();
        if cleaned > 0 {
            info!("Cleaned up {} old connection stats", cleaned);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTrackerStats {
    pub total_attempts: usize,
    pub direct_successes: usize,
    pub relay_used: usize,
    pub success_rate: f64,
}

/// Relay server for TURN-style fallback
pub struct RelayServer {
    connections: Arc<RwLock<HashMap<String, RelayConnection>>>,
    tracker: Arc<ConnectionTracker>,
}

#[derive(Debug, Clone)]
struct RelayConnection {
    session_id: String,
    client_addr: String,
    agent_id: String,
    client_socket: Option<String>,
    agent_socket: Option<String>,
    bytes_relayed: u64,
    created_at: i64,
}

impl RelayServer {
    pub fn new(tracker: Arc<ConnectionTracker>) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            tracker,
        }
    }

    /// Create a relay connection for a session
    pub async fn create_relay(
        &self,
        session_id: String,
        client_addr: String,
        agent_id: String,
    ) -> Result<String> {
        info!("Creating relay connection: {} ({} -> {})", session_id, client_addr, agent_id);

        let connection = RelayConnection {
            session_id: session_id.clone(),
            client_addr: client_addr.clone(),
            agent_id: agent_id.clone(),
            client_socket: None,
            agent_socket: None,
            bytes_relayed: 0,
            created_at: current_time_ms(),
        };

        self.connections.write().await.insert(session_id.clone(), connection);

        // Return relay endpoint
        let relay_endpoint = format!("relay:{}", session_id);
        Ok(relay_endpoint)
    }

    /// Relay data from client to agent
    pub async fn relay_to_agent(&self, session_id: &str, data: Vec<u8>) -> Result<()> {
        let mut connections = self.connections.write().await;

        if let Some(conn) = connections.get_mut(session_id) {
            conn.bytes_relayed += data.len() as u64;

            // In a real implementation, this would:
            // 1. Find the agent's WebSocket connection
            // 2. Send the data through the WebSocket
            // 3. Handle errors and retries

            debug!("Relayed {} bytes to agent for session {}", data.len(), session_id);
            Ok(())
        } else {
            Err(crate::error::Error::BadRequest(
                "Relay connection not found".to_string()
            ))
        }
    }

    /// Relay data from agent to client
    pub async fn relay_to_client(&self, session_id: &str, data: Vec<u8>) -> Result<()> {
        let mut connections = self.connections.write().await;

        if let Some(conn) = connections.get_mut(session_id) {
            conn.bytes_relayed += data.len() as u64;

            // In a real implementation, this would:
            // 1. Find the client's connection (HTTP or WebSocket)
            // 2. Send the data to the client
            // 3. Handle streaming and backpressure

            debug!("Relayed {} bytes to client for session {}", data.len(), session_id);
            Ok(())
        } else {
            Err(crate::error::Error::BadRequest(
                "Relay connection not found".to_string()
            ))
        }
    }

    /// Close a relay connection
    pub async fn close_relay(&self, session_id: &str) {
        let conn = self.connections.write().await.remove(session_id);

        if let Some(conn) = conn {
            info!(
                "Closed relay connection: {} (bytes_relayed: {})",
                session_id, conn.bytes_relayed
            );

            // Track the relay usage
            self.tracker.mark_failed(session_id, true).await;
        }
    }

    /// Get active relay count
    pub async fn active_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Clean up old relay connections
    pub async fn cleanup_old(&self, max_age_ms: i64) {
        let now = current_time_ms();
        let cutoff = now - max_age_ms;

        let mut connections = self.connections.write().await;
        let before = connections.len();

        connections.retain(|_, conn| conn.created_at > cutoff);

        let cleaned = before - connections.len();
        if cleaned > 0 {
            info!("Cleaned up {} old relay connections", cleaned);
        }
    }
}

/// UDP hole puncher using real UDP sockets
pub struct UdpHolePuncher {
    punch_duration: Duration,
    punch_interval: Duration,
}

impl UdpHolePuncher {
    pub fn new() -> Self {
        Self {
            punch_duration: Duration::from_secs(5),
            punch_interval: Duration::from_millis(200),
        }
    }

    /// Perform UDP hole punching to a remote endpoint
    pub async fn punch_hole(
        &self,
        local_addr: &str,
        remote_addr: &str,
    ) -> Result<()> {
        info!(
            "Starting UDP hole punching: {} -> {}",
            local_addr, remote_addr
        );

        // Bind UDP socket
        let socket = UdpSocket::bind(local_addr)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Failed to bind UDP socket: {}", e)))?;

        let remote: std::net::SocketAddr = remote_addr.parse()
            .map_err(|e| crate::error::Error::Internal(format!("Invalid remote address: {}", e)))?;

        // Send punch packets
        let start = SystemTime::now();
        let mut punch_count = 0;

        while start.elapsed().unwrap_or(Duration::from_secs(0)) < self.punch_duration {
            // Send small punch packet
            let punch_packet = [0u8; 1]; // Empty packet

            socket.send_to(&punch_packet, remote)
                .await
                .map_err(|e| {
                    warn!("Failed to send punch packet: {}", e);
                    e
                })
                .ok();

            punch_count += 1;
            debug!("Sent punch packet {} to {}", punch_count, remote_addr);

            tokio::time::sleep(self.punch_interval).await;
        }

        info!(
            "Completed UDP hole punching: {} packets sent to {}",
            punch_count, remote_addr
        );

        Ok(())
    }

    /// Listen for incoming connections after hole punching
    pub async fn listen_for_connection(
        &self,
        local_addr: &str,
        timeout_ms: u64,
    ) -> Result<Vec<u8>> {
        info!("Listening for incoming connection on {}", local_addr);

        let socket = UdpSocket::bind(local_addr)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Failed to bind UDP socket: {}", e)))?;

        let mut buf = [0u8; 65536];
        let timeout = Duration::from_millis(timeout_ms);

        match tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) => {
                info!("Received {} bytes from {}", len, src);
                Ok(buf[..len].to_vec())
            }
            Ok(Err(e)) => {
                Err(crate::error::Error::Internal(format!("Receive error: {}", e)))
            }
            Err(_) => {
                Err(crate::error::Error::Internal("Timeout waiting for connection".to_string()))
            }
        }
    }
}

impl Default for UdpHolePuncher {
    fn default() -> Self {
        Self::new()
    }
}

fn current_time_ms() -> i64 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_err(|e| crate::error::Error::Internal(format!("Time error: {}", e)))
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_tracker() {
        let tracker = ConnectionTracker::new();

        tracker.track_attempt(
            "test-session".to_string(),
            "test.example.com".to_string(),
            "192.168.1.100:12345".to_string(),
            "agent-1".to_string(),
            true,
        ).await;

        tracker.mark_success("test-session", 50, true).await;

        let stats = tracker.get_stats().await;
        assert_eq!(stats.total_attempts, 1);
        assert_eq!(stats.direct_successes, 1);
        assert_eq!(stats.success_rate, 1.0);
    }

    #[tokio::test]
    async fn test_relay_server() {
        let tracker = Arc::new(ConnectionTracker::new());
        let relay = RelayServer::new(tracker);

        let endpoint = relay.create_relay(
            "test-session".to_string(),
            "192.168.1.100:12345".to_string(),
            "agent-1".to_string(),
        ).await.unwrap();

        assert!(endpoint.starts_with("relay:"));
        assert_eq!(relay.active_count().await, 1);

        relay.close_relay("test-session").await;
        assert_eq!(relay.active_count().await, 0);
    }
}
