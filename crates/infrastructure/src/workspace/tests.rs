//! TEST-WORKSPACE with real Git: workspaces cloned from a fixture source, a newer
//! source captured into an owned mirror snapshot, then the update policy applied to
//! clean, dirty, staged, untracked-collision, ignored-collision, ahead, detached,
//! diverged, other-branch, in-progress and filter-attribute workspaces.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use gbm_application::snapshot::{CaptureRequest, ObservationClock, capture_snapshot};
use gbm_application::workspace::{SkipReason, WorkspaceOutcome, update_workspace};
use gbm_domain::identity::{GitHubRepoId, mirror_relative_path};
use gbm_test_fixtures::{Sandbox, source_repo};

use super::*;
use crate::filesystem::OwnedRoot;
use crate::git::mirror::{GitMirror, RunnerRefReader};
use crate::git::runner::GitBinary;
use crate::storage::{establish, validate_candidate};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Clock(AtomicU64);

impl ObservationClock for Clock {
    fn next(&self) -> (u64, i64) {
        let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        (n, i64::try_from(n).unwrap_or(0))
    }
}

/// Owned root + mirror + runner HOME, and the captured target of `main`.
struct Env {
    base: PathBuf,
    home: PathBuf,
    mirror: PathBuf,
}

impl Env {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("gbm-ws-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("root"))?;
        let home = base.join("home");
        fs::create_dir(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        let established = establish(
            &validate_candidate(&base.join("root"))?,
            "root-1",
            "2026-10-02T00:00:00Z",
        )?;
        let root = OwnedRoot::verify(established.root().path(), "root-1")?;
        let mirror = root.create_dir(&mirror_relative_path(GitHubRepoId::parse("77")?))?;
        Ok(Self {
            base: fs::canonicalize(&base)?,
            home: fs::canonicalize(home)?,
            mirror,
        })
    }

    fn runner(&self) -> Result<GitRunner, Box<dyn std::error::Error>> {
        Ok(GitRunner::new(GitBinary::locate()?, &self.home)?)
    }

    /// Captures `source` into snapshot `c1`; returns the captured `main` OID.
    fn capture(
        &self,
        sandbox: &Sandbox,
        source: &Path,
    ) -> Result<ObjectId, Box<dyn std::error::Error>> {
        let location = Location::local(&fs::canonicalize(source)?)?;
        let reader = RunnerRefReader::new(
            self.runner()?,
            location.clone(),
            None,
            RunLimits::default(),
            CancelToken::default(),
        );
        let mirror = GitMirror::new(
            self.runner()?,
            self.mirror.clone(),
            location,
            None,
            RunLimits::default(),
            CancelToken::default(),
        );
        capture_snapshot(
            &reader,
            &mirror,
            &Clock(AtomicU64::new(0)),
            &CaptureRequest {
                repository_id: "r",
                job_id: "j",
                snapshot_id: "c1",
            },
        )?;
        let tip = sandbox.git(source, &["rev-parse", "main"])?;
        Ok(ObjectId::parse(&tip).ok_or("oid")?)
    }

    fn workspace(&self, dir: &Path) -> Result<GitWorkspace, Box<dyn std::error::Error>> {
        Ok(GitWorkspace::new(
            self.runner()?,
            fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()),
            self.mirror.clone(),
            "refs/gbm/snapshots/c1/heads/main".to_owned(),
            "job-1",
            RunLimits::default(),
            CancelToken::default(),
        ))
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// Worktree files (excluding `.git/`) plus HEAD, index and branch refs, byte for byte.
fn fingerprint(
    sandbox: &Sandbox,
    dir: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            let rel = path.strip_prefix(dir)?.to_string_lossy().into_owned();
            if rel == ".git" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(rel, fs::read(&path)?);
            }
        }
    }
    for file in ["HEAD", "index"] {
        if let Ok(bytes) = fs::read(dir.join(".git").join(file)) {
            out.insert(format!(".git/{file}"), bytes);
        }
    }
    let refs = sandbox.git(dir, &["for-each-ref", "refs/heads/", "refs/tags/"])?;
    out.insert("refs".to_owned(), refs.into_bytes());
    Ok(out)
}

