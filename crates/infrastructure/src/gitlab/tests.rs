//! TEST-PAT (GitLab) against the loopback controlled adapter.

use gbm_application::providers::{
    ProviderError, TokenIdentity, TokenKind, assess_gitlab_destination,
};
use gbm_domain::contract::secret::SecretString;

use super::GitLabClient;
use crate::http::Limits;
use crate::http::test_server::{Reply, TestServer};

const SENTINEL: &str = "glpat-TESTSENTINELGITLAB01";
type TestResult = Result<(), Box<dyn std::error::Error>>;

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

fn token_body(scopes: &str, active: bool, revoked: bool, expires: &str) -> String {
    format!(
        r#"{{"id":5,"name":"backup","revoked":{revoked},"created_at":"2026-01-01T00:00:00.000Z","scopes":{scopes},"user_id":77,"last_used_at":null,"active":{active},"expires_at":{expires}}}"#
    )
}

const USER: &str = r#"{"id":77,"username":"backup-bot","name":"Backup Bot"}"#;

async fn validate(
    replies: Vec<Reply>,
) -> Result<(Result<TokenIdentity, ProviderError>, Vec<String>), Box<dyn std::error::Error>> {
    let server = TestServer::start(replies).await?;
    let client = GitLabClient::for_test(&server.origin, &Limits::default())?;
    let outcome = client
        .validate_token(&SecretString::new(SENTINEL.to_owned()))
        .await;
    Ok((outcome, server.requests()))
}

#[tokio::test]
async fn valid_token_with_api_scope() -> TestResult {
    let (outcome, requests) = validate(vec![
        reply(
            200,
            &[],
            &token_body(r#"["api","read_user"]"#, true, false, r#""2026-12-31""#),
        ),
        reply(200, &[], USER),
    ])
    .await?;
    let identity = outcome?;
    assert_eq!(
        (
            identity.provider_user_id.as_str(),
            identity.username.as_str()
        ),
        ("77", "backup-bot")
    );
    assert_eq!(
        identity.kind,
        TokenKind::GitLabPersonal {
            scopes: vec!["api".into(), "read_user".into()]
        }
    );
    assert_eq!(identity.expires_at.as_deref(), Some("2026-12-31T00:00:00Z"));
    assert!(assess_gitlab_destination(&identity).is_ok());
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].starts_with("GET /api/v4/personal_access_tokens/self HTTP/1.1"),
        "{}",
        requests[0]
    );
    assert!(requests[1].starts_with("GET /api/v4/user HTTP/1.1"));
    for head in &requests {
        assert!(
            head.to_ascii_lowercase()
                .contains(&format!("private-token: {}", SENTINEL.to_ascii_lowercase()))
        );
        assert!(
            !head.lines().next().unwrap_or_default().contains(SENTINEL),
            "token never in the URL"
        );
    }
    Ok(())
}

