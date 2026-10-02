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

use crate::http::{Limits, ProviderHttp, classify_failure, header_str};

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

#[derive(Deserialize)]
struct NamespaceBody {
    id: u64,
    full_path: String,
}

/// One namespace the token can see. Whether a project may be created there is not
/// reported by this endpoint, so it is left unknown for the destination check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceInfo {
    pub id: String,
    pub full_path: String,
}

/// Upper bound for one page (GitLab's own maximum is 100).
pub const MAX_NAMESPACE_PAGE: u32 = 100;

impl GitLabClient {
    /// `GET /api/v4/namespaces?page=&per_page=`; returns the page and the next page
    /// number from `X-Next-Page` (absent or empty = last page). Read-only.
    pub async fn list_namespaces(
        &self,
        token: &SecretString,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<NamespaceInfo>, Option<u32>), ProviderError> {
        precheck_token(token.expose())?;
        let per_page = per_page.clamp(1, MAX_NAMESPACE_PAGE);
        let path = format!("api/v4/namespaces?page={}&per_page={per_page}", page.max(1));
        let response = self
            .http
            .get(&path, (PRIVATE_TOKEN, token.expose()), &[])
            .await?;
        if !response.status.is_success() {
            return Err(classify_failure(&response));
        }
        let items: Vec<NamespaceBody> = serde_json::from_slice(&response.body)
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        let items = items
            .into_iter()
            .filter(|ns| !ns.full_path.is_empty() && ns.full_path.chars().count() <= 512)
            .map(|ns| NamespaceInfo {
                id: format!("gl-ns-{}", ns.id),
                full_path: ns.full_path,
            })
            .collect();
        let next = header_str(&response.headers, "x-next-page").and_then(|v| v.trim().parse().ok());
        Ok((items, next))
    }
}

#[cfg(test)]
mod tests;
