//! TEST-GIT-INPUT, TEST-PROCESS-CANCEL and TEST-CREDENTIAL-BROKER (argv/env half)
//! against real Git on local fixtures and small `/bin/sh` stand-ins for a hostile or
//! stuck `git`. No network, no personal repositories.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use gbm_domain::refs::parse_ls_remote;
use gbm_test_fixtures::{Sandbox, empty_bare, source_repo};

use super::*;
use crate::git_auth::{AllowedHost, HelperCommand};
use crate::platform::ProcessId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SENTINEL: &str = "glpat-RUNNERSENTINEL-0000";

/// A private 0700 scratch directory (runner HOME, fake binaries, markers).
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("gbm-run-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        Ok(Self(fs::canonicalize(dir)?))
    }

    fn home(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = self.0.join("home");
        fs::create_dir_all(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        Ok(home)
    }

    /// Writes an executable `/bin/sh` script standing in for `git`.
    fn script(&self, name: &str, body: &str) -> Result<GitBinary, Box<dyn std::error::Error>> {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        // `/tmp` is sticky world-writable, which `GitBinary::at` rightly refuses.
        Ok(GitBinary::trusted_for_tests(&path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn limits(timeout_ms: u64, grace_ms: u64) -> RunLimits {
    RunLimits {
        stdout_bytes: 64 * 1024,
        stderr_bytes: 16 * 1024,
        timeout: Duration::from_millis(timeout_ms),
        grace: Duration::from_millis(grace_ms),
    }
}

/// Retries a spawn that failed with ETXTBSY (a test thread forking while another test
/// still had a freshly written script open); any other error is returned.
fn run(
    runner: &GitRunner,
    invocation: &GitInvocation,
    work: &WorkDir,
    limits: RunLimits,
    cancel: &CancelToken,
    known: &[&SecretString],
) -> Result<GitOutput, RunError> {
    let mut last = Err(RunError::Spawn);
    for _ in 0..20 {
        last = runner.run(invocation, work, limits, cancel, known);
        if last != Err(RunError::Spawn) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    last
}

fn read_pids(path: &Path) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(path)?
        .split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect())
}

fn wait_dead(pids: &[u32]) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if pids.iter().all(|&pid| !platform::is_running(pid)) {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

// ---------------------------------------------------------------- binary and input

#[test]
fn only_trusted_git_binaries_are_accepted() -> TestResult {
    let scratch = Scratch::new("bin")?;
    assert!(GitBinary::at(Path::new("git")).is_err(), "relative path");
    let writable = scratch.0.join("writable");
    fs::write(&writable, "#!/bin/sh\n")?;
    fs::set_permissions(&writable, fs::Permissions::from_mode(0o777))?;
    assert_eq!(GitBinary::at(&writable), Err(RunError::UntrustedBinary));
    let plain = scratch.0.join("plain");
    fs::write(&plain, "#!/bin/sh\n")?;
    fs::set_permissions(&plain, fs::Permissions::from_mode(0o600))?;
    assert_eq!(
        GitBinary::at(&plain),
        Err(RunError::UntrustedBinary),
        "not executable"
    );
    let open_dir = scratch.0.join("open");
    fs::create_dir(&open_dir)?;
    fs::set_permissions(&open_dir, fs::Permissions::from_mode(0o777))?;
    let inside = open_dir.join("git");
    fs::write(&inside, "#!/bin/sh\n")?;
    fs::set_permissions(&inside, fs::Permissions::from_mode(0o700))?;
    assert_eq!(
        GitBinary::at(&inside),
        Err(RunError::UntrustedBinary),
        "writable dir"
    );
    // A private 0700 dir under the world-writable /tmp is still refused (ancestor).
    let private = scratch.0.join("private-git");
    fs::write(&private, "#!/bin/sh\n")?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    assert_eq!(
        GitBinary::at(&private),
        Err(RunError::UntrustedBinary),
        "ancestor /tmp"
    );
    assert!(GitBinary::locate().is_ok(), "system git is trusted");
    Ok(())
}

#[test]
fn only_exact_allowlisted_options_and_typed_values_are_accepted() -> TestResult {
    // Review H1: Git accepts unambiguous abbreviations (`--upload-pa`, `--exe`,
    // `--temp`), so anything not an exact allowlisted word is rejected.
    for (sub, flag) in [
        (Subcommand::Fetch, "--upload-pa"),
        (Subcommand::Fetch, "--upload-pack"),
        (Subcommand::LsRemote, "--exe"),
        (Subcommand::LsRemote, "-u"),
        (Subcommand::Init, "--temp"),
        (Subcommand::Init, "--separate"),
        (Subcommand::Fetch, "--recurse"),
        (Subcommand::Fetch, "-o"),
        (Subcommand::Fetch, "--refmap"),
        (Subcommand::Fetch, "--server-option=x"),
        (Subcommand::Fetch, "-c"),
        (Subcommand::Fetch, "--symref"),
        (Subcommand::Version, "--build-options"),
    ] {
        assert_eq!(
            GitInvocation::new(sub, vec![Arg::Flag(flag)]),
            Err(RunError::InvalidArgument),
            "{sub:?} {flag}"
        );
    }
    // No option after `--`, and only one `--`.
    assert!(
        GitInvocation::new(
            Subcommand::Fetch,
            vec![Arg::EndOfOptions, Arg::Flag("--prune")]
        )
        .is_err()
    );
    assert!(
        GitInvocation::new(
            Subcommand::Fetch,
            vec![Arg::EndOfOptions, Arg::EndOfOptions]
        )
        .is_err()
    );
    assert!(
        GitInvocation::new(
            Subcommand::Fetch,
            vec![
                Arg::Flag("--no-tags"),
                Arg::Flag("--prune"),
                Arg::EndOfOptions
            ]
        )
        .is_ok()
    );

    // Refspecs: explicit full names only.
    for (src, dst) in [
        ("refs/heads/$(touch /tmp/pwned)", "refs/gbm/x"),
        ("refs/heads/*", "refs/gbm/staging/1/heads/*"),
        ("HEAD", "refs/gbm/x"),
        ("-refs/heads/a", "refs/gbm/x"),
        ("refs/heads/a", "refs/gbm/../x"),
        ("refs/heads/a\nb", "refs/gbm/x"),
        ("refs/heads/a:b", "refs/gbm/x"),
        ("refs/heads/a", "refs/gbm/x.lock"),
    ] {
        assert!(RefSpec::new(src, dst, true).is_err(), "{src} -> {dst}");
    }
    assert!(
        RefSpec::new(
            "refs/heads/odd/naïve-ü",
            "refs/gbm/staging/j1/heads/odd/naïve-ü",
            true
        )
        .is_ok()
    );
    assert!(RefPrefix::new("refs/gbm/staging/j1/").is_ok());
    assert!(RefPrefix::new("refs/gbm/staging/j1").is_err());

    // Review M1: URLs only through RemoteTarget (allowlisted host, no userinfo).
    for bad in [
        "https://tok@github.com/a/b",
        "https://user:pw@github.com/a/b",
        "https://evil.example/a/b",
        "ext::sh -c x",
        "file:///etc",
        "git@github.com:a/b",
    ] {
        assert!(RemoteTarget::from_url(bad).is_err(), "{bad}");
    }
    Ok(())
}

#[test]
fn hardening_comes_first_and_local_transport_is_opt_in() -> TestResult {
    let remote = Location::Remote(RemoteTarget::new(AllowedHost::GitHub, "octo/hello")?);
    let argv = GitInvocation::ls_remote(&remote)?.argv();
    let sub = argv
        .iter()
        .position(|a| a == "ls-remote")
        .ok_or("no subcommand")?;
    assert_eq!(
        &argv[sub..],
        [
            "ls-remote",
            "--symref",
            "--",
            "https://github.com/octo/hello"
        ]
    );
    for option in [
        "protocol.allow=never",
        "core.hooksPath=/dev/null",
        "credential.helper=",
    ] {
        assert!(argv[..sub].iter().any(|a| a == option), "{option}");
    }
    assert!(!argv.iter().any(|a| a == "protocol.file.allow=always"));
    let scratch = Scratch::new("loc")?;
    let local = GitInvocation::ls_remote(&Location::local(&scratch.0)?)?.argv();
    assert!(local.iter().any(|a| a == "protocol.file.allow=always"));
    // Local locations must be canonical absolute paths.
    assert!(Location::local(Path::new("relative")).is_err());
    assert!(
        Location::local(
            &scratch
                .0
                .join("..")
                .join(scratch.0.file_name().ok_or("name")?)
        )
        .is_err()
    );
    let link = scratch.0.join("link");
    std::os::unix::fs::symlink(&scratch.0, &link)?;
    assert!(Location::local(&link).is_err());
    Ok(())
}

// ---------------------------------------------------------------- real git

#[test]
fn real_ls_remote_through_the_runner_parses_completely() -> TestResult {
    let sandbox = Sandbox::new("runner-ls")?;
    let source = source_repo(&sandbox)?;
    let scratch = Scratch::new("ls")?;
    let runner = GitRunner::new(GitBinary::locate()?, &scratch.home()?)?;
    let invocation = GitInvocation::ls_remote(&Location::local(&fs::canonicalize(&source.path)?)?)?;
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        RunLimits::default(),
        &CancelToken::default(),
        &[],
    )?;
    let ad = parse_ls_remote(out.success_stdout()?)?;
    assert_eq!(ad.refs.len(), 7);
    Ok(())
}

#[test]
fn hostile_global_and_repository_config_cannot_execute_code() -> TestResult {
    // TEST-GIT-INPUT: a global config in the runner HOME and a repository config in
    // the target GIT_DIR both try to run `touch <marker>` via url.insteadOf -> ext::,
    // hooks (reference-transaction, post-fetch-ish) and core.fsmonitor.
    let sandbox = Sandbox::new("runner-hostile")?;
    let source = source_repo(&sandbox)?;
    let source_path = fs::canonicalize(&source.path)?;
    let scratch = Scratch::new("hostile")?;
    let home = scratch.home()?;
    let marker = scratch.0.join("PWNED");
    let touch = format!("ext::sh -c touch% {}", marker.display());
    let evil = format!(
        "[url \"{touch}\"]\n\tinsteadOf = {src}\n[protocol]\n\tallow = always\n[core]\n\thooksPath = {hooks}\n\tfsmonitor = {hooks}/fsmon\n[credential]\n\thelper = !touch {m}\n",
        src = source_path.display(),
        hooks = scratch.0.join("hooks").display(),
        m = marker.display(),
    );
    fs::write(home.join(".gitconfig"), &evil)?;
    let hooks = scratch.0.join("hooks");
    fs::create_dir(&hooks)?;
    for hook in [
        "reference-transaction",
        "post-checkout",
        "pre-auto-gc",
        "fsmon",
    ] {
        let path = hooks.join(hook);
        fs::write(&path, format!("#!/bin/sh\ntouch {}\n", marker.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    let target = empty_bare(&sandbox, "target.git")?;
    let target = fs::canonicalize(target)?;
    fs::write(
        target.join("config"),
        format!("[core]\n\tbare = true\n{evil}"),
    )?;
    fs::copy(
        hooks.join("reference-transaction"),
        target.join("hooks/reference-transaction"),
    )?;
    fs::set_permissions(
        target.join("hooks/reference-transaction"),
        fs::Permissions::from_mode(0o700),
    )?;

    let runner = GitRunner::new(GitBinary::locate()?, &home)?;
    let work = WorkDir::GitDir(target.clone());
    let source_location = Location::local(&source_path)?;
    let fetch_args = |refspec: RefSpec| {
        vec![
            Arg::Flag("--no-tags"),
            Arg::EndOfOptions,
            Arg::Location(source_location.clone()),
            Arg::RefSpec(refspec),
        ]
    };
    let fetch = |refspec: RefSpec| -> Result<GitOutput, Box<dyn std::error::Error>> {
        let invocation = GitInvocation::new(Subcommand::Fetch, fetch_args(refspec))?;
        Ok(run(
            &runner,
            &invocation,
            &work,
            limits(30_000, 2_000),
            &CancelToken::default(),
            &[],
        )?)
    };
    // Case A: a valid refspec, so Git reaches the transport; the repository config
    // rewrites the source to ext:: and sets protocol.allow=always. Refused.
    let staging = RefSpec::new("refs/heads/main", "refs/gbm/staging/t1/heads/main", true)?;
    let out = fetch(staging.clone())?;
    assert!(
        !marker.exists(),
        "ext:: did not run (stderr: {})",
        out.stderr
    );
    assert!(matches!(out.termination, Termination::Exited(code) if code != 0));
    assert!(
        out.stderr.contains("transport 'ext' not allowed"),
        "refused by protocol policy, not by accident: {}",
        out.stderr
    );
    // Case B: a shell-like ref cannot even be expressed (checked before spawn).
    assert!(RefSpec::new("refs/heads/$(touch PWNED)", "refs/gbm/x", true).is_err());

    // Without the hostile url rewrite the same fetch succeeds and still runs no hook.
    fs::write(target.join("config"), "[core]\n\tbare = true\n")?;
    let clean = GitInvocation::new(Subcommand::Fetch, fetch_args(staging))?;
    let out = run(
        &runner,
        &clean,
        &WorkDir::GitDir(target.clone()),
        limits(30_000, 2_000),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(out.termination, Termination::Exited(0), "{}", out.stderr);
    assert!(!marker.exists(), "reference-transaction hook did not run");
    Ok(())
}

// ---------------------------------------------------------------- process control

#[test]
fn cancel_stops_a_flooding_tree_with_helper_and_drains_boundedly() -> TestResult {
    // TEST-PROCESS-CANCEL.
    let scratch = Scratch::new("cancel")?;
    let pids = scratch.0.join("pids");
    let binary = scratch.script(
        "git",
        &format!(
            "echo $$ >> {p}\nsleep 600 & echo $! >> {p}\n(yes out) & echo $! >> {p}\n(yes err >&2) & echo $! >> {p}\nwait",
            p = pids.display()
        ),
    )?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let cancel = CancelToken::default();
    let trigger = cancel.clone();
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        trigger.cancel();
    });
    let started = Instant::now();
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(60_000, 1_000),
        &cancel,
        &[],
    )?;
    let _ = canceller.join();
    assert_eq!(out.termination, Termination::Cancelled);
    assert!(out.reaped, "the stopped child was reaped");
    assert!(out.stdout_truncated && out.stderr_truncated);
    assert!(
        out.output_complete,
        "pipes closed after the tree was stopped"
    );
    assert!(
        out.success_stdout().is_err(),
        "a cancelled run is never a result"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    let spawned = read_pids(&pids)?;
    assert_eq!(spawned.len(), 4);
    assert!(
        wait_dead(&spawned),
        "whole tree stopped and reaped: {spawned:?}"
    );
    Ok(())
}

#[test]
fn timeout_escalates_to_kill_when_term_is_ignored() -> TestResult {
    let scratch = Scratch::new("timeout")?;
    let pids = scratch.0.join("pids");
    let binary = scratch.script(
        "git",
        &format!(
            "trap '' TERM\necho $$ >> {p}\nsleep 600 & echo $! >> {p}\nwait",
            p = pids.display()
        ),
    )?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    let started = Instant::now();
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(300, 300),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(out.termination, Termination::TimedOut);
    assert!(out.reaped, "KILL escalation ended the TERM-ignoring child");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(wait_dead(&read_pids(&pids)?));
    Ok(())
}

#[test]
fn background_children_do_not_outlive_a_normal_exit() -> TestResult {
    let scratch = Scratch::new("leftover")?;
    let pids = scratch.0.join("pids");
    let binary = scratch.script(
        "git",
        &format!(
            "sleep 600 & echo $! >> {p}\necho done\nexit 0",
            p = pids.display()
        ),
    )?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    let started = Instant::now();
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(out.termination, Termination::Exited(0));
    assert_eq!(out.success_stdout()?, "done\n");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "did not wait for the sleeper"
    );
    assert!(wait_dead(&read_pids(&pids)?));
    Ok(())
}

#[test]
fn oversized_or_invalid_stdout_is_never_a_success() -> TestResult {
    let scratch = Scratch::new("bounds")?;
    let runner = GitRunner::new(
        scratch.script("git", "head -c 200000 /dev/zero")?,
        &scratch.home()?,
    )?;
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(out.termination, Termination::Exited(0));
    assert!(out.stdout_truncated);
    assert_eq!(out.success_stdout(), Err(RunError::OutputIncomplete));

    let bad = GitRunner::new(
        scratch.script("git2", "printf '\\377\\376'")?,
        &scratch.home()?,
    )?;
    let out = run(
        &bad,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(out.success_stdout(), Err(RunError::NotUtf8));

    let failing = GitRunner::new(
        scratch.script("git3", "echo partial; exit 128")?,
        &scratch.home()?,
    )?;
    let out = run(
        &failing,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(
        out.success_stdout(),
        Err(RunError::Failed(Termination::Exited(128)))
    );
    Ok(())
}

// ---------------------------------------------------------------- credentials

#[test]
fn child_argv_and_env_carry_no_secret_and_stderr_is_redacted() -> TestResult {
    // TEST-CREDENTIAL-BROKER (argv/env half; broker binding is covered in git_auth).
    let scratch = Scratch::new("env")?;
    let dump = scratch.0.join("dump");
    let binary = scratch.script(
        "git",
        &format!(
            "env > {d}\nprintf '%s\\n' \"$@\" >> {d}\necho \"fatal: auth for https://x:{SENTINEL}@github.com failed {SENTINEL}\" >&2\nexit 1",
            d = dump.display()
        ),
    )?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let helper = HelperCommand {
        program: PathBuf::from("/opt/gbm/gbm-desktop"),
        args: vec!["credential-helper".to_owned()],
        socket: scratch.0.join("broker.sock"),
    };
    let remote = Location::Remote(RemoteTarget::new(AllowedHost::GitHub, "octo/private")?);
    let invocation = GitInvocation::ls_remote(&remote)?.with_auth(AuthArgs::for_helper(&helper)?);
    let secret = SecretString::new(SENTINEL.to_owned());
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[&secret],
    )?;
    let dumped = fs::read_to_string(&dump)?;
    assert!(!dumped.contains(SENTINEL), "no token in env/argv");
    assert!(dumped.contains("credential.helper=/opt/gbm/gbm-desktop"));
    let allowed = [
        "PATH",
        "HOME",
        "LC_ALL",
        "GIT_TERMINAL_PROMPT",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_CEILING_DIRECTORIES",
        "PWD",
        "SHLVL",
        "_",
        "OLDPWD",
    ];
    for line in dumped.lines().take_while(|l| !l.starts_with("-c")) {
        if let Some((key, _)) = line.split_once('=') {
            assert!(allowed.contains(&key), "unexpected env {key}");
        }
    }
    assert!(
        !out.stderr.contains(SENTINEL),
        "stderr redacted: {}",
        out.stderr
    );
    assert!(out.stderr.contains("[REDACTED]"));
    Ok(())
}

#[test]
fn process_ids_bind_start_time() -> TestResult {
    let me = ProcessId::of(std::process::id()).ok_or("self")?;
    assert!(me.is_alive());
    let stale = ProcessId {
        pid: me.pid,
        start_time: me.start_time.wrapping_add(1),
    };
    assert!(
        !stale.is_alive(),
        "a different start time is a different process"
    );
    Ok(())
}

#[test]
fn a_setsid_helper_seen_while_running_is_killed_after_exit() -> TestResult {
    // Review M3: a helper in its own session is outside the process group; the periodic
    // descendant scan records it, so it does not survive a normal exit.
    let scratch = Scratch::new("setsid")?;
    let pids = scratch.0.join("pids");
    let binary = scratch.script(
        "git",
        &format!(
            "setsid sleep 600 </dev/null >/dev/null 2>&1 & echo $! >> {p}\nsleep 1\necho ok",
            p = pids.display()
        ),
    )?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[],
    )?;
    assert_eq!(out.success_stdout()?, "ok\n");
    let helper = read_pids(&pids)?;
    assert_eq!(helper.len(), 1);
    assert!(wait_dead(&helper), "setsid helper killed: {helper:?}");
    Ok(())
}

#[test]
fn truncated_stderr_never_leaks_a_split_secret_and_debug_hides_stdout() -> TestResult {
    // Review L3: the cut may split a token; the partial last line is dropped.
    let scratch = Scratch::new("split")?;
    // 16384-byte bound: filler line (16369 + 1) + "tail " + 9 bytes of the token, so
    // the cut lands inside the token ("glpat-RUN").
    let filler = "x".repeat(16 * 1024 - 15);
    let binary = scratch.script(
        "git",
        &format!("printf 'stdout-secret-data'\nprintf '%s\\n' '{filler}' >&2\nprintf 'tail {SENTINEL}\\n' >&2"),
    )?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    let out = run(
        &runner,
        &invocation,
        &WorkDir::Neutral,
        limits(30_000, 1_000),
        &CancelToken::default(),
        &[],
    )?;
    assert!(out.stderr_truncated);
    assert!(
        !out.stderr.contains("glpat-RUN"),
        "no partial token prefix: {}",
        out.stderr
    );
    assert!(out.stderr.ends_with('\n') || out.stderr.is_empty());
    let debug = format!("{out:?}");
    assert!(!debug.contains("stdout-secret-data") && debug.contains("stdout_len"));
    Ok(())
}

#[test]
fn a_cancelled_token_starts_nothing() -> TestResult {
    let scratch = Scratch::new("precancel")?;
    let marker = scratch.0.join("ran");
    let binary = scratch.script("git", &format!("touch {}", marker.display()))?;
    let runner = GitRunner::new(binary, &scratch.home()?)?;
    let cancel = CancelToken::default();
    cancel.cancel();
    let invocation = GitInvocation::new(Subcommand::Version, Vec::new())?;
    assert_eq!(
        runner.run(
            &invocation,
            &WorkDir::Neutral,
            limits(30_000, 1_000),
            &cancel,
            &[]
        ),
        Err(RunError::Cancelled)
    );
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn bundle_arguments_are_typed_and_scoped_to_bundle() -> TestResult {
    let scratch = Scratch::new("bundle-args")?;
    let file = scratch.0.join("snap.bundle");
    let bundle = BundleFile::new(&file)?;
    let glob = RefPrefix::new("refs/gbm/snapshots/s1/")?;
    let ok = GitInvocation::new(
        Subcommand::Bundle,
        vec![
            Arg::Flag("create"),
            Arg::Bundle(bundle.clone()),
            Arg::RefGlob(glob.clone()),
        ],
    )?;
    let argv = ok.argv();
    assert_eq!(
        argv.last().map(String::as_str),
        Some("--glob=refs/gbm/snapshots/s1/*")
    );
    // Bundle/glob words are refused for other subcommands.
    assert!(GitInvocation::new(Subcommand::Fetch, vec![Arg::RefGlob(glob)]).is_err());
    assert!(GitInvocation::new(Subcommand::Fetch, vec![Arg::Bundle(bundle)]).is_err());
    assert!(GitInvocation::new(Subcommand::Bundle, vec![Arg::Flag("unbundle")]).is_err());
    // Bundle paths: absolute, canonical parent, safe `.bundle` name.
    for bad in [
        PathBuf::from("snap.bundle"),
        scratch.0.join("snap.tar"),
        scratch.0.join("-x.bundle"),
        scratch.0.join("a b.bundle"),
        scratch.0.join("missing-dir").join("x.bundle"),
        scratch.0.join("..").join("x.bundle"),
    ] {
        assert!(BundleFile::new(&bad).is_err(), "{}", bad.display());
    }
    Ok(())
}
