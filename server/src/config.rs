use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub server_id: String,
    pub http_host: String,
    pub http_port: u16,
    pub tcp_host: String,
    #[allow(dead_code)]
    pub tcp_port_start: u16,
    #[allow(dead_code)]
    pub tcp_port_end: u16,
    #[allow(dead_code)]
    pub base_domain: String,
    pub turso_url: String,
    pub turso_token: String,
    pub heartbeat_interval_ms: u64,
    pub leader_timeout_ms: u64,
    pub agent_timeout_ms: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            server_id: env::var("SERVER_ID").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
            http_host: env::var("HTTP_HOST").expect("HTTP_HOST required"),
            http_port: env::var("HTTP_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("Invalid HTTP_PORT"),
            tcp_host: env::var("TCP_HOST").expect("TCP_HOST required"),
            tcp_port_start: env::var("TCP_PORT_START")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()
                .expect("Invalid TCP_PORT_START"),
            tcp_port_end: env::var("TCP_PORT_END")
                .unwrap_or_else(|_| "20000".to_string())
                .parse()
                .expect("Invalid TCP_PORT_END"),
            base_domain: env::var("BASE_DOMAIN").expect("BASE_DOMAIN required"),
            turso_url: env::var("TURSO_URL").expect("TURSO_URL required"),
            turso_token: env::var("TURSO_TOKEN").expect("TURSO_TOKEN required"),
            heartbeat_interval_ms: env::var("HEARTBEAT_INTERVAL_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()
                .expect("Invalid HEARTBEAT_INTERVAL_MS"),
            leader_timeout_ms: env::var("LEADER_TIMEOUT_MS")
                .unwrap_or_else(|_| "15000".to_string())
                .parse()
                .expect("Invalid LEADER_TIMEOUT_MS"),
            agent_timeout_ms: env::var("AGENT_TIMEOUT_MS")
                .unwrap_or_else(|_| "30000".to_string())
                .parse()
                .expect("Invalid AGENT_TIMEOUT_MS"),
        }
    }

    #[allow(dead_code)]
    pub fn tcp_port_range(&self) -> std::ops::Range<u16> {
        self.tcp_port_start..self.tcp_port_end
    }
}
