#[cfg(test)]
mod tests;

use jrok::db::Db;
use jrok::agent::AgentRegistry;

#[tokio::test]
async fn test_database_connection() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
}

#[tokio::test]
async fn test_agent_registration() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let registry = AgentRegistry::new(db, "test-server");
    
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
async fn test_leader_election() {
    let db = Db::new("file::memory:", "").await.expect("Failed to connect");
    let cluster = jrok::cluster::Cluster::new(
        db,
        jrok::config::Config {
            server_id: "test-server".to_string(),
            http_host: "localhost".to_string(),
            tcp_host: "127.0.0.1".to_string(),
            base_domain: "test.com".to_string(),
            turso_url: "file::memory:".to_string(),
            turso_token: "".to_string(),
            heartbeat_interval_ms: 5000,
            leader_timeout_ms: 15000,
            agent_timeout_ms: 30000,
            http_port: 8080,
            tcp_port_start: 10000,
            tcp_port_end: 20000,
        },
    );
    
    cluster.start().await;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    assert!(cluster.is_leader());
}
