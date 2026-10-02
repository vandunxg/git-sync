//! GitLab PAT identity and scope validation (destination provider, gitlab.com only).
//!
//! `GET /api/v4/personal_access_tokens/self` (scopes, active, revoked, expires_at,
//! user_id) and `GET /api/v4/user` (id, username), both with `PRIVATE-TOKEN`
//! (docs.gitlab.com/api/personal_access_tokens, read 2026-10-02). 401 = invalid or
//! expired token; 429 carries `Retry-After`. Read-only: no write is attempted.

use gbm_application::providers::{
    ProviderError, ProviderKind, TokenIdentity, TokenKind, precheck_token,
};
use gbm_domain::contract::secret::SecretString;
use reqwest::header::HeaderName;
use serde::Deserialize;

use crate::http::{Limits, ProviderHttp, classify_failure};

pub const GITLAB_ORIGIN: &str = "https://gitlab.com/";
const PRIVATE_TOKEN: HeaderName = HeaderName::from_static("private-token");

#[derive(Deserialize)]
struct TokenBody {
    active: bool,
    revoked: bool,
    scopes: Vec<String>,
    user_id: u64,
    expires_at: Option<String>,
}

#[derive(Deserialize)]
struct UserBody {
    id: u64,
    username: String,
}

pub struct GitLabClient {
    http: ProviderHttp,
}

/// GitLab reports `expires_at` as a date (`YYYY-MM-DD`). The conservative instant is
/// the start of that day in UTC, so the app never shows a token as valid for longer.
fn expiry_instant(date: &str) -> Result<String, ProviderError> {
    let candidate = format!("{date}T00:00:00Z");
    if date.len() == 10 && gbm_domain::contract::schema::is_rfc3339(&candidate) {
        Ok(candidate)
    } else {
        Err(ProviderError::UnexpectedResponse)
    }
}

impl GitLabClient {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::new(GITLAB_ORIGIN, &Limits::default())?,
        })
    }

    #[cfg(test)]
    fn for_test(origin: &str, limits: &Limits) -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::for_test(origin, limits)?,
        })
    }

    /// Validates the token (two read requests) and returns identity + reported scopes.
    pub async fn validate_token(
        &self,
        token: &SecretString,
    ) -> Result<TokenIdentity, ProviderError> {
        precheck_token(token.expose())?;
        let auth = (PRIVATE_TOKEN, token.expose());
        let response = self
            .http
            .get("api/v4/personal_access_tokens/self", auth.clone(), &[])
            .await?;
        if !response.status.is_success() {
            return Err(classify_failure(&response));
        }
        let token_info: TokenBody = serde_json::from_slice(&response.body)
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        if token_info.revoked || !token_info.active {
            return Err(ProviderError::Revoked);
        }
        let expires_at = token_info
            .expires_at
            .as_deref()
            .map(expiry_instant)
            .transpose()?;
        let response = self.http.get("api/v4/user", auth, &[]).await?;
        if !response.status.is_success() {
            return Err(classify_failure(&response));
        }
        let user: UserBody = serde_json::from_slice(&response.body)
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        if user.id != token_info.user_id
            || user.username.is_empty()
            || user.username.chars().count() > 256
        {
            return Err(ProviderError::UnexpectedResponse);
        }
        Ok(TokenIdentity {
            provider: ProviderKind::GitLab,
            provider_user_id: user.id.to_string(),
            username: user.username,
            kind: TokenKind::GitLabPersonal {
                scopes: token_info.scopes,
            },
            expires_at,
        })
    }
}

#[cfg(test)]
mod tests;
