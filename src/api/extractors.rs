use crate::api::utils::get_node_or_virtual_root;
use crate::{
    api::utils::{verify_access, RequiredAccess, TargetParent},
    auth::CurrentUser,
    db::{
        models::NodeType,
        models::{AccessLevel, Node},
    },
    error::AppError,
    AppState,
};
use axum::{
    extract::{FromRequestParts, Path, Query},
    http::request::Parts,
};
use serde::Deserialize;
use uuid::Uuid;

pub struct RequireRead(pub Node);
pub struct RequireWrite(pub Node);
pub struct RequireOwner(pub Node);

#[derive(Deserialize, Debug)]
struct NodePath {
    id: Uuid,
}

async fn extract_and_verify(
    parts: &mut Parts,
    state: &AppState,
    access: RequiredAccess,
) -> Result<Node, AppError> {
    let user = CurrentUser::from_request_parts(parts, state).await?;

    let Path(path_params) = Path::<NodePath>::from_request_parts(parts, state)
        .await
        .map_err(|_| AppError::BadRequest("Invalid Node ID in path".to_string()))?;

    let id = path_params.id;

    let node = get_node_or_virtual_root(state, id, user.id).await?;

    verify_access(state, &node, user.id, access).await?;

    Ok(node)
}

impl FromRequestParts<AppState> for RequireRead {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let node =
            extract_and_verify(parts, state, RequiredAccess::Level(AccessLevel::Read)).await?;
        Ok(RequireRead(node))
    }
}

impl FromRequestParts<AppState> for RequireWrite {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let node =
            extract_and_verify(parts, state, RequiredAccess::Level(AccessLevel::Write)).await?;
        Ok(RequireWrite(node))
    }
}

impl FromRequestParts<AppState> for RequireOwner {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let node = extract_and_verify(parts, state, RequiredAccess::Owner).await?;
        Ok(RequireOwner(node))
    }
}

#[derive(Deserialize, Debug)]
pub struct TargetParentQuery {
    pub parent_id: Option<Uuid>,
}

pub(super) struct RequireParentWrite(pub TargetParent);

impl FromRequestParts<AppState> for RequireParentWrite {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;

        let query = Query::<TargetParentQuery>::from_request_parts(parts, state)
            .await
            .map(|q| q.0)
            .unwrap_or(TargetParentQuery { parent_id: None });

        let target_id = query.parent_id.unwrap_or(user.id);

        if target_id == user.id {
            Ok(RequireParentWrite(TargetParent::Root))
        } else {
            let parent =
                state.db.get_node(target_id).await?.ok_or_else(|| {
                    AppError::NotFound("Target parent folder not found".to_string())
                })?;

            if parent.node_type != NodeType::Folder {
                return Err(AppError::BadRequest("Parent must be a folder".to_string()));
            }

            verify_access(
                state,
                &parent,
                user.id,
                RequiredAccess::Level(AccessLevel::Write),
            )
            .await?;
            Ok(RequireParentWrite(TargetParent::Node(parent)))
        }
    }
}
