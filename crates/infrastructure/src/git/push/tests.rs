//! TEST-ATOMIC-PUSH, TEST-PLAN-LEASE (lease enforcement) and TEST-READONLY-SOURCE with
//! real Git: a fixture source captured into an owned mirror, plans built by the
//! application layer, and pushes into local bare destinations configured to refuse
//! deletes or atomic pushes. No network and no hosted service is used.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gbm_application::apply::{ApplyBlock, ApplyOutcome, apply_authorized};
use gbm_application::observation::SourceRefReader;
use gbm_application::plan::{
    ApplyAuthority, ApplyContext, AuthorizeError, DestinationIdentity, PlanOutcome, PlanRequest,
    SyncPlan, SyncPolicy, authorize_apply, build_plan,
};
use gbm_application::recovery::{RecoveryRequest, RecoverySnapshot, SnapshotStore, StoreError};
use gbm_application::snapshot::{
    CaptureRequest, ObservationClock, SnapshotManifest, capture_snapshot,
};
use gbm_domain::contract::RepositorySummarySourceState as SourceState;
use gbm_domain::identity::{GitHubRepoId, mirror_relative_path};
use gbm_domain::plan::OperationKind;
use gbm_domain::refs::{RefMap, parse_ls_remote};
use gbm_test_fixtures::{Sandbox, SourceRepo, empty_bare, source_repo};

use super::*;
use crate::filesystem::OwnedRoot;
use crate::git::mirror::{GitMirror, RunnerRefReader};
use crate::git::runner::GitBinary;
use crate::storage::{establish, validate_candidate};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Res<T> = Result<T, Box<dyn std::error::Error>>;

struct Clock(AtomicU64);

impl ObservationClock for Clock {
    fn next(&self) -> (u64, i64) {
        let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        (n, i64::try_from(n).unwrap_or(0))
    }
}

/// Non-destructive plans never consult the store.
struct NoStore;

impl SnapshotStore for NoStore {
    fn create(&self, _: &RecoveryRequest) -> Result<RecoverySnapshot, StoreError> {
        Err(StoreError::Failed)
    }
    fn verify(&self, _: &str) -> Result<RecoverySnapshot, StoreError> {
        Err(StoreError::NotFound)
    }
}

const AUTO: SyncPolicy = SyncPolicy {
    revision: 1,
    auto_apply_non_destructive: true,
};

struct Env {
    base: PathBuf,
    home: PathBuf,
    mirror: PathBuf,
}

