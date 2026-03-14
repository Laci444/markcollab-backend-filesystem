CREATE EXTENSION IF NOT EXISTS ltree;
CREATE TYPE access_level AS ENUM ('none', 'read', 'write');
CREATE TYPE node_type AS ENUM ('file', 'folder');

CREATE TABLE IF NOT EXISTS vfs_nodes
(
    id                  UUID PRIMARY KEY,
    owner_id            UUID         NOT NULL,
    node_type           node_type    NOT NULL,
    name                VARCHAR(255) NOT NULL,
    path                ltree        NOT NULL,
    storage_key         VARCHAR(255),
    size_bytes          BIGINT       NOT NULL DEFAULT 0,
    public_access_level access_level NOT NULL DEFAULT 'none',
    created_at          TIMESTAMPTZ  NOT NULL,
    updated_at          TIMESTAMPTZ  NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_vfs_nodes_path_gist ON vfs_nodes USING GIST (path);

CREATE TABLE IF NOT EXISTS node_permissions
(
    node_id      UUID         NOT NULL REFERENCES vfs_nodes (id) ON DELETE CASCADE,
    user_id      UUID         NOT NULL,
    access_level access_level NOT NULL DEFAULT 'read',
    PRIMARY KEY (node_id, user_id)
);