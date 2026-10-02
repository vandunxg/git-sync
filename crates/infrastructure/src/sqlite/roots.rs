//! Backup roots chosen by the user. The ownership marker ties a row to the
//! `.gbm/root-manifest.json` written into that folder (WORKSPACE_AND_FILESYSTEM.md).

use sqlx::Row;

use super::{Database, StoreError, bump_revision, finish, require_id, require_timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootRecord {
    pub id: String,
    pub canonical_path: String,
    pub ownership_marker: String,
    pub created_at: String,
}

impl Database {
    /// Inserts a root. A path already registered is refused (UNIQUE constraint).
    pub async fn insert_root(&self, root: &RootRecord) -> Result<i64, StoreError> {
        require_id("id", &root.id)?;
        require_id("ownership_marker", &root.ownership_marker)?;
        require_timestamp("created_at", &root.created_at)?;
        if !root.canonical_path.starts_with('/') || root.canonical_path.chars().count() > 4096 {
            return Err(StoreError::InvalidInput("canonical_path"));
        }
        let mut tx = self.begin().await?;
        let result = async {
            sqlx::query("INSERT INTO roots (id, canonical_path, ownership_marker, created_at) VALUES (?, ?, ?, ?)")
                .bind(&root.id)
                .bind(&root.canonical_path)
                .bind(&root.ownership_marker)
                .bind(&root.created_at)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?;
            bump_revision(&mut tx).await
        }
        .await;
        finish(tx, result).await
    }

    pub async fn list_roots(&self) -> Result<Vec<RootRecord>, StoreError> {
        let rows = sqlx::query("SELECT id, canonical_path, ownership_marker, created_at FROM roots ORDER BY created_at, id")
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::from_sqlx)?;
        rows.iter()
            .map(|row| {
                Ok(RootRecord {
                    id: row.try_get("id").map_err(StoreError::from_sqlx)?,
                    canonical_path: row
                        .try_get("canonical_path")
                        .map_err(StoreError::from_sqlx)?,
                    ownership_marker: row
                        .try_get("ownership_marker")
                        .map_err(StoreError::from_sqlx)?,
                    created_at: row.try_get("created_at").map_err(StoreError::from_sqlx)?,
                })
            })
            .collect()
    }
}
