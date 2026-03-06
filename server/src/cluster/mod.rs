use crate::agent::AgentRegistry;
use crate::config::Config;
use crate::db::Db;
use crate::error::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, error};

#[derive(Clone)]
pub struct Cluster {
    db: Arc<Db>,
    config: Config,
    agent_registry: Arc<AgentRegistry>,
    is_leader: Arc<AtomicBool>,
}

impl Cluster {
    pub fn new(db: Arc<Db>, config: Config, agent_registry: Arc<AgentRegistry>) -> Self {
        Self {
            db,
            config,
            agent_registry,
            is_leader: Arc::new(AtomicBool::new(false)),
        }
    }
    
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::Relaxed)
    }
    
    pub fn leader_arc(&self) -> Arc<AtomicBool> {
        self.is_leader.clone()
    }
    
    pub async fn start(&self) {
        self.register_server().await.expect("Failed to register server");
        
        tokio::spawn(Self::heartbeat_loop(
            self.db.clone(),
            self.config.clone(),
            self.is_leader.clone(),
        ));
        
        tokio::spawn(Self::cleanup_loop(
            self.db.clone(),
            self.config.clone(),
            self.agent_registry.clone(),
        ));
    }
    
    async fn register_server(&self) -> Result<()> {
        self.db.register_server(&self.config.server_id, &self.config.http_host, &self.config.tcp_host).await?;
        info!("Server {} registered", self.config.server_id);
        Ok(())
    }
    
    async fn heartbeat_loop(db: Arc<Db>, config: Config, is_leader: Arc<AtomicBool>) {
        let mut tick = interval(Duration::from_millis(config.heartbeat_interval_ms));
        
        loop {
            tick.tick().await;
            
            if let Err(e) = db.send_heartbeat(&config.server_id).await {
                error!("Failed to send heartbeat: {}", e);
                continue;
            }
            
            let now = std::time::SystemTime::UNIX_EPOCH
                .elapsed()
                .map_err(|e| crate::error::Error::Internal(format!("Time error: {}", e)))
                .unwrap()
                .as_millis() as i64;
            
            match db.get_cluster_state().await {
                Ok(Some(state)) => {
                    let leader_expired = now - state.last_heartbeat > config.leader_timeout_ms as i64;
                    
                    if state.leader_id.is_none() || leader_expired {
                        if Self::try_become_leader(&db, &config, now).await {
                            is_leader.store(true, Ordering::Relaxed);
                            info!("Became leader (term {})", state.leader_term + 1);
                        }
                    } else if state.leader_id.as_deref() == Some(config.server_id.as_str()) {
                        if Self::renew_leadership(&db, &config, now).await {
                            is_leader.store(true, Ordering::Relaxed);
                        } else {
                            is_leader.store(false, Ordering::Relaxed);
                        }
                    } else {
                        is_leader.store(false, Ordering::Relaxed);
                    }
                }
                Ok(None) => {
                    if Self::try_become_leader(&db, &config, now).await {
                        is_leader.store(true, Ordering::Relaxed);
                        info!("Became leader (initial)");
                    }
                }
                Err(e) => {
                    error!("Failed to get cluster state: {}", e);
                }
            }
        }
    }
    
    async fn try_become_leader(db: &Db, config: &Config, now: i64) -> bool {
        let new_term = match db.get_cluster_state().await {
            Ok(Some(state)) => state.leader_term + 1,
            _ => 1,
        };
        
        match db.become_leader(&config.server_id, new_term, now).await {
            Ok(true) => true,
            _ => false,
        }
    }
    
    async fn renew_leadership(db: &Db, config: &Config, now: i64) -> bool {
        match db.renew_leadership(&config.server_id, now).await {
            Ok(true) => true,
            _ => false,
        }
    }
    
    async fn cleanup_loop(db: Arc<Db>, config: Config, agent_registry: Arc<AgentRegistry>) {
        let mut tick = interval(Duration::from_secs(30));

        loop {
            tick.tick().await;

            if let Err(e) = db.mark_servers_unhealthy(config.leader_timeout_ms as u64).await {
                error!("Failed to mark unhealthy servers: {}", e);
            }

            if let Err(e) = agent_registry.cleanup_stale(config.agent_timeout_ms as i64).await {
                error!("Failed to cleanup stale agents from registry: {}", e);
            }

            if let Err(e) = db.cleanup_stale_agents(&config.server_id, config.agent_timeout_ms).await {
                error!("Failed to cleanup stale agents from database: {}", e);
            }

            let count = agent_registry.count().await;
            info!("Active agents: {}", count);
        }
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        self.db.mark_server_unhealthy(&self.config.server_id).await?;
        self.db.unregister_all_agents(&self.config.server_id).await?;
        info!("Server {} shutdown complete", self.config.server_id);
        Ok(())
    }
}
