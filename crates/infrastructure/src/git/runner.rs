//! Secure, bounded Git process runner (REQ-19, REQ-22, REQ-45; P05-T01).
//!
//! - Binary: an absolute, canonical `git` that is a regular executable owned by root or
//!   the current user; neither it nor any ancestor directory is group/world-writable.
//! - Arguments: typed `Arg`s only. Options must be exact entries of the subcommand's
//!   allowlist (Git accepts abbreviated long options, so a denylist is unsound); URLs
//!   enter only as `Location` (allowlisted HTTPS host, no userinfo) and refs only as
//!   validated full names. Hardening `-c` options always come first; command-line
//!   config outranks repository config. Local file transport is enabled only when a
//!   `Location::Local` is an argument; broker words only via `AuthArgs`.
//! - Environment: cleared, then a closed allowlist (no system/global config, no
//!   prompts, fixed system `PATH`, private `HOME`). Credentials never travel in argv or
//!   env: authenticated calls add only the broker helper words.
//! - Process: its own process group; stdout/stderr drained concurrently into bounded
//!   buffers (excess read and discarded so the child never blocks on a full pipe);
//!   descendants are recorded periodically while it runs; timeout and cancellation send
//!   TERM, then KILL after a grace period, to the group and every recorded descendant
//!   (PID + start time, never a reused PID); the child is reaped; leftover group members
//!   and recorded descendants are killed after any exit.
//! - Output: stderr is redacted (complete lines) before it is returned; stdout is only
//!   for parsing and must be checked with `success_stdout` (exit 0, reaped, untruncated,
//!   complete, UTF-8).
//!
//! Residual risk (owner acceptance required): a descendant that `setsid`s and is
//! reparented between two scans escapes signalling; its pipe ends then keep a drain
//! thread alive and the run reports `output_complete == false` (never a success).

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use gbm_domain::contract::secret::SecretString;

use gbm_domain::refs::ObjectId;

use crate::git_auth::{HelperCommand, RemoteTarget, git_auth_args, git_child_env, redact};
use crate::platform::{self, ProcessId, StopSignal};

/// Well-known system locations searched by `GitBinary::locate` (no `$PATH` lookup).
pub const GIT_CANDIDATES: [&str; 3] = ["/usr/bin/git", "/bin/git", "/usr/local/bin/git"];
/// Fixed `PATH` for Git and anything it spawns (system directories only).
const CHILD_PATH: &str = "/usr/bin:/bin";
/// How often the supervisor checks cancellation and the deadline.
const POLL: Duration = Duration::from_millis(25);
/// Descendants are scanned every this many polls while the child runs, so helpers that
/// later detach are already known when cleanup happens.
const SCAN_EVERY_POLLS: u32 = 8;
const MAX_ARGS: usize = 256;
const MAX_REF_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    GitNotFound,
    UntrustedBinary,
    InvalidArgument,
    InvalidLocation,
    InvalidWorkDir,
    Spawn,
    /// The token was already cancelled; nothing was started.
    Cancelled,
    /// The command did not exit 0 (or was stopped).
    Failed(Termination),
    /// Output exceeded its bound, the pipes did not close or the child was not reaped.
    OutputIncomplete,
    NotUtf8,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitNotFound => f.write_str("git is not installed in a trusted location"),
            Self::UntrustedBinary => f.write_str("the git binary is not trusted"),
            Self::InvalidArgument => f.write_str("a git argument was rejected"),
            Self::InvalidLocation => f.write_str("the repository location was rejected"),
            Self::InvalidWorkDir => f.write_str("the git working directory was rejected"),
            Self::Spawn => f.write_str("git could not be started"),
            Self::Cancelled => f.write_str("cancelled before git was started"),
            Self::Failed(termination) => write!(f, "git did not succeed: {termination:?}"),
            Self::OutputIncomplete => f.write_str("git output was incomplete"),
            Self::NotUtf8 => f.write_str("git output was not valid UTF-8"),
        }
    }
}

impl std::error::Error for RunError {}

/// A validated Git executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBinary(PathBuf);

