//! PAT identity validation results and capability assessment (REQ-11, REQ-41).
//!
//! Identity proves who the token belongs to, not what it may do. Assessments report
//! only what the provider exposed: GitHub classic scopes (`X-OAuth-Scopes`) and GitLab
//! token scopes (`GET /personal_access_tokens/self`). GitHub fine-grained tokens do
//! not expose their permissions on `/user`, so they are reported as "checked per
//! operation", never as granted. Namespace/project permissions are checked later by
//! the operations that need them. References: docs.github.com scopes-for-oauth-apps,
//! docs.gitlab.com security/tokens/access_token_scopes (read 2026-10-02).

use std::fmt;

use gbm_domain::contract::ErrorCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
        }
    }
}

/// What kind of token the provider reported. Never inferred from the token prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// GitHub returned `X-OAuth-Scopes` (classic PAT or OAuth token).
    GitHubScoped { scopes: Vec<String> },
    /// GitHub returned no scopes header (e.g. fine-grained PAT): permissions unknown.
    GitHubUnscoped,
    /// GitLab personal access token with the scopes it reported.
    GitLabPersonal { scopes: Vec<String> },
}

/// Verified identity of a token. Contains no secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenIdentity {
    pub provider: ProviderKind,
    /// Immutable numeric provider user ID, as a string.
    pub provider_user_id: String,
    pub username: String,
    pub kind: TokenKind,
    /// RFC 3339 expiry when the provider reported one; `None` means unknown.
    pub expires_at: Option<String>,
}

/// Sanitized, typed validation failures. None carries token or response text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    EmptyToken,
    /// Contains characters that cannot be a token (whitespace, control characters).
    MalformedToken,
    /// 401: wrong, expired or revoked token (the provider does not say which).
    InvalidOrExpired,
    /// The provider reports the token as revoked or inactive.
    Revoked,
    InsufficientScope {
        missing: Vec<&'static str>,
    },
    AccessDenied,
    RateLimited {
        retry_after_seconds: Option<u64>,
    },
    Timeout,
    Network,
    ProviderUnavailable,
    /// Redirect, oversized or unparsable response, or a contract-violating payload.
    UnexpectedResponse,
}

impl ProviderError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::EmptyToken | Self::MalformedToken | Self::InvalidOrExpired | Self::Revoked => {
                ErrorCode::AuthRequired
            }
            Self::InsufficientScope { .. } | Self::AccessDenied => ErrorCode::AccessDenied,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Timeout => ErrorCode::Timeout,
            Self::Network | Self::ProviderUnavailable => ErrorCode::NetworkUnavailable,
            Self::UnexpectedResponse => ErrorCode::ContractMismatch,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Timeout | Self::Network | Self::ProviderUnavailable
        )
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyToken => f.write_str("enter a personal access token"),
            Self::MalformedToken => f.write_str("the token contains invalid characters"),
            Self::InvalidOrExpired => {
                f.write_str("the provider rejected the token (invalid, expired or revoked)")
            }
            Self::Revoked => f.write_str("the provider reports this token as revoked or inactive"),
            Self::InsufficientScope { missing } => {
                write!(
                    f,
                    "the token is missing required scopes: {}",
                    missing.join(", ")
                )
            }
            Self::AccessDenied => f.write_str("the provider denied access"),
            Self::RateLimited { .. } => f.write_str("the provider rate limit was reached"),
            Self::Timeout => f.write_str("the provider did not respond in time"),
            Self::Network => f.write_str("the provider could not be reached"),
            Self::ProviderUnavailable => f.write_str("the provider reported a server error"),
            Self::UnexpectedResponse => f.write_str("the provider returned an unexpected response"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Rejects obviously unusable input before any network request.
pub fn precheck_token(token: &str) -> Result<(), ProviderError> {
    if token.is_empty() {
        return Err(ProviderError::EmptyToken);
    }
    if !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(ProviderError::MalformedToken);
    }
    Ok(())
}

/// Capability summary for `AccountSummary.capabilitySummary` plus non-blocking warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    pub capability_summary: Vec<String>,
    pub warnings: Vec<String>,
}

fn has(scopes: &[String], scope: &str) -> bool {
    scopes.iter().any(|s| s == scope)
}

/// GitHub is the read-only source. Classic tokens need `repo` for private repositories
/// (`public_repo` limits access to public ones); without either they cannot read code.
pub fn assess_github_source(identity: &TokenIdentity) -> Result<Assessment, ProviderError> {
    match &identity.kind {
        TokenKind::GitHubScoped { scopes } => {
            if has(scopes, "repo") {
                let mut warnings = Vec::new();
                warnings.push(
                    "Classic token scope `repo` also grants write access; the app only reads from GitHub."
                        .to_owned(),
                );
                Ok(Assessment {
                    capability_summary: vec![format!("Scopes: {}", scopes.join(", "))],
                    warnings,
                })
            } else if has(scopes, "public_repo") {
                Ok(Assessment {
                    capability_summary: vec![format!("Scopes: {}", scopes.join(", "))],
                    warnings: vec![
                        "Only public repositories are accessible with `public_repo`.".to_owned(),
                    ],
                })
            } else {
                Err(ProviderError::InsufficientScope {
                    missing: vec!["repo"],
                })
            }
        }
        TokenKind::GitHubUnscoped => Ok(Assessment {
            capability_summary: vec![
                "Fine-grained or unscoped token: repository access is checked during discovery."
                    .to_owned(),
            ],
            warnings: vec![
                "Only repositories this token was granted will be discovered.".to_owned(),
            ],
        }),
        TokenKind::GitLabPersonal { .. } => Err(ProviderError::UnexpectedResponse),
    }
}

