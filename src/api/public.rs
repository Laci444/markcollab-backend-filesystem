use super::utils::{
    generate_path, get_node_or_virtual_root, verify_access, RequiredAccess, TargetParent,
};
use crate::{
    api::extractors::{RequireOwner, RequireParentWrite, RequireRead, RequireWrite},
    auth::CurrentUser,
    db::models::Permission,
    db::{
        models::AccessLevel,
        models::{Node, NodeType},
    },
    error::AppError,
    AppState,
};
use axum::{
    body::Body, extract::Multipart,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::delete,
    routing::put,
    routing::{get, post},
    Json,
    Router,
};
use chrono::Utc;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{json, Value};
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/folders", get(get_root_folder).post(create_folder))
        .route(
            "/folders/{id}",
            get(get_folder).patch(update_node).delete(delete_node),
        )
        .route("/folders/{id}/access", put(update_access))
        .route("/files", post(create_file))
        .route(
            "/files/{id}",
            get(get_file).patch(update_node).delete(delete_node),
        )
        .route("/files/{id}/content", get(download_file))
        .route("/files/{id}/access", put(update_access))
        .route("/files/{id}/room", post(open_room))
        .route(
            "/files/{id}/permissions",
            get(get_permission).post(grant_permission),
        )
        .route(
            "/files/{id}/permissions/{user_id}",
            delete(remove_permission),
        )
        .with_state(state)
}

