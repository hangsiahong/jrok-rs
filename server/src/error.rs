use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] libsql::Error),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Tunnel not found: {0}")]
    TunnelNotFound(String),

    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Subdomain already taken: {0}")]
    SubdomainTaken(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not leader")]
    NotLeader,

    #[error("No available TCP port")]
    NoTcpPort,

    #[error("Connection timeout")]
    Timeout,

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for Error {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        Error::WebSocket(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