impl GitBinary {
    pub fn locate() -> Result<Self, RunError> {
        GIT_CANDIDATES
            .iter()
            .find_map(|candidate| Self::at(Path::new(candidate)).ok())
            .ok_or(RunError::GitNotFound)
    }

    /// The canonical file must be a regular executable owned by root or the current
    /// user, and neither it nor any ancestor directory may be group/world-writable.
    pub fn at(path: &Path) -> Result<Self, RunError> {
        if !path.is_absolute() {
            return Err(RunError::UntrustedBinary);
        }
        let canonical = fs::canonicalize(path).map_err(|_| RunError::GitNotFound)?;
        let uid = platform::current_uid().map_err(|_| RunError::UntrustedBinary)?;
        let trusted = |path: &Path| {
            fs::metadata(path).is_ok_and(|meta| {
                (meta.uid() == 0 || meta.uid() == uid) && meta.permissions().mode() & 0o022 == 0
            })
        };
        let meta = fs::metadata(&canonical).map_err(|_| RunError::GitNotFound)?;
        let ok = meta.is_file()
            && meta.permissions().mode() & 0o111 != 0
            && canonical.ancestors().all(trusted);
        if ok {
            Ok(Self(canonical))
        } else {
            Err(RunError::UntrustedBinary)
        }
    }

    /// Test-only: a stand-in script in a private scratch directory under `/tmp`,
    /// whose sticky world-writable ancestor `GitBinary::at` correctly refuses.
    #[cfg(test)]
    pub(crate) fn trusted_for_tests(path: &Path) -> Self {
        Self(path.to_path_buf())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Where a repository is: an allowlisted HTTPS remote (no userinfo) or a canonical
/// local path. URLs can only enter an invocation through this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Remote(RemoteTarget),
    Local(PathBuf),
}

impl Location {
    /// Accepts only an absolute, canonical (no symlink, no `..`), UTF-8 path.
    pub fn local(path: &Path) -> Result<Self, RunError> {
        let canonical = fs::canonicalize(path).map_err(|_| RunError::InvalidLocation)?;
        if !path.is_absolute() || canonical != path || path.to_str().is_none() {
            return Err(RunError::InvalidLocation);
        }
        Ok(Self::Local(canonical))
    }