fn clone(
    sandbox: &Sandbox,
    source: &Path,
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = sandbox.path().join(name);
    sandbox.git(
        sandbox.path(),
        &[
            "clone",
            "-q",
            "--",
            &source.to_string_lossy(),
            &dir.to_string_lossy(),
        ],
    )?;
    Ok(dir)
}

fn commit(sandbox: &Sandbox, dir: &Path, file: &str, text: &str, message: &str) -> TestResult {
    fs::write(dir.join(file), text)?;
    sandbox.git(dir, &["add", "--", file])?;
    sandbox.git(dir, &["commit", "-q", "-m", message])?;
    Ok(())
}

#[test]
fn only_the_clean_fast_forward_is_applied_and_every_other_workspace_is_unchanged() -> TestResult {
    // TEST-WORKSPACE.
    let sandbox = Sandbox::new("ws-policy")?;
    let source = source_repo(&sandbox)?;
    let clean = clone(&sandbox, &source.path, "ws-clean")?;
    let dirty = clone(&sandbox, &source.path, "ws-dirty")?;
    fs::write(dirty.join("README.md"), "local edit\n")?;
    let staged = clone(&sandbox, &source.path, "ws-staged")?;
    fs::write(staged.join("staged.txt"), "staged\n")?;
    sandbox.git(&staged, &["add", "--", "staged.txt"])?;
    let collide = clone(&sandbox, &source.path, "ws-untracked")?;
    fs::write(collide.join("new.txt"), "my untracked notes\n")?;
    // Ignored (so invisible to status) local file at the path upstream adds.
    let ignored = clone(&sandbox, &source.path, "ws-ignored")?;
    fs::write(
        ignored.join(".git").join("info").join("exclude"),
        "new.txt\n",
    )?;
    fs::write(ignored.join("new.txt"), "LOCAL secret\n")?;
    let detached = clone(&sandbox, &source.path, "ws-detached")?;
    sandbox.git(&detached, &["checkout", "-q", "--detach", "HEAD~1"])?;
    let diverged = clone(&sandbox, &source.path, "ws-diverged")?;
    commit(&sandbox, &diverged, "local.txt", "local\n", "local only")?;
    let other = clone(&sandbox, &source.path, "ws-other")?;
    sandbox.git(&other, &["checkout", "-q", "feature"])?;
    let merging = clone(&sandbox, &source.path, "ws-merging")?;
    let head = sandbox.git(&merging, &["rev-parse", "HEAD"])?;
    fs::write(merging.join(".git").join("MERGE_HEAD"), format!("{head}\n"))?;

    // The source moves on: new file `new.txt` (collides with the untracked one).
    commit(
        &sandbox,
        &source.path,
        "new.txt",
        "from source\n",
        "upstream change",
    )?;
    // Ahead: has the new upstream commit plus a local one on top.
    let ahead = clone(&sandbox, &source.path, "ws-ahead")?;
    commit(&sandbox, &ahead, "mine.txt", "mine\n", "local on top")?;

    let env = Env::new("policy")?;
    let target = env.capture(&sandbox, &source.path)?;
    let skipped = [
        (&dirty, SkipReason::Dirty),
        (&staged, SkipReason::Dirty),
        (&collide, SkipReason::Untracked),
        (&ignored, SkipReason::WouldOverwriteLocalFiles),
        (&detached, SkipReason::Detached),
        (&diverged, SkipReason::Diverged),
        (&other, SkipReason::OtherBranch),
        (&merging, SkipReason::OperationInProgress),
        (&ahead, SkipReason::Ahead),
    ];
    for (dir, reason) in skipped {
        let before = fingerprint(&sandbox, dir)?;
        let outcome = update_workspace(&env.workspace(dir)?, "main", &target)?;
        assert_eq!(
            outcome,
            WorkspaceOutcome::Skipped(reason),
            "{}",
            dir.display()
        );
        assert_eq!(
            fingerprint(&sandbox, dir)?,
            before,
            "{} unchanged",
            dir.display()
        );
        let leftover = sandbox.git(dir, &["for-each-ref", "refs/gbm/"])?;
        assert!(
            leftover.is_empty(),
            "no temporary ref left in {}",
            dir.display()
        );
    }
    assert_eq!(
        fs::read_to_string(collide.join("new.txt"))?,
        "my untracked notes\n"
    );
    assert_eq!(
        fs::read_to_string(ignored.join("new.txt"))?,
        "LOCAL secret\n"
    );

    let old = sandbox.git(&clean, &["rev-parse", "HEAD"])?;
    let outcome = update_workspace(&env.workspace(&clean)?, "main", &target)?;
    assert_eq!(
        outcome,
        WorkspaceOutcome::Updated {
            from: ObjectId::parse(&old).ok_or("oid")?,
            to: target.clone()
        }
    );
    assert_eq!(
        sandbox.git(&clean, &["rev-parse", "HEAD"])?,
        target.as_str()
    );
    assert_eq!(fs::read_to_string(clean.join("new.txt"))?, "from source\n");
    assert!(sandbox.git(&clean, &["status", "--porcelain"])?.is_empty());
    assert_eq!(
        sandbox.git(&clean, &["rev-parse", "--abbrev-ref", "HEAD"])?,
        "main"
    );
    // Running again is a no-op.
    assert_eq!(
        update_workspace(&env.workspace(&clean)?, "main", &target)?,
        WorkspaceOutcome::Current
    );
    Ok(())
}

