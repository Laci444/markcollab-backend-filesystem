use super::{models::Node, Repository};
use crate::db::models::{NodeType, Permission};
use crate::error::AppError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct InMemoryRepository {
    nodes: Arc<RwLock<HashMap<Uuid, Node>>>,
    permissions: Arc<RwLock<HashMap<(Uuid, Uuid), Permission>>>,
}

impl InMemoryRepository {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            permissions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Repository for InMemoryRepository {
    async fn create_node(&self, node: Node) -> Result<Node, AppError> {
        let mut lock = self.nodes.write().await;
        lock.insert(node.id, node.clone());
        Ok(node)
    }

    async fn get_node(&self, id: Uuid) -> Result<Option<Node>, AppError> {
        let lock = self.nodes.read().await;
        Ok(lock.get(&id).cloned())
    }

    async fn update_node(&self, node: Node) -> Result<Node, AppError> {
        let mut lock = self.nodes.write().await;
        if !lock.contains_key(&node.id) {
            return Err(AppError::NotFound(format!("Node {} not found", node.id)));
        }
        lock.insert(node.id, node.clone());
        Ok(node)
    }

    async fn delete_node(&self, id: Uuid) -> Result<(), AppError> {
        let mut nodes_lock = self.nodes.write().await;
        let mut perms_lock = self.permissions.write().await;

        let target_path = if let Some(node) = nodes_lock.get(&id) {
            node.path.clone()
        } else {
            return Ok(());
        };

        let prefix = format!("{}.", target_path);

        let ids_to_delete: Vec<Uuid> = nodes_lock
            .iter()
            .filter(|(node_id, node)| **node_id == id || node.path.starts_with(&prefix))
            .map(|(node_id, _)| *node_id)
            .collect();

        for node_id in ids_to_delete {
            nodes_lock.remove(&node_id);
            perms_lock.retain(|(p_node_id, _), _| *p_node_id != node_id);
        }

        Ok(())
    }

    async fn get_children(&self, parent_path: &str) -> Result<Vec<Node>, AppError> {
        let lock = self.nodes.read().await;
        let parent_segments_count = parent_path.split('.').count();

        let mut children: Vec<Node> = lock
            .values()
            .filter(|n| {
                // Simulates ltree hierarchical search: matches direct children only
                n.path.starts_with(&format!("{}.", parent_path))
                    && n.path.split('.').count() == parent_segments_count + 1
            })
            .cloned()
            .collect();

        children.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(children)
    }

    async fn get_ancestors(&self, node_id: Uuid) -> Result<Vec<Node>, AppError> {
        let lock = self.nodes.read().await;
        let target = lock
            .get(&node_id)
            .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?;

        let mut ancestors: Vec<Node> = lock
            .values()
            .filter(|n| target.path.starts_with(&format!("{}.", n.path)))
            .cloned()
            .collect();

        ancestors.sort_by_key(|n| n.path.len());
        Ok(ancestors)
    }

    async fn get_descendants(&self, node_id: Uuid) -> Result<Vec<Node>, AppError> {
        let lock = self.nodes.read().await;
        let target = lock
            .get(&node_id)
            .ok_or_else(|| AppError::NotFound(format!("Node {} not found", node_id)))?;

        let descendants: Vec<Node> = lock
            .values()
            .filter(|n| n.id == node_id || n.path.starts_with(&format!("{}.", target.path)))
            .cloned()
            .collect();

        Ok(descendants)
    }

    async fn get_or_create_root(&self, user_id: Uuid) -> Result<Node, AppError> {
        let mut lock = self.nodes.write().await;

        if let Some(existing) = lock.get(&user_id) {
            return Ok(existing.clone());
        }

        let node = Node {
            id: user_id,
            owner_id: user_id,
            node_type: NodeType::Folder,
            name: "My Drive".to_string(),
            path: user_id.simple().to_string(),
            storage_key: None,
            size_bytes: 0,
            public_access_level: crate::db::models::AccessLevel::None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        lock.insert(user_id, node.clone());
        Ok(node)
    }

    async fn get_user_permission(
        &self,
        node_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Permission>, AppError> {
        let nodes_lock = self.nodes.read().await;
        let perms_lock = self.permissions.read().await;

        let target_node = match nodes_lock.get(&node_id) {
            Some(n) => n,
            None => return Ok(None),
        };

        let mut best_perm: Option<Permission> = None;

        for perm in perms_lock.values() {
            if perm.user_id == user_id {
                if let Some(perm_node) = nodes_lock.get(&perm.node_id) {
                    let is_ancestor = target_node.path == perm_node.path
                        || target_node
                            .path
                            .starts_with(&format!("{}.", perm_node.path));

                    if is_ancestor {
                        match &best_perm {
                            None => best_perm = Some(perm.clone()),
                            Some(existing) if perm.access_level > existing.access_level => {
                                best_perm = Some(perm.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(best_perm)
    }

    async fn get_node_permissions(&self, node_id: Uuid) -> Result<Vec<Permission>, AppError> {
        let lock = self.permissions.read().await;

        let perms: Vec<Permission> = lock
            .values()
            .filter(|p| p.node_id == node_id)
            .cloned()
            .collect();

        Ok(perms)
    }

    async fn grant_permission(&self, perm: Permission) -> Result<(), AppError> {
        let mut lock = self.permissions.write().await;

        // This acts as an UPSERT: if the user already has a permission for this node,
        // it overwrites it with the new access_level. Otherwise, it creates it.
        lock.insert((perm.node_id, perm.user_id), perm);

        Ok(())
    }

    async fn revoke_permission(&self, node_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let mut lock = self.permissions.write().await;

        lock.remove(&(node_id, user_id));

        Ok(())
    }
}
