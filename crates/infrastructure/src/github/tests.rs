//! TEST-PAT (GitHub) against the loopback controlled adapter.

use std::time::Duration;

use gbm_application::providers::{ProviderError, TokenKind, assess_github_source};
use gbm_domain::contract::secret::SecretString;

use super::GitHubClient;
use crate::http::Limits;
use crate::http::test_server::{Reply, TestServer};

const SENTINEL: &str = "ghp_TESTSENTINELGITHUB0001";
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn token() -> SecretString {
    SecretString::new(SENTINEL.to_owned())
}

fn reply(code: u16, headers: &[(&str, &str)], body: &str) -> Reply {
    Reply::Respond(
        code,
        headers
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect(),
        body.to_owned(),
    )
}

async fn validate(
    replies: Vec<Reply>,
) -> Result<
    (
        Result<gbm_application::providers::TokenIdentity, ProviderError>,
        Vec<String>,
    ),
    Box<dyn std::error::Error>,
> {
    let server = TestServer::start(replies).await?;
    let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
    let outcome = client.validate_token(&token()).await;
    Ok((outcome, server.requests()))
}

#[tokio::test]
async fn classic_token_identity_and_scopes() -> TestResult {
    let (outcome, requests) = validate(vec![reply(
        200,
        &[("x-oauth-scopes", "repo, read:user")],
        r#"{"id": 9007199254740993, "login": "octo", "extra": 1}"#,
    )])
    .await?;
    let identity = outcome?;
    assert_eq!(
        identity.provider_user_id, "9007199254740993",
        "IDs are not squeezed through f64"
    );
    assert_eq!(identity.username, "octo");
    assert_eq!(
        identity.kind,
        TokenKind::GitHubScoped {
            scopes: vec!["repo".into(), "read:user".into()]
        }
    );
    assert_eq!(identity.expires_at, None, "expiry is not fabricated");
    assert!(assess_github_source(&identity).is_ok());
    assert_eq!(requests.len(), 1);
    let head = requests[0].to_ascii_lowercase();
    assert!(head.starts_with("get /user http/1.1"), "{head}");
    assert!(head.contains(&format!(
        "authorization: bearer {}",
        SENTINEL.to_ascii_lowercase()
    )));
    assert!(head.contains("x-github-api-version: 2026-03-10"));
    assert!(head.contains("user-agent: git-backup-manager/"));
    assert!(
        !requests[0]
            .lines()
            .next()
            .unwrap_or_default()
            .contains(SENTINEL),
        "token never in the URL"
    );
    Ok(())
}

#[tokio::test]
async fn fine_grained_token_permissions_are_not_guessed() -> TestResult {
    let (outcome, _) = validate(vec![reply(200, &[], r#"{"id": 1, "login": "octo"}"#)]).await?;
    let identity = outcome?;
    assert_eq!(identity.kind, TokenKind::GitHubUnscoped);
    let assessment = assess_github_source(&identity)?;
    assert!(assessment.capability_summary[0].contains("checked during discovery"));
    Ok(())
}

#[tokio::test]
async fn insufficient_scope_is_actionable() -> TestResult {
    let (outcome, _) = validate(vec![reply(
        200,
        &[("x-oauth-scopes", "read:user")],
        r#"{"id": 1, "login": "octo"}"#,
    )])
    .await?;
    assert_eq!(
        assess_github_source(&outcome?),
        Err(ProviderError::InsufficientScope {
            missing: vec!["repo"]
        })
    );
    Ok(())
}

#[tokio::test]
async fn failures_are_typed() -> TestResult {
    let cases: Vec<(Reply, ProviderError)> = vec![
        (
            reply(401, &[], r#"{"message":"Bad credentials"}"#),
            ProviderError::InvalidOrExpired,
        ),
        (
            reply(403, &[("x-ratelimit-remaining", "0")], "{}"),
            ProviderError::RateLimited {
                retry_after_seconds: None,
            },
        ),
        (
            reply(403, &[("retry-after", "60")], "{}"),
            ProviderError::RateLimited {
                retry_after_seconds: Some(60),
            },
        ),
        (
            reply(429, &[("retry-after", "7")], "{}"),
            ProviderError::RateLimited {
                retry_after_seconds: Some(7),
            },
        ),
        (
            reply(403, &[("x-ratelimit-remaining", "4999")], "{}"),
            ProviderError::AccessDenied,
        ),
        (
            reply(502, &[], "bad gateway"),
            ProviderError::ProviderUnavailable,
        ),
        (
            reply(302, &[("location", "https://evil.example/steal")], ""),
            ProviderError::UnexpectedResponse,
        ),
        (
            reply(200, &[], "not json"),
            ProviderError::UnexpectedResponse,
        ),
        (
            reply(200, &[], r#"{"id": "1", "login": "octo"}"#),
            ProviderError::UnexpectedResponse,
        ),
        (
            reply(200, &[], r#"{"id": 1, "login": ""}"#),
            ProviderError::UnexpectedResponse,
        ),
    ];
    for (canned, expected) in cases {
        let (outcome, requests) =
            validate(vec![canned, reply(200, &[], r#"{"id":1,"login":"x"}"#)]).await?;
        assert_eq!(outcome.as_ref().err(), Some(&expected));
        assert_eq!(
            requests.len(),
            1,
            "no redirect following or retries for {expected:?}"
        );
        assert!(!format!("{expected} {expected:?}").contains(SENTINEL));
    }
    Ok(())
}

#[tokio::test]
async fn oversized_and_slow_responses_are_bounded() -> TestResult {
    // Valid JSON that would otherwise parse: only the size bound can reject it.
    let padded = format!(
        r#"{{"id": 1, "login": "octo", "pad": "{}"}}"#,
        "x".repeat(1024 * 1024)
    );
    let (outcome, _) = validate(vec![reply(200, &[], &padded)]).await?;
    assert_eq!(outcome.err(), Some(ProviderError::UnexpectedResponse));
    let server = TestServer::start(vec![Reply::Hang]).await?;
    let limits = Limits {
        timeout: Duration::from_millis(300),
        connect_timeout: Duration::from_millis(300),
    };
    let client = GitHubClient::for_test(&server.origin, &limits)?;
    assert_eq!(
        client.validate_token(&token()).await.err(),
        Some(ProviderError::Timeout)
    );
    Ok(())
}

#[tokio::test]
async fn empty_and_malformed_tokens_never_reach_the_network() -> TestResult {
    let server = TestServer::start(vec![reply(200, &[], "{}")]).await?;
    let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
    assert_eq!(
        client
            .validate_token(&SecretString::new(String::new()))
            .await
            .err(),
        Some(ProviderError::EmptyToken)
    );
    let injected = SecretString::new("ghp_x\r\nX-Evil: 1".to_owned());
    assert_eq!(
        client.validate_token(&injected).await.err(),
        Some(ProviderError::MalformedToken)
    );
    assert!(server.requests().is_empty());
    Ok(())
}

#[test]
fn production_client_is_https_only_on_the_fixed_origin() -> TestResult {
    assert_eq!(super::GITHUB_API_ORIGIN, "https://api.github.com/");
    let _client = GitHubClient::new()?;
    Ok(())
}
