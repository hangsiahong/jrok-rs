// API routes - TEMPORARILY DISABLED due to axum version conflict
// This code will be re-enabled when we resolve the libsql/tonic dependency issue
// The database functions in db/mod.rs are still used for authentication

/*
use axum::response::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    State(db): State<Arc<Db>>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>> {
    let id = uuid::Uuid::new_v4().to_string();
    let key = format!("jrok_{}", uuid::Uuid::new_v4());
    let key_prefix = key.chars().take(8).collect();

    let api_key = db.create_api_key(&id, &key, Some(&req.name)).await?;

    Ok(Json(ApiKeyResponse {
        id: api_key.id,
        name: api_key.name.unwrap_or_default(),
        key,
        key_prefix: api_key.key_prefix,
        created_at: api_key.created_at,
    }))
}

pub async fn list_api_keys(
    State(db): State<Arc<Db>>,
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
    State(db): State<Arc<Db>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<&'static str> {
    db.delete_api_key(&id).await?;
    Ok("API key deleted")
}

pub async fn validate_api_key_direct(
    State(db): State<Arc<Db>>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<bool>> {
    let key = req.get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::Error::Validation("Missing 'key' field".to_string()))?;

    let valid = db.validate_api_key(key).await?;
    Ok(Json(valid))
}
*/