#[test]
fn ignored_files_are_never_replaced_but_expected_replacements_proceed() -> TestResult {
    let sandbox = Sandbox::new("ws-ignored")?;
    let source = source_repo(&sandbox)?;
    commit(
        &sandbox,
        &source.path,
        "swap",
        "tracked file\n",
        "swap file",
    )?;
    // Ignored local file where upstream needs a directory.
    let parent = clone(&sandbox, &source.path, "ws-parent")?;
    fs::write(parent.join(".git").join("info").join("exclude"), "cfg\n")?;
    fs::write(parent.join("cfg"), "LOCAL\n")?;
    // Ignored directory that upstream adds a new, non-colliding file into.
    let build = clone(&sandbox, &source.path, "ws-build")?;
    fs::write(build.join(".git").join("info").join("exclude"), "build/\n")?;
    fs::create_dir(build.join("build"))?;
    fs::write(build.join("build").join("local.o"), "object\n")?;
    // Clean: the tracked file `swap` becomes a directory upstream.
    let plain = clone(&sandbox, &source.path, "ws-plain")?;

    sandbox.git(&source.path, &["rm", "-q", "--", "swap"])?;
    for dir in ["swap", "cfg", "build"] {
        fs::create_dir_all(source.path.join(dir))?;
    }
    for file in ["swap/inner.txt", "cfg/app.json", "build/gen.txt"] {
        fs::write(source.path.join(file), "upstream\n")?;
        sandbox.git(&source.path, &["add", "-f", "--", file])?;
    }
    sandbox.git(&source.path, &["commit", "-q", "-m", "restructure"])?;
    let env = Env::new("ignored")?;
    let target = env.capture(&sandbox, &source.path)?;

    let before = fingerprint(&sandbox, &parent)?;
    assert_eq!(
        update_workspace(&env.workspace(&parent)?, "main", &target)?,
        WorkspaceOutcome::Skipped(SkipReason::WouldOverwriteLocalFiles)
    );
    assert_eq!(fingerprint(&sandbox, &parent)?, before);
    assert!(
        sandbox
            .git(&parent, &["for-each-ref", "refs/gbm/"])?
            .is_empty()
    );

    for dir in [&build, &plain] {
        assert!(
            matches!(
                update_workspace(&env.workspace(dir)?, "main", &target)?,
                WorkspaceOutcome::Updated { .. }
            ),
            "{}",
            dir.display()
        );
        assert_eq!(sandbox.git(dir, &["rev-parse", "HEAD"])?, target.as_str());
    }
    assert_eq!(fs::read(build.join("build").join("local.o"))?, b"object\n");
    assert_eq!(
        fs::read_to_string(plain.join("swap").join("inner.txt"))?,
        "upstream\n"
    );
    Ok(())
}

