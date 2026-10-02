//! TEST-BUNDLE-RESTORE and TEST-SNAPSHOT with real Git: capture a fixture source into
//! an owned mirror (P05-T02 pieces), write a recovery bundle, then restore it with the
//! source gone, corrupt it, and gate destructive plans on it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};

use gbm_application::recovery::{ChangeKind, GateError, PlannedChange, require_recovery};
use gbm_application::snapshot::{
    CaptureRequest, ObservationClock, SnapshotManifest, capture_snapshot,
};
use gbm_domain::identity::mirror_relative_path;
use gbm_test_fixtures::{Sandbox, empty_bare, source_repo};

use super::*;
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

struct Env {
    base: PathBuf,
    root: OwnedRoot,
    home: PathBuf,
    mirror: PathBuf,
    repo: GitHubRepoId,
}

impl Env {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let base =
            std::env::temp_dir().join(format!("gbm-snap-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("root"))?;
        fs::create_dir_all(base.join("restore"))?;
        let home = base.join("home");
        fs::create_dir(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        let established = establish(
            &validate_candidate(&base.join("root"))?,
            "root-1",
            "2026-10-02T00:00:00Z",
        )?;
        let root = OwnedRoot::verify(established.root().path(), "root-1")?;
        let repo = GitHubRepoId::parse("4242")?;
        let mirror = root.create_dir(&mirror_relative_path(repo))?;
        Ok(Self {
            base: fs::canonicalize(&base)?,
            root,
            home: fs::canonicalize(home)?,
            mirror,
            repo,
        })
    }

    fn runner(&self) -> Result<GitRunner, Box<dyn std::error::Error>> {
        Ok(GitRunner::new(GitBinary::locate()?, &self.home)?)
    }

    fn store(&self) -> Result<BundleStore, Box<dyn std::error::Error>> {
        Ok(BundleStore::new(
            self.runner()?,
            self.root.clone(),
            self.mirror.clone(),
            self.repo,
            RunLimits::default(),
            CancelToken::default(),
        ))
    }

    /// Captures `source` into the mirror as snapshot namespace `capture`.
    fn capture(
        &self,
        source: &Path,
        capture: &str,
    ) -> Result<SnapshotManifest, Box<dyn std::error::Error>> {
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
        let request = CaptureRequest {
            repository_id: "repo-1",
            job_id: "job-1",
            snapshot_id: capture,
        };
        Ok(capture_snapshot(&reader, &mirror, &Clock(AtomicU64::new(0)), &request)?.manifest)
    }

    fn fresh_restore_dir(&self, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dir = self.base.join("restore").join(name);
        fs::create_dir(&dir)?;
        Ok(fs::canonicalize(dir)?)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn recovery_request(
    manifest: &SnapshotManifest,
    id: &str,
) -> Result<RecoveryRequest, Box<dyn std::error::Error>> {
    Ok(RecoveryRequest {
        repository_id: "repo-1".to_owned(),
        snapshot_id: id.to_owned(),
        namespace: manifest.ref_namespace.clone().ok_or("no namespace")?,
        refs: manifest.refs.clone(),
        reason: "before rewrite of refs/heads/feature".to_owned(),
        created_at_ms: 1_000,
    })
}

#[test]
fn a_bundle_restores_every_ref_with_the_source_gone() -> TestResult {
    // TEST-BUNDLE-RESTORE.
    let sandbox = Sandbox::new("snap-restore")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("restore")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    let snapshot = store.create(&recovery_request(&captured, "r1")?)?;
    assert_eq!(snapshot.refs, captured.refs);
    assert!(snapshot.bundle_sha256.is_some() && snapshot.bundle_bytes > 0);

    // Source unavailable: move it away entirely.
    fs::rename(&source.path, sandbox.path().join("gone"))?;
    let dest = env.fresh_restore_dir("r1")?;
    let report = store.restore("r1", &dest)?;
    assert_eq!(
        report.refs, captured.refs,
        "all branches and tags with exact OIDs"
    );
    // Annotated tag object and a non-default branch are present as tag/commit objects.
    let dest_text = dest.to_string_lossy().into_owned();
    let kind = sandbox.git(
        sandbox.path(),
        &["--git-dir", &dest_text, "cat-file", "-t", "refs/tags/v1.0"],
    )?;
    assert_eq!(kind, "tag");
    let feature = sandbox.git(
        sandbox.path(),
        &["--git-dir", &dest_text, "rev-parse", "refs/heads/feature"],
    )?;
    assert_eq!(feature, source.feature_tip_after_rewrite);
    // No remote is configured: nothing could be fetched from the source later.
    let remotes = sandbox.git(
        sandbox.path(),
        &[
            "--git-dir",
            &dest_text,
            "config",
            "--get-regexp",
            "^remote\\.",
        ],
    );
    assert!(remotes.map(|t| t.is_empty()).unwrap_or(true));
    sandbox.git(
        sandbox.path(),
        &["--git-dir", &dest_text, "fsck", "--full", "--strict"],
    )?;
    Ok(())
}

#[test]
fn snapshot_directory_holds_a_complete_manifest_and_no_staging() -> TestResult {
    let sandbox = Sandbox::new("snap-manifest")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("manifest")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    let dir = env.root.open_dir(&store.relative("r1")?)?;
    let manifest: Value = serde_json::from_str(&fs::read_to_string(dir.join(MANIFEST_FILE))?)?;
    assert_eq!(manifest["coverage"]["lfs"], "not_covered");
    assert_eq!(manifest["coverage"]["submodules"], "not_covered");
    assert_eq!(manifest["refs"].as_array().map(Vec::len), Some(7));
    assert_eq!(manifest["objectFormat"], "sha1");
    let text = manifest.to_string();
    assert!(!text.contains("glpat") && !text.contains("ghp_") && !text.contains("token"));
    let names: Vec<String> = fs::read_dir(dir.parent().ok_or("parent")?)?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(names, ["r1"], "no staging or verify leftovers: {names:?}");
    assert_eq!(
        fs::metadata(dir.join(MANIFEST_FILE))?.permissions().mode() & 0o777,
        0o600
    );
    // Append-only: the same id is refused.
    assert_eq!(
        store.create(&recovery_request(&captured, "r1")?),
        Err(StoreError::Exists)
    );
    Ok(())
}

#[test]
fn corruption_is_detected_and_blocks_destructive_plans() -> TestResult {
    // TEST-SNAPSHOT with real storage: good -> allowed; corrupt -> blocked.
    let sandbox = Sandbox::new("snap-corrupt")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("corrupt")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    let feature = RefName::parse("refs/heads/feature").ok_or("name")?;
    let old = captured.refs.get(&feature).cloned().ok_or("feature")?;
    let plan = vec![PlannedChange {
        name: feature,
        old: Some(old),
        new: ObjectId::parse(&source.feature_tip_before_rewrite),
        kind: ChangeKind::Rewrite,
    }];
    assert!(require_recovery(&plan, &store, Some("r1"))?.is_some());

    // Flip one byte in the middle of the bundle: checksum mismatch.
    let bundle = env.root.open_dir(&store.relative("r1")?)?.join(BUNDLE_FILE);
    let mut bytes = fs::read(&bundle)?;
    let middle = bytes.len() / 2;
    if let Some(byte) = bytes.get_mut(middle) {
        *byte ^= 0xff;
    }
    fs::write(&bundle, &bytes)?;
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    assert_eq!(
        require_recovery(&plan, &store, Some("r1")),
        Err(GateError::RecoveryUnverified(StoreError::Corrupt))
    );
    assert_eq!(store.verify("missing"), Err(StoreError::NotFound));
    Ok(())
}

#[test]
fn a_tampered_manifest_cannot_vouch_for_a_bundle() -> TestResult {
    let sandbox = Sandbox::new("snap-tamper")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("tamper")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    let dir = env.root.open_dir(&store.relative("r1")?)?;
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    // Claim the old tag pointed elsewhere: heads/restore no longer match the manifest.
    if let Some(refs) = manifest["refs"].as_array_mut()
        && let Some(entry) = refs.iter_mut().find(|e| e["name"] == "refs/tags/moving")
    {
        entry["oid"] = Value::String(source.moved_tag_before.clone());
    }
    fs::write(&path, manifest.to_string())?;
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    Ok(())
}

#[test]
fn failed_writes_leave_nothing_and_empty_snapshots_are_explicit() -> TestResult {
    let sandbox = Sandbox::new("snap-fail")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("fail")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    // A namespace that holds no refs: the bundle cannot be created -> error, no dir.
    let mut bad = recovery_request(&captured, "r2")?;
    bad.namespace = "refs/gbm/snapshots/nothing-here/".to_owned();
    assert!(store.create(&bad).is_err());
    let parent = env.root.open_dir(&RelativePath::parse(
        &snapshots_relative_dir(env.repo).to_string(),
    )?)?;
    assert_eq!(
        fs::read_dir(&parent)?.count(),
        0,
        "no staging/verify leftovers"
    );
    // Invalid requests are refused before touching storage.
    let mut outside = recovery_request(&captured, "r3")?;
    outside.namespace = "refs/heads/".to_owned();
    assert_eq!(store.create(&outside), Err(StoreError::InvalidRequest));
    let mut traversal = recovery_request(&captured, "../r4")?;
    traversal.snapshot_id = "../r4".to_owned();
    assert_eq!(store.create(&traversal), Err(StoreError::InvalidRequest));

    // Empty repository: explicit manifest without a bundle; restore yields no refs.
    let empty_source = empty_bare(&sandbox, "empty.git")?;
    let env2 = Env::new("fail-empty")?;
    let empty = env2.capture(&empty_source, "c0")?;
    let store2 = env2.store()?;
    let request = RecoveryRequest {
        namespace: "refs/gbm/snapshots/c0/".to_owned(),
        ..recovery_request(
            &SnapshotManifest {
                ref_namespace: Some("refs/gbm/snapshots/c0/".to_owned()),
                ..empty
            },
            "e1",
        )?
    };
    let snapshot = store2.create(&request)?;
    assert_eq!(snapshot.bundle_sha256, None);
    assert!(
        !env2
            .root
            .open_dir(&store2.relative("e1")?)?
            .join(BUNDLE_FILE)
            .exists()
    );
    assert_eq!(store2.verify("e1")?.refs, RefMap::default());
    let report = store2.restore("e1", &env2.fresh_restore_dir("e1")?)?;
    assert!(report.refs.is_empty() && report.object_format.is_none());
    Ok(())
}

#[test]
fn restore_refuses_non_empty_or_backup_root_destinations() -> TestResult {
    let sandbox = Sandbox::new("snap-dest")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("dest")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    let busy = env.fresh_restore_dir("busy")?;
    fs::write(busy.join("keep.txt"), "user data")?;
    assert_eq!(store.restore("r1", &busy), Err(StoreError::InvalidRequest));
    assert_eq!(fs::read_to_string(busy.join("keep.txt"))?, "user data");
    let inside = env.root.create_dir(&RelativePath::parse("restore-here")?)?;
    assert_eq!(
        store.restore("r1", &inside),
        Err(StoreError::InvalidRequest)
    );
    Ok(())
}

#[test]
fn write_failures_are_classified() {
    assert_eq!(
        write_failure("fatal: write error: No space left on device"),
        StoreError::NoSpace
    );
    assert_eq!(write_failure("fatal: something else"), StoreError::Failed);
    assert_eq!(
        io_failure(&std::io::Error::from_raw_os_error(28)),
        StoreError::NoSpace
    );
}

#[test]
fn trailing_bytes_git_may_ignore_still_fail_the_checksum() -> TestResult {
    let sandbox = Sandbox::new("snap-append")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("append")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    let bundle = env.root.open_dir(&store.relative("r1")?)?.join(BUNDLE_FILE);
    let mut file = OpenOptions::new().append(true).open(&bundle)?;
    file.write_all(b"appended")?;
    drop(file);
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    Ok(())
}

#[test]
fn a_valid_but_different_bundle_fails_the_recorded_checksum() -> TestResult {
    // Only the manifest checksum distinguishes this: Git accepts the replacement (same
    // refs and objects, v3 header) and a restore from it would succeed.
    let sandbox = Sandbox::new("snap-swap")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("swap")?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    let bundle = env.root.open_dir(&store.relative("r1")?)?.join(BUNDLE_FILE);
    let before = fs::read(&bundle)?;
    let mirror = env.mirror.to_string_lossy().into_owned();
    let bundle_text = bundle.to_string_lossy().into_owned();
    fs::remove_file(&bundle)?;
    sandbox.git(
        sandbox.path(),
        &[
            "--git-dir",
            &mirror,
            "bundle",
            "create",
            "--version=3",
            &bundle_text,
            "--glob=refs/gbm/snapshots/c1/*",
        ],
    )?;
    assert_ne!(fs::read(&bundle)?, before, "replacement differs");
    sandbox.git(
        sandbox.path(),
        &["--git-dir", &mirror, "bundle", "verify", &bundle_text],
    )?;
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    Ok(())
}

/// Replaces the snapshot's bundle via `make` (given mirror and bundle paths), then
/// rewrites the manifest's sha256/bytes to match, so only the bundle *content* checks
/// (heads, restore, fsck) can catch the problem.
fn swap_bundle_and_fix_checksum(
    env: &Env,
    store: &BundleStore,
    make: impl FnOnce(&str, &Path) -> TestResult,
) -> TestResult {
    let dir = env.root.open_dir(&store.relative("r1")?)?;
    let bundle = dir.join(BUNDLE_FILE);
    fs::remove_file(&bundle)?;
    make(&env.mirror.to_string_lossy(), &bundle)?;
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    manifest["bundle"]["sha256"] = Value::String(sha256_hex(File::open(&bundle)?)?);
    manifest["bundle"]["bytes"] = Value::from(fs::metadata(&bundle)?.len());
    fs::write(&path, manifest.to_string())?;
    Ok(())
}

fn prepared(
    label: &str,
) -> Result<(Sandbox, Env, BundleStore, SnapshotManifest), Box<dyn std::error::Error>> {
    let sandbox = Sandbox::new(label)?;
    let source = source_repo(&sandbox)?;
    let env = Env::new(label)?;
    let captured = env.capture(&source.path, "c1")?;
    let store = env.store()?;
    store.create(&recovery_request(&captured, "r1")?)?;
    Ok((sandbox, env, store, captured))
}

#[test]
fn a_thin_bundle_with_matching_heads_and_checksum_fails_the_restore() -> TestResult {
    // Review 1(a): heads identical, checksum rewritten; only the fresh restore (missing
    // prerequisite objects) can reject it.
    let (sandbox, env, store, captured) = prepared("snap-thin")?;
    let root_commit = sandbox.git(
        sandbox.path(),
        &[
            "--git-dir",
            &env.mirror.to_string_lossy(),
            "rev-list",
            "--max-parents=0",
            "refs/gbm/snapshots/c1/heads/main",
        ],
    )?;
    swap_bundle_and_fix_checksum(&env, &store, |mirror, bundle| {
        sandbox.git(
            sandbox.path(),
            &[
                "--git-dir",
                mirror,
                "bundle",
                "create",
                &bundle.to_string_lossy(),
                "--glob=refs/gbm/snapshots/c1/*",
                &format!("^{root_commit}"),
            ],
        )?;
        Ok(())
    })?;
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    let feature = RefName::parse("refs/heads/feature").ok_or("name")?;
    let plan = vec![PlannedChange {
        old: captured.refs.get(&feature).cloned(),
        new: ObjectId::parse(&"9".repeat(40)),
        name: feature,
        kind: ChangeKind::Rewrite,
    }];
    assert_eq!(
        require_recovery(&plan, &store, Some("r1")),
        Err(GateError::RecoveryUnverified(StoreError::Corrupt))
    );
    Ok(())
}

#[test]
fn a_bundle_with_an_extra_ref_and_matching_checksum_is_rejected() -> TestResult {
    // Review 1(b).
    let (sandbox, env, store, _) = prepared("snap-extra")?;
    let mirror = env.mirror.to_string_lossy().into_owned();
    sandbox.git(
        sandbox.path(),
        &[
            "--git-dir",
            &mirror,
            "update-ref",
            "refs/gbm/snapshots/c1/heads/extra",
            "refs/gbm/snapshots/c1/heads/main",
        ],
    )?;
    swap_bundle_and_fix_checksum(&env, &store, |mirror, bundle| {
        sandbox.git(
            sandbox.path(),
            &[
                "--git-dir",
                mirror,
                "bundle",
                "create",
                &bundle.to_string_lossy(),
                "--glob=refs/gbm/snapshots/c1/*",
            ],
        )?;
        Ok(())
    })?;
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    Ok(())
}

#[test]
fn a_truncated_pack_with_matching_checksum_fails_fetch_or_fsck() -> TestResult {
    // Review 1(c): header (heads) intact, pack data cut short, checksum rewritten.
    let (sandbox, env, store, _) = prepared("snap-trunc")?;
    swap_bundle_and_fix_checksum(&env, &store, |mirror, bundle| {
        sandbox.git(
            sandbox.path(),
            &[
                "--git-dir",
                mirror,
                "bundle",
                "create",
                &bundle.to_string_lossy(),
                "--glob=refs/gbm/snapshots/c1/*",
            ],
        )?;
        let bytes = fs::read(bundle)?;
        fs::write(bundle, bytes.get(..bytes.len() * 3 / 4).ok_or("cut")?)?;
        Ok(())
    })?;
    assert_eq!(store.verify("r1"), Err(StoreError::Corrupt));
    Ok(())
}

#[test]
fn manifests_from_other_repositories_kinds_or_shapes_are_rejected() -> TestResult {
    // Review 1(d) and 4.
    let (_sandbox, env, store, _) = prepared("snap-shape")?;
    let path = env
        .root
        .open_dir(&store.relative("r1")?)?
        .join(MANIFEST_FILE);
    let original = fs::read_to_string(&path)?;
    let edits: [(&str, Value); 6] = [
        ("bundle", Value::Null),
        ("sourceRepositoryId", Value::String("1".to_owned())),
        ("kind", Value::String("capture".to_owned())),
        ("manifestVersion", Value::from(2)),
        (
            "bundleRefNamespace",
            Value::String("refs/heads/".to_owned()),
        ),
        ("refs", Value::Array(Vec::new())),
    ];
    for (key, value) in edits {
        let mut manifest: Value = serde_json::from_str(&original)?;
        manifest[key] = value;
        if key == "bundle" {
            manifest["refs"] = Value::Array(Vec::new());
            manifest["objectFormat"] = Value::Null;
        }
        fs::write(&path, manifest.to_string())?;
        assert_eq!(store.verify("r1"), Err(StoreError::Corrupt), "{key}");
    }
    fs::write(&path, original)?;
    assert!(
        store.verify("r1").is_ok(),
        "the untouched manifest still verifies"
    );
    Ok(())
}
