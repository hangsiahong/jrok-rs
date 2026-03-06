// API routes - DISABLED due to axum version conflict
// Use CLI tool instead: cargo run --bin jrok-cli -- keys create
// This will be re-enabled once we migrate away from libsql or the conflict is resolved

pub mod keys;

/*
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::agent::AgentRegistry;
use crate::cluster::Cluster;
use crate::db::Db;
use crate::error::{Error, Result};

pub async fn handle_rejection(_err: axum::extract::rejection::JsonRejection) -> impl IntoResponse {
    (StatusCode::BAD_REQUEST, "Invalid JSON")
}
*/