/// GitLab is the destination: creating projects needs `api`, which for personal access
/// tokens also covers Git-over-HTTP push. `write_repository` alone can push to existing
/// projects but cannot create them, so `api` is required. `sudo` is never needed.
pub fn assess_gitlab_destination(identity: &TokenIdentity) -> Result<Assessment, ProviderError> {
    let TokenKind::GitLabPersonal { scopes } = &identity.kind else {
        return Err(ProviderError::UnexpectedResponse);
    };
    if !has(scopes, "api") {
        return Err(ProviderError::InsufficientScope {
            missing: vec!["api"],
        });
    }
    let mut warnings = vec![
        "Namespace permission to create projects is checked when the destination is chosen."
            .to_owned(),
    ];
    if has(scopes, "sudo") {
        warnings.push(
            "The token has `sudo`, which this app never needs; consider a narrower token."
                .to_owned(),
        );
    }
    if identity.expires_at.is_none() {
        warnings.push("The provider reported no expiry date for this token.".to_owned());
    }
    Ok(Assessment {
        capability_summary: vec![format!("Scopes: {}", scopes.join(", "))],
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(kind: TokenKind) -> TokenIdentity {
        TokenIdentity {
            provider: ProviderKind::GitHub,
            provider_user_id: "1".into(),
            username: "octo".into(),
            kind,
            expires_at: None,
        }
    }

    fn scoped(list: &[&str]) -> TokenKind {
        TokenKind::GitHubScoped {
            scopes: list.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn gitlab(list: &[&str]) -> TokenKind {
        TokenKind::GitLabPersonal {
            scopes: list.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn precheck_rejects_empty_and_malformed_tokens() {
        assert_eq!(precheck_token(""), Err(ProviderError::EmptyToken));
        for bad in ["abc def", "abc\n", "tab\tx", "ünïcode"] {
            assert_eq!(
                precheck_token(bad),
                Err(ProviderError::MalformedToken),
                "{bad:?}"
            );
        }
        assert_eq!(precheck_token("ghp_abcDEF123"), Ok(()));
    }

    #[test]
    fn github_scope_assessment() {
        assert!(assess_github_source(&identity(scoped(&["repo", "read:user"]))).is_ok());
        let public = assess_github_source(&identity(scoped(&["public_repo"])));
        assert!(public.is_ok_and(|a| a.warnings.iter().any(|w| w.contains("public"))));
        assert_eq!(
            assess_github_source(&identity(scoped(&["read:user"]))),
            Err(ProviderError::InsufficientScope {
                missing: vec!["repo"]
            })
        );
        assert_eq!(
            assess_github_source(&identity(scoped(&[]))),
            Err(ProviderError::InsufficientScope {
                missing: vec!["repo"]
            })
        );
        let unscoped = assess_github_source(&identity(TokenKind::GitHubUnscoped));
        assert!(
            unscoped.is_ok_and(|a| a.capability_summary[0].contains("checked during discovery"))
        );
    }

    #[test]
    fn gitlab_scope_assessment_never_claims_more_than_scopes() {
        for insufficient in [
            &["read_api"][..],
            &["write_repository"],
            &["read_user", "read_repository"],
        ] {
            assert_eq!(
                assess_gitlab_destination(&identity(gitlab(insufficient))),
                Err(ProviderError::InsufficientScope {
                    missing: vec!["api"]
                })
            );
        }
        let ok = assess_gitlab_destination(&identity(gitlab(&["api"])));
        assert!(ok.as_ref().is_ok_and(|a| {
            a.warnings
                .iter()
                .any(|w| w.contains("Namespace permission"))
        }));
        let sudo = assess_gitlab_destination(&identity(gitlab(&["api", "sudo"])));
        assert!(sudo.is_ok_and(|a| a.warnings.iter().any(|w| w.contains("sudo"))));
        assert_eq!(
            assess_gitlab_destination(&identity(scoped(&["repo"]))),
            Err(ProviderError::UnexpectedResponse)
        );
    }

    #[test]
    fn errors_map_to_contract_codes() {
        assert_eq!(
            ProviderError::InvalidOrExpired.error_code(),
            ErrorCode::AuthRequired
        );
        assert_eq!(
            ProviderError::RateLimited {
                retry_after_seconds: Some(5)
            }
            .error_code(),
            ErrorCode::RateLimited
        );
        assert!(ProviderError::Timeout.retryable());
        assert!(!ProviderError::InvalidOrExpired.retryable());
        assert_eq!(
            ProviderError::UnexpectedResponse.error_code(),
            ErrorCode::ContractMismatch
        );
    }
}
