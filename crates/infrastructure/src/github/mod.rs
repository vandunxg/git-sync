//! GitHub PAT identity validation (read-only source provider).
//!
//! `GET /user` with `Authorization: Bearer`, `Accept: application/vnd.github+json` and
//! `X-GitHub-Api-Version: 2026-03-10` (docs.github.com/rest/users/users, read
//! 2026-10-02). Classic tokens report scopes in `X-OAuth-Scopes`; when the header is
//! absent the token's permissions are unknown and are not guessed. No expiry is
//! reported here, so `expires_at` stays `None` (AUTH_AND_LIFECYCLE.md).

use gbm_application::providers::{
    ProviderError, ProviderKind, TokenIdentity, TokenKind, precheck_token,
};
use gbm_domain::contract::secret::SecretString;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderName, HeaderValue};
use serde::Deserialize;

use crate::http::{HttpResponse, Limits, ProviderHttp, classify_failure, header_str, retry_after};

pub const GITHUB_API_ORIGIN: &str = "https://api.github.com/";
const API_VERSION: &str = "2026-03-10";

#[derive(Deserialize)]
struct UserBody {
    id: u64,
    login: String,
}

pub struct GitHubClient {
    http: ProviderHttp,
}

impl GitHubClient {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::new(GITHUB_API_ORIGIN, &Limits::default())?,
        })
    }

    #[cfg(test)]
    fn for_test(origin: &str, limits: &Limits) -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::for_test(origin, limits)?,
        })
    }

    /// Validates the token and returns the identity. Makes exactly one read request.
    pub async fn validate_token(
        &self,
        token: &SecretString,
    ) -> Result<TokenIdentity, ProviderError> {
        precheck_token(token.expose())?;
        let bearer = format!("Bearer {}", token.expose());
        let extra = [
            (
                ACCEPT,
                HeaderValue::from_static("application/vnd.github+json"),
            ),
            (
                HeaderName::from_static("x-github-api-version"),
                HeaderValue::from_static(API_VERSION),
            ),
        ];
        let response = self
            .http
            .get("user", (AUTHORIZATION, &bearer), &extra)
            .await?;
        if !response.status.is_success() {
            return Err(classify(&response));
        }
        let body: UserBody = serde_json::from_slice(&response.body)
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        if body.login.is_empty() || body.login.chars().count() > 256 {
            return Err(ProviderError::UnexpectedResponse);
        }
        let kind = match header_str(&response.headers, "x-oauth-scopes") {
            Some(list) => TokenKind::GitHubScoped {
                scopes: list
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
            },
            None => TokenKind::GitHubUnscoped,
        };
        Ok(TokenIdentity {
            provider: ProviderKind::GitHub,
            provider_user_id: body.id.to_string(),
            username: body.login,
            kind,
            expires_at: None,
        })
    }
}

/// GitHub signals primary rate limits with 403/429 and `x-ratelimit-remaining: 0`.
fn classify(response: &HttpResponse) -> ProviderError {
    let exhausted = header_str(&response.headers, "x-ratelimit-remaining") == Some("0");
    if response.status.as_u16() == 403 && (exhausted || retry_after(&response.headers).is_some()) {
        return ProviderError::RateLimited {
            retry_after_seconds: retry_after(&response.headers),
        };
    }
    classify_failure(response)
}

#[cfg(test)]
mod tests;
