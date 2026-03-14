use super::{
    models::{Node, Permission},
    Repository,
};
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Repository for PostgresRepository {
    async fn create_node(&self, node: Node) -> Result<Node, AppError> {
        let created = sqlx::query_as::<_, Node>(
            r#"
            INSERT INTO vfs_nodes
            (id, owner_id, node_type, name, path, storage_key, size_bytes, public_access_level, created_at, updated_at)
            VALUES ($1, $2, $3::node_type, $4, $5::ltree, $6, $7, $8::access_level, $9, $10)
            RETURNING id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            "#
        )
            .bind(node.id)
            .bind(node.owner_id)
            .bind(&node.node_type)
            .bind(&node.name)
            .bind(&node.path)
            .bind(&node.storage_key)
            .bind(node.size_bytes)
            .bind(&node.public_access_level)
            .bind(node.created_at)
            .bind(node.updated_at)
            .fetch_one(&self.pool).await?;

        Ok(created)
    }

    async fn get_node(&self, id: Uuid) -> Result<Option<Node>, AppError> {
        let node = sqlx::query_as::<_, Node>(
            r#"
            SELECT id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            FROM vfs_nodes
            WHERE id = $1
            "#
        )
            .bind(id)
            .fetch_optional(&self.pool).await?;

        Ok(node)
    }

    async fn update_node(&self, node: Node) -> Result<Node, AppError> {
        let updated = sqlx::query_as::<_, Node>(
            r#"
            UPDATE vfs_nodes
            SET name = $1, path = $2::ltree, storage_key = $3, size_bytes = $4, public_access_level = $5::access_level, updated_at = $6
            WHERE id = $7
            RETURNING id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            "#
        )
            .bind(&node.name)
            .bind(&node.path)
            .bind(&node.storage_key)
            .bind(node.size_bytes)
            .bind(&node.public_access_level)
            .bind(node.updated_at)
            .bind(node.id)
            .fetch_one(&self.pool).await?;

        Ok(updated)
    }

    async fn delete_node(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            "DELETE FROM vfs_nodes WHERE path <@ (SELECT path FROM vfs_nodes WHERE id = $1)",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_children(&self, parent_path: &str) -> Result<Vec<Node>, AppError> {
        let lquery = format!("{}.*{{1}}", parent_path);
        let children = sqlx::query_as::<_, Node>(
            r#"
            SELECT id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            FROM vfs_nodes
            WHERE path ~ $1::lquery
            ORDER BY name ASC
            "#
        )
            .bind(lquery)
            .fetch_all(&self.pool).await?;

        Ok(children)
    }

    async fn get_ancestors(&self, node_id: Uuid) -> Result<Vec<Node>, AppError> {
        let ancestors = sqlx::query_as::<_, Node>(
            r#"
            SELECT id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            FROM vfs_nodes
            WHERE path @> (SELECT path FROM vfs_nodes WHERE id = $1)
              AND id!= $1
            ORDER BY nlevel(path) ASC
            "#
        )
            .bind(node_id)
            .fetch_all(&self.pool).await?;

        Ok(ancestors)
    }

    async fn get_descendants(&self, node_id: Uuid) -> Result<Vec<Node>, AppError> {
        let descendants = sqlx::query_as::<_, Node>(
            r#"
            SELECT id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            FROM vfs_nodes
            WHERE path <@ (SELECT path FROM vfs_nodes WHERE id = $1)
            "#
        )
            .bind(node_id)
            .fetch_all(&self.pool).await?;

        Ok(descendants)
    }

    async fn get_or_create_root(&self, user_id: Uuid) -> Result<Node, AppError> {
        let path_str = user_id.simple().to_string();
        let now = chrono::Utc::now();

        let node = sqlx::query_as::<_, Node>(
            r#"
            INSERT INTO vfs_nodes
            (id, owner_id, node_type, name, path, storage_key, size_bytes, public_access_level, created_at, updated_at)
            VALUES ($1, $1, 'folder'::node_type, 'My Drive', $2::ltree, NULL, 0, 'none'::access_level, $3, $3)
            ON CONFLICT (id)
            DO UPDATE SET id = EXCLUDED.id
            RETURNING id, owner_id, node_type, name, path::text as path, storage_key, size_bytes, public_access_level, created_at, updated_at
            "#
        )
            .bind(user_id)
            .bind(path_str)
            .bind(now)
            .fetch_one(&self.pool).await?;

        Ok(node)
    }

    async fn get_user_permission(
        &self,
        node_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Permission>, AppError> {
        let perm = sqlx::query_as::<_, Permission>(
            r#"
            SELECT p.node_id, p.user_id, p.access_level
            FROM node_permissions p
            JOIN vfs_nodes v ON p.node_id = v.id
            WHERE p.user_id = $1
              AND v.path @> (SELECT path FROM vfs_nodes WHERE id = $2)
            ORDER BY p.access_level DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(perm)
    }

    async fn get_node_permissions(&self, node_id: Uuid) -> Result<Vec<Permission>, AppError> {
        let perms = sqlx::query_as::<_, Permission>(
            "SELECT node_id, user_id, access_level FROM node_permissions WHERE node_id = $1",
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(perms)
    }

    async fn grant_permission(&self, perm: Permission) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO node_permissions (node_id, user_id, access_level)
            VALUES ($1, $2, $3::access_level)
            ON CONFLICT (node_id, user_id)
            DO UPDATE SET access_level = EXCLUDED.access_level
            "#,
        )
        .bind(perm.node_id)
        .bind(perm.user_id)
        .bind(&perm.access_level)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn revoke_permission(&self, node_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        sqlx::query("DELETE FROM node_permissions WHERE node_id = $1 AND user_id = $2")
            .bind(node_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