impl Env {
    fn new(label: &str) -> Res<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let base =
            std::env::temp_dir().join(format!("gbm-push-{label}-{}-{n}", std::process::id()));
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
        let mirror = root.create_dir(&mirror_relative_path(GitHubRepoId::parse("88")?))?;
        Ok(Self {
            base: fs::canonicalize(&base)?,
            home: fs::canonicalize(home)?,
            mirror,
        })
    }

    fn runner(&self) -> Res<GitRunner> {
        Ok(GitRunner::new(GitBinary::locate()?, &self.home)?)
    }

    fn reader(&self, repo: &Path) -> Res<RunnerRefReader> {
        Ok(RunnerRefReader::new(
            self.runner()?,
            Location::local(&fs::canonicalize(repo)?)?,
            None,
            RunLimits::default(),
            CancelToken::default(),
        ))
    }

    /// Complete selected refs of `repo` via ls-remote (read-only).
    fn observe(&self, repo: &Path) -> Res<RefMap> {
        let text = self
            .reader(repo)?
            .read_advertisement("r")
            .map_err(|e| format!("{e:?}"))?;
        Ok(parse_ls_remote(&text)?.refs)
    }

    fn capture(&self, source: &Path, snapshot_id: &str) -> Res<SnapshotManifest> {
        let location = Location::local(&fs::canonicalize(source)?)?;
        let mirror = GitMirror::new(
            self.runner()?,
            self.mirror.clone(),
            location,
            None,
            RunLimits::default(),
            CancelToken::default(),
        );
        let outcome = capture_snapshot(
            &self.reader(source)?,
            &mirror,
            &Clock(AtomicU64::new(0)),
            &CaptureRequest {
                repository_id: "repo-88",
                job_id: &format!("job-{snapshot_id}"),
                snapshot_id,
            },
        )?;
        Ok(outcome.manifest)
    }

    fn pusher(&self, destination: &Path) -> Res<GitPusher> {
        Ok(GitPusher::to_local_for_tests(
            self.runner()?,
            self.mirror.clone(),
            Location::local(&fs::canonicalize(destination)?)?,
            RunLimits::default(),
            CancelToken::default(),
        ))
    }

    fn ancestry(&self) -> Res<MirrorAncestry> {
        Ok(MirrorAncestry::new(
            self.runner()?,
            self.mirror.clone(),
            RunLimits::default(),
            CancelToken::default(),
        ))
    }

    fn plan(
        &self,
        source: &Path,
        snapshot: &SnapshotManifest,
        destination: &Path,
        last_verified: Option<&RefMap>,
    ) -> Res<SyncPlan> {
        let state = if self.observe(source)? == snapshot.refs {
            SourceState::Current
        } else {
            SourceState::Changed
        };
        let observed = self.observe(destination)?;
        let request = PlanRequest {
            plan_id: "plan-1",
            mapping_id: "map-1",
            snapshot,
            source_state: state,
            destination: &observed,
            destination_identity: &identity(),
            last_verified,
            policy: AUTO,
            now_ms: 1_000,
            ttl_ms: 600_000,
        };
        match build_plan(&request, &self.ancestry()?) {
            PlanOutcome::Planned(plan) => Ok(*plan),
            PlanOutcome::Blocked(block) => Err(format!("blocked: {block:?}").into()),
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn identity() -> DestinationIdentity {
    DestinationIdentity {
        gitlab_account_id: "acc-gl".to_owned(),
        project_id: 42,
    }
}

fn authority(plan: &SyncPlan) -> ApplyAuthority {
    ApplyAuthority {
        plan_id: plan.id.clone(),
        plan_digest: plan.plan_digest.clone(),
        operations: plan.operations.clone(),
        recovery: None,
    }
}

/// Every file under `dir` with its bytes (to prove a repository was not written).
fn fingerprint(dir: &Path) -> Res<BTreeMap<PathBuf, Vec<u8>>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(path.clone(), fs::read(&path)?);
            }
        }
    }
    Ok(out)
}

/// Makes a tree read-only for the duration of a test and restores it on drop.
struct ReadOnly(PathBuf);

impl ReadOnly {
    fn set(dir: &Path, writable: bool) -> Res<()> {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let meta = fs::symlink_metadata(&current)?;
            if meta.file_type().is_symlink() {
                continue;
            }
            let mode = meta.permissions().mode();
            let new = if writable {
                mode | 0o200
            } else {
                mode & !0o222
            };
            if meta.is_dir() {
                for entry in fs::read_dir(&current)? {
                    stack.push(entry?.path());
                }
            }
            fs::set_permissions(&current, fs::Permissions::from_mode(new))?;
        }
        Ok(())
    }

    fn new(dir: &Path) -> Res<Self> {
        Self::set(dir, false)?;
        Ok(Self(dir.to_path_buf()))
    }
}

impl Drop for ReadOnly {
    fn drop(&mut self) {
        let _ = Self::set(&self.0, true);
    }
}

/// First backup of the fixture source into an empty destination; returns the
/// verified destination state.
fn first_backup(env: &Env, source: &SourceRepo, destination: &Path) -> Res<RefMap> {
    let snapshot = env.capture(&source.path, "c1")?;
    let plan = env.plan(&source.path, &snapshot, destination, None)?;
    assert!(
        plan.operations
            .iter()
            .all(|op| op.kind == OperationKind::Create)
    );
    let outcome = apply_authorized(&authority(&plan), &env.pusher(destination)?);
    assert!(matches!(outcome, ApplyOutcome::Applied(_)), "{outcome:?}");
    let verified = env.observe(destination)?;
    assert_eq!(verified, snapshot.refs);
    Ok(verified)
}

