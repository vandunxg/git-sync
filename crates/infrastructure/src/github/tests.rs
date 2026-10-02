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

// --- TEST-PAGINATION / TEST-SCOPE / TEST-NO-DELETE-ON-404 (GitHub discovery) -------

mod discovery {
    use std::sync::{Arc, Mutex};

    use gbm_application::discovery::{
        CatalogEntry, DiscoveryScope, Incomplete, Relation, reconcile,
    };
    use gbm_application::providers::ProviderError;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::{SENTINEL, TestResult, token};
    use crate::github::GitHubClient;
    use crate::http::Limits;

    /// Status, headers, body.
    type CannedReply = (u16, Vec<(&'static str, String)>, String);

    /// Loopback server whose canned headers/bodies may contain `{origin}`, replaced by
    /// the bound address (pagination links are absolute URLs).
    struct PagedServer {
        origin: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    async fn serve(replies: Vec<CannedReply>) -> std::io::Result<PagedServer> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}/", listener.local_addr()?);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let base = origin.clone();
        tokio::spawn(async move {
            for (code, headers, body) in replies {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut head = Vec::new();
                let mut buf = [0u8; 4096];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                if let Ok(mut log) = seen.lock() {
                    log.push(String::from_utf8_lossy(&head).into_owned());
                }
                let body = body.replace("{origin}", &base);
                let mut text = format!(
                    "HTTP/1.1 {code} X\r\ncontent-length: {}\r\nconnection: close\r\n",
                    body.len()
                );
                for (name, value) in headers {
                    text.push_str(&format!("{name}: {}\r\n", value.replace("{origin}", &base)));
                }
                text.push_str("\r\n");
                text.push_str(&body);
                let _ = socket.write_all(text.as_bytes()).await;
            }
        });
        Ok(PagedServer { origin, requests })
    }

    fn entry(id: u64, owner: &str, kind: &str, name: &str) -> String {
        format!(
            r#"{{"id":{id},"name":"{name}","full_name":"{owner}/{name}","owner":{{"login":"{owner}","type":"{kind}"}},"private":false,"fork":false,"archived":false,"default_branch":"main","pushed_at":"2026-09-30T10:00:00Z"}}"#
        )
    }

    fn page(entries: &[String]) -> String {
        format!("[{}]", entries.join(","))
    }

    fn next(n: u32) -> Vec<(&'static str, String)> {
        vec![(
            "link",
            format!(
                r#"<{{origin}}user/repos?page={n}&per_page=100>; rel="next", <{{origin}}user/repos?page=3>; rel="last""#
            ),
        )]
    }

    const ALL: DiscoveryScope = DiscoveryScope {
        include_private: true,
        include_forks: true,
        include_archived: true,
        include_collaborations: true,
        include_organizations: true,
    };

    #[tokio::test]
    async fn walks_every_page_with_absolute_next_links() -> TestResult {
        let server = serve(vec![
            (
                200,
                next(2),
                page(&[entry(1, "me", "User", "a"), entry(2, "me", "User", "b")]),
            ),
            (200, next(3), page(&[entry(3, "acme", "Organization", "c")])),
            (200, vec![], page(&[entry(4, "friend", "User", "d")])),
        ])
        .await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client.discover_repositories(&token(), "me", ALL, 10).await;
        assert!(discovery.is_complete(), "{:?}", discovery.incomplete);
        assert_eq!(discovery.pages_fetched, 3);
        let relations: Vec<_> = discovery
            .items
            .iter()
            .map(|r| (r.full_name.as_str(), r.relation))
            .collect();
        assert_eq!(
            relations,
            [
                ("me/a", Relation::Owner),
                ("me/b", Relation::Owner),
                ("acme/c", Relation::Organization),
                ("friend/d", Relation::Collaborator)
            ]
        );
        let requests = server
            .requests
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default();
        assert!(requests[0].starts_with(
            "GET /user/repos?affiliation=owner%2Ccollaborator%2Corganization_member&visibility=all"
        ));
        assert!(requests[1].starts_with("GET /user/repos?page=2&per_page=100"));
        assert!(
            requests
                .iter()
                .all(|r| !r.lines().next().unwrap_or_default().contains(SENTINEL)),
            "token never in the URL"
        );
        Ok(())
    }

    /// TEST-PAGINATION: three pages, page two rate-limits or is forbidden.
    #[tokio::test]
    async fn failing_middle_page_keeps_prior_items_and_is_incomplete() -> TestResult {
        for (code, headers, expected) in [
            (
                403,
                vec![
                    ("x-ratelimit-remaining", "0".to_owned()),
                    ("retry-after", "60".to_owned()),
                ],
                ProviderError::RateLimited {
                    retry_after_seconds: Some(60),
                },
            ),
            (403, vec![], ProviderError::AccessDenied),
            (
                429,
                vec![],
                ProviderError::RateLimited {
                    retry_after_seconds: None,
                },
            ),
        ] {
            let server = serve(vec![
                (
                    200,
                    next(2),
                    page(&[entry(1, "me", "User", "a"), entry(2, "me", "User", "b")]),
                ),
                (code, headers, "{}".to_owned()),
            ])
            .await?;
            let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
            let discovery = client.discover_repositories(&token(), "me", ALL, 10).await;
            assert_eq!(discovery.items.len(), 2, "page one is kept");
            assert_eq!(
                discovery.incomplete,
                Some(Incomplete::PageFailed {
                    page: 2,
                    error: expected
                })
            );

            // TEST-NO-DELETE-ON-404: nothing known is deleted or flagged on a partial scan.
            let existing = vec![
                CatalogEntry {
                    provider_repository_id: "1".into(),
                    full_name: "me/a".into(),
                },
                CatalogEntry {
                    provider_repository_id: "99".into(),
                    full_name: "me/on-page-three".into(),
                },
            ];
            let delta = reconcile(&existing, &discovery);
            assert!(!delta.complete);
            assert_eq!(delta.not_observed, ["99"]);
            assert!(delta.source_unavailable_candidates.is_empty());
        }
        Ok(())
    }

    /// Review H1: commas inside a next URL must not truncate it, and an unparsable
    /// `Link` header must never be read as "last page".
    #[tokio::test]
    async fn raw_commas_in_next_link_are_followed_and_garbage_fails_closed() -> TestResult {
        let comma_link = vec![(
            "link",
            r#"<{origin}user/repos?affiliation=owner,collaborator&page=2>; rel="next", <{origin}user/repos?affiliation=owner,collaborator&page=2>; rel="last""#.to_owned(),
        )];
        let server = serve(vec![
            (200, comma_link, page(&[entry(1, "me", "User", "a")])),
            (200, vec![], page(&[entry(2, "me", "User", "b")])),
        ])
        .await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client.discover_repositories(&token(), "me", ALL, 10).await;
        assert!(discovery.is_complete());
        assert_eq!(discovery.items.len(), 2);
        let requests = server
            .requests
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default();
        assert!(requests[1].starts_with("GET /user/repos?affiliation=owner,collaborator&page=2 "));

        let server = serve(vec![(
            200,
            vec![("link", "garbage; rel=next".to_owned())],
            page(&[entry(1, "me", "User", "a")]),
        )])
        .await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client.discover_repositories(&token(), "me", ALL, 10).await;
        assert_eq!(
            discovery.incomplete,
            Some(Incomplete::PageFailed {
                page: 2,
                error: ProviderError::UnexpectedResponse
            })
        );
        Ok(())
    }

    #[tokio::test]
    async fn foreign_next_link_stops_without_following_it() -> TestResult {
        let server = serve(vec![(
            200,
            vec![(
                "link",
                r#"<https://evil.example/user/repos?page=2>; rel="next""#.to_owned(),
            )],
            page(&[entry(1, "me", "User", "a")]),
        )])
        .await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client.discover_repositories(&token(), "me", ALL, 10).await;
        assert_eq!(discovery.items.len(), 1);
        assert_eq!(
            discovery.incomplete,
            Some(Incomplete::PageFailed {
                page: 2,
                error: ProviderError::UnexpectedResponse
            })
        );
        assert_eq!(
            server.requests.lock().map(|r| r.len()).unwrap_or(0),
            1,
            "the foreign URL was never requested"
        );
        Ok(())
    }

    #[tokio::test]
    async fn malformed_entries_and_page_limit_make_it_incomplete() -> TestResult {
        let bad = r#"{"id":5,"name":"../x","full_name":"me/../x","owner":{"login":"me","type":"User"},"private":false,"fork":false,"archived":false,"default_branch":null,"pushed_at":null}"#.to_owned();
        let server = serve(vec![(
            200,
            vec![],
            page(&[entry(1, "me", "User", "a"), bad, "{}".to_owned()]),
        )])
        .await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client.discover_repositories(&token(), "me", ALL, 10).await;
        assert_eq!(discovery.items.len(), 1);
        assert_eq!(
            discovery.incomplete,
            Some(Incomplete::RejectedEntries { count: 2 })
        );

        let server = serve(vec![(200, next(2), page(&[entry(1, "me", "User", "a")]))]).await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client.discover_repositories(&token(), "me", ALL, 1).await;
        assert_eq!(
            discovery.incomplete,
            Some(Incomplete::PageLimitReached { pages: 1 })
        );
        Ok(())
    }

    /// TEST-SCOPE: defaults request owned repositories only and drop others locally too.
    #[tokio::test]
    async fn default_scope_requests_and_keeps_owned_only() -> TestResult {
        let scope = DiscoveryScope {
            include_collaborations: false,
            include_organizations: false,
            ..ALL
        };
        let server = serve(vec![(
            200,
            vec![],
            page(&[
                entry(1, "me", "User", "a"),
                entry(2, "acme", "Organization", "c"),
            ]),
        )])
        .await?;
        let client = GitHubClient::for_test(&server.origin, &Limits::default())?;
        let discovery = client
            .discover_repositories(&token(), "me", scope, 10)
            .await;
        assert_eq!(
            discovery
                .items
                .iter()
                .map(|r| r.full_name.as_str())
                .collect::<Vec<_>>(),
            ["me/a"]
        );
        assert_eq!(discovery.out_of_scope, ["2"]);
        let requests = server
            .requests
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default();
        assert!(requests[0].starts_with("GET /user/repos?affiliation=owner&"));
        Ok(())
    }
}