#[tokio::test]
async fn insufficient_scope_and_missing_expiry() -> TestResult {
    let (outcome, _) = validate(vec![
        reply(
            200,
            &[],
            &token_body(r#"["read_api","write_repository"]"#, true, false, "null"),
        ),
        reply(200, &[], USER),
    ])
    .await?;
    let identity = outcome?;
    assert_eq!(identity.expires_at, None);
    assert_eq!(
        assess_gitlab_destination(&identity),
        Err(ProviderError::InsufficientScope {
            missing: vec!["api"]
        })
    );
    Ok(())
}

#[tokio::test]
async fn revoked_inactive_invalid_and_inconsistent_tokens() -> TestResult {
    let cases: Vec<(Vec<Reply>, ProviderError, usize)> = vec![
        (
            vec![reply(
                200,
                &[],
                &token_body(r#"["api"]"#, true, true, "null"),
            )],
            ProviderError::Revoked,
            1,
        ),
        (
            vec![reply(
                200,
                &[],
                &token_body(r#"["api"]"#, false, false, r#""2025-01-01""#),
            )],
            ProviderError::Revoked,
            1,
        ),
        (
            vec![reply(401, &[], r#"{"message":"401 Unauthorized"}"#)],
            ProviderError::InvalidOrExpired,
            1,
        ),
        (
            vec![reply(429, &[("retry-after", "30")], "{}")],
            ProviderError::RateLimited {
                retry_after_seconds: Some(30),
            },
            1,
        ),
        (vec![reply(403, &[], "{}")], ProviderError::AccessDenied, 1),
        (
            vec![reply(503, &[], "")],
            ProviderError::ProviderUnavailable,
            1,
        ),
        (
            vec![reply(301, &[("location", "https://gitlab.example/")], "")],
            ProviderError::UnexpectedResponse,
            1,
        ),
        (
            vec![reply(
                200,
                &[],
                &token_body(r#"["api"]"#, true, false, r#""31/12/2026""#),
            )],
            ProviderError::UnexpectedResponse,
            1,
        ),
        (
            vec![
                reply(200, &[], &token_body(r#"["api"]"#, true, false, "null")),
                reply(200, &[], r#"{"id":78,"username":"someone-else"}"#),
            ],
            ProviderError::UnexpectedResponse,
            2,
        ),
        (
            vec![
                reply(200, &[], &token_body(r#"["api"]"#, true, false, "null")),
                reply(401, &[], "{}"),
            ],
            ProviderError::InvalidOrExpired,
            2,
        ),
    ];
    for (replies, expected, calls) in cases {
        let (outcome, requests) = validate(replies).await?;
        assert_eq!(outcome.as_ref().err(), Some(&expected));
        assert_eq!(requests.len(), calls, "{expected:?}");
        assert!(!format!("{expected} {expected:?}").contains(SENTINEL));
    }
    Ok(())
}

#[tokio::test]
async fn empty_token_never_reaches_the_network() -> TestResult {
    let server = TestServer::start(vec![reply(200, &[], "{}")]).await?;
    let client = GitLabClient::for_test(&server.origin, &Limits::default())?;
    assert_eq!(
        client
            .validate_token(&SecretString::new(String::new()))
            .await
            .err(),
        Some(ProviderError::EmptyToken)
    );
    assert!(server.requests().is_empty());
    Ok(())
}

#[test]
fn production_client_targets_gitlab_com_over_https() -> TestResult {
    assert_eq!(super::GITLAB_ORIGIN, "https://gitlab.com/");
    let _client = GitLabClient::new()?;
    Ok(())
}

#[tokio::test]
async fn namespaces_are_listed_with_next_page_and_never_marked_creatable() -> TestResult {
    let server = TestServer::start(vec![reply(
        200,
        &[("x-next-page", "2")],
        r#"[{"id":1,"name":"backup-bot","path":"backup-bot","kind":"user","full_path":"backup-bot"},
            {"id":9,"name":"Mirrors","path":"mirrors","kind":"group","full_path":"acme/mirrors"}]"#,
    )])
    .await?;
    let client = GitLabClient::for_test(&server.origin, &Limits::default())?;
    let (items, next) = client
        .list_namespaces(&SecretString::new(SENTINEL.to_owned()), 1, 500)
        .await?;
    assert_eq!(
        items
            .iter()
            .map(|n| n.full_path.as_str())
            .collect::<Vec<_>>(),
        ["backup-bot", "acme/mirrors"]
    );
    assert_eq!(items[0].id, "gl-ns-1");
    assert_eq!(next, Some(2));
    let requests = server.requests();
    assert!(
        requests[0].contains("per_page=100"),
        "page size is clamped: {requests:?}"
    );
    assert!(
        requests
            .iter()
            .all(|r| !r.contains(SENTINEL) || r.to_ascii_lowercase().contains("private-token"))
    );
    Ok(())
}

#[tokio::test]
async fn namespaces_auth_failure_is_typed() -> TestResult {
    let server = TestServer::start(vec![reply(401, &[], "{}")]).await?;
    let client = GitLabClient::for_test(&server.origin, &Limits::default())?;
    let outcome = client
        .list_namespaces(&SecretString::new(SENTINEL.to_owned()), 1, 20)
        .await;
    assert_eq!(outcome.err(), Some(ProviderError::InvalidOrExpired));
    Ok(())
}