    fn arg(&self) -> String {
        match self {
            Self::Remote(remote) => remote.url(),
            Self::Local(path) => path.to_string_lossy().into_owned(),
        }
    }
}

/// A full ref name under `refs/` following `git check-ref-format` (no globs), never
/// starting with `-`.
fn is_full_ref(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("refs/") else {
        return false;
    };
    let forbidden = |c: char| {
        c.is_ascii_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    };
    name.len() <= MAX_REF_BYTES
        && !rest.is_empty()
        && !rest.ends_with('.')
        && !rest.contains("..")
        && !rest.contains("@{")
        && !rest.chars().any(forbidden)
        && rest
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

/// An explicit `[+]<src>:<dst>` refspec of two full ref names (no wildcards).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSpec(String);

impl RefSpec {
    pub fn new(source: &str, destination: &str, force: bool) -> Result<Self, RunError> {
        if !is_full_ref(source) || !is_full_ref(destination) {
            return Err(RunError::InvalidArgument);
        }
        let plus = if force { "+" } else { "" };
        Ok(Self(format!("{plus}{source}:{destination}")))
    }
}

/// A full `refs/...` name (for `update-ref -d`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullRef(String);

impl FullRef {
    pub fn new(name: &str) -> Result<Self, RunError> {
        if is_full_ref(name) {
            Ok(Self(name.to_owned()))
        } else {
            Err(RunError::InvalidArgument)
        }
    }
}

/// A `refs/...` prefix pattern ending in `/` (for `for-each-ref`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefPrefix(String);

impl RefPrefix {
    pub fn new(prefix: &str) -> Result<Self, RunError> {
        match prefix.strip_suffix('/') {
            Some(base) if is_full_ref(base) => Ok(Self(prefix.to_owned())),
            _ => Err(RunError::InvalidArgument),
        }
    }
}

/// A bundle file path: absolute, UTF-8, inside an existing canonical directory, with a
/// `.bundle` file name of safe characters. The file itself may not exist yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile(PathBuf);

impl BundleFile {
    pub fn new(path: &Path) -> Result<Self, RunError> {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
        else {
            return Err(RunError::InvalidArgument);
        };
        let safe_name = name.ends_with(".bundle")
            && !name.starts_with(['-', '.'])
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        let canonical_parent = fs::canonicalize(parent).map_err(|_| RunError::InvalidArgument)?;
        if !path.is_absolute()
            || !safe_name
            || canonical_parent != parent
            || path.to_str().is_none()
        {
            return Err(RunError::InvalidArgument);
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// One typed command-line word. Free-form strings cannot be expressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    /// Must be one of the subcommand's exact allowlisted options (no abbreviations).
    Flag(&'static str),
    /// `--`; no flag may follow it.
    EndOfOptions,
    Location(Location),
    RefSpec(RefSpec),
    RefPrefix(RefPrefix),
    Ref(FullRef),
    Oid(ObjectId),
    /// `bundle` only: the bundle file.
    Bundle(BundleFile),
    /// `bundle create` only: `--glob=<prefix>*`, every ref under one namespace.
    RefGlob(RefPrefix),
}

/// Subcommands the app may run. Anything else cannot be expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subcommand {
    LsRemote,
    Fetch,
    ForEachRef,
    RevParse,
    CatFile,
    Init,
    Fsck,
    UpdateRef,
    Bundle,
    Status,
    MergeBase,
    Merge,
    LsTree,
    DiffTree,
    Version,
}

impl Subcommand {
    fn as_str(self) -> &'static str {
        match self {
            Self::LsRemote => "ls-remote",
            Self::Fetch => "fetch",
            Self::ForEachRef => "for-each-ref",
            Self::RevParse => "rev-parse",
            Self::CatFile => "cat-file",
            Self::Init => "init",
            Self::Fsck => "fsck",
            Self::UpdateRef => "update-ref",
            Self::Bundle => "bundle",
            Self::Status => "status",
            Self::MergeBase => "merge-base",
            Self::Merge => "merge",
            Self::LsTree => "ls-tree",
            Self::DiffTree => "diff-tree",
            Self::Version => "version",
        }
    }

    /// Exact option strings allowed for this subcommand. Git accepts unambiguous
    /// abbreviations of long options, so only whole, listed words are accepted.
    fn allowed_flags(self) -> &'static [&'static str] {
        match self {
            Self::LsRemote => &["--symref"],
            Self::Fetch => &[
                "--no-tags",
                "--prune",
                "--force",
                "--no-recurse-submodules",
                "--no-write-fetch-head",
                "--atomic",
                "--quiet",
                "--no-auto-gc",
                "--no-auto-maintenance",
                "--no-show-forced-updates",
            ],
            Self::ForEachRef => &["--format=%(objectname)%09%(objecttype)%09%(refname)"],
            Self::RevParse => &["--verify", "--quiet", "--show-object-format"],
            Self::CatFile => &["-e", "-t", "blob"],
            Self::Init => &[
                "--bare",
                "--quiet",
                "--object-format=sha1",
                "--object-format=sha256",
            ],
            Self::Fsck => &["--full", "--strict", "--no-dangling", "--no-progress"],
            Self::UpdateRef => &["-d", "--no-deref"],
            Self::Bundle => &["create", "verify", "list-heads", "--quiet"],
            Self::Status => &[
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
                "--ignore-submodules=none",
                "-z",
            ],
            Self::MergeBase => &["--is-ancestor"],
            // Fast-forward only; never a merge commit, never a checkout of another branch.
            Self::Merge => &["--ff-only", "--quiet", "--no-stat", "--no-autostash"],
            Self::LsTree => &["-r", "-z", "--full-tree"],
            // Paths added/deleted between two commits; plumbing, no external diff.
            Self::DiffTree => &[
                "-r",
                "-z",
                "--name-status",
                "--no-renames",
                "--diff-filter=AD",
            ],
            Self::Version => &[],
        }
    }
}

/// Broker helper `-c` words; constructible only from a validated `HelperCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthArgs(Vec<String>);

impl AuthArgs {
    pub fn for_helper(helper: &HelperCommand) -> Result<Self, RunError> {
        git_auth_args(helper)
            .map(Self)
            .map_err(|_| RunError::InvalidArgument)
    }
}

/// Base hardening for every invocation (before any other word). Command-line config
/// outranks repository config for these keys. Repository-local `url.*.insteadOf`,
/// `remote.*` and `include*` cannot be overridden this way: URLs are therefore always
/// explicit `Location`s (never remote names), and `GIT_DIR` repositories are only
/// app-managed mirrors.
const HARDENING: [&str; 29] = [
    "credential.helper=",
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "core.askPass=",
    "core.sshCommand=",
    "core.alternateRefsCommand=",
    "core.attributesFile=/dev/null",
    "core.excludesFile=/dev/null",
    "core.pager=cat",
    "protocol.allow=never",
    "protocol.https.allow=always",
    "protocol.version=2",
    "http.sslVerify=true",
    "http.proxy=",
    "http.extraHeader=",
    "http.followRedirects=false",
    "http.cookieFile=",
    "fetch.recurseSubmodules=false",
    "submodule.recurse=false",
    "uploadpack.packObjectsHook=",
    "init.templateDir=",
    "merge.autoStash=false",
    "rebase.autoStash=false",
    "gc.auto=0",
    "maintenance.auto=false",
    "transfer.fsckObjects=true",
    "fetch.fsckObjects=true",
    "receive.fsckObjects=true",
    "safe.bareRepository=explicit",
];

/// A validated command line (without the binary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInvocation {
    subcommand: Subcommand,
    args: Vec<Arg>,
    auth: Option<AuthArgs>,
}

impl GitInvocation {
    pub fn new(subcommand: Subcommand, args: Vec<Arg>) -> Result<Self, RunError> {
        let allowed = subcommand.allowed_flags();
        let mut options_ended = false;
        for arg in &args {
            match arg {
                Arg::Flag(flag) if options_ended || !allowed.contains(flag) => {
                    return Err(RunError::InvalidArgument);
                }
                Arg::Bundle(_) | Arg::RefGlob(_) if subcommand != Subcommand::Bundle => {
                    return Err(RunError::InvalidArgument);
                }
                Arg::EndOfOptions if options_ended => return Err(RunError::InvalidArgument),
                Arg::EndOfOptions => options_ended = true,
                _ => {}
            }
        }
        if args.len() > MAX_ARGS {
            return Err(RunError::InvalidArgument);
        }
        Ok(Self {
            subcommand,
            args,
            auth: None,
        })
    }