#[test]
fn full_lifecycle_never_writes_the_source() -> TestResult {
    // TEST-READONLY-SOURCE: the source tree is read-only for the whole scan, plan,
    // authorize and apply; every Git write targets the mirror or the destination.
    let sandbox = Sandbox::new("push-readonly")?;
    let source = source_repo(&sandbox)?;
    let destination = empty_bare(&sandbox, "dest")?;
    let env = Env::new("readonly")?;
    let before = fingerprint(&source.path)?;
    let guard = ReadOnly::new(&source.path)?;

    let snapshot = env.capture(&source.path, "c1")?;
    let plan = env.plan(&source.path, &snapshot, &destination, None)?;
    let source_now = env.observe(&source.path)?;
    let destination_now = env.observe(&destination)?;
    let authority = authorize_apply(
        &plan,
        None,
        &ApplyContext {
            source_now: Some(&source_now),
            destination_now: Some(&destination_now),
            destination_identity: &identity(),
            last_verified_now: None,
            policy: AUTO,
            now_ms: 2_000,
        },
        &NoStore,
        None,
    )?;
    let outcome = apply_authorized(&authority, &env.pusher(&destination)?);
    assert!(matches!(outcome, ApplyOutcome::Applied(_)), "{outcome:?}");
    assert_eq!(env.observe(&destination)?, snapshot.refs);
    // Annotated tag objects are kept as-is (not peeled).
    let tag = sandbox.git(&destination, &["cat-file", "-t", "refs/tags/v1.0"])?;
    assert_eq!(tag, "tag");
    drop(guard);
    assert_eq!(fingerprint(&source.path)?, before, "source untouched");
    // Only selected heads/tags reached the destination; no app namespaces.
    let all = sandbox.git(&destination, &["for-each-ref", "--format=%(refname)"])?;
    assert!(
        all.lines()
            .all(|r| r.starts_with("refs/heads/") || r.starts_with("refs/tags/"))
    );
    Ok(())
}

/// Rewrites `feature`, moves tag `moving` and deletes `release/1.x` in the source,
/// captures it as `c2` and plans against the verified destination.
fn destructive_plan(
    env: &Env,
    sandbox: &Sandbox,
    source: &SourceRepo,
    destination: &Path,
    verified: &RefMap,
) -> Res<SyncPlan> {
    sandbox.git(&source.path, &["checkout", "-q", "feature"])?;
    sandbox.git(
        &source.path,
        &["commit", "-q", "--amend", "-m", "feature rewritten again"],
    )?;
    sandbox.git(&source.path, &["checkout", "-q", "main"])?;
    sandbox.git(&source.path, &["tag", "-f", "moving", "HEAD~1"])?;
    sandbox.git(&source.path, &["branch", "-D", "release/1.x"])?;
    let snapshot = env.capture(&source.path, "c2")?;
    let plan = env.plan(&source.path, &snapshot, destination, Some(verified))?;
    let kinds: Vec<OperationKind> = plan.operations.iter().map(|o| o.kind).collect();
    assert_eq!(
        kinds,
        vec![
            OperationKind::Rewrite,
            OperationKind::DeleteRef,
            OperationKind::MoveTag
        ],
        "{:?}",
        plan.operations
    );
    Ok(plan)
}

