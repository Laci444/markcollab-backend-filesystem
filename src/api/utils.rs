use crate::db::models::NodeType;
use crate::{
    db::models::{AccessLevel, Node},
    error::AppError,
    AppState,
};
use chrono::Utc;
use uuid::Uuid;

pub(super) enum RequiredAccess {
    Level(AccessLevel),
    Owner,
}

pub(super) async fn verify_access(
    state: &AppState,
    node: &Node,
    user_id: Uuid,
    required: RequiredAccess,
) -> Result<(), AppError> {
    let ancestors = state.db.get_ancestors(node.id).await?;

    if node.owner_id == user_id || ancestors.iter().any(|a| a.owner_id == user_id) {
        return Ok(());
    }

    let required_level = match required {
        RequiredAccess::Owner => {
            return Err(AppError::Forbidden(
                "Only the owner can perform this action".to_string(),
            ));
        }
        RequiredAccess::Level(level) => level,
    };

    let max_public_access = ancestors
        .iter()
        .chain(std::iter::once(node))
        .map(|n| &n.public_access_level)
        .max()
        .unwrap_or(&AccessLevel::None);

    if max_public_access >= &required_level {
        return Ok(());
    }

    if let Some(perm) = state.db.get_user_permission(node.id, user_id).await? {
        if perm.access_level >= required_level {
            return Ok(());
        }
    }

    Err(AppError::Forbidden(
        "You do not have permission to access this resource".to_string(),
    ))
}

pub enum TargetParent {
    Root,
    Node(Node),
}

pub(super) fn generate_path(parent: &TargetParent, new_id: Uuid, user_id: Uuid) -> String {
    let id_str = new_id.simple().to_string();
    match parent {
        TargetParent::Node(p) => format!("{}.{}", p.path, id_str),
        // Implicit root generation based on user_id
        TargetParent::Root => format!("{}.{}", user_id.simple(), id_str),
    }
}

pub async fn get_node_or_virtual_root(
    state: &AppState,
    target_id: Uuid,
    user_id: Uuid,
) -> Result<Node, AppError> {
    match state.db.get_node(target_id).await? {
        Some(node) => Ok(node),
        None if target_id == user_id => {
            // Virtual Root Node: Handled gracefully even if it wasn't saved in DB yet
            Ok(Node {
                id: user_id,
                owner_id: user_id,
                node_type: NodeType::Folder,
                name: "My Drive".to_string(),
                path: user_id.simple().to_string(),
                storage_key: None,
                size_bytes: 0,
                public_access_level: AccessLevel::None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
        }
        None => Err(AppError::NotFound("Resource not found".to_string())),
    }
}