    /// `git ls-remote --symref -- <location>` (read-only).
    pub fn ls_remote(location: &Location) -> Result<Self, RunError> {
        Self::new(
            Subcommand::LsRemote,
            vec![
                Arg::Flag("--symref"),
                Arg::EndOfOptions,
                Arg::Location(location.clone()),
            ],
        )
    }

    pub fn with_auth(mut self, auth: AuthArgs) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Git's local/file transport is enabled only when a `Location::Local` is an
    /// argument of this invocation.
    fn uses_local_transport(&self) -> bool {
        self.args
            .iter()
            .any(|arg| matches!(arg, Arg::Location(Location::Local(_))))
    }

    pub fn argv(&self) -> Vec<String> {
        let mut argv: Vec<String> = HARDENING
            .iter()
            .flat_map(|option| ["-c".to_owned(), (*option).to_owned()])
            .collect();
        if let Some(auth) = &self.auth {
            argv.extend(auth.0.iter().cloned());
        }
        if self.uses_local_transport() {
            argv.extend(["-c".to_owned(), "protocol.file.allow=always".to_owned()]);
        }
        argv.push(self.subcommand.as_str().to_owned());
        argv.extend(self.args.iter().map(|arg| match arg {
            Arg::Flag(flag) => (*flag).to_owned(),
            Arg::EndOfOptions => "--".to_owned(),
            Arg::Location(location) => location.arg(),
            Arg::RefSpec(spec) => spec.0.clone(),
            Arg::RefPrefix(prefix) => prefix.0.clone(),
            Arg::Ref(name) => name.0.clone(),
            Arg::Bundle(file) => file.0.to_string_lossy().into_owned(),
            Arg::RefGlob(prefix) => format!("--glob={}*", prefix.0),
            Arg::Oid(oid) => oid.as_str().to_owned(),
        }));
        argv
    }
}

/// Where the child runs: a neutral private directory (not a repository) or a bare
/// repository given explicitly through `GIT_DIR` (no discovery).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkDir {
    Neutral,
    GitDir(PathBuf),
    /// A non-bare working tree whose `.git` is a real directory (not a file/symlink):
    /// `GIT_DIR=<dir>/.git`, `GIT_WORK_TREE=<dir>`, no discovery.
    WorkTree(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub timeout: Duration,
    /// Time between TERM and KILL, and for pipes to close after exit.
    pub grace: Duration,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            stdout_bytes: 16 * 1024 * 1024,
            stderr_bytes: 64 * 1024,
            timeout: Duration::from_secs(600),
            grace: Duration::from_secs(3),
        }
    }
}

