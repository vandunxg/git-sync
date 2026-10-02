//! Deterministic local Git fixtures for integration tests (TEST_STRATEGY.md).
//!
//! Test-only crate: production crates must never depend on it. Every Git child runs
//! with a cleared environment, a private `HOME`, no system/global config, no
//! credential helpers, fixed identities and fixed timestamps, so object IDs are
//! reproducible and the owner's real Git configuration is never read or written.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// Fixed time base (2026-01-01T00:00:00Z). The clock advances one minute per Git call,
/// so OIDs are reproducible for the same builder sequence and Git version.
const EPOCH: u64 = 1_767_225_600;

#[derive(Debug)]
pub enum FixtureError {
    /// A required tool is not installed (e.g. git-lfs); the dependent test is BLOCKED.
    ToolUnavailable(&'static str),
    Io(std::io::Error),
    Git {
        args: String,
        stderr: String,
    },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToolUnavailable(tool) => write!(f, "BLOCKED: {tool} is not installed"),
            Self::Io(error) => write!(f, "fixture I/O error: {error}"),
            Self::Git { args, stderr } => write!(f, "git {args} failed: {stderr}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<std::io::Error> for FixtureError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, FixtureError>;

/// A private temporary directory removed on drop.
pub struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    clock: AtomicU64,
}

impl Sandbox {
    pub fn new(label: &str) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "gbm-fixture-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(unique);
        // `create_dir` (not `_all`) fails if the path already exists, so a pre-created
        // or attacker-placed directory is never adopted as the sandbox.
        fs::create_dir(&root)?;
        let home = root.join("home");
        fs::create_dir(&home)?;
        Ok(Self {
            root,
            home,
            clock: AtomicU64::new(0),
        })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Runs `git` in `dir` with the isolated environment; fails on non-zero exit.
    pub fn git(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let output = self.git_output(dir, args)?;
        if !output.status.success() {
            return Err(FixtureError::Git {
                args: args.join(" "),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned())
    }

    fn git_output(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        let tick = self.clock.fetch_add(60, Ordering::Relaxed);
        let date = format!("@{} +0000", EPOCH + tick);
        let path = std::env::var_os("PATH").unwrap_or_default();
        Ok(Command::new("git")
            .args(["-c", "credential.helper=", "-c", "core.hooksPath=/dev/null"])
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "protocol.file.allow=always",
            ])
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", path)
            .env("HOME", &self.home)
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Fixture Author")
            .env("GIT_AUTHOR_EMAIL", "author@fixture.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@fixture.invalid")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()?)
    }

    fn write(&self, dir: &Path, file: &str, contents: &str) -> Result<()> {
        let target = dir.join(file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, contents)?;
        Ok(())
    }

    fn commit(&self, dir: &Path, file: &str, contents: &str, message: &str) -> Result<String> {
        self.write(dir, file, contents)?;
        self.git(dir, &["add", "--", file])?;
        self.git(dir, &["commit", "-q", "-m", message])?;
        self.git(dir, &["rev-parse", "HEAD"])
    }

    fn init(&self, name: &str, bare: bool) -> Result<PathBuf> {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir)?;
        let mut args = vec!["init", "-q"];
        if bare {
            args.push("--bare");
        }
        self.git(&dir, &args)?;
        Ok(dir)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir is harmless, and Drop cannot report errors.
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Object IDs recorded while building the source fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRepo {
    pub path: PathBuf,
    pub main_tip: String,
    pub feature_tip_before_rewrite: String,
    pub feature_tip_after_rewrite: String,
    pub moved_tag_before: String,
    pub moved_tag_after: String,
}

/// Builds a non-bare source repository with: `main` (3 commits), `feature` rewritten by
/// a non-fast-forward amend, `release/1.x`, an annotated tag `v1.0`, a lightweight tag
/// `light`, a tag `moving` that is re-pointed, and a branch name with unusual but legal
/// characters (`odd/naïve-ü`).
pub fn source_repo(sandbox: &Sandbox) -> Result<SourceRepo> {
    let dir = sandbox.init("source", false)?;
    sandbox.commit(&dir, "README.md", "one\n", "c1")?;
    let c2 = sandbox.commit(&dir, "README.md", "two\n", "c2")?;
    sandbox.git(&dir, &["tag", "-a", "v1.0", "-m", "annotated v1.0"])?;
    sandbox.git(&dir, &["tag", "light"])?;
    sandbox.git(&dir, &["tag", "moving"])?;
    sandbox.git(&dir, &["branch", "release/1.x"])?;
    sandbox.git(&dir, &["checkout", "-q", "-b", "feature"])?;
    let before = sandbox.commit(&dir, "feature.txt", "draft\n", "feature draft")?;
    sandbox.write(&dir, "feature.txt", "rewritten\n")?;
    sandbox.git(&dir, &["add", "--", "feature.txt"])?;
    sandbox.git(
        &dir,
        &["commit", "-q", "--amend", "-m", "feature rewritten"],
    )?;
    let after = sandbox.git(&dir, &["rev-parse", "HEAD"])?;
    sandbox.git(&dir, &["checkout", "-q", "main"])?;
    let main_tip = sandbox.commit(&dir, "README.md", "three\n", "c3")?;
    sandbox.git(&dir, &["tag", "-f", "moving"])?;
    sandbox.git(&dir, &["branch", "odd/naïve-ü"])?;
    let moved_tag_after = sandbox.git(&dir, &["rev-parse", "moving"])?;
    Ok(SourceRepo {
        path: dir,
        main_tip,
        feature_tip_before_rewrite: before,
        feature_tip_after_rewrite: after,
        moved_tag_before: c2,
        moved_tag_after,
    })
}

/// An empty bare repository (no refs), e.g. a newly created destination.
pub fn empty_bare(sandbox: &Sandbox, name: &str) -> Result<PathBuf> {
    sandbox.init(name, true)
}

/// Workspace states the sync engine must never overwrite (ADR-006).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspaces {
    pub dirty: PathBuf,
    pub ahead: PathBuf,
    pub detached: PathBuf,
}

/// Clones `source` three times and leaves them dirty, ahead of origin and detached.
pub fn workspaces(sandbox: &Sandbox, source: &Path) -> Result<Workspaces> {
    let source = source.to_string_lossy().into_owned();
    let make = |name: &str| -> Result<PathBuf> {
        let dir = sandbox.root.join(name);
        let target = dir.to_string_lossy().into_owned();
        sandbox.git(&sandbox.root, &["clone", "-q", "--", &source, &target])?;
        Ok(dir)
    };
    let dirty = make("ws-dirty")?;
    let ahead = make("ws-ahead")?;
    let detached = make("ws-detached")?;
    sandbox.write(&dirty, "README.md", "local uncommitted edit\n")?;
    sandbox.write(&dirty, "untracked.txt", "untracked\n")?;
    sandbox.commit(&ahead, "local.txt", "local commit\n", "local only")?;
    sandbox.git(&detached, &["checkout", "-q", "--detach", "HEAD~1"])?;
    Ok(Workspaces {
        dirty,
        ahead,
        detached,
    })
}

/// Ref names that hostile providers or repositories could present. Each must be
/// rejected or safely handled by native validation; Git itself refuses some of them.
pub const HOSTILE_REF_NAMES: &[&str] = &[
    "refs/heads/../escape",
    "refs/heads/-option",
    "refs/heads/a..b",
    "refs/heads/with space",
    "refs/heads/trailing.lock",
    "refs/heads/ctrl\u{7}",
    "refs/heads/line\nbreak",
    "refs/heads/@{upstream}",
    "refs/pull/1/head",
    "refs/remotes/origin/main",
    "refs/notes/commits",
];

/// Creates an LFS-tracked file. Returns `ToolUnavailable` when git-lfs is missing,
/// so dependent tests report BLOCKED instead of silently passing.
pub fn lfs_repo(sandbox: &Sandbox) -> Result<PathBuf> {
    let probe = sandbox.git_output(&sandbox.root, &["lfs", "version"])?;
    if !probe.status.success() {
        return Err(FixtureError::ToolUnavailable("git-lfs"));
    }
    let dir = sandbox.init("lfs-source", false)?;
    sandbox.git(&dir, &["lfs", "install", "--local"])?;
    sandbox.git(&dir, &["lfs", "track", "*.bin"])?;
    sandbox.commit(
        &dir,
        ".gitattributes",
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
        "lfs attributes",
    )?;
    sandbox.commit(
        &dir,
        "payload.bin",
        "binary-payload-fixture\n",
        "lfs payload",
    )?;
    Ok(dir)
}
