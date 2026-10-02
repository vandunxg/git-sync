//! Real `git ls-remote` output from local fixture repositories (no network, no
//! personal repositories), fed through the production argv and domain parser.
//! The fixture `Sandbox` runs Git with a cleared environment and no user config.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use gbm_application::observation::{ReadError, SourceRefReader, observe_source};
use gbm_domain::contract::RepositorySummarySourceState as SourceState;
use gbm_domain::observation::{ObservationOutcome, SourceView};
use gbm_domain::refs::{Advertisement, RefMap, diff, parse_ls_remote};
use gbm_test_fixtures::{Sandbox, empty_bare, source_repo};

use super::*;
use crate::git_auth::AllowedHost;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Test-only reader: runs the production argv against a local path via the sandbox.
struct LocalReader<'a> {
    sandbox: &'a Sandbox,
    location: PathBuf,
}

impl SourceRefReader for LocalReader<'_> {
    fn read_advertisement(&self, _: &str) -> Result<String, ReadError> {
        let args = args_for(&self.location.to_string_lossy());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.sandbox
            .git(self.sandbox.path(), &args)
            .map(|out| if out.is_empty() { out } else { out + "\n" })
            .map_err(|_| ReadError::Transport)
    }
}

fn observe(reader: &LocalReader<'_>) -> Result<Advertisement, Box<dyn std::error::Error>> {
    match observe_source(reader, "repo", 1, 0).outcome {
        ObservationOutcome::Complete(ad) => Ok(ad),
        other => Err(format!("observation not complete: {other:?}").into()),
    }
}

#[test]
fn argv_is_read_only_and_option_safe() -> TestResult {
    let remote = RemoteTarget::new(AllowedHost::GitHub, "octo/hello.git")?;
    assert_eq!(
        ls_remote_args(&remote),
        [
            "ls-remote",
            "--symref",
            "--",
            "https://github.com/octo/hello.git"
        ]
    );
    Ok(())
}

#[test]
fn real_git_changes_on_non_default_branch_and_annotated_tag_are_detected() -> TestResult {
    // TEST-REFS on real Git output.
    let sandbox = Sandbox::new("refs")?;
    let source = source_repo(&sandbox)?;
    let reader = LocalReader {
        sandbox: &sandbox,
        location: source.path.clone(),
    };
    let before = observe(&reader)?;
    let names: Vec<String> = before.refs.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(
        names,
        [
            "refs/heads/feature",
            "refs/heads/main",
            "refs/heads/odd/naïve-ü",
            "refs/heads/release/1.x",
            "refs/tags/light",
            "refs/tags/moving",
            "refs/tags/v1.0",
        ]
    );
    // No HEAD and no `^{}` entry in the compared map; HEAD target recorded separately.
    assert_eq!(before.head_target.as_deref(), Some("refs/heads/main"));
    assert_eq!(before.peeled.len(), 1, "only v1.0 is annotated");
    let main_before = sandbox.git(&source.path, &["rev-parse", "main"])?;

    // Change a non-default branch and move the annotated tag; main/HEAD untouched.
    sandbox.git(&source.path, &["checkout", "-q", "release/1.x"])?;
    sandbox.git(
        &source.path,
        &["commit", "-q", "--allow-empty", "-m", "fix"],
    )?;
    sandbox.git(&source.path, &["checkout", "-q", "main"])?;
    sandbox.git(
        &source.path,
        &["tag", "-f", "-a", "v1.0", "-m", "moved v1.0", "release/1.x"],
    )?;
    assert_eq!(
        sandbox.git(&source.path, &["rev-parse", "main"])?,
        main_before
    );

    let after = observe(&reader)?;
    let changed: Vec<String> = diff(&before.refs, &after.refs)?
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(changed, ["refs/heads/release/1.x", "refs/tags/v1.0"]);
    // The stored tag value is the tag object, not the peeled commit.
    let tag_object = sandbox.git(&source.path, &["rev-parse", "refs/tags/v1.0"])?;
    let v1 = after
        .refs
        .iter()
        .find(|(n, _)| n.as_str() == "refs/tags/v1.0")
        .map(|(_, oid)| oid.to_string());
    assert_eq!(v1, Some(tag_object));
    Ok(())
}

#[test]
fn real_empty_repository_observes_as_explicit_empty_and_detects_first_commit() -> TestResult {
    // TEST-EMPTY-REPO (observation half; empty manifest capture is P05-T02).
    let sandbox = Sandbox::new("empty")?;
    let bare = empty_bare(&sandbox, "empty.git")?;
    let reader = LocalReader {
        sandbox: &sandbox,
        location: bare.clone(),
    };
    let empty = observe(&reader)?;
    assert!(empty.refs.is_empty());
    assert!(empty.peeled.is_empty());

    let mut view = SourceView::default();
    view.apply(observe_source(&reader, "repo", 1, 0));
    let empty_snapshot = RefMap::default();
    assert_eq!(
        view.source_state(Some(&empty_snapshot), 0, 60_000),
        SourceState::Current
    );

    // First commit pushed by a test-local client into the local bare repo.
    let work = sandbox.path().join("first");
    fs::create_dir(&work)?;
    sandbox.git(&work, &["init", "-q"])?;
    sandbox.git(&work, &["commit", "-q", "--allow-empty", "-m", "first"])?;
    let bare_text = bare.to_string_lossy().into_owned();
    sandbox.git(&work, &["push", "-q", &bare_text, "main"])?;
    view.apply(observe_source(&reader, "repo", 2, 1));
    assert_eq!(
        view.source_state(Some(&empty_snapshot), 1, 60_000),
        SourceState::Changed
    );
    let first = view.last_complete().map(|ad| ad.refs.len());
    assert_eq!(first, Some(1));
    Ok(())
}

