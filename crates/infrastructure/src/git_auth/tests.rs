// TEST-CREDENTIAL-BROKER and the git_auth part of TEST-REDACTION. Uses a test-only
// sentinel token; real Git is exercised through `git credential fill` with no network.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use gbm_domain::contract::secret::SecretString;

use super::*;
use crate::platform::PrivateRuntimeDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SENTINEL: &str = "glpat-BROKERSENTINEL0000";
const REPO_PATH: &str = "vandunxg/file_processing.git";

fn runtime_dir(name: &str) -> Result<(PathBuf, PrivateRuntimeDir), Box<dyn std::error::Error>> {
    let base = std::env::temp_dir().join(format!("gbm-broker-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base)?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700))?;
    let dir = PrivateRuntimeDir::under(&base)?;
    Ok((base, dir))
}

fn grant(job: &str, ttl: Duration) -> Result<JobGrant, RemoteError> {
    Ok(JobGrant {
        job_id: job.to_owned(),
        target: RemoteTarget::new(AllowedHost::GitLab, REPO_PATH)?,
        username: "vandunxg".to_owned(),
        secret: SecretString::new(SENTINEL.to_owned()),
        ttl,
    })
}

async fn ask(
    socket: &Path,
    operation: &'static str,
    request: &str,
) -> Result<Result<String, HelperError>, tokio::task::JoinError> {
    let socket = socket.to_owned();
    let request = request.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        relay(&socket, operation, request.as_bytes(), &mut out)
            .map(|()| String::from_utf8_lossy(&out).into_owned())
    })
    .await
}