/// Cooperative cancellation shared with the caller (e.g. a job's Cancel action).
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// How the child ended. `Cancelled`/`TimedOut` win over the raw exit status: the
/// app stopped it, so nothing it printed is trusted as a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Exited(i32),
    Signaled(i32),
    Cancelled,
    TimedOut,
    /// The child could not be confirmed reaped within the grace period.
    Unknown,
}

#[derive(Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub termination: Termination,
    stdout: Vec<u8>,
    pub stdout_truncated: bool,
    /// Redacted, lossily decoded stderr; safe for logs.
    pub stderr: String,
    pub stderr_truncated: bool,
    /// Both pipes reached end-of-file within the grace period.
    pub output_complete: bool,
    /// The child's exit status was collected (it is not left as a zombie or running).
    pub reaped: bool,
}

impl GitOutput {
    /// Stdout of a fully successful run: exit 0, complete, within bounds, valid UTF-8.
    pub fn success_stdout(&self) -> Result<&str, RunError> {
        if self.termination != Termination::Exited(0) {
            return Err(RunError::Failed(self.termination));
        }
        if self.stdout_truncated || !self.output_complete || !self.reaped {
            return Err(RunError::OutputIncomplete);
        }
        std::str::from_utf8(&self.stdout).map_err(|_| RunError::NotUtf8)
    }
}

/// Runs validated invocations of one trusted binary with a private `HOME`.
#[derive(Debug, Clone)]
pub struct GitRunner {
    binary: GitBinary,
    home: PathBuf,
}

