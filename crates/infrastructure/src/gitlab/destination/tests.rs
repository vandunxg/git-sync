//! TEST-PROJECT-CREATE / TEST-PRIVACY through the real HTTP adapter against the
//! loopback controlled server (no GitLab account is touched).

use std::time::Duration;

use gbm_application::destination::{DestinationBlock, EnsureOutcome, ensure_project};

use super::*;
use crate::http::Limits;
use crate::http::test_server::{Reply, TestServer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SENTINEL: &str = "glpat-TESTSENTINELGITLAB01";
const MARKER: &str = "f3a9c1d2e4b5a6978877";

fn ok(code: u16, body: &str) -> Reply {
    Reply::Respond(code, Vec::new(), body.to_owned())
}

fn project_json(origin: &str, id: u64, path: &str, description: &str, empty: bool) -> String {
    format!(
        r#"{{"id":{id},"path":"x","path_with_namespace":"{path}","namespace":{{"id":77,"full_path":"backups"}},"visibility":"private","http_url_to_repo":"{origin}{path}.git","description":"{description}","empty_repo":{empty},"permissions":{{"project_access":{{"access_level":30}},"group_access":{{"access_level":50}}}}}}"#
    )
}

fn intent() -> CreateIntent {
    CreateIntent {
        namespace_id: 77,
        namespace_full_path: "backups".to_owned(),
        path: "tool-gh42".to_owned(),
        visibility: Visibility::Private,
        source_private: true,
        marker: MARKER.to_owned(),
    }
}

/// Starts the loopback server; reply bodies may use the origin placeholder.
async fn server(
    replies: impl FnOnce(&str) -> Vec<Reply>,
) -> Result<TestServer, Box<dyn std::error::Error>> {
    Ok(TestServer::start(replies("{{ORIGIN}}/")).await?)
}

fn client(server: &TestServer, timeout: Duration) -> Result<GitLabClient, ProviderError> {
    GitLabClient::for_test(
        &server.origin,
        &Limits {
            timeout,
            connect_timeout: timeout,
        },
    )
}

fn token() -> SecretString {
    SecretString::new(SENTINEL.to_owned())
}

#[tokio::test]
async fn lookup_by_exact_path_parses_and_validates() -> TestResult {
    let srv = server(|o| {
        vec![
            ok(404, r#"{"message":"404 Project Not Found"}"#),
            ok(200, &project_json(o, 9, "backups/tool-gh42", "x", true)),
        ]
    })
    .await?;
    let gitlab = client(&srv, Duration::from_secs(5))?;
    let token = token();
    let api = GitLabDestination::new(&gitlab, &token)?;
    assert_eq!(api.find_by_path("backups/tool-gh42").await?, None);
    let found = api
        .find_by_path("backups/tool-gh42")
        .await?
        .ok_or("found")?;
    assert_eq!(found.project_id, 9);
    assert_eq!(found.namespace_id, 77);
    assert_eq!(found.access_level, Some(50));
    assert_eq!(found.visibility, Visibility::Private);
    let requests = srv.requests();
    assert!(
        requests[0].starts_with("GET /api/v4/projects/backups%2Ftool-gh42 HTTP/1.1"),
        "{}",
        requests[0]
    );
    assert!(requests[0].to_ascii_lowercase().contains("private-token: "));
    Ok(())
}

#[tokio::test]
async fn untrusted_project_payloads_are_rejected() -> TestResult {
    let srv = server(|o| {
        vec![
            // Clone URL on another host.
            ok(
                200,
                &project_json("https://evil.example/", 9, "backups/tool-gh42", "x", true),
            ),
            // Userinfo in the clone URL.
            ok(
                200,
                &project_json(o, 9, "backups/tool-gh42", "x", true)
                    .replace("\"{{ORIGIN}}/", "\"http://user:pw@{{HOSTPORT}}/"),
            ),
            // A different project answered for the requested path (rename route).
            ok(200, &project_json(o, 9, "backups/renamed", "x", true)),
            // Unknown visibility.
            ok(
                200,
                &project_json(o, 9, "backups/tool-gh42", "x", true)
                    .replace("\"private\"", "\"secret\""),
            ),
            // Redirects are not followed.
            Reply::Respond(
                301,
                vec![("location".to_owned(), "https://evil.example/".to_owned())],
                String::new(),
            ),
        ]
    })
    .await?;
    let gitlab = client(&srv, Duration::from_secs(5))?;
    let token = token();
    let api = GitLabDestination::new(&gitlab, &token)?;
    for case in 0..5 {
        assert!(
            api.find_by_path("backups/tool-gh42").await.is_err(),
            "case {case}"
        );
    }
    assert!(api.find_by_path("backups/../x").await.is_err());
    assert_eq!(srv.requests().len(), 5, "unsafe input never sent");
    Ok(())
}

#[tokio::test]
async fn create_sends_a_private_empty_project_request() -> TestResult {
    let srv = server(|o| {
        vec![
            ok(404, "{}"),
            ok(
                201,
                &project_json(
                    o,
                    1000,
                    "backups/tool-gh42",
                    &format!("Git Backup Manager backup. gbm-intent:{MARKER}"),
                    true,
                ),
            ),
        ]
    })
    .await?;
    let gitlab = client(&srv, Duration::from_secs(5))?;
    let token = token();
    let api = GitLabDestination::new(&gitlab, &token)?;
    let outcome = ensure_project(&api, &intent()).await?;
    assert!(matches!(outcome, EnsureOutcome::Created(ref p) if p.project_id == 1000));
    let requests = srv.requests();
    assert_eq!(requests.len(), 2);
    let post = &requests[1];
    assert!(post.starts_with("POST /api/v4/projects HTTP/1.1"), "{post}");
    let body = post.split("\r\n\r\n").nth(1).ok_or("body")?;
    let json: serde_json::Value = serde_json::from_str(body)?;
    assert_eq!(json["visibility"], "private");
    assert_eq!(json["initialize_with_readme"], false);
    assert_eq!(json["path"], "tool-gh42");
    assert_eq!(json["namespace_id"], 77);
    assert_eq!(
        json["description"],
        format!("Git Backup Manager backup. gbm-intent:{MARKER}")
    );
    // The token travels only in its header, never in the body.
    assert!(!body.contains(SENTINEL));
    Ok(())
}

#[tokio::test]
async fn timed_out_create_is_reconciled_by_exact_lookup() -> TestResult {
    // TEST-PROJECT-CREATE: the POST hangs past the client timeout (GitLab may have
    // created it); the follow-up lookup finds the project with this intent's marker.
    let srv = server(|o| {
        vec![
            ok(404, "{}"),
            Reply::Hang,
            ok(
                200,
                &project_json(
                    o,
                    1000,
                    "backups/tool-gh42",
                    &format!("Git Backup Manager backup. gbm-intent:{MARKER}"),
                    true,
                ),
            ),
        ]
    })
    .await?;
    let gitlab = client(&srv, Duration::from_millis(500))?;
    let token = token();
    let api = GitLabDestination::new(&gitlab, &token)?;
    let outcome = ensure_project(&api, &intent()).await?;
    assert!(matches!(outcome, EnsureOutcome::Reconciled(ref p) if p.project_id == 1000));
    let requests = srv.requests();
    let posts = requests.iter().filter(|r| r.starts_with("POST ")).count();
    assert_eq!(posts, 1, "never a second create");
    assert!(requests[2].starts_with("GET /api/v4/projects/backups%2Ftool-gh42 "));
    Ok(())
}

#[tokio::test]
async fn a_same_name_unrelated_project_blocks_without_any_write() -> TestResult {
    let srv = server(|o| {
        vec![ok(
            200,
            &project_json(o, 5, "backups/tool-gh42", "my own project", false),
        )]
    })
    .await?;
    let gitlab = client(&srv, Duration::from_secs(5))?;
    let token = token();
    let api = GitLabDestination::new(&gitlab, &token)?;
    assert_eq!(
        ensure_project(&api, &intent()).await?,
        EnsureOutcome::Blocked(DestinationBlock::Foreign { project_id: 5 })
    );
    assert!(srv.requests().iter().all(|r| r.starts_with("GET ")));
    Ok(())
}

#[tokio::test]
async fn a_rejected_create_with_nothing_found_is_an_error() -> TestResult {
    let srv = server(|_| {
        vec![
            ok(404, "{}"),
            ok(400, r#"{"message":{"path":["has already been taken"]}}"#),
            ok(404, "{}"),
        ]
    })
    .await?;
    let gitlab = client(&srv, Duration::from_secs(5))?;
    let token = token();
    let api = GitLabDestination::new(&gitlab, &token)?;
    assert_eq!(
        ensure_project(&api, &intent()).await,
        Err(ProviderError::UnexpectedResponse)
    );
    Ok(())
}
