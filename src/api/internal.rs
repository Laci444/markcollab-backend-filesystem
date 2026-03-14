use crate::{error::AppError, AppState};
use axum::{
    body::Body, extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json,
    Router,
};
use futures_util::stream::StreamExt;
use serde_json::{json, Value};
use tracing::debug;
use uuid::Uuid;

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .route("/files/{id}/storage-reference", get(get_storage_ref))
        .route("/files/{id}/content", put(update_content))
        .with_state(state)
}

async fn get_storage_ref(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    debug!("Fetching storage reference for file: {}", id);

    let node = state
        .db
        .get_node(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("File {} not found", id)))?;

    Ok(Json(json!({
        "status": "authorized",
        "storage_key": node.storage_key.unwrap_or_else(|| "none".to_string())
    })))
}

async fn update_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Body,
) -> Result<StatusCode, AppError> {
    debug!("Streaming and updating content for file: {}", id);

    let mut node = state
        .db
        .get_node(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("File {} not found", id)))?;

    let storage_key = node
        .storage_key
        .clone()
        .unwrap_or_else(|| node.id.simple().to_string());

    let mut writer = state.storage.writer(&storage_key).await?;
    let mut size_bytes = 0;

    let mut stream = body.into_data_stream();

    while let Some(chunk) = stream.next().await {
        let bytes =
            chunk.map_err(|e| AppError::Internal(anyhow::anyhow!("Stream error: {}", e)))?;
        size_bytes += bytes.len() as i64;
        writer.write(bytes).await?;
    }

    writer.close().await?;

    if node.storage_key.is_none() || node.size_bytes != size_bytes {
        node.storage_key = Some(storage_key);
        node.size_bytes = size_bytes;
        state.db.update_node(node).await?;
    }

    Ok(StatusCode::OK)
}