impl GitRunner {
    /// `private_dir` must be an absolute directory private to this user (e.g. a
    /// `PrivateRuntimeDir`); it is the child's `HOME` and neutral working directory.
    pub fn new(binary: GitBinary, private_dir: &Path) -> Result<Self, RunError> {
        let meta = fs::symlink_metadata(private_dir).map_err(|_| RunError::InvalidWorkDir)?;
        let uid = platform::current_uid().map_err(|_| RunError::InvalidWorkDir)?;
        if !private_dir.is_absolute()
            || !meta.is_dir()
            || meta.uid() != uid
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err(RunError::InvalidWorkDir);
        }
        Ok(Self {
            binary,
            home: private_dir.to_path_buf(),
        })
    }

    fn environment(
        &self,
        work: &WorkDir,
    ) -> Result<(PathBuf, Vec<(OsString, OsString)>), RunError> {
        let mut env = git_child_env(CHILD_PATH.as_ref(), &self.home);
        let cwd = match work {
            WorkDir::Neutral => self.home.clone(),
            WorkDir::GitDir(dir) => {
                let canonical = fs::canonicalize(dir).map_err(|_| RunError::InvalidWorkDir)?;
                if !dir.is_absolute() || canonical != *dir || !canonical.is_dir() {
                    return Err(RunError::InvalidWorkDir);
                }
                env.push(("GIT_DIR".into(), canonical.clone().into_os_string()));
                canonical
            }
            WorkDir::WorkTree(dir) => {
                let canonical = fs::canonicalize(dir).map_err(|_| RunError::InvalidWorkDir)?;
                let git_dir = canonical.join(".git");
                let real_git_dir = fs::symlink_metadata(&git_dir)
                    .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink());
                if !dir.is_absolute() || canonical != *dir || !canonical.is_dir() || !real_git_dir {
                    return Err(RunError::InvalidWorkDir);
                }
                env.push(("GIT_DIR".into(), git_dir.into_os_string()));
                env.push(("GIT_WORK_TREE".into(), canonical.clone().into_os_string()));
                // Inspection must not refresh the user's index as a side effect.
                env.push(("GIT_OPTIONAL_LOCKS".into(), "0".into()));
                canonical
            }
        };
        // Never discover a repository above the working directory.
        env.push((
            "GIT_CEILING_DIRECTORIES".into(),
            cwd.clone().into_os_string(),
        ));
        Ok((cwd, env))
    }

    pub fn run(
        &self,
        invocation: &GitInvocation,
        work: &WorkDir,
        limits: RunLimits,
        cancel: &CancelToken,
        known_secrets: &[&SecretString],
    ) -> Result<GitOutput, RunError> {
        if cancel.is_cancelled() {
            return Err(RunError::Cancelled);
        }
        let (cwd, env) = self.environment(work)?;
        let mut command = Command::new(self.binary.path());
        command
            .args(invocation.argv())
            .current_dir(&cwd)
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().map_err(|_| RunError::Spawn)?;
        let pid = child.id();
        let leader = ProcessId::of(pid);
        let stdout = child
            .stdout
            .take()
            .map(|pipe| drain(pipe, limits.stdout_bytes));
        let stderr = child
            .stderr
            .take()
            .map(|pipe| drain(pipe, limits.stderr_bytes));
        let (sender, receiver) = mpsc::channel();
        // The waiter owns the child and reaps it; the supervisor signals by PID.
        thread::spawn(move || {
            let _ = sender.send(child.wait());
        });

        let started = Instant::now();
        let mut stopped: Option<Termination> = None;
        let mut kill_at: Option<Instant> = None;
        let mut give_up_at: Option<Instant> = None;
        let mut known_tree: Vec<ProcessId> = Vec::new();
        let mut polls: u32 = 0;
        let status: Option<ExitStatus> = loop {
            match receiver.recv_timeout(POLL) {
                Ok(result) => break result.ok(),
                Err(RecvTimeoutError::Disconnected) => break None,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            polls = polls.wrapping_add(1);
            if polls.is_multiple_of(SCAN_EVERY_POLLS) {
                remember_descendants(pid, &mut known_tree);
            }
            if stopped.is_none() {
                if cancel.is_cancelled() {
                    stopped = Some(Termination::Cancelled);
                } else if now.duration_since(started) >= limits.timeout {
                    stopped = Some(Termination::TimedOut);
                }
                if stopped.is_some() {
                    signal_tree(pid, leader, &mut known_tree, StopSignal::Terminate);
                    kill_at = Some(now + limits.grace);
                }
            } else if kill_at.is_some_and(|at| now >= at) {
                signal_tree(pid, leader, &mut known_tree, StopSignal::Kill);
                kill_at = None;
                give_up_at = Some(now + limits.grace);
            } else if give_up_at.is_some_and(|at| now >= at) {
                break None;
            }
        };
        // Background members of the group (e.g. a helper left behind) must not outlive
        // the call. The leader is reaped now, so signal members individually.
        for member in platform::group_members(pid) {
            if Some(member) != leader {
                platform::signal_process(member, StopSignal::Kill);
            }
        }
        for id in &known_tree {
            platform::signal_process(*id, StopSignal::Kill);
        }

        let deadline = Instant::now() + limits.grace;
        let (stdout, stdout_truncated, stdout_done) = finish(stdout, deadline);
        let (stderr, stderr_truncated, stderr_done) = finish(stderr, deadline);
        let reaped = status.is_some();
        let termination = stopped.unwrap_or(match status {
            Some(status) => match (status.code(), status.signal()) {
                (Some(code), _) => Termination::Exited(code),
                (None, Some(signal)) => Termination::Signaled(signal),
                (None, None) => Termination::Unknown,
            },
            None => Termination::Unknown,
        });
        Ok(GitOutput {
            termination,
            stdout,
            stdout_truncated,
            stderr: redact(&complete_lines(&stderr, stderr_truncated), known_secrets),
            stderr_truncated,
            output_complete: stdout_done && stderr_done,
            reaped,
        })
    }
}

