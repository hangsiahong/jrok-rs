// API key management endpoints

use axum::{extract::{Path, State}, response::Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::agent::AgentRegistry;
use crate::cluster::Cluster;
use crate::db::Db;
use crate::error::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub key: String,
    pub key_prefix: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyListResponse {
    pub keys: Vec<ApiKeyInfo>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: Option<String>,
    pub key_prefix: String,
    pub created_at: i64,
}

pub async fn create_api_key(
    State((_registry, db, _cluster)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>> {
    let id = uuid::Uuid::new_v4().to_string();
    let key = format!("jrok_{}", uuid::Uuid::new_v4());
    let key_prefix: String = key.chars().take(8).collect();

    let api_key = db.create_api_key(&id, &key, Some(&req.name)).await?;

    tracing::info!("Created API key: {} ({})", api_key.key_prefix, req.name);

    Ok(Json(ApiKeyResponse {
        id: api_key.id,
        name: api_key.name.unwrap_or_default(),
        key,
        key_prefix: api_key.key_prefix,
        created_at: api_key.created_at,
    }))
}

pub async fn list_api_keys(
    State((_registry, db, _cluster)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
) -> Result<Json<ApiKeyListResponse>> {
    let keys = db.list_api_keys().await?;

    let key_infos: Vec<ApiKeyInfo> = keys
        .into_iter()
        .map(|k| ApiKeyInfo {
            id: k.id,
            name: k.name,
            key_prefix: k.key_prefix,
            created_at: k.created_at,
        })
        .collect();

    Ok(Json(ApiKeyListResponse { keys: key_infos }))
}

pub async fn delete_api_key(
    State((_registry, db, _cluster)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
    Path(id): Path<String>,
) -> Result<&'static str> {
    db.delete_api_key(&id).await?;
    tracing::info!("Deleted API key: {}", id);
    Ok("API key deleted")
}

pub async fn validate_api_key_direct(
    State((_registry, db, _cluster)): State<(Arc<AgentRegistry>, Arc<Db>, Arc<Cluster>)>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<bool>> {
    let key = req.get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::Error::Validation("Missing 'key' field".to_string()))?;

    let valid = db.validate_api_key(key).await?;
    Ok(Json(valid))
}
