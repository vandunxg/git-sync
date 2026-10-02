//! Activity log rows hold only sanitized, bounded messages and reference IDs.

use super::jobs::require_error_code;
use super::{Database, StoreError, bump_revision, finish, require_id, require_timestamp};

const MAX_MESSAGE_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewActivity<'a> {
    pub id: &'a str,
    pub repository_id: Option<&'a str>,
    pub job_id: Option<&'a str>,
    pub occurred_at: &'a str,
    pub severity: Severity,
    pub event_type: &'a str,
    /// Must already be sanitized by the caller; rejected if it looks like it carries
    /// credentials or exceeds the bound.
    pub safe_message: &'a str,
    pub error_code: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRecord {
    pub id: String,
    pub occurred_at: String,
    pub severity: String,
    pub event_type: String,
    pub safe_message: String,
}

/// Defence in depth only: redaction happens before this point (R-SEC); this refuses
/// obvious credential material instead of persisting it.
fn looks_secret_bearing(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    [
        "ghp_",
        "gho_",
        "github_pat_",
        "glpat-",
        "authorization:",
        "bearer ",
        "private-token",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
        || lowered.contains("://") && lowered.contains('@')
}

impl Database {
    pub async fn append_activity(&self, entry: &NewActivity<'_>) -> Result<i64, StoreError> {
        require_id("id", entry.id)?;
        require_id("event_type", entry.event_type)?;
        require_timestamp("occurred_at", entry.occurred_at)?;
        for (field, value) in [
            ("repository_id", entry.repository_id),
            ("job_id", entry.job_id),
        ] {
            if let Some(v) = value {
                require_id(field, v)?;
            }
        }
        require_error_code(entry.error_code)?;
        let chars = entry.safe_message.chars().count();
        if chars == 0 || chars > MAX_MESSAGE_CHARS || looks_secret_bearing(entry.safe_message) {
            return Err(StoreError::InvalidInput("safe_message"));
        }
        let mut tx = self.begin().await?;
        let result = async {
            sqlx::query(
            "INSERT INTO activity (id, repository_id, job_id, occurred_at, severity, event_type, \
             safe_message, error_code) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(entry.id)
        .bind(entry.repository_id)
        .bind(entry.job_id)
        .bind(entry.occurred_at)
        .bind(entry.severity.as_str())
        .bind(entry.event_type)
        .bind(entry.safe_message)
        .bind(entry.error_code)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from_sqlx)?;
            bump_revision(&mut tx).await
        }
        .await;
        finish(tx, result).await
    }

    /// Newest first, bounded page (stable order on (occurred_at, id)).
    pub async fn list_activity(&self, limit: u32) -> Result<Vec<ActivityRecord>, StoreError> {
        let limit = i64::from(limit.clamp(1, 200));
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, occurred_at, severity, event_type, safe_message FROM activity \
             ORDER BY occurred_at DESC, id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from_sqlx)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, occurred_at, severity, event_type, safe_message)| ActivityRecord {
                    id,
                    occurred_at,
                    severity,
                    event_type,
                    safe_message,
                },
            )
            .collect())
    }
}