fn remember_descendants(pid: u32, known: &mut Vec<ProcessId>) {
    for id in platform::descendants(pid) {
        if !known.contains(&id) {
            known.push(id);
        }
    }
}

/// Signals the group led by `pid` and every descendant seen now or earlier (start-time
/// checked). The group is signalled only while the leader instance is still present;
/// the waiter may reap it concurrently, but a process group ID stays pinned while any
/// member exists, so the remaining check-then-kill window cannot hit a foreign group
/// in practice.
fn signal_tree(
    pid: u32,
    leader: Option<ProcessId>,
    known: &mut Vec<ProcessId>,
    signal: StopSignal,
) {
    remember_descendants(pid, known);
    if leader.is_some_and(|id| platform::start_time(id.pid) == Some(id.start_time)) {
        platform::signal_group(pid, signal);
    }
    for id in known.iter() {
        platform::signal_process(*id, signal);
    }
}

/// Stdout bytes are deliberately omitted (only their length), so `{:?}` logging of an
/// output never prints repository data.
impl fmt::Debug for GitOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitOutput")
            .field("termination", &self.termination)
            .field("stdout_len", &self.stdout.len())
            .field("stdout_truncated", &self.stdout_truncated)
            .field("stderr", &self.stderr)
            .field("stderr_truncated", &self.stderr_truncated)
            .field("output_complete", &self.output_complete)
            .field("reaped", &self.reaped)
            .finish()
    }
}

/// Decodes stderr for redaction. When it was cut at the bound, the trailing partial
/// line is dropped so a secret split by the cut cannot leave an unredacted prefix.
fn complete_lines(bytes: &[u8], truncated: bool) -> String {
    let kept = if truncated {
        bytes
            .iter()
            .rposition(|&b| b == b'\n')
            .and_then(|end| bytes.get(..=end))
            .unwrap_or_default()
    } else {
        bytes
    };
    String::from_utf8_lossy(kept).into_owned()
}

type Drained = JoinHandle<(Vec<u8>, bool)>;

/// Reads `pipe` to EOF, keeping at most `limit` bytes and discarding the rest.
fn drain(mut pipe: impl Read + Send + 'static, limit: usize) -> Drained {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut chunk = [0_u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    let room = limit.saturating_sub(kept.len());
                    let take = read.min(room);
                    kept.extend_from_slice(chunk.get(..take).unwrap_or_default());
                    truncated |= take < read;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => {
                    truncated = true;
                    break;
                }
            }
        }
        (kept, truncated)
    })
}

/// Joins a drain thread if it finishes before `deadline`; otherwise leaves it detached
/// and reports the output as incomplete.
fn finish(handle: Option<Drained>, deadline: Instant) -> (Vec<u8>, bool, bool) {
    let Some(handle) = handle else {
        return (Vec::new(), false, false);
    };
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(POLL);
    }
    if !handle.is_finished() {
        return (Vec::new(), true, false);
    }
    match handle.join() {
        Ok((bytes, truncated)) => (bytes, truncated, true),
        Err(_) => (Vec::new(), true, false),
    }
}

#[cfg(test)]
mod tests;
