//! Account records. Only an opaque credential reference is stored, never a secret.

use sqlx::Row;

use super::{Database, StoreError, bump_revision, finish, require_id, require_timestamp};

const PROVIDERS: [&str; 2] = ["github", "gitlab"];
const AUTH_STATES: [&str; 5] = [
    "unknown",
    "connected",
    "expired",
    "forbidden",
    "disconnected",
];
const STORAGE_MODES: [&str; 3] = ["keyring", "session_only", "none"];
/// Prefixes of real provider tokens; a credential reference must never look like one.
const TOKEN_PREFIXES: [&str; 8] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "gloas-",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount<'a> {
    pub id: &'a str,
    pub provider: &'a str,
    pub provider_user_id: &'a str,
    pub username: &'a str,
    pub credential_ref: Option<&'a str>,
    pub auth_state: &'a str,
    pub storage_mode: &'a str,
    pub verified_at: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRecord {
    pub id: String,
    pub provider: String,
    pub provider_user_id: String,
    pub username: String,
    pub credential_ref: Option<String>,
    pub auth_state: String,
    pub storage_mode: String,
    pub verified_at: Option<String>,
}

/// A credential reference is an app-generated `cred:<id>` handle into the keyring.
pub(crate) fn validate_credential_ref(value: &str) -> Result<(), StoreError> {
    let Some(rest) = value.strip_prefix("cred:") else {
        return Err(StoreError::InvalidInput("credential_ref"));
    };
    let lowered = rest.to_ascii_lowercase();
    // A long all-hex body looks like a classic 40-hex personal access token.
    let token_like_hex = rest.len() >= 32 && rest.bytes().all(|b| b.is_ascii_hexdigit());
    if TOKEN_PREFIXES.iter().any(|p| lowered.starts_with(p)) || rest.len() > 64 || token_like_hex {
        return Err(StoreError::InvalidInput("credential_ref"));
    }
    require_id("credential_ref", rest)
}

fn require_member(field: &'static str, value: &str, allowed: &[&str]) -> Result<(), StoreError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(StoreError::InvalidInput(field))
    }
}

impl Database {
    /// Inserts or updates the account identified by (provider, provider_user_id).
    pub async fn upsert_account(&self, account: &NewAccount<'_>) -> Result<i64, StoreError> {
        require_id("id", account.id)?;
        require_member("provider", account.provider, &PROVIDERS)?;
        require_member("auth_state", account.auth_state, &AUTH_STATES)?;
        require_member("storage_mode", account.storage_mode, &STORAGE_MODES)?;
        // GitHub and GitLab both identify users by numeric IDs (REST `id`).
        if account.provider_user_id.is_empty()
            || !account.provider_user_id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(StoreError::InvalidInput("provider_user_id"));
        }
        if account.username.is_empty() || account.username.chars().count() > 256 {
            return Err(StoreError::InvalidInput("username"));
        }
        // `none` means no stored credential; keyring/session modes require a reference.
        match (account.storage_mode, account.credential_ref) {
            ("none", None) => {}
            (_, Some(reference)) if account.storage_mode != "none" => {
                validate_credential_ref(reference)?;
            }
            _ => return Err(StoreError::InvalidInput("credential_ref")),
        }
        if let Some(at) = account.verified_at {
            require_timestamp("verified_at", at)?;
        }
        let mut tx = self.begin().await?;
        let result = async {
            sqlx::query(
                "INSERT INTO accounts (id, provider, provider_user_id, username, credential_ref, \
             auth_state, storage_mode, verified_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(provider, provider_user_id) DO UPDATE SET username = excluded.username, \
             credential_ref = excluded.credential_ref, auth_state = excluded.auth_state, \
             storage_mode = excluded.storage_mode, verified_at = excluded.verified_at",
            )
            .bind(account.id)
            .bind(account.provider)
            .bind(account.provider_user_id)
            .bind(account.username)
            .bind(account.credential_ref)
            .bind(account.auth_state)
            .bind(account.storage_mode)
            .bind(account.verified_at)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            bump_revision(&mut tx).await
        }
        .await;
        finish(tx, result).await
    }

    /// Removes the credential reference and marks the account disconnected. The row is
    /// kept so historical mappings and backups are not cascaded away (DATA_MODEL.md).
    pub async fn disconnect_account(&self, id: &str) -> Result<i64, StoreError> {
        require_id("id", id)?;
        let mut tx = self.begin().await?;
        let result = async {
            let updated = sqlx::query(
                "UPDATE accounts SET credential_ref = NULL, auth_state = 'disconnected', \
             storage_mode = 'none' WHERE id = ?",
            )
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            if updated.rows_affected() == 0 {
                return Err(StoreError::NotFound);
            }
            bump_revision(&mut tx).await
        }
        .await;
        finish(tx, result).await
    }

    pub async fn list_accounts(&self) -> Result<Vec<AccountRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, provider, provider_user_id, username, credential_ref, auth_state, \
             storage_mode, verified_at FROM accounts ORDER BY provider, username, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from_sqlx)?;
        rows.iter()
            .map(|row| {
                Ok(AccountRecord {
                    id: row.try_get("id").map_err(StoreError::from_sqlx)?,
                    provider: row.try_get("provider").map_err(StoreError::from_sqlx)?,
                    provider_user_id: row
                        .try_get("provider_user_id")
                        .map_err(StoreError::from_sqlx)?,
                    username: row.try_get("username").map_err(StoreError::from_sqlx)?,
                    credential_ref: row
                        .try_get("credential_ref")
                        .map_err(StoreError::from_sqlx)?,
                    auth_state: row.try_get("auth_state").map_err(StoreError::from_sqlx)?,
                    storage_mode: row.try_get("storage_mode").map_err(StoreError::from_sqlx)?,
                    verified_at: row.try_get("verified_at").map_err(StoreError::from_sqlx)?,
                })
            })
            .collect()
    }
}
