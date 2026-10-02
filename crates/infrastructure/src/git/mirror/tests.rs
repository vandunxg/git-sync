//! TEST-REFS / TEST-EMPTY-REPO / TEST-VERIFY with real Git: local fixture sources, an
//! app-owned root established by `storage`, the mirror created by `OwnedRoot`, and the
//! application capture use case driving `GitMirror` + `RunnerRefReader`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use gbm_application::observation::SourceRefReader;
use gbm_application::snapshot::{
    CaptureError, CaptureRequest, MirrorError, ObservationClock, SourceRecheck, capture_snapshot,
};
use gbm_domain::identity::{GitHubRepoId, mirror_relative_path};
use gbm_domain::observation::UnavailableReason;
use gbm_domain::refs::{ObjectId, RefName, parse_ls_remote};
use gbm_test_fixtures::{Sandbox, empty_bare, source_repo};

use super::*;
use crate::filesystem::OwnedRoot;
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

/// A private scratch area with an established backup root and runner HOME.
struct Env {
    base: PathBuf,
    root: OwnedRoot,
    home: PathBuf,
}

impl Env {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let base =
            std::env::temp_dir().join(format!("gbm-mirror-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("root"))?;
        let home = base.join("home");
        fs::create_dir(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        let candidate = validate_candidate(&base.join("root"))?;
        let established = establish(&candidate, "root-1", "2026-10-02T00:00:00Z")?;
        let root = OwnedRoot::verify(established.root().path(), "root-1")?;
        Ok(Self {
            base: fs::canonicalize(&base)?,
            root,
            home: fs::canonicalize(home)?,
        })
    }

    fn runner(&self) -> Result<GitRunner, Box<dyn std::error::Error>> {
        Ok(GitRunner::new(GitBinary::locate()?, &self.home)?)
    }

    fn mirror_dir(&self, id: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Ok(self
            .root
            .create_dir(&mirror_relative_path(GitHubRepoId::parse(id)?))?)
    }

    fn adapters(
        &self,
        mirror: &Path,
        source: &Path,
    ) -> Result<(RunnerRefReader, GitMirror), Box<dyn std::error::Error>> {
        self.adapters_with(mirror, source, CancelToken::default())
    }

    fn adapters_with(
        &self,
        mirror: &Path,
        source: &Path,
        cancel: CancelToken,
    ) -> Result<(RunnerRefReader, GitMirror), Box<dyn std::error::Error>> {
        let location = Location::local(&fs::canonicalize(source)?)?;
        let reader = RunnerRefReader::new(
            self.runner()?,
            location.clone(),
            None,
            RunLimits::default(),
            cancel.clone(),
        );
        let mirror = GitMirror::new(
            self.runner()?,
            mirror.to_path_buf(),
            location,
            None,
            RunLimits::default(),
            cancel,
        );
        Ok((reader, mirror))
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn request<'a>(job: &'a str, snapshot: &'a str) -> CaptureRequest<'a> {
    CaptureRequest {
        repository_id: "repo-1",
        job_id: job,
        snapshot_id: snapshot,
    }
}

fn ls_remote(
    sandbox: &Sandbox,
    path: &Path,
) -> Result<gbm_domain::refs::Advertisement, Box<dyn std::error::Error>> {
    let text = sandbox.git(
        sandbox.path(),
        &["ls-remote", "--symref", "--", &path.to_string_lossy()],
    )?;
    Ok(parse_ls_remote(&(text + "\n"))?)
}

#[test]
fn captures_all_refs_of_a_real_repository_into_an_owned_mirror() -> TestResult {
    // TEST-REFS (capture): every head/tag with exact OIDs, annotated tag objects kept.
    let sandbox = Sandbox::new("mirror-capture")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("capture")?;
    let dir = env.mirror_dir("101")?;
    let (reader, mirror) = env.adapters(&dir, &source.path)?;
    let out = capture_snapshot(
        &reader,
        &mirror,
        &Clock(AtomicU64::new(0)),
        &request("j1", "s1"),
    )?;
    let expected = ls_remote(&sandbox, &source.path)?;
    assert_eq!(out.manifest.refs, expected.refs);
    assert_eq!(out.recheck, SourceRecheck::Unchanged);
    // Snapshot namespace in the real mirror equals the manifest; no staging remains.
    let snapshot = mirror.read_refs("refs/gbm/snapshots/s1/")?;
    assert_eq!(snapshot.len(), 7);
    assert!(mirror.read_refs("refs/gbm/staging/")?.is_empty());
    let tag = RefName::parse("refs/tags/v1.0").ok_or("name")?;
    let tag_object = sandbox.git(&source.path, &["rev-parse", "refs/tags/v1.0"])?;
    assert_eq!(
        out.manifest.refs.get(&tag).map(ToString::to_string),
        Some(tag_object.clone())
    );
    assert!(
        snapshot
            .iter()
            .any(|(n, o)| n == "refs/gbm/snapshots/s1/tags/v1.0" && o.as_str() == tag_object)
    );
    // The mirror is a valid bare repository with all objects.
    let mirror_text = dir.to_string_lossy().into_owned();
    sandbox.git(
        sandbox.path(),
        &["--git-dir", &mirror_text, "fsck", "--no-dangling"],
    )?;
    Ok(())
}

#[test]
fn second_capture_keeps_the_first_snapshot_and_sees_new_changes() -> TestResult {
    let sandbox = Sandbox::new("mirror-two")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("two")?;
    let dir = env.mirror_dir("102")?;
    let (reader, mirror) = env.adapters(&dir, &source.path)?;
    let clock = Clock(AtomicU64::new(0));
    let first = capture_snapshot(&reader, &mirror, &clock, &request("j1", "s1"))?;
    sandbox.git(&source.path, &["checkout", "-q", "release/1.x"])?;
    sandbox.git(
        &source.path,
        &["commit", "-q", "--allow-empty", "-m", "fix"],
    )?;
    sandbox.git(&source.path, &["checkout", "-q", "main"])?;
    sandbox.git(&source.path, &["tag", "-d", "light"])?;
    let second = capture_snapshot(&reader, &mirror, &clock, &request("j2", "s2"))?;
    assert_ne!(
        first.manifest.ref_set_digest,
        second.manifest.ref_set_digest
    );
    assert_eq!(second.manifest.refs.len(), 6);
    // The first snapshot is intact, including the tag deleted at the source.
    let s1 = mirror.read_refs("refs/gbm/snapshots/s1/")?;
    assert_eq!(s1.len(), 7);
    assert!(s1.iter().any(|(n, _)| n.ends_with("/tags/light")));
    // Reusing a snapshot id is refused.
    assert_eq!(
        capture_snapshot(&reader, &mirror, &clock, &request("j3", "s1")),
        Err(CaptureError::SnapshotExists)
    );
    Ok(())
}

#[test]
fn empty_source_creates_no_mirror_content_and_first_commit_is_captured() -> TestResult {
    // TEST-EMPTY-REPO: explicit empty manifest; nothing written, no fake HEAD.
    let sandbox = Sandbox::new("mirror-empty")?;
    let bare = empty_bare(&sandbox, "empty.git")?;
    let env = Env::new("empty")?;
    let dir = env.mirror_dir("103")?;
    let (reader, mirror) = env.adapters(&dir, &bare)?;
    let clock = Clock(AtomicU64::new(0));
    let out = capture_snapshot(&reader, &mirror, &clock, &request("j1", "s1"))?;
    assert!(out.manifest.refs.is_empty());
    assert_eq!(out.manifest.ref_namespace, None);
    assert_eq!(fs::read_dir(&dir)?.count(), 0, "mirror untouched");

    let work = sandbox.path().join("first");
    fs::create_dir(&work)?;
    sandbox.git(&work, &["init", "-q"])?;
    sandbox.git(&work, &["commit", "-q", "--allow-empty", "-m", "first"])?;
    sandbox.git(&work, &["push", "-q", &bare.to_string_lossy(), "main"])?;
    let next = capture_snapshot(&reader, &mirror, &clock, &request("j2", "s2"))?;
    assert_eq!(next.manifest.refs.len(), 1);
    assert_eq!(mirror.read_refs("refs/gbm/snapshots/s2/")?.len(), 1);
    Ok(())
}

/// Reader that commits to the source right after its first listing, so the following
/// fetch sees newer tips than the first observation.
struct MovingReader<'a> {
    inner: RunnerRefReader,
    sandbox: &'a Sandbox,
    source: PathBuf,
    moved: AtomicBool,
}

impl SourceRefReader for MovingReader<'_> {
    fn read_advertisement(&self, id: &str) -> Result<String, ReadError> {
        let text = self.inner.read_advertisement(id);
        if !self.moved.swap(true, Ordering::SeqCst) {
            let _ = self.sandbox.git(
                &self.source,
                &["commit", "-q", "--allow-empty", "-m", "mid"],
            );
        }
        text
    }
}

#[test]
fn a_real_source_moving_mid_capture_is_recorded_coherently() -> TestResult {
    // TEST-VERIFY: the staged map differs from observation 1; it is accepted only
    // because a fresh complete observation equals it.
    let sandbox = Sandbox::new("mirror-moving")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("moving")?;
    let dir = env.mirror_dir("104")?;
    let (inner, mirror) = env.adapters(&dir, &source.path)?;
    let reader = MovingReader {
        inner,
        sandbox: &sandbox,
        source: source.path.clone(),
        moved: AtomicBool::new(false),
    };
    let out = capture_snapshot(
        &reader,
        &mirror,
        &Clock(AtomicU64::new(0)),
        &request("j1", "s1"),
    )?;
    let main = RefName::parse("refs/heads/main").ok_or("name")?;
    let tip = sandbox.git(&source.path, &["rev-parse", "main"])?;
    assert_ne!(tip, source.main_tip, "the source really moved");
    assert_eq!(
        out.manifest.refs.get(&main).map(ToString::to_string),
        Some(tip)
    );
    assert_eq!(out.manifest.refs, ls_remote(&sandbox, &source.path)?.refs);
    Ok(())
}

#[test]
fn object_format_mismatch_and_missing_source_are_explicit() -> TestResult {
    let sandbox = Sandbox::new("mirror-format")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("format")?;
    let dir = env.mirror_dir("105")?;
    let (reader, mirror) = env.adapters(&dir, &source.path)?;
    capture_snapshot(
        &reader,
        &mirror,
        &Clock(AtomicU64::new(0)),
        &request("j1", "s1"),
    )?;
    // A SHA-256 source cannot be captured into the existing SHA-1 mirror.
    let sha256 = sandbox.path().join("sha256");
    fs::create_dir(&sha256)?;
    sandbox.git(&sha256, &["init", "-q", "--object-format=sha256"])?;
    sandbox.git(&sha256, &["commit", "-q", "--allow-empty", "-m", "x"])?;
    let (reader256, mirror256) = env.adapters(&dir, &sha256)?;
    assert_eq!(
        capture_snapshot(
            &reader256,
            &mirror256,
            &Clock(AtomicU64::new(10)),
            &request("j2", "s2")
        ),
        Err(CaptureError::Mirror(MirrorError::ObjectFormatMismatch))
    );
    // A folder that is not a repository is "not found", never "empty".
    let not_repo = sandbox.path().join("not-a-repo");
    fs::create_dir(&not_repo)?;
    let (missing, mirror_missing) = env.adapters(&dir, &not_repo)?;
    assert_eq!(
        capture_snapshot(
            &missing,
            &mirror_missing,
            &Clock(AtomicU64::new(20)),
            &request("j3", "s3")
        ),
        Err(CaptureError::SourceUnavailable(UnavailableReason::NotFound))
    );
    Ok(())
}

#[test]
fn more_than_one_fetch_batch_of_refs_is_captured_and_promoted_completely() -> TestResult {
    // Review #6: >200 refs exercise the batched fetch and batched atomic copy.
    let sandbox = Sandbox::new("mirror-many")?;
    let source = source_repo(&sandbox)?;
    for i in 0..230 {
        sandbox.git(
            &source.path,
            &["update-ref", &format!("refs/heads/bulk/b{i:03}"), "main"],
        )?;
    }
    let env = Env::new("many")?;
    let dir = env.mirror_dir("106")?;
    let (reader, mirror) = env.adapters(&dir, &source.path)?;
    let out = capture_snapshot(
        &reader,
        &mirror,
        &Clock(AtomicU64::new(0)),
        &request("j1", "s1"),
    )?;
    assert_eq!(out.manifest.refs.len(), 237);
    assert_eq!(out.manifest.refs, ls_remote(&sandbox, &source.path)?.refs);
    assert_eq!(mirror.read_refs("refs/gbm/snapshots/s1/")?.len(), 237);
    assert!(mirror.read_refs("refs/gbm/staging/")?.is_empty());
    assert!(out.staging_removed);
    Ok(())
}

#[test]
fn a_corrupted_mirror_blocks_promotion() -> TestResult {
    // Review #6: real fsck failure -> IntegrityFailed, no new snapshot namespace.
    let sandbox = Sandbox::new("mirror-corrupt")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("corrupt")?;
    let dir = env.mirror_dir("107")?;
    let (reader, mirror) = env.adapters(&dir, &source.path)?;
    let clock = Clock(AtomicU64::new(0));
    capture_snapshot(&reader, &mirror, &clock, &request("j1", "s1"))?;
    // A garbage loose object (not valid zlib) inside the mirror's object store.
    let objects = dir.join("objects").join("ab");
    fs::create_dir_all(&objects)?;
    fs::write(objects.join("c".repeat(38)), b"not a git object")?;
    sandbox.git(
        &source.path,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    )?;
    assert_eq!(
        capture_snapshot(&reader, &mirror, &clock, &request("j2", "s2")),
        Err(CaptureError::IntegrityFailed)
    );
    assert!(mirror.read_refs("refs/gbm/snapshots/s2/")?.is_empty());
    assert!(mirror.read_refs("refs/gbm/staging/")?.is_empty());
    assert_eq!(
        mirror.read_refs("refs/gbm/snapshots/s1/")?.len(),
        7,
        "old snapshot kept"
    );
    Ok(())
}

#[test]
fn cancelled_jobs_report_cancelled_and_deletions_are_exact() -> TestResult {
    let sandbox = Sandbox::new("mirror-cancel")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("cancel")?;
    let dir = env.mirror_dir("108")?;
    let cancel = CancelToken::default();
    cancel.cancel();
    let (reader, mirror) = env.adapters_with(&dir, &source.path, cancel)?;
    assert_eq!(
        capture_snapshot(
            &reader,
            &mirror,
            &Clock(AtomicU64::new(0)),
            &request("j1", "s1")
        ),
        Err(CaptureError::Cancelled)
    );
    // Deletion requires the expected OID and an app namespace.
    let (reader, mirror) = env.adapters(&dir, &source.path)?;
    capture_snapshot(
        &reader,
        &mirror,
        &Clock(AtomicU64::new(0)),
        &request("j2", "s2"),
    )?;
    let refs = mirror.read_refs("refs/gbm/snapshots/s2/")?;
    let (name, _) = refs.first().ok_or("no ref")?;
    let wrong = ObjectId::parse(&"1".repeat(40)).ok_or("oid")?;
    assert!(
        mirror
            .delete_refs(&[(name.clone(), wrong.clone())])
            .is_err()
    );
    assert_eq!(
        mirror.read_refs("refs/gbm/snapshots/s2/")?.len(),
        7,
        "wrong OID refused"
    );
    let _ = wrong;
    // A real ref outside the app namespaces, with its correct OID, is still refused.
    let (_, oid) = refs.first().ok_or("no ref")?;
    let mirror_text = dir.to_string_lossy().into_owned();
    sandbox.git(
        sandbox.path(),
        &[
            "--git-dir",
            &mirror_text,
            "update-ref",
            "refs/heads/keep",
            oid.as_str(),
        ],
    )?;
    assert!(
        mirror
            .delete_refs(&[("refs/heads/keep".to_owned(), oid.clone())])
            .is_err()
    );
    let kept = sandbox.git(
        sandbox.path(),
        &["--git-dir", &mirror_text, "rev-parse", "refs/heads/keep"],
    )?;
    assert_eq!(kept, oid.as_str());
    assert!(mirror.remove_namespace("refs/heads/").is_err());
    assert!(
        mirror.remove_namespace("refs/gbm/snapshots/").is_err(),
        "all snapshots"
    );
    assert_eq!(mirror.read_refs("refs/gbm/snapshots/s2/")?.len(), 7);
    Ok(())
}