#[test]
fn diff_tree_parsing_is_fail_closed() {
    assert!(
        parse_added_deleted("A\0a/b\0D\0c\0").is_ok_and(|(a, d)| a == ["a/b"] && d.contains("c"))
    );
    for bad in ["A\0../x\0", "A\0x/.GIT/config\0", "M\0x\0", "A\0"] {
        assert!(parse_added_deleted(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn filter_attributes_in_the_target_block_the_update() -> TestResult {
    let sandbox = Sandbox::new("ws-filter")?;
    let source = source_repo(&sandbox)?;
    let ws = clone(&sandbox, &source.path, "ws")?;
    fs::create_dir_all(source.path.join("assets"))?;
    commit(
        &sandbox,
        &source.path,
        "assets/.gitattributes",
        "*.bin filter=custom\n",
        "attrs",
    )?;
    let env = Env::new("filter")?;
    let target = env.capture(&sandbox, &source.path)?;
    let before = fingerprint(&sandbox, &ws)?;
    assert_eq!(
        update_workspace(&env.workspace(&ws)?, "main", &target)?,
        WorkspaceOutcome::Skipped(SkipReason::FiltersUnsupported)
    );
    assert_eq!(fingerprint(&sandbox, &ws)?, before);
    Ok(())
}

#[test]
fn missing_and_unsupported_layouts_are_explicit() -> TestResult {
    let sandbox = Sandbox::new("ws-layout")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("layout")?;
    let target = env.capture(&sandbox, &source.path)?;
    let missing = sandbox.path().join("not-there");
    assert_eq!(
        update_workspace(&env.workspace(&missing)?, "main", &target)?,
        WorkspaceOutcome::Skipped(SkipReason::Missing)
    );
    // `.git` as a symlink (or gitdir file) is not a layout the app manages.
    let real = clone(&sandbox, &source.path, "real")?;
    let linked = sandbox.path().join("linked");
    fs::create_dir(&linked)?;
    std::os::unix::fs::symlink(real.join(".git"), linked.join(".git"))?;
    assert_eq!(
        update_workspace(&env.workspace(&linked)?, "main", &target),
        Err(WorkspaceError::Unsupported)
    );
    Ok(())
}

#[test]
fn hooks_in_the_workspace_do_not_run_during_an_update() -> TestResult {
    let sandbox = Sandbox::new("ws-hooks")?;
    let source = source_repo(&sandbox)?;
    let ws = clone(&sandbox, &source.path, "ws")?;
    let marker = sandbox.path().join("HOOK-RAN");
    for hook in ["post-merge", "reference-transaction", "post-checkout"] {
        let path = ws.join(".git").join("hooks").join(hook);
        fs::write(&path, format!("#!/bin/sh\ntouch {}\n", marker.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    commit(&sandbox, &source.path, "x.txt", "x\n", "x")?;
    let env = Env::new("hooks")?;
    let target = env.capture(&sandbox, &source.path)?;
    assert!(matches!(
        update_workspace(&env.workspace(&ws)?, "main", &target)?,
        WorkspaceOutcome::Updated { .. }
    ));
    assert!(!marker.exists(), "no user hook ran");
    Ok(())
}

#[test]
fn status_parsing_is_fail_closed() {
    assert!(
        parse_status("# branch.oid (initial)\0# branch.head main\0")
            .is_ok_and(|s| s.0 == HeadState::Unborn)
    );
    assert!(parse_status("# branch.head main\0garbage\0").is_err());
    assert!(
        parse_status("1 .M N... 100644 100644 100644 a a x\0").is_err(),
        "no head header"
    );
    let renamed = parse_status(&format!(
        "# branch.oid {}\0# branch.head main\02 R. N... 100644 100644 100644 a b R100 new\0? weird\0",
        "1".repeat(40)
    ));
    // The rename's original-path field ("? weird" here) is skipped, not read as untracked.
    assert!(renamed.is_ok_and(|s| s.2 && !s.3));
}