#[test]
fn a_stale_lease_rejects_the_whole_atomic_push() -> TestResult {
    // TEST-PLAN-LEASE at the Git level: the destination moves after planning.
    let sandbox = Sandbox::new("push-lease")?;
    let source = source_repo(&sandbox)?;
    let destination = empty_bare(&sandbox, "dest")?;
    let env = Env::new("lease")?;
    let verified = first_backup(&env, &source, &destination)?;
    let plan = destructive_plan(&env, &sandbox, &source, &destination, &verified)?;

    // Someone else pushes to `feature` on the destination.
    sandbox.git(
        &source.path,
        &[
            "push",
            "-q",
            "-f",
            &destination.to_string_lossy(),
            "main:refs/heads/feature",
        ],
    )?;
    let moved = env.observe(&destination)?;
    let report = env
        .pusher(&destination)?
        .push(&plan.operations, true)
        .map_err(|e| format!("{e:?}"))?;
    assert!(
        report
            .refs
            .iter()
            .any(|(_, s)| *s == RefPushStatus::Rejected(RejectReason::StaleLease))
    );
    assert!(
        report
            .refs
            .iter()
            .all(|(_, s)| matches!(s, RefPushStatus::Rejected(_)))
    );
    assert_eq!(env.observe(&destination)?, moved, "nothing overwritten");
    // The application gate would have refused it before Git anyway.
    let source_now = env.observe(&source.path)?;
    assert_eq!(
        authorize_apply(
            &plan,
            None,
            &ApplyContext {
                source_now: Some(&source_now),
                destination_now: Some(&moved),
                destination_identity: &identity(),
                last_verified_now: Some(&verified),
                policy: AUTO,
                now_ms: 2_000,
            },
            &NoStore,
            None,
        ),
        Err(AuthorizeError::DestinationChanged)
    );
    Ok(())
}

#[test]
fn a_refused_ref_aborts_the_atomic_push() -> TestResult {
    // TEST-ATOMIC-PUSH: the server refuses one ref (deletes prohibited, like a
    // protected branch); no other ref of the plan is applied.
    let sandbox = Sandbox::new("push-refused")?;
    let source = source_repo(&sandbox)?;
    let destination = empty_bare(&sandbox, "dest")?;
    let env = Env::new("refused")?;
    let verified = first_backup(&env, &source, &destination)?;
    let plan = destructive_plan(&env, &sandbox, &source, &destination, &verified)?;
    sandbox.git(&destination, &["config", "receive.denyDeletes", "true"])?;
    let outcome = apply_authorized(&authority(&plan), &env.pusher(&destination)?);
    let ApplyOutcome::Rejected(report) = outcome else {
        return Err(format!("expected rejected: {outcome:?}").into());
    };
    assert!(
        report
            .refs
            .iter()
            .any(|(_, s)| *s == RefPushStatus::Rejected(RejectReason::RemoteRejected))
    );
    assert_eq!(
        env.observe(&destination)?,
        verified,
        "destination unchanged"
    );
    Ok(())
}

#[test]
fn without_atomic_support_destructive_plans_block_and_others_report_partial() -> TestResult {
    let sandbox = Sandbox::new("push-noatomic")?;
    let source = source_repo(&sandbox)?;
    let destination = empty_bare(&sandbox, "dest")?;
    let env = Env::new("noatomic")?;
    let verified = first_backup(&env, &source, &destination)?;
    sandbox.git(
        &destination,
        &["config", "receive.advertiseAtomic", "false"],
    )?;
    let plan = destructive_plan(&env, &sandbox, &source, &destination, &verified)?;
    assert_eq!(
        apply_authorized(&authority(&plan), &env.pusher(&destination)?),
        ApplyOutcome::Blocked(ApplyBlock::DestructiveNeedsAtomic)
    );
    assert_eq!(env.observe(&destination)?, verified, "nothing sent");

    // Non-destructive: two new branches, one of which someone created meanwhile.
    sandbox.git(&source.path, &["branch", "new-a", "main"])?;
    sandbox.git(&source.path, &["branch", "new-b", "main"])?;
    let mut operations = Vec::new();
    for name in ["refs/heads/new-a", "refs/heads/new-b"] {
        operations.push(RefOperation {
            kind: OperationKind::Create,
            name: gbm_domain::refs::RefName::parse(name).ok_or("name")?,
            expected_destination: None,
            source: Some(ObjectId::parse(&source.main_tip).ok_or("oid")?),
        });
    }
    sandbox.git(
        &source.path,
        &[
            "push",
            "-q",
            &destination.to_string_lossy(),
            "HEAD~1:refs/heads/new-b",
        ],
    )?;
    let outcome = apply_authorized(
        &ApplyAuthority {
            plan_id: "plan-2".to_owned(),
            plan_digest: "d".repeat(64),
            operations,
            recovery: None,
        },
        &env.pusher(&destination)?,
    );
    let ApplyOutcome::Partial(report) = outcome else {
        return Err(format!("expected partial: {outcome:?}").into());
    };
    assert_eq!(report.refs[0].1, RefPushStatus::Updated);
    assert_eq!(
        report.refs[1].1,
        RefPushStatus::Rejected(RejectReason::StaleLease)
    );
    let after = env.observe(&destination)?;
    let b = gbm_domain::refs::RefName::parse("refs/heads/new-b").ok_or("name")?;
    assert_ne!(
        after.get(&b).map(ObjectId::as_str),
        Some(source.main_tip.as_str()),
        "the other party's ref is kept"
    );
    Ok(())
}