#[derive(Deserialize, Debug)]
pub struct FormatQuery {
    pub format: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct CreateNodeReq {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct UpdateNodeReq {
    pub name: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateAccessReq {
    pub public_access_level: AccessLevel,
}

#[derive(Deserialize, Debug)]
pub struct GrantPermReq {
    pub user_id: Uuid,
    pub access_level: AccessLevel,
}

async fn get_root_folder(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<FormatQuery>,
) -> Result<Json<Value>, AppError> {
    debug!("Fetching root folder for user: {}", user.id);

    let node = get_node_or_virtual_root(&state, user.id, user.id).await?;

    if query.format.as_deref() == Some("metadata") {
        return Ok(Json(json!(node)));
    }

    let children = state.db.get_children(&node.path).await?;

    Ok(Json(json!({
        "metadata": node,
        "children": children
    })))
}

async fn create_folder(
    RequireParentWrite(target_parent): RequireParentWrite,
    State(state): State<AppState>,
    user: CurrentUser,
    Json(payload): Json<CreateNodeReq>,
) -> Result<(StatusCode, Json<Node>), AppError> {
    debug!("Creating new folder: {}", payload.name);

    let id = Uuid::new_v4();

    if let TargetParent::Root = target_parent {
        state.db.get_or_create_root(user.id).await?;
    }

    let path = generate_path(&target_parent, id, user.id);

    let node = Node {
        id,
        owner_id: user.id,
        node_type: NodeType::Folder,
        name: payload.name,
        path,
        storage_key: None,
        size_bytes: 0,
        public_access_level: AccessLevel::None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created = state.db.create_node(node).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn get_folder(
    RequireRead(node): RequireRead,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    user: CurrentUser,
    Query(query): Query<FormatQuery>,
) -> Result<Json<Value>, AppError> {
    debug!("Fetching folder: {} for user: {}", id, user.id);

    if node.node_type != NodeType::Folder {
        return Err(AppError::BadRequest(
            "Requested ID is not a folder".to_string(),
        ));
    }

    if query.format.as_deref() == Some("metadata") {
        // respond with only metadata
        return Ok(Json(json!(node)));
    }

    let children = state.db.get_children(&node.path).await?;
    Ok(Json(json!({
        "metadata": node,
        "children": children
    })))
}

async fn create_file(
    RequireParentWrite(target_parent): RequireParentWrite,
    State(state): State<AppState>,
    user: CurrentUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Node>), AppError> {
    debug!("Creating new file via multipart upload");

    let id = Uuid::new_v4();
    let storage_key = id.simple().to_string();

    let mut final_name = "NewFile".to_string();
    let mut size_bytes = 0i64;
    let mut file_uploaded = false;

    // Process multipart fields one by one to avoid loading everything into RAM
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "name" => {
                final_name = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
            }
            "file" => {
                let mut writer = state.storage.writer(&storage_key).await?;

                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?
                {
                    size_bytes += chunk.len() as i64;
                    writer.write(chunk).await?;
                }
                writer.close().await?;
                file_uploaded = true;
            }
            _ => {
                debug!("Ignoring unknown multipart field: {}", field_name);
            }
        }
    }

    if let TargetParent::Root = target_parent {
        state.db.get_or_create_root(user.id).await?;
    }

    let path = generate_path(&target_parent, id, user.id);
    let final_storage_key = if file_uploaded {
        Some(storage_key)
    } else {
        None
    };

    let node = Node {
        id,
        owner_id: user.id,
        node_type: NodeType::File,
        name: final_name,
        path,
        storage_key: final_storage_key,
        size_bytes,
        public_access_level: AccessLevel::None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created = state.db.create_node(node).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn get_file(
    RequireRead(node): RequireRead,
    user: CurrentUser,
) -> Result<Json<Node>, AppError> {
    debug!("Fetching file metadata: {} for user: {}", node.id, user.id);

    if node.node_type != NodeType::File {
        return Err(AppError::BadRequest(
            "Requested ID is not a file".to_string(),
        ));
    }

    Ok(Json(node))
}

async fn download_file(
    RequireRead(node): RequireRead,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Response, AppError> {
    debug!("Streaming file content: {} for user: {}", node.id, user.id);

    if node.node_type != NodeType::File {
        return Err(AppError::BadRequest(
            "Requested ID is not a file".to_string(),
        ));
    }

    let storage_key = node
        .storage_key
        .ok_or_else(|| AppError::NotFound("File has no content".to_string()))?;

    let reader = state.storage.reader(&storage_key).await?;
    let stream = reader.into_bytes_stream(..).await?;
    let body = Body::from_stream(stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", node.name),
        )
        .header(header::CONTENT_LENGTH, node.size_bytes)
        .body(body)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    Ok(response)
}

async fn update_node(
    RequireWrite(mut node): RequireWrite,
    State(state): State<AppState>,
    user: CurrentUser,
    Json(payload): Json<UpdateNodeReq>,
) -> Result<Json<Node>, AppError> {
    debug!("Updating node: {} for user: {}", node.id, user.id);

    if let Some(new_name) = payload.name {
        node.name = new_name;
    }

    if let Some(new_parent_id) = payload.parent_id {
        if node.owner_id != user.id {
            return Err(AppError::Forbidden(
                "Only the owner can perform this action".to_string(),
            ));
        }
        let target_parent = if new_parent_id == user.id {
            state.db.get_or_create_root(user.id).await?;
            TargetParent::Root
        } else {
            let parent =
                state.db.get_node(new_parent_id).await?.ok_or_else(|| {
                    AppError::NotFound("Target parent folder not found".to_string())
                })?;

            if parent.node_type != NodeType::Folder {
                return Err(AppError::BadRequest("Parent must be a folder".to_string()));
            }

            verify_access(
                &state,
                &parent,
                user.id,
                RequiredAccess::Level(AccessLevel::Write),
            )
            .await?;
            TargetParent::Node(parent)
        };

        node.path = generate_path(&target_parent, node.id, user.id);
    }

    node.updated_at = Utc::now();
    let updated = state.db.update_node(node).await?;
    Ok(Json(updated))
}

async fn update_access(
    RequireOwner(mut node): RequireOwner,
    State(state): State<AppState>,
    user: CurrentUser,
    Json(payload): Json<UpdateAccessReq>,
) -> Result<Json<Node>, AppError> {
    debug!(
        "Updating public access for node: {} by user: {}",
        node.id, user.id
    );

    node.public_access_level = payload.public_access_level;
    node.updated_at = Utc::now();

    let updated = state.db.update_node(node).await?;
    Ok(Json(updated))
}

async fn delete_node(
    RequireOwner(node): RequireOwner,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<StatusCode, AppError> {
    debug!("Deleting node: {} for user: {}", node.id, user.id);

    let descendants = state.db.get_descendants(node.id).await?;

    for child in descendants {
        if let Some(key) = child.storage_key {
            if let Err(e) = state.storage.delete(&key).await {
                tracing::error!(
                    "Failed to delete physical file from storage {}: {:?}",
                    key,
                    e
                );
            }
        }
    }

    state.db.delete_node(node.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn open_room(
    RequireWrite(node): RequireWrite,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Value>, AppError> {
    debug!(
        "Initiating collaborative room for file: {} by user: {}",
        node.id, user.id
    );

    if node.node_type != NodeType::File {
        return Err(AppError::BadRequest(
            "Requested ID is not a file".to_string(),
        ));
    }

    let storage_key = node
        .storage_key
        .clone()
        .ok_or_else(|| AppError::NotFound("File has no content yet".to_string()))?;
    let presigned_req = state
        .storage
        .presign_read(&storage_key, Duration::from_hours(1))
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to generate presigned URL: {}", e))
        })?;

    //let sync_ms_url = format!("{}/internal/v1/rooms", state.config.sync_ms_internal_url);
    let sync_ms_url = "http://localhost:3030/v1/rooms";

    let payload = json!({
        "id": node.id,
        "retention_seconds": 60 * 5,
        "initial_state_url": presigned_req.uri().to_string(),
    });

    info!(room_id = %node.id, "Calling Sync MS to initialize/verify room");

    let client = reqwest::Client::new();

    let response =
        //state
        //.http_client
        client.post(Url::from_str(sync_ms_url).unwrap())
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Service Unavailable: Sync MS unreachable: {}", e))
        })?;

    if !response.status().is_success() {
        warn!("{}", response.text().await.unwrap());
        return Err(AppError::Internal(anyhow::anyhow!(
            "Failed to initialize room in Sync MS"
        )));
    }

    Ok(Json(json!({
        "room_id": node.id,
        "ws_url": format!("ws://sync-ms/room/{}", node.id), // TODO: think through
    })))
}

async fn get_permission(
    RequireWrite(node): RequireWrite,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<Permission>>, AppError> {
    debug!(
        "Fetching permissions for file: {} by user: {}",
        node.id, user.id
    );
    let perms = state.db.get_node_permissions(node.id).await?;
    Ok(Json(perms))
}

async fn grant_permission(
    RequireOwner(node): RequireOwner,
    State(state): State<AppState>,
    user: CurrentUser,
    Json(payload): Json<GrantPermReq>,
) -> Result<(StatusCode, Json<Permission>), AppError> {
    debug!(
        "Granting permission for file: {} by user: {}",
        node.id, user.id
    );

    let perm = Permission {
        node_id: node.id,
        user_id: payload.user_id,
        access_level: payload.access_level,
    };

    state.db.grant_permission(perm.clone()).await?;
    Ok((StatusCode::CREATED, Json(perm)))
}

async fn remove_permission(
    RequireOwner(node): RequireOwner,
    State(state): State<AppState>,
    user: CurrentUser,
    Path((_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    debug!(
        "Removing permission for file: {} by user: {}",
        node.id, user.id
    );

    state.db.revoke_permission(node.id, target_user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