/// Size, mode, mtime seconds and nanoseconds of one path.
type Stat = (u64, u32, i64, i64);

/// Every path under `dir` with its size, mode and modification time.
fn tree_state(dir: &Path) -> Result<BTreeMap<PathBuf, Stat>, std::io::Error> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                stack.push(path.clone());
            }
            out.insert(
                path,
                (meta.len(), meta.mode(), meta.mtime(), meta.mtime_nsec()),
            );
        }
    }
    Ok(out)
}

fn set_tree_mode(dir: &Path, writable: bool) -> Result<(), std::io::Error> {
    let paths: Vec<PathBuf> = tree_state(dir)?
        .into_keys()
        .chain([dir.to_path_buf()])
        .collect();
    for path in paths.iter().rev() {
        let meta = fs::symlink_metadata(path)?;
        let base = if meta.is_dir() { 0o555 } else { 0o444 };
        let mode = if writable { base | 0o200 } else { base };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[test]
fn observing_a_read_only_source_writes_nothing() -> TestResult {
    // TEST-READONLY-SOURCE (observation part): a source that cannot accept any write
    // is fully observable, and its files are byte-for-byte unchanged afterwards.
    let sandbox = Sandbox::new("readonly")?;
    let source = source_repo(&sandbox)?;
    let bare = sandbox.path().join("source-ro.git");
    let source_text = source.path.to_string_lossy().into_owned();
    let bare_text = bare.to_string_lossy().into_owned();
    sandbox.git(
        sandbox.path(),
        &["clone", "-q", "--bare", "--", &source_text, &bare_text],
    )?;
    set_tree_mode(&bare, false)?;
    let before = tree_state(&bare)?;

    let reader = LocalReader {
        sandbox: &sandbox,
        location: bare.clone(),
    };
    let ad = observe(&reader)?;
    assert_eq!(ad.refs.len(), 7);
    // A push to this source is refused by the filesystem, proving it is write-proof.
    let work = sandbox.path().join("pusher");
    let work_text = work.to_string_lossy().into_owned();
    sandbox.git(
        sandbox.path(),
        &["clone", "-q", "--", &bare_text, &work_text],
    )?;
    sandbox.git(&work, &["commit", "-q", "--allow-empty", "-m", "x"])?;
    assert!(
        sandbox
            .git(&work, &["push", "-q", "origin", "main"])
            .is_err()
    );

    assert_eq!(tree_state(&bare)?, before, "source tree unchanged");
    set_tree_mode(&bare, true)?;
    Ok(())
}

#[test]
fn real_advertisement_parses_identically_twice() -> TestResult {
    // Determinism guard: the same source yields the same map (no ordering artefacts).
    let sandbox = Sandbox::new("stable")?;
    let source = source_repo(&sandbox)?;
    let args = args_for(&source.path.to_string_lossy());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    // The sandbox trims the final newline that Git always writes; restore it.
    let a = parse_ls_remote(&(sandbox.git(sandbox.path(), &args)? + "\n"))?;
    let b = parse_ls_remote(&(sandbox.git(sandbox.path(), &args)? + "\n"))?;
    assert_eq!(a, b);
    Ok(())
}

#[test]
fn real_v0_and_v2_output_with_symrefs_and_foreign_annotated_refs_parses() -> TestResult {
    // Review M1/M2/L2: a symbolic branch, a remote-tracking symbolic HEAD, an annotated
    // tag object under a custom namespace and a detached HEAD at an annotated tag must
    // all parse under both wire protocols, with only heads/tags compared.
    let sandbox = Sandbox::new("protocols")?;
    let source = source_repo(&sandbox)?;
    let dir = &source.path;
    let tag_object = sandbox.git(dir, &["rev-parse", "refs/tags/v1.0"])?;
    sandbox.git(dir, &["update-ref", "refs/custom/x", &tag_object])?;
    sandbox.git(
        dir,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
    )?;
    sandbox.git(dir, &["update-ref", "refs/remotes/origin/main", "main"])?;
    sandbox.git(
        dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    )?;
    let location = dir.to_string_lossy().into_owned();
    let mut maps = Vec::new();
    for version in ["protocol.version=0", "protocol.version=2"] {
        for detached in [false, true] {
            if detached {
                sandbox.git(dir, &["checkout", "-q", "--detach", "v1.0"])?;
            }
            let out = sandbox.git(
                sandbox.path(),
                &["-c", version, "ls-remote", "--symref", "--", &location],
            )?;
            let ad = parse_ls_remote(&(out + "\n"))
                .map_err(|e| format!("{version} detached={detached}: {e}"))?;
            assert_eq!(ad.refs.len(), 8, "{version}: 7 fixture refs + alias");
            assert!(ad.refs.iter().all(|(n, _)| !n.as_str().contains("custom")));
            maps.push(ad.refs);
            if detached {
                sandbox.git(dir, &["checkout", "-q", "main"])?;
            }
        }
    }
    assert!(maps.windows(2).all(|w| w[0] == w[1]), "same map every way");
    Ok(())
}