#[test]
fn ancestry_comes_from_the_mirror_and_unknown_is_none() -> TestResult {
    let sandbox = Sandbox::new("push-ancestry")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("ancestry")?;
    env.capture(&source.path, "c1")?;
    let oracle = env.ancestry()?;
    let oid = |text: &str| ObjectId::parse(text).ok_or("oid");
    assert_eq!(
        oracle.is_ancestor(&oid(&source.moved_tag_before)?, &oid(&source.main_tip)?),
        Some(true)
    );
    assert_eq!(
        oracle.is_ancestor(&oid(&source.main_tip)?, &oid(&source.moved_tag_before)?),
        Some(false)
    );
    assert_eq!(
        oracle.is_ancestor(&oid(&"f".repeat(40))?, &oid(&source.main_tip)?),
        None,
        "missing object is unknown, never 'not an ancestor' or 'ancestor'"
    );
    Ok(())
}

#[test]
fn porcelain_reports_must_be_complete_and_about_the_sent_refs() -> TestResult {
    let ops = vec![RefOperation {
        kind: OperationKind::Create,
        name: gbm_domain::refs::RefName::parse("refs/heads/a").ok_or("name")?,
        expected_destination: None,
        source: Some(ObjectId::parse(&"a".repeat(40)).ok_or("oid")?),
    }];
    let line = format!("*\t{}:refs/heads/a\t[new branch]\n", "a".repeat(40));
    assert!(parse_porcelain(&format!("To x\n{line}Done\n"), &ops).is_some());
    assert!(
        parse_porcelain(&format!("To x\n{line}"), &ops).is_none(),
        "no Done"
    );
    assert!(
        parse_porcelain(&format!("To x\n{line}{line}Done\n"), &ops).is_none(),
        "duplicate"
    );
    assert!(
        parse_porcelain("To x\nDone\n", &ops).is_none(),
        "missing ref"
    );
    let other = format!("*\t{}:refs/heads/b\t[new branch]\nDone\n", "a".repeat(40));
    assert!(parse_porcelain(&other, &ops).is_none(), "other ref");
    let odd = format!("?\t{}:refs/heads/a\tx\nDone\n", "a".repeat(40));
    assert!(parse_porcelain(&odd, &ops).is_none(), "unknown flag");
    Ok(())
}

#[test]
fn a_ref_the_server_did_not_report_makes_the_push_unknown() -> TestResult {
    let ops = vec![RefOperation {
        kind: OperationKind::Create,
        name: gbm_domain::refs::RefName::parse("refs/heads/a").ok_or("name")?,
        expected_destination: None,
        source: Some(ObjectId::parse(&"a".repeat(40)).ok_or("oid")?),
    }];
    let failure = format!(
        "To x\n!\t{}:refs/heads/a\t[remote failure] (remote failed to report status)\nDone\n",
        "a".repeat(40)
    );
    assert!(parse_porcelain(&failure, &ops).is_none());
    // Server-written text after `[remote rejected]` cannot pose as a lease failure.
    let spoof = format!(
        "To x\n!\t{}:refs/heads/a\t[remote rejected] (stale info)\nDone\n",
        "a".repeat(40)
    );
    let report = parse_porcelain(&spoof, &ops).ok_or("report")?;
    assert_eq!(
        report.refs[0].1,
        RefPushStatus::Rejected(RejectReason::RemoteRejected)
    );
    Ok(())
}

