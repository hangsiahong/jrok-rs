// API routes - TEMPORARILY DISABLED due to axum version conflict
// This code will be re-enabled when we resolve the libsql/tonic dependency issue

pub mod keys;

// TEMPORARILY DISABLED
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
use crate::db::Db;
use crate::error::{Error, Result};

pub async fn handle_rejection(_err: axum::extract::rejection::JsonRejection) -> impl IntoResponse {
    (StatusCode::BAD_REQUEST, "Invalid JSON")
}
*/
