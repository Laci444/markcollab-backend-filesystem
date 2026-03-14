pub mod inmemory;
pub mod models;
pub mod postresql;

use crate::db::models::Permission;
use crate::error::AppError;
use async_trait::async_trait;
use models::Node;
use uuid::Uuid;

#[async_trait]
pub trait Repository: Send + Sync + 'static {
    async fn create_node(&self, node: Node) -> Result<Node, AppError>;
    async fn get_node(&self, id: Uuid) -> Result<Option<Node>, AppError>;
    async fn update_node(&self, node: Node) -> Result<Node, AppError>;
    async fn delete_node(&self, id: Uuid) -> Result<(), AppError>;

    async fn get_children(&self, parent_path: &str) -> Result<Vec<Node>, AppError>;
    async fn get_ancestors(&self, node_id: Uuid) -> Result<Vec<Node>, AppError>;
    async fn get_descendants(&self, node_id: Uuid) -> Result<Vec<Node>, AppError>;
    async fn get_or_create_root(&self, user_id: Uuid) -> Result<Node, AppError>;

    async fn get_user_permission(
        &self,
        node_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Permission>, AppError>;
    async fn get_node_permissions(&self, node_id: Uuid) -> Result<Vec<Permission>, AppError>;
    async fn grant_permission(&self, perm: Permission) -> Result<(), AppError>;
    async fn revoke_permission(&self, node_id: Uuid, user_id: Uuid) -> Result<(), AppError>;
}