#[test]
fn exit_status_and_report_must_agree() -> TestResult {
    let ops = vec![RefOperation {
        kind: OperationKind::Create,
        name: gbm_domain::refs::RefName::parse("refs/heads/a").ok_or("name")?,
        expected_destination: None,
        source: Some(ObjectId::parse(&"a".repeat(40)).ok_or("oid")?),
    }];
    let ok = format!(
        "To x\n*\t{}:refs/heads/a\t[new branch]\nDone\n",
        "a".repeat(40)
    );
    let bad = format!(
        "To x\n!\t{}:refs/heads/a\t[rejected] (stale info)\nDone\n",
        "a".repeat(40)
    );
    let out =
        |t, stdout: &str, stderr: &str, complete| GitOutput::for_tests(t, stdout, stderr, complete);
    assert!(classify_output(&out(Termination::Exited(0), &ok, "", true), &ops, true).is_ok());
    assert!(classify_output(&out(Termination::Exited(1), &bad, "", true), &ops, true).is_ok());
    for (output, label) in [
        (
            out(Termination::Exited(0), &bad, "", true),
            "exit 0 with a rejection",
        ),
        (
            out(Termination::Exited(1), &ok, "", true),
            "exit 1 with all ok",
        ),
        (
            out(Termination::Exited(128), "", "fatal: other", true),
            "other fatal",
        ),
        (out(Termination::Cancelled, &ok, "", true), "cancelled"),
        (out(Termination::TimedOut, &ok, "", true), "timed out"),
        (
            out(Termination::Exited(0), &ok, "", false),
            "incomplete output",
        ),
    ] {
        assert_eq!(
            classify_output(&output, &ops, true),
            Err(PushError::Unknown),
            "{label}"
        );
    }
    let unsupported = out(
        Termination::Exited(128),
        "",
        "fatal: the receiving end does not support --atomic push\n",
        true,
    );
    assert_eq!(
        classify_output(&unsupported, &ops, true),
        Err(PushError::AtomicUnsupported)
    );
    assert_eq!(
        classify_output(&unsupported, &ops, false),
        Err(PushError::Unknown),
        "only meaningful for an atomic request"
    );
    Ok(())
}

#[test]
fn the_production_writer_only_pushes_to_gitlab() -> TestResult {
    let env = Env::new("hosts")?;
    let github = RemoteTarget::from_url("https://github.com/octo/tool.git")?;
    assert!(
        GitPusher::new(
            env.runner()?,
            env.mirror.clone(),
            github,
            None,
            RunLimits::default(),
            CancelToken::default()
        )
        .is_err()
    );
    let gitlab = RemoteTarget::from_url("https://gitlab.com/backups/tool-gh42.git")?;
    assert!(
        GitPusher::new(
            env.runner()?,
            env.mirror.clone(),
            gitlab,
            None,
            RunLimits::default(),
            CancelToken::default()
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn replace_refs_cannot_turn_a_rewrite_into_a_fast_forward() -> TestResult {
    let sandbox = Sandbox::new("push-replace")?;
    let source = source_repo(&sandbox)?;
    let env = Env::new("replace")?;
    env.capture(&source.path, "c1")?;
    let before = ObjectId::parse(&source.main_tip).ok_or("oid")?;
    let after = ObjectId::parse(&source.feature_tip_after_rewrite).ok_or("oid")?;
    let oracle = env.ancestry()?;
    assert_eq!(oracle.is_ancestor(&before, &after), Some(false));
    // Both commits are in the mirror and unrelated. Graft: pretend `after` descends
    // from `before`, via refs/replace in the mirror.
    let fake = sandbox.git(
        &env.mirror,
        &[
            "--git-dir=.",
            "commit-tree",
            &format!("{}^{{tree}}", source.feature_tip_after_rewrite),
            "-p",
            &source.main_tip,
            "-m",
            "graft",
        ],
    )?;
    sandbox.git(
        &env.mirror,
        &[
            "--git-dir=.",
            "replace",
            &source.feature_tip_after_rewrite,
            &fake,
        ],
    )?;
    assert_eq!(oracle.is_ancestor(&before, &after), Some(false));
    Ok(())
}
