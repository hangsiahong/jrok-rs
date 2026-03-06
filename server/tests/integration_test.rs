#[cfg(test)]
mod tests;

use jrok::db::{Db, Protocol};
use jrok::agent::AgentRegistry;
use std::sync::Arc;

#[tokio::test]
async fn test_database_connection() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
}

#[tokio::test]
async fn test_agent_registration() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let registry = AgentRegistry::new(db.clone(), "test-server".to_string());

    let agent_id = registry.register(
        "test-subdomain".to_string(),
        3000,
        "localhost".to_string(),
        Protocol::Http,
    ).await.expect("Failed to register");

    let agent = registry.get_by_subdomain("test-subdomain").await;
    assert!(agent.is_some());
}

#[tokio::test]
async fn test_tcp_agent_registration() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let registry = AgentRegistry::new(db.clone(), "test-server".to_string());

    let agent_id = registry.register(
        "tcp-test".to_string(),
        22,
        "localhost".to_string(),
        Protocol::Tcp,
    ).await.expect("Failed to register TCP agent");

    let agent = registry.get_by_subdomain("tcp-test").await;
    assert!(agent.is_some());

    // Verify it's a TCP agent
    if let Some((_id, agent_state)) = agent {
        assert_eq!(agent_state.protocol, Protocol::Tcp);
        assert_eq!(agent_state.local_port, 22);
    }
}

#[tokio::test]
async fn test_agent_heartbeat() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let registry = AgentRegistry::new(db.clone(), "test-server".to_string());

    let agent_id = registry.register(
        "heartbeat-test".to_string(),
        3000,
        "localhost".to_string(),
        Protocol::Http,
    ).await.expect("Failed to register");

    // Simulate heartbeat
    registry.update_heartbeat(&agent_id).await.expect("Failed to update heartbeat");

    // Verify agent is still registered
    let agent = registry.get_by_subdomain("heartbeat-test").await;
    assert!(agent.is_some());
}

#[tokio::test]
async fn test_agent_cleanup() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let registry = AgentRegistry::new(db.clone(), "test-server".to_string());

    let agent_id = registry.register(
        "cleanup-test".to_string(),
        3000,
        "localhost".to_string(),
        Protocol::Http,
    ).await.expect("Failed to register");

    // Cleanup agents with very short timeout
    registry.cleanup_stale(1).await.expect("Failed to cleanup");

    // Agent should still be there (heartbeat just happened)
    let agent = registry.get_by_subdomain("cleanup-test").await;
    assert!(agent.is_some());
}

#[tokio::test]
async fn test_cross_server_routing() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let registry = AgentRegistry::new(db.clone(), "server-1".to_string());

    // Register agent on server-1
    let _agent_id = registry.register(
        "cross-server-test".to_string(),
        3000,
        "localhost".to_string(),
        Protocol::Http,
    ).await.expect("Failed to register");

    // Query which server has the agent
    let server_id = registry.get_agent_server("cross-server-test").await.expect("Failed to query");
    assert_eq!(server_id, Some("server-1".to_string()));
}

#[tokio::test]
async fn test_invalid_api_key() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");

    // Test with invalid key
    let result = db.validate_api_key("invalid_key").await.expect("Validation failed");
    assert_eq!(result, false);
}

#[tokio::test]
async fn test_api_key_creation() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");

    let api_key = db.create_api_key(
        "test-key-id",
        "jrok_test_secret_key",
        Some("Test Key"),
    ).await.expect("Failed to create API key");

    assert_eq!(api_key.id, "test-key-id");
    assert_eq!(api_key.name, Some("Test Key".to_string()));

    // Verify the key can be validated
    let result = db.validate_api_key("jrok_test_secret_key").await.expect("Validation failed");
    assert_eq!(result, true);
}

#[tokio::test]
async fn test_tcp_port_allocation() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");

    // Allocate a TCP port
    let port: Option<jrok::db::TcpPort> = db.allocate_tcp_port(
        "tunnel-1",
        "server-1",
        10000,
        10010,
    ).await.expect("Failed to allocate port");

    assert!(port.is_some());
    if let Some(tcp_port) = port {
        assert_eq!(tcp_port.port, 10000);
    }

    // Try to allocate again - should get next port
    let port2: Option<jrok::db::TcpPort> = db.allocate_tcp_port(
        "tunnel-2",
        "server-1",
        10000,
        10010,
    ).await.expect("Failed to allocate port");

    assert!(port2.is_some());
}

// Manual testing guide
/*
**To run these tests:**
```bash
cargo test
```

**To run specific tests:**
```bash
cargo test test_agent_registration
cargo test test_tcp_agent_registration
```

**To run with output:**
```bash
cargo test -- --nocapture
```

**Manual Integration Testing:**

1. **Start Server:**
   ```bash
   cargo run --bin jrok-server
   ```

2. **Register Agent (using websocat):**
   ```bash
   # Install websocat: cargo install websocat
   websocat ws://localhost:8080/ws/agent
   # Paste this message:
   {"type":"register","subdomain":"test","local_port":3000,"local_host":"localhost","protocol":"http","api_key":"your-key"}
   ```

3. **Test HTTP Tunnel:**
   ```bash
   # In another terminal, start a simple HTTP server
   python3 -m http.server 3000

   # Access via tunnel
   curl http://localhost:8080/test/
   ```

4. **Test TCP Tunnel:**
   ```bash
   # Register TCP agent
   websocat ws://localhost:8080/ws/agent
   {"type":"register","subdomain":"ssh","local_port":22,"local_host":"localhost","protocol":"tcp","api_key":"your-key"}

   # Connect to allocated TCP port
   ssh -p 10000 user@localhost
   ```
*/