fn request(protocol: &str, host: &str, path: &str) -> String {
    format!("protocol={protocol}\nhost={host}\npath={path}\n\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn bound_job_peer_receives_only_its_credential() -> TestResult {
    let (base, dir) = runtime_dir("allow")?;
    let broker = start_broker(&dir, grant("job-allow", Duration::from_secs(30))?).await?;
    let mode = fs::metadata(broker.socket_path())?.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket is private to the user");
    broker.bind_peer(std::process::id());

    let answer = ask(
        broker.socket_path(),
        "get",
        &request("https", "gitlab.com", REPO_PATH),
    )
    .await??;
    assert_eq!(
        answer,
        format!("username=vandunxg\npassword={SENTINEL}\n\n")
    );
    assert_eq!(broker.served(), 1);
    assert!(broker.denials().is_empty());
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_protocol_host_or_path_is_denied() -> TestResult {
    let (base, dir) = runtime_dir("deny")?;
    let broker = start_broker(&dir, grant("job-deny", Duration::from_secs(30))?).await?;
    broker.bind_peer(std::process::id());
    let cases = [
        (
            request("http", "gitlab.com", REPO_PATH),
            Denial::ProtocolNotHttps,
        ),
        (
            request("https", "github.com", REPO_PATH),
            Denial::HostMismatch,
        ),
        (
            request("https", "gitlab.com.evil.example", REPO_PATH),
            Denial::HostMismatch,
        ),
        (
            request("https", "gitlab.com", "vandunxg/other.git"),
            Denial::PathMismatch,
        ),
        (
            "protocol=https\nhost=gitlab.com\n\n".to_owned(),
            Denial::PathMismatch,
        ),
        (
            "protocol=https\nhost=gitlab.com\npath=a\rb\n\n".to_owned(),
            Denial::Malformed,
        ),
    ];
    for (input, _) in &cases {
        let answer = ask(broker.socket_path(), "get", input).await?;
        // Malformed input is rejected by the helper itself before reaching the broker.
        let answer = answer.unwrap_or_default();
        assert!(!answer.contains(SENTINEL), "no credential for {input:?}");
        assert!(answer.is_empty());
    }
    let expected: Vec<Denial> = cases
        .iter()
        .map(|(_, d)| *d)
        .filter(|d| *d != Denial::Malformed)
        .collect();
    assert_eq!(broker.denials(), expected);
    assert_eq!(broker.served(), 0);
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn unbound_or_foreign_peer_is_denied() -> TestResult {
    let (base, dir) = runtime_dir("peer")?;
    let broker = start_broker(&dir, grant("job-peer", Duration::from_secs(30))?).await?;
    let ok_request = request("https", "gitlab.com", REPO_PATH);

    let answer = ask(broker.socket_path(), "get", &ok_request).await?;
    assert!(
        matches!(
            answer.as_deref(),
            Ok("") | Err(HelperError::BrokerUnavailable)
        ),
        "no peer bound yet: {answer:?}"
    );

    // Bind to an unrelated process: the test process is not in its tree.
    let mut other = Command::new("sleep").arg("10").spawn()?;
    broker.bind_peer(other.id());
    let answer = ask(broker.socket_path(), "get", &ok_request).await?;
    assert!(
        matches!(
            answer.as_deref(),
            Ok("") | Err(HelperError::BrokerUnavailable)
        ),
        "foreign tree: {answer:?}"
    );
    other.kill()?;
    other.wait()?;

    assert_eq!(
        broker.denials(),
        vec![Denial::PeerNotBound, Denial::PeerNotInJobProcessTree]
    );
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn store_and_erase_are_never_forwarded() -> TestResult {
    let (base, dir) = runtime_dir("store")?;
    let broker = start_broker(&dir, grant("job-store", Duration::from_secs(30))?).await?;
    broker.bind_peer(std::process::id());
    let input = format!(
        "protocol=https\nhost=gitlab.com\npath={REPO_PATH}\nusername=x\npassword={SENTINEL}\n\n"
    );
    for operation in ["store", "erase"] {
        assert_eq!(ask(broker.socket_path(), operation, &input).await??, "");
    }
    assert_eq!(
        ask(broker.socket_path(), "list", &input).await?,
        Err(HelperError::UnknownOperation)
    );
    assert_eq!(broker.served(), 0);
    assert!(
        broker.denials().is_empty(),
        "the broker was never contacted"
    );
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn broker_expires_and_removes_its_socket() -> TestResult {
    let (base, dir) = runtime_dir("expire")?;
    let broker = start_broker(&dir, grant("job-expire", Duration::from_millis(200))?).await?;
    broker.bind_peer(std::process::id());
    let socket = broker.socket_path().to_owned();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(broker.is_finished());
    assert!(!socket.exists(), "socket removed on expiry");
    assert_eq!(
        ask(&socket, "get", &request("https", "gitlab.com", REPO_PATH)).await?,
        Err(HelperError::BrokerUnavailable)
    );
    drop(broker);
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn drop_removes_socket_and_duplicate_job_is_refused() -> TestResult {
    let (base, dir) = runtime_dir("drop")?;
    let broker = start_broker(&dir, grant("job-dup", Duration::from_secs(30))?).await?;
    let socket = broker.socket_path().to_owned();
    assert_eq!(
        start_broker(&dir, grant("job-dup", Duration::from_secs(30))?)
            .await
            .err(),
        Some(BrokerError::AlreadyRunning)
    );
    drop(broker);
    assert!(!socket.exists());
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_grants_are_rejected() -> TestResult {
    let (base, dir) = runtime_dir("invalid")?;
    let target = RemoteTarget::new(AllowedHost::GitHub, "owner/repo")?;
    let make = |job: &str, user: &str, secret: &str, ttl: Duration| JobGrant {
        job_id: job.to_owned(),
        target: target.clone(),
        username: user.to_owned(),
        secret: SecretString::new(secret.to_owned()),
        ttl,
    };
    let cases = [
        make("job-1", "u", "tok\npassword=x", Duration::from_secs(5)),
        make("job-1", "u\nx", "tok", Duration::from_secs(5)),
        make("job-1", "u", "", Duration::from_secs(5)),
        make("../job", "u", "tok", Duration::from_secs(5)),
        make("job-1", "u", "tok", Duration::ZERO),
        make("job-1", "u", "tok", Duration::from_secs(7200)),
    ];
    for case in cases {
        assert_eq!(
            start_broker(&dir, case).await.err(),
            Some(BrokerError::InvalidGrant)
        );
    }
    fs::remove_dir_all(base)?;
    Ok(())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
}

/// Test-only helper speaking the same relay protocol as `relay` (the production helper
/// binary lands with the Git runner in P05-T01).
const PY_HELPER: &str = r#"
import socket, sys
op = sys.argv[-1]
sock_path = sys.argv[sys.argv.index("--socket") + 1]
data = sys.stdin.read()
if op != "get":
    sys.exit(0)
lines = [l for l in data.split("\n") if l]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sock_path)
s.sendall(("\n".join(lines + ["operation=get"]) + "\n\n").encode())
s.shutdown(socket.SHUT_WR)
resp = b""
while True:
    chunk = s.recv(4096)
    if not chunk:
        break
    resp += chunk
sys.stdout.write(resp.decode() if resp.strip() else "")
"#;

struct GitRun {
    stdout: String,
    stderr: String,
    success: bool,
    proc_clean: bool,
    argv: Vec<String>,
    env: Vec<(OsString, OsString)>,
}

fn run_git_fill(
    broker: &BrokerHandle,
    home: &Path,
    global_config: &Path,
    path: &str,
) -> Result<GitRun, Box<dyn std::error::Error>> {
    let python = find_on_path("python3").ok_or("python3 is required for this test")?;
    let script = home.join("helper.py");
    fs::write(&script, PY_HELPER)?;
    let helper = HelperCommand {
        program: python,
        args: vec![script.to_string_lossy().into_owned()],
        socket: broker.socket_path().to_owned(),
    };
    let mut argv = git_auth_args(&helper)?;
    argv.extend(["credential".to_owned(), "fill".to_owned()]);
    let mut env = git_child_env(&std::env::var_os("PATH").unwrap_or_default(), home);
    // Hostile inherited configuration: a global helper that would leak a fake password.
    env.retain(|(k, _)| k != "GIT_CONFIG_GLOBAL");
    env.push((
        "GIT_CONFIG_GLOBAL".into(),
        global_config.as_os_str().to_owned(),
    ));

    let mut child = Command::new("git")
        .args(&argv)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    broker.bind_peer(child.id());
    // Inspect the live child: neither argv nor environment may hold the secret.
    let proc_clean = ["cmdline", "environ"].iter().all(|file| {
        fs::read(format!("/proc/{}/{file}", child.id()))
            .is_ok_and(|bytes| !String::from_utf8_lossy(&bytes).contains(SENTINEL))
    });
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(format!("protocol=https\nhost=gitlab.com\npath={path}\n\n").as_bytes())?;
    let output = child.wait_with_output()?;
    Ok(GitRun {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
        proc_clean,
        argv,
        env,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn real_git_gets_credential_only_through_the_bound_broker() -> TestResult {
    let (base, dir) = runtime_dir("git")?;
    let home = base.join("home");
    fs::create_dir(&home)?;
    let hostile = home.join("hostile.gitconfig");
    fs::write(
        &hostile,
        "[credential]\n\thelper = \"!f() { echo username=leak; echo password=LEAKED-GLOBAL-HELPER; }; f\"\n",
    )?;

    let broker = start_broker(&dir, grant("job-git", Duration::from_secs(30))?).await?;
    let broker_ref = &broker;
    let (home_c, hostile_c) = (home.clone(), hostile.clone());
    let allowed =
        tokio::task::block_in_place(|| run_git_fill(broker_ref, &home_c, &hostile_c, REPO_PATH))?;
    assert!(
        allowed.success,
        "git credential fill failed: {}",
        allowed.stderr
    );
    assert!(
        allowed.stdout.contains(&format!("password={SENTINEL}")),
        "{}",
        allowed.stdout
    );
    assert!(
        !allowed.stdout.contains("LEAKED-GLOBAL-HELPER"),
        "inherited helper list was reset"
    );

    // Credential-free argv/env (REQ-45).
    assert!(allowed.argv.iter().all(|a| !a.contains(SENTINEL)));
    assert!(
        allowed
            .env
            .iter()
            .all(|(k, v)| !k.to_string_lossy().contains(SENTINEL)
                && !v.to_string_lossy().contains(SENTINEL))
    );
    assert!(allowed.argv.contains(&"credential.helper=".to_owned()));
    assert!(
        allowed.proc_clean,
        "/proc/<git>/cmdline and environ hold no secret"
    );

    let denied = tokio::task::block_in_place(|| {
        run_git_fill(broker_ref, &home, &hostile, "vandunxg/other.git")
    })?;
    assert!(!denied.success, "no prompt fallback when the broker denies");
    assert!(!denied.stdout.contains(SENTINEL) && !denied.stderr.contains(SENTINEL));
    assert!(!denied.stdout.contains("LEAKED-GLOBAL-HELPER"));
    assert_eq!(broker.denials(), vec![Denial::PathMismatch]);

    // No file under the runtime dir or HOME holds the secret (no temp-file transport).
    for root in [dir.path(), home.as_path()] {
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_file() {
                assert!(
                    !fs::read_to_string(&path)
                        .unwrap_or_default()
                        .contains(SENTINEL),
                    "{}",
                    path.display()
                );
            }
        }
    }
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn remote_urls_are_allowlisted_and_credential_free() -> Result<(), RemoteError> {
    let ok = RemoteTarget::from_url("https://gitlab.com/group/sub-group/repo.name.git")?;
    assert_eq!(
        (ok.host(), ok.path()),
        (AllowedHost::GitLab, "group/sub-group/repo.name.git")
    );
    assert_eq!(ok.url(), "https://gitlab.com/group/sub-group/repo.name.git");
    let bad = [
        ("http://github.com/o/r.git", RemoteError::NotHttps),
        ("ext::sh -c touch% /tmp/x", RemoteError::NotHttps),
        ("git@github.com:o/r.git", RemoteError::NotHttps),
        (
            "https://user:glpat-x@gitlab.com/o/r.git",
            RemoteError::HasUserinfo,
        ),
        ("https://token@github.com/o/r.git", RemoteError::HasUserinfo),
        ("https://evil.example/o/r.git", RemoteError::HostNotAllowed),
        (
            "https://github.com.evil.example/o/r.git",
            RemoteError::HostNotAllowed,
        ),
        (
            "https://github.com:8443/o/r.git",
            RemoteError::HostNotAllowed,
        ),
        ("https://github.com/o/../r.git", RemoteError::UnsafePath),
        ("https://github.com/o/r.git?x=1", RemoteError::UnsafePath),
        ("https://github.com/o/%2e%2e", RemoteError::UnsafePath),
        ("https://github.com/-o/r", RemoteError::UnsafePath),
        ("https://github.com/r", RemoteError::UnsafePath),
        ("https://github.com/o//r", RemoteError::UnsafePath),
    ];
    for (url, expected) in bad {
        assert_eq!(RemoteTarget::from_url(url), Err(expected), "{url}");
    }
    Ok(())
}

#[test]
fn helper_command_is_quoted_and_absolute() -> Result<(), UnsafeHelperPath> {
    let helper = HelperCommand {
        program: PathBuf::from("/opt/git-backup-manager/gbm"),
        args: vec!["git-credential".into()],
        socket: PathBuf::from("/run/user/1000/git-backup-manager/job-1.sock"),
    };
    let args = git_auth_args(&helper)?;
    assert_eq!(
        args[3],
        "credential.helper=/opt/git-backup-manager/gbm 'git-credential' '--socket' '/run/user/1000/git-backup-manager/job-1.sock'"
    );
    for program in [
        "/tmp/it's",
        "/opt/Git Backup/gbm",
        "/opt/$(id)/gbm",
        "/opt/a;b",
    ] {
        let unsafe_program = HelperCommand {
            program: PathBuf::from(program),
            ..helper.clone()
        };
        assert_eq!(
            git_auth_args(&unsafe_program),
            Err(UnsafeHelperPath),
            "{program}"
        );
    }
    let quote_in_arg = HelperCommand {
        args: vec!["it's".into()],
        ..helper.clone()
    };
    assert_eq!(git_auth_args(&quote_in_arg), Err(UnsafeHelperPath));
    let relative = HelperCommand {
        program: PathBuf::from("gbm"),
        ..helper
    };
    assert_eq!(git_auth_args(&relative), Err(UnsafeHelperPath));
    Ok(())
}

#[test]
fn child_env_is_a_closed_allowlist() {
    let env = git_child_env("/usr/bin".as_ref(), Path::new("/home/app"));
    let keys: Vec<_> = env
        .iter()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        keys,
        [
            "PATH",
            "HOME",
            "LC_ALL",
            "GIT_TERMINAL_PROMPT",
            "GIT_CONFIG_NOSYSTEM",
            "GIT_CONFIG_GLOBAL"
        ]
    );
    assert!(
        !keys
            .iter()
            .any(|k| k.contains("ASKPASS") || k.contains("TOKEN"))
    );
}

#[test]
fn protocol_parser_is_bounded_and_rejects_injection() {
    assert_eq!(
        Attributes::parse(&vec![b'a'; MAX_MESSAGE_BYTES + 1]),
        Err(ProtocolError::TooLarge)
    );
    assert_eq!(
        Attributes::parse(b"host=a\rpassword=x\n"),
        Err(ProtocolError::Malformed)
    );
    assert_eq!(
        Attributes::parse(b"novalue\n"),
        Err(ProtocolError::Malformed)
    );
    assert_eq!(
        Attributes::parse(&[0xff, b'\n']),
        Err(ProtocolError::NotUtf8)
    );
    let parsed = Attributes::parse(b"host=a\nwwwauth[]=Basic\nhost=b\n\nignored=1\n");
    assert_eq!(
        parsed.map(|a| (
            a.get("host").map(str::to_owned),
            a.get("ignored").is_none(),
            a.get("wwwauth[]").is_none()
        )),
        Ok((Some("b".to_owned()), true, true))
    );
    let debug = format!(
        "{:?}",
        Attributes::parse(format!("password={SENTINEL}\n").as_bytes())
    );
    assert!(!debug.contains(SENTINEL), "Debug never prints values");
}

#[test]
fn redaction_removes_known_and_token_shaped_secrets() {
    let known = SecretString::new("plain-secret-value".to_owned());
    let text = format!(
        "push to https://oauth2:{SENTINEL}@gitlab.com/o/r.git failed; token={SENTINEL}; env plain-secret-value; \
         gh github_pat_11ABCDEFG0123456789_abcdef; short ghp_x ok; word-glpat-notatoken stays"
    );
    let out = redact(&text, &[&known]);
    assert!(!out.contains(SENTINEL));
    assert!(!out.contains("plain-secret-value"));
    assert!(!out.contains("github_pat_11ABCDEFG"));
    assert!(!out.contains("oauth2"), "userinfo removed: {out}");
    assert!(out.contains("https://[REDACTED]@gitlab.com/o/r.git"));
    assert!(
        out.contains("short ghp_x ok"),
        "short non-token text untouched: {out}"
    );
    assert!(
        out.contains("word-glpat-notatoken"),
        "no boundary, not a token: {out}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn silent_client_does_not_block_the_job_helper() -> TestResult {
    let (base, dir) = runtime_dir("silent")?;
    let broker = start_broker(&dir, grant("job-silent", Duration::from_secs(30))?).await?;
    broker.bind_peer(std::process::id());
    // An in-tree client that connects and never writes.
    let _silent = std::os::unix::net::UnixStream::connect(broker.socket_path())?;
    let started = std::time::Instant::now();
    let answer = ask(
        broker.socket_path(),
        "get",
        &request("https", "gitlab.com", REPO_PATH),
    )
    .await??;
    assert!(answer.contains(SENTINEL));
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "served while the silent client is pending"
    );
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn exited_job_process_is_no_longer_trusted() -> TestResult {
    let (base, dir) = runtime_dir("exited")?;
    let broker = start_broker(&dir, grant("job-exited", Duration::from_secs(30))?).await?;
    let mut child = Command::new("sleep").arg("10").spawn()?;
    broker.bind_peer(child.id());
    child.kill()?;
    child.wait()?;
    let answer = ask(
        broker.socket_path(),
        "get",
        &request("https", "gitlab.com", REPO_PATH),
    )
    .await?;
    assert!(!answer.unwrap_or_default().contains(SENTINEL));
    assert_eq!(broker.denials(), vec![Denial::JobProcessExited]);
    broker.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn other_jobs_broker_never_serves_this_jobs_request() -> TestResult {
    let (base, dir) = runtime_dir("cross")?;
    let job_a = start_broker(&dir, grant("job-a", Duration::from_secs(30))?).await?;
    let other = JobGrant {
        target: RemoteTarget::new(AllowedHost::GitLab, "vandunxg/other.git")?,
        ..grant("job-b", Duration::from_secs(30))?
    };
    let job_b = start_broker(&dir, other).await?;
    job_a.bind_peer(std::process::id());
    job_b.bind_peer(std::process::id());
    // Job A's repository asked on job B's socket.
    let answer = ask(
        job_b.socket_path(),
        "get",
        &request("https", "gitlab.com", REPO_PATH),
    )
    .await??;
    assert!(answer.is_empty());
    assert_eq!(job_b.denials(), vec![Denial::PathMismatch]);
    job_a.shutdown().await;
    job_b.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn old_handle_never_removes_a_newer_brokers_socket() -> TestResult {
    let (base, dir) = runtime_dir("reuse")?;
    let first = start_broker(&dir, grant("job-reuse", Duration::from_millis(150))?).await?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(first.is_finished());
    let second = start_broker(&dir, grant("job-reuse", Duration::from_secs(30))?).await?;
    drop(first);
    assert!(
        second.socket_path().exists(),
        "the newer socket survives the old handle"
    );
    second.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn stale_sockets_are_swept_and_live_ones_kept() -> TestResult {
    let (base, dir) = runtime_dir("sweep")?;
    let stale = dir.path().join("job-crashed.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale)?);
    assert!(stale.exists());
    let live = start_broker(&dir, grant("job-live", Duration::from_secs(30))?).await?;
    let removed = tokio::task::block_in_place(|| sweep_stale_sockets(&dir));
    assert_eq!(removed, 1);
    assert!(!stale.exists());
    assert!(live.socket_path().exists());
    live.shutdown().await;
    fs::remove_dir_all(base)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn overlong_socket_path_is_a_clear_error() -> TestResult {
    let base = std::env::temp_dir().join(format!(
        "gbm-broker-{}-{}",
        "x".repeat(70),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base)?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700))?;
    let dir = PrivateRuntimeDir::under(&base)?;
    let long_job = format!("job-{}", "y".repeat(40));
    assert_eq!(
        start_broker(&dir, grant(&long_job, Duration::from_secs(5))?)
            .await
            .err(),
        Some(BrokerError::SocketPathTooLong)
    );
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn hardening_options_override_hostile_repository_config() -> TestResult {
    let base = std::env::temp_dir().join(format!("gbm-harden-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base)?;
    let repo = base.join("repo.git");
    let status = Command::new("git")
        .args(["init", "--bare", "-q"])
        .arg(&repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()?;
    assert!(status.success());
    fs::write(
        repo.join("config"),
        "[core]\n\tbare = true\n\thooksPath = /tmp/evil-hooks\n\tfsmonitor = /tmp/evil-fsmonitor\n\taskPass = /tmp/evil-askpass\n[protocol]\n\tallow = always\n[http]\n\tsslVerify = false\n\tfollowRedirects = true\n[credential]\n\thelper = /tmp/evil-helper\n",
    )?;
    let helper = HelperCommand {
        program: PathBuf::from("/usr/bin/gbm"),
        args: vec![],
        socket: PathBuf::from("/run/user/1000/git-backup-manager/job-1.sock"),
    };
    let args = git_auth_args(&helper)?;
    let get = |key: &str, all: bool| -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(&args)
            .args(["config", if all { "--get-all" } else { "--get" }, key])
            .env_clear()
            .envs(git_child_env(
                &std::env::var_os("PATH").unwrap_or_default(),
                &base,
            ))
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    assert_eq!(get("core.hooksPath", false)?, "/dev/null");
    assert_eq!(get("core.fsmonitor", false)?, "false");
    assert_eq!(get("core.askPass", false)?, "");
    assert_eq!(get("protocol.allow", false)?, "never");
    assert_eq!(get("protocol.https.allow", false)?, "always");
    assert_eq!(get("http.sslVerify", false)?, "true");
    assert_eq!(get("http.followRedirects", false)?, "false");
    // The repository helper still appears in config, but the empty `credential.helper=`
    // entry that follows it resets Git's helper list (proven by the real-git test).
    let helpers = get("credential.helper", true)?;
    let lines: Vec<&str> = helpers.lines().collect();
    assert_eq!(
        lines.last().copied(),
        Some("/usr/bin/gbm '--socket' '/run/user/1000/git-backup-manager/job-1.sock'")
    );
    assert!(
        lines.contains(&""),
        "reset entry present after repository helpers: {lines:?}"
    );
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn redaction_covers_headers_encodings_and_more_prefixes() {
    let known = SecretString::new("p@ss/w0rd secret".to_owned());
    let text = "Authorization: Basic dXNlcjpnbHBhdC14eHg=\n\
                proxy-authorization: Bearer abc.def\n\
                runner glrt-ABCDEFGH12345678 deploy gldt-ZYXWVUTS98765432\n\
                url https://u:pa/ss@gitlab.com/o/r.git\n\
                encoded p%40ss%2Fw0rd%20secret and raw p@ss/w0rd secret";
    let out = redact(text, &[&known]);
    for leaked in [
        "dXNlcjpnbHBhdC14eHg",
        "abc.def",
        "glrt-ABCDEFGH",
        "gldt-ZYXW",
        "pa/ss",
        "p%40ss",
        "p@ss/w0rd",
    ] {
        assert!(!out.contains(leaked), "{leaked} leaked in {out}");
    }
    assert!(out.contains("Authorization: [REDACTED]"));
    assert!(out.contains("https://[REDACTED]@gitlab.com/o/r.git"));
}

#[test]
fn redaction_is_linear_on_hostile_input() {
    let hostile = "ghp_x ".repeat(200_000);
    let started = std::time::Instant::now();
    let out = redact(&hostile, &[]);
    assert_eq!(out.len(), hostile.len(), "short words are not tokens");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "took {:?}",
        started.elapsed()
    );
}
