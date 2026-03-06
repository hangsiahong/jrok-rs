mod models;

pub use models::*;

use crate::error::{Error, Result};
use libsql::{params, Connection, Builder};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const MIGRATION_SQL: &str = include_str!("../../migrations/001_init.sql");

#[derive(Clone)]
pub struct Db {
    conn: Connection,
}

impl Db {
    pub async fn new(url: &str, token: &str) -> Result<Self> {
        let conn = Builder::new_remote(url.to_string(), token.to_string())
            .build()
            .await?
            .connect()?;
        
        let db = Self { conn };
        db.run_migrations().await?;
        
        Ok(db)
    }
    
    async fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(MIGRATION_SQL).await?;
        Ok(())
    }
    
    pub async fn register_server(&self, id: &str, http_host: &str, tcp_host: &str) -> Result<()> {
        let now = current_time_ms();
        self.conn
            .execute(
                "INSERT INTO servers (id, http_host, tcp_host, started_at, last_heartbeat, is_healthy)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)
                 ON CONFLICT(id) DO UPDATE SET last_heartbeat = ?5, is_healthy = 1",
                params![id, http_host, tcp_host, now, now, now],
            )
            .await?;
        Ok(())
    }
    
    pub async fn send_heartbeat(&self, server_id: &str) -> Result<()> {
        let now = current_time_ms();
        self.conn
            .execute(
                "UPDATE servers SET last_heartbeat = ?1 WHERE id = ?2",
                params![now, server_id],
            )
            .await?;
        Ok(())
    }
    
    pub async fn mark_servers_unhealthy(&self, timeout_ms: u64) -> Result<()> {
        let cutoff = current_time_ms() - timeout_ms as i64;
        self.conn
            .execute(
                "UPDATE servers SET is_healthy = 0 WHERE last_heartbeat < ?1 AND is_healthy = 1",
                params![cutoff],
            )
            .await?;
        
        self.conn
            .execute(
                "UPDATE agents SET active = 0 WHERE server_id IN (SELECT id FROM servers WHERE is_healthy = 0) AND active = 1",
                params![],
            )
            .await?;
        
        Ok(())
    }
    
    pub async fn mark_server_unhealthy(&self, server_id: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE servers SET is_healthy = 0 WHERE id = ?1",
                params![server_id],
            )
            .await?;
        Ok(())
    }
    
    pub async fn get_server(&self, server_id: &str) -> Result<Option<Server>> {
        let mut rows = self.conn
            .query(
                "SELECT id, http_host, tcp_host, started_at, last_heartbeat, is_healthy FROM servers WHERE id = ?1",
                params![server_id],
            )
            .await?;
        
        if let Some(row) = rows.next().await? {
            let server = Server {
                id: row.get::<String>(0)?,
                http_host: row.get::<String>(1)?,
                tcp_host: row.get::<String>(2)?,
                started_at: row.get::<i64>(3)?,
                last_heartbeat: row.get::<i64>(4)?,
                is_healthy: row.get::<i64>(5)? == 1,
            };
            return Ok(Some(server));
        }
        
        Ok(None)
    }
    
    pub async fn get_cluster_state(&self) -> Result<Option<ClusterState>> {
        let mut rows = self.conn
            .query(
                "SELECT leader_id, leader_term, last_heartbeat FROM cluster_state WHERE id = 1",
                params![],
            )
            .await?;
        
        if let Some(row) = rows.next().await? {
            let leader_id: Option<String> = match row.get::<Option<String>>(0)? {
                Some(s) if s.is_empty() => None,
                other => other,
            };
            
            return Ok(Some(ClusterState {
                leader_id,
                leader_term: row.get::<i64>(1)?,
                last_heartbeat: row.get::<i64>(2)?,
            }));
        }
        
        Ok(None)
    }
    
    pub async fn become_leader(&self, server_id: &str, term: i64, now: i64) -> Result<bool> {
        let result = self.conn
            .execute(
                "UPDATE cluster_state SET leader_id = ?1, leader_term = ?2, last_heartbeat = ?3
                 WHERE id = 1 AND (leader_id IS NULL OR last_heartbeat < ?4)",
                params![server_id, term, now, now - 15000],
            )
            .await?;
        
        Ok(result > 0)
    }
    
    pub async fn renew_leadership(&self, server_id: &str, now: i64) -> Result<bool> {
        let result = self.conn
            .execute(
                "UPDATE cluster_state SET last_heartbeat = ?1 WHERE id = 1 AND leader_id = ?2",
                params![now, server_id],
            )
            .await?;
        
        Ok(result > 0)
    }
    
    pub async fn create_api_key(&self, id: &str, key: &str, name: Option<&str>) -> Result<ApiKey> {
        let key_hash = hash_api_key(key);
        let key_prefix = key_prefix(key);
        let now = current_time_ms();
        
        self.conn
            .execute(
                "INSERT INTO api_keys (id, key_hash, key_prefix, name, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, key_hash.clone(), key_prefix.clone(), name, now],
            )
            .await?;
        
        Ok(ApiKey {
            id: id.to_string(),
            key_hash,
            key_prefix,
            name: name.map(String::from),
            created_at: now,
        })
    }
    
    pub async fn validate_api_key(&self, key: &str) -> Result<bool> {
        let key_hash = hash_api_key(key);
        let mut rows = self.conn
            .query(
                "SELECT id FROM api_keys WHERE key_hash = ?1",
                params![key_hash],
            )
            .await?;

        Ok(rows.next().await?.is_some())
    }

    pub async fn list_api_keys(&self) -> Result<Vec<ApiKey>> {
        let mut rows = self.conn
            .query(
                "SELECT id, key_hash, key_prefix, name, created_at FROM api_keys ORDER BY created_at DESC",
                params![],
            )
            .await?;

        let mut keys = Vec::new();
        while let Some(row) = rows.next().await? {
            keys.push(ApiKey {
                id: row.get::<String>(0)?,
                key_hash: row.get::<String>(1)?,
                key_prefix: row.get::<String>(2)?,
                name: row.get::<Option<String>>(3)?,
                created_at: row.get::<i64>(4)?,
            });
        }

        Ok(keys)
    }

    pub async fn delete_api_key(&self, id: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM api_keys WHERE id = ?1",
                params![id],
            )
            .await?;

        Ok(())
    }

    pub async fn get_agent_server(&self, subdomain: &str) -> Result<Option<String>> {
        let mut rows = self.conn
            .query(
                "SELECT server_id FROM agents WHERE subdomain = ?1 AND active = 1 ORDER BY connected_at DESC LIMIT 1",
                params![subdomain],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let server_id = row.get::<String>(0)?;
            return Ok(Some(server_id));
        }

        Ok(None)
    }

    pub async fn get_agent_by_subdomain(&self, subdomain: &str) -> Result<Option<Agent>> {
        let mut rows = self.conn
            .query(
                "SELECT id, subdomain, server_id, tunnel_id, local_port, local_host, protocol, connection_token, connected_at, last_heartbeat, active
                 FROM agents WHERE subdomain = ?1 AND active = 1 ORDER BY connected_at DESC LIMIT 1",
                params![subdomain],
            )
            .await?;
        
        if let Some(row) = rows.next().await? {
            return Ok(Some(Agent {
                id: row.get::<String>(0)?,
                subdomain: row.get::<String>(1)?,
                server_id: row.get::<String>(2)?,
                tunnel_id: Some(row.get::<String>(3)?),
                local_port: row.get::<i64>(4)? as u16,
                local_host: row.get::<String>(5)?,
                protocol: Protocol::from(row.get::<String>(6)?.as_str()),
                connection_token: row.get::<String>(7)?,
                connected_at: row.get::<i64>(8)?,
                last_heartbeat: row.get::<i64>(9)?,
                active: row.get::<i64>(10)? != 0,
            }));
        }
        
        Ok(None)
    }
    
    pub async fn get_agent_by_id(&self, id: &str) -> Result<Option<Agent>> {
        let mut rows = self.conn
            .query(
                "SELECT id, subdomain, server_id, tunnel_id, local_port, local_host, protocol, connection_token, connected_at, last_heartbeat, active
                 FROM agents WHERE id = ?1",
                params![id],
            )
            .await?;
        
        if let Some(row) = rows.next().await? {
            return Ok(Some(Agent {
                id: row.get::<String>(0)?,
                subdomain: row.get::<String>(1)?,
                server_id: row.get::<String>(2)?,
                tunnel_id: Some(row.get::<String>(3)?),
                local_port: row.get::<i64>(4)? as u16,
                local_host: row.get::<String>(5)?,
                protocol: Protocol::from(row.get::<String>(6)?.as_str()),
                connection_token: row.get::<String>(7)?,
                connected_at: row.get::<i64>(8)?,
                last_heartbeat: row.get::<i64>(9)?,
                active: row.get::<i64>(10)? != 0,
            }));
        }
        
        Ok(None)
    }
    
    pub async fn register_agent(&self, agent: &Agent) -> Result<()> {
        let now = current_time_ms();
        
        self.conn
            .execute(
                "INSERT INTO agents (id, subdomain, server_id, tunnel_id, local_port, local_host, protocol, connection_token, connected_at, last_heartbeat, active)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1)
                ON CONFLICT(subdomain) DO UPDATE SET 
                    server_id = excluded.server_id,
                    tunnel_id = excluded.tunnel_id,
                    local_port = excluded.local_port,
                    local_host = excluded.local_host,
                    protocol = excluded.protocol,
                    connection_token = excluded.connection_token,
                    connected_at = excluded.connected_at,
                    last_heartbeat = ?10,
                    active = 1
                WHERE subdomain = ?2",
                params![
                    agent.id.clone(),
                    agent.subdomain.clone(),
                    agent.server_id.clone(),
                    agent.tunnel_id.clone(),
                    agent.local_port as i64,
                    agent.local_host.clone(),
                    agent.protocol.to_string(),
                    agent.connection_token.clone(),
                    agent.connected_at,
                    now,
                    now,
                    agent.subdomain.clone(),
                ],
            )
            .await?;
        
        Ok(())
    }
    
    pub async fn unregister_agent(&self, id: &str) -> Result<()> {
        let now = current_time_ms();
        self.conn
            .execute(
                "UPDATE agents SET active = 0, last_heartbeat = ?1 WHERE id = ?2",
                params![now, id],
            )
            .await?;
        Ok(())
    }
    
    pub async fn unregister_all_agents(&self, server_id: &str) -> Result<()> {
        let now = current_time_ms();
        self.conn
            .execute(
                "UPDATE agents SET active = 0, last_heartbeat = ?1 WHERE server_id = ?2",
                params![now, server_id],
            )
            .await?;
        Ok(())
    }
    
    pub async fn send_agent_heartbeat(&self, id: &str) -> Result<()> {
        let now = current_time_ms();
        self.conn
            .execute(
                "UPDATE agents SET last_heartbeat = ?1 WHERE id = ?2",
                params![now, id],
            )
            .await?;
        Ok(())
    }
    
    pub async fn cleanup_stale_agents(&self, server_id: &str, timeout_ms: u64) -> Result<u64> {
        let cutoff = current_time_ms() - timeout_ms as i64;
        
        let result = self.conn
            .execute(
                "UPDATE agents SET active = 0 WHERE server_id = ?1 AND last_heartbeat < ?2 AND active = 1",
                params![server_id, cutoff],
            )
            .await?;
        
        Ok(result)
    }
    
    pub async fn create_tunnel(&self, tunnel: &Tunnel) -> Result<()> {
        let now = current_time_ms();
        
        self.conn
            .execute(
                "INSERT INTO tunnels (id, subdomain, protocol, tcp_port, api_key_id, created_at, updated_at, active)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)
                ON CONFLICT(subdomain) DO UPDATE SET 
                    protocol = excluded.protocol,
                    tcp_port = excluded.tcp_port,
                    updated_at = ?7,
                    active = 1
                WHERE subdomain = ?2",
                params![
                    tunnel.id.clone(),
                    tunnel.subdomain.clone(),
                    tunnel.protocol.to_string(),
                    tunnel.tcp_port.map(|p| p as i64),
                    tunnel.api_key_id.clone(),
                    now,
                    now,
                    now,
                    tunnel.subdomain.clone(),
                ],
            )
            .await?;
        
        Ok(())
    }
    
    pub async fn get_tunnel_by_subdomain(&self, subdomain: &str) -> Result<Option<Tunnel>> {
        let mut rows = self.conn
            .query(
                "SELECT id, subdomain, protocol, tcp_port, api_key_id, created_at, updated_at, active
                 FROM tunnels WHERE subdomain = ?1",
                params![subdomain],
            )
            .await?;
        
        if let Some(row) = rows.next().await? {
            return Ok(Some(Tunnel {
                id: row.get::<String>(0)?,
                subdomain: row.get::<String>(1)?,
                protocol: Protocol::from(row.get::<String>(2)?.as_str()),
                tcp_port: row.get::<Option<i64>>(3)?.map(|p| p as u16),
                api_key_id: row.get::<Option<String>>(4)?,
                created_at: row.get::<i64>(5)?,
                updated_at: row.get::<i64>(6)?,
                active: row.get::<i64>(7)? != 0,
            }));
        }
        
        Ok(None)
    }
    
    pub async fn allocate_tcp_port(&self, tunnel_id: &str, server_id: &str, start_port: u16, end_port: u16) -> Result<Option<TcpPort>> {
        for port in start_port..end_port {
            let result = self.conn
                .execute(
                    "INSERT INTO tcp_ports (port, tunnel_id, server_id) VALUES (?1, ?2, ?3)",
                    params![port as i64, tunnel_id, server_id],
                )
                .await;
            
            if result.is_ok() {
                return Ok(Some(TcpPort {
                    port,
                    tunnel_id: tunnel_id.to_string(),
                    server_id: server_id.to_string(),
                }));
            }
        }
        
        Ok(None)
    }
    
    pub async fn deallocate_tcp_port(&self, port: u16) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM tcp_ports WHERE port = ?1",
                params![port as i64],
            )
            .await?;
        Ok(())
    }
}

fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Error::Internal(format!("Time error: {}", e)))
        .unwrap()
        .as_millis() as i64
}

fn hash_api_key(key: &str) -> String {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(b"jrok-api-key-v1")
        .map_err(|e| Error::Internal(format!("HMAC error: {}", e)))
        .unwrap();
    mac.update(key.as_bytes());
    let result = mac.finalize();
    base64::engine::general_purpose::STANDARD.encode(result.into_bytes())
}

#[allow(dead_code)]
fn key_prefix(key: &str) -> String {
    key.chars().take(8).collect()
}
