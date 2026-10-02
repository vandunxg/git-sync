//! Settings document with optimistic concurrency. No defaults are invented here: an
//! unsaved database returns `None`, and defaults come from the settings task.

use super::{Database, StoreError, bump_revision, finish};

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsRecord {
    pub revision: i64,
    pub document: serde_json::Value,
}

fn validate(document: &serde_json::Value) -> Result<(), StoreError> {
    match gbm_domain::contract::schema::validate_definition("Settings", document) {
        Ok(violations) if violations.is_empty() => Ok(()),
        _ => Err(StoreError::InvalidInput("settings")),
    }
}

impl Database {
    pub async fn get_settings(&self) -> Result<Option<SettingsRecord>, StoreError> {
        let row: Option<(i64, String)> =
            sqlx::query_as("SELECT revision, document_json FROM settings WHERE singleton = 1")
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::from_sqlx)?;
        let Some((revision, text)) = row else {
            return Ok(None);
        };
        let document: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| StoreError::Corrupt)?;
        validate(&document).map_err(|_| StoreError::Corrupt)?;
        Ok(Some(SettingsRecord { revision, document }))
    }

    /// Writes the settings document if the stored revision still equals
    /// `expected_revision` (`None` = nothing stored yet). Returns the new revision.
    pub async fn put_settings(
        &self,
        document: &serde_json::Value,
        expected_revision: Option<i64>,
    ) -> Result<i64, StoreError> {
        validate(document)?;
        let text =
            serde_json::to_string(document).map_err(|_| StoreError::InvalidInput("settings"))?;
        let mut tx = self.begin().await?;
        let result = async {
            let current: Option<(i64,)> =
                sqlx::query_as("SELECT revision FROM settings WHERE singleton = 1")
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(StoreError::from_sqlx)?;
            let current = current.map(|(r,)| r);
            if current != expected_revision {
                return Err(StoreError::StaleRevision {
                    current: current.unwrap_or(0),
                });
            }
            let revision = bump_revision(&mut tx).await?;
            sqlx::query(
                "INSERT INTO settings (singleton, revision, document_json) VALUES (1, ?, ?) \
             ON CONFLICT(singleton) DO UPDATE SET revision = excluded.revision, \
             document_json = excluded.document_json",
            )
            .bind(revision)
            .bind(&text)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            Ok(revision)
        }
        .await;
        finish(tx, result).await
    }
}
