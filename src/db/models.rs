use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;

#[derive(Type, Serialize, Deserialize, Debug, Clone, PartialOrd, PartialEq, Eq, Ord)]
#[sqlx(type_name = "access_level", rename_all = "lowercase")]
pub enum AccessLevel {
    None,
    Read,
    Write,
}

#[derive(Type, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[sqlx(type_name = "node_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    File,
    Folder,
}

#[derive(Type, Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct Node {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub node_type: NodeType,
    pub name: String,
    pub path: String, // Simulates PostgreSQL ltree (e.g., root.uuid1.uuid2)
    #[serde(skip_serializing)]
    pub storage_key: Option<String>,
    pub size_bytes: i64,
    pub public_access_level: AccessLevel,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Type, Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct Permission {
    pub node_id: Uuid,
    pub user_id: Uuid,
    pub access_level: AccessLevel,
}
