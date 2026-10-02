//! GitHub PAT identity validation (read-only source provider).
//!
//! `GET /user` with `Authorization: Bearer`, `Accept: application/vnd.github+json` and
//! `X-GitHub-Api-Version: 2026-03-10` (docs.github.com/rest/users/users, read
//! 2026-10-02). Classic tokens report scopes in `X-OAuth-Scopes`; when the header is
//! absent the token's permissions are unknown and are not guessed. No expiry is
//! reported here, so `expires_at` stays `None` (AUTH_AND_LIFECYCLE.md).

use gbm_application::discovery::{
    DiscoveredRepository, Discovery, DiscoveryBuilder, DiscoveryScope, Incomplete, Relation,
};
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
    /// Bound API origin (ends with `/`); pagination links must stay on it.
    origin: String,
}

impl GitHubClient {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::new(GITHUB_API_ORIGIN, &Limits::default())?,
            origin: GITHUB_API_ORIGIN.to_owned(),
        })
    }

    #[cfg(test)]
    fn for_test(origin: &str, limits: &Limits) -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::for_test(origin, limits)?,
            origin: origin.to_owned(),
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

// --- repository discovery (P04-T01) ------------------------------------------------
//
// `GET /user/repos?affiliation=..&visibility=all&sort=full_name&per_page=100`
// (docs.github.com/rest/repos/repos#list-repositories-for-the-authenticated-user,
// read 2026-10-02). Pages are followed through the `Link: <..>; rel="next"` header,
// which must stay on the API origin. Fine-grained tokens list only granted repositories.

/// Hard cap on pages per discovery (200 x 50 repositories).
pub const MAX_DISCOVERY_PAGES: u32 = 200;
/// 50 full repository objects (~5-9 KB each) stay well inside the 1 MiB body limit.
const PER_PAGE: u32 = 50;

#[derive(Deserialize)]
struct RepoOwner {
    login: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct RepoBody {
    id: u64,
    name: String,
    full_name: String,
    owner: RepoOwner,
    private: bool,
    fork: bool,
    archived: bool,
    default_branch: Option<String>,
    pushed_at: Option<String>,
}

fn safe_segment(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 100
        && text != "."
        && text != ".."
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Converts one payload entry; `None` for entries that violate basic invariants.
fn to_discovered(body: RepoBody, login: &str) -> Option<DiscoveredRepository> {
    let expected = format!("{}/{}", body.owner.login, body.name);
    if !safe_segment(&body.owner.login) || !safe_segment(&body.name) || body.full_name != expected {
        return None;
    }
    let relation = if body.owner.login.eq_ignore_ascii_case(login) {
        Relation::Owner
    } else if body.owner.kind == "Organization" {
        Relation::Organization
    } else {
        Relation::Collaborator
    };
    let default_branch = body
        .default_branch
        .filter(|b| !b.is_empty() && b.len() <= 255 && !b.contains(['\n', '\0']));
    let pushed_at = body
        .pushed_at
        .filter(|t| gbm_domain::contract::schema::is_rfc3339(t));
    Some(DiscoveredRepository {
        provider_repository_id: body.id.to_string(),
        owner: body.owner.login,
        name: body.name,
        full_name: body.full_name,
        default_branch,
        is_private: body.private,
        is_fork: body.fork,
        is_archived: body.archived,
        relation,
        pushed_at,
    })
}

/// Parses RFC 8288 link-values by scanning `<target>` delimiters, so commas inside a
/// target (e.g. an echoed `affiliation=owner,collaborator`) never split it.
fn link_values(header: &str) -> Option<Vec<(&str, Vec<&str>)>> {
    let mut out = Vec::new();
    let mut rest = header.trim_start();
    while !rest.is_empty() {
        let after_open = rest.strip_prefix('<')?;
        let close = after_open.find('>')?;
        let target = &after_open[..close];
        let tail = &after_open[close + 1..];
        // Parameters run until the next link-value (`, <`) or the end.
        let end = tail
            .find(", <")
            .or_else(|| tail.find(",<"))
            .unwrap_or(tail.len());
        let params = tail[..end]
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        out.push((target, params));
        rest = tail[end..].trim_start_matches(',').trim_start();
    }
    Some(out)
}

/// The `rel="next"` target across all `Link` headers, as a path relative to `origin`.
/// Fails closed: an unparsable header or a next link to another origin is an error,
/// never "last page".
fn next_link(
    headers: &reqwest::header::HeaderMap,
    origin: &str,
) -> Result<Option<String>, ProviderError> {
    let mut next = None;
    for value in headers.get_all("link") {
        let text = value
            .to_str()
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        let values = link_values(text).ok_or(ProviderError::UnexpectedResponse)?;
        for (target, params) in values {
            let is_next = params.iter().any(|p| {
                p.strip_prefix("rel=")
                    .map(|r| r.trim_matches('"'))
                    .is_some_and(|r| r.split_ascii_whitespace().any(|t| t == "next"))
            });
            if is_next {
                if next.is_some() {
                    return Err(ProviderError::UnexpectedResponse);
                }
                let path = target
                    .strip_prefix(origin)
                    .ok_or(ProviderError::UnexpectedResponse)?;
                next = Some(path.to_owned());
            }
        }
    }
    Ok(next)
}

impl GitHubClient {
    /// Lists every repository visible to the token within `scope`, page by page. A
    /// failing page ends the walk with an incomplete discovery that keeps prior items.
    /// `login` must be the username from the validated token identity (it decides
    /// ownership). "Complete" means every page was read; offset pagination cannot rule
    /// out a repository renamed or created between page requests, so the next scan and
    /// the scheduled full reconciliation remain necessary.
    pub async fn discover_repositories(
        &self,
        token: &SecretString,
        login: &str,
        scope: DiscoveryScope,
        max_pages: u32,
    ) -> Discovery {
        let mut builder = DiscoveryBuilder::new(scope);
        if let Err(error) = precheck_token(token.expose()) {
            return builder.finish(Some(Incomplete::PageFailed { page: 1, error }));
        }
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
        let mut path = format!(
            "user/repos?affiliation={}&visibility=all&sort=full_name&per_page={PER_PAGE}",
            scope.affiliations().join("%2C")
        );
        loop {
            let page = builder.pages() + 1;
            if page > max_pages {
                return builder.finish(Some(Incomplete::PageLimitReached { pages: max_pages }));
            }
            let response = match self.http.get(&path, (AUTHORIZATION, &bearer), &extra).await {
                Ok(response) => response,
                Err(error) => return builder.finish(Some(Incomplete::PageFailed { page, error })),
            };
            if !response.status.is_success() {
                return builder.finish(Some(Incomplete::PageFailed {
                    page,
                    error: classify(&response),
                }));
            }
            let Ok(entries) = serde_json::from_slice::<Vec<serde_json::Value>>(&response.body)
            else {
                return builder.finish(Some(Incomplete::PageFailed {
                    page,
                    error: ProviderError::UnexpectedResponse,
                }));
            };
            let mut items = Vec::with_capacity(entries.len());
            let mut rejected = 0;
            for entry in entries {
                match serde_json::from_value::<RepoBody>(entry)
                    .ok()
                    .and_then(|b| to_discovered(b, login))
                {
                    Some(item) => items.push(item),
                    None => rejected += 1,
                }
            }
            builder.add_page(items, rejected);
            match next_link(&response.headers, &self.origin) {
                Ok(Some(next)) => path = next,
                Ok(None) => return builder.finish(None),
                Err(error) => {
                    return builder.finish(Some(Incomplete::PageFailed {
                        page: page + 1,
                        error,
                    }));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
