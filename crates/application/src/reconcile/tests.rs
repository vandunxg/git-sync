//! TEST-CRASH at the decision level (the checkpoint values a restart would load; the
//! store round trip is covered in infrastructure `sqlite::tests::reconcile_store`),
//! cancel honesty and resume coalescing. Real child-process cancel/reaping is TEST-PROCESS-
//! CANCEL in infrastructure/src/git/runner; a stale PID is never an input here.

use std::cell::{Cell, RefCell};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use gbm_domain::contract::RepositorySummarySourceState as SourceState;
use gbm_domain::plan::{AncestryOracle, OperationKind};
use gbm_domain::refs::{ObjectId, RefName};

use super::*;
use crate::destination::{
    CreateIntent, DestinationApi, EnsureOutcome, ProjectView, Visibility, ensure_project,
};
use crate::plan::{
    ApplyContext, AuthorizeError, DestinationIdentity, PlanOutcome, PlanRequest, SyncPolicy,
    approve_plan, authorize_apply, build_plan,
};
use crate::providers::ProviderError;
use crate::recovery::{RecoveryRequest, RecoverySnapshot, SnapshotStore, StoreError};
use crate::snapshot::{Coverage, SnapshotManifest};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn map(entries: &[(&str, char)]) -> Result<RefMap, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for (name, fill) in entries {
        out.push((
            RefName::parse(name).ok_or("name")?,
            ObjectId::parse(&fill.to_string().repeat(40)).ok_or("oid")?,
        ));
    }
    Ok(RefMap::from_entries(out)?)
}

fn manifest(id: &str, refs: RefMap) -> SnapshotManifest {
    SnapshotManifest {
        repository_id: "repo-1".to_owned(),
        snapshot_id: id.to_owned(),
        object_format: refs.format(),
        ref_set_digest: ref_set_digest(&refs),
        refs,
        head_target: None,
        observed_at_ms: 1,
        ref_namespace: None,
        git_refs: Coverage::Complete,
        lfs: Coverage::NotChecked,
        submodules: Coverage::NotChecked,
    }
}

struct Linear;

impl AncestryOracle for Linear {
    fn is_ancestor(&self, _: &ObjectId, _: &ObjectId) -> Option<bool> {
        Some(true)
    }
}

const POLICY: SyncPolicy = SyncPolicy {
    revision: 1,
    auto_apply_non_destructive: false,
};

fn identity() -> DestinationIdentity {
    DestinationIdentity {
        gitlab_account_id: "acc-gl".to_owned(),
        project_id: 42,
    }
}

fn plan(
    id: &str,
    snapshot: &SnapshotManifest,
    destination: &RefMap,
) -> Result<SyncPlan, Box<dyn std::error::Error>> {
    let request = PlanRequest {
        plan_id: id,
        mapping_id: "map-1",
        snapshot,
        source_state: SourceState::Current,
        destination,
        destination_identity: &identity(),
        last_verified: Some(destination),
        policy: POLICY,
        now_ms: 1_000,
        ttl_ms: 600_000,
    };
    match build_plan(&request, &Linear) {
        PlanOutcome::Planned(plan) => Ok(*plan),
        PlanOutcome::Blocked(block) => Err(format!("{block:?}").into()),
    }
}

#[test]
fn interrupted_before_any_write_gets_a_new_attempt() {
    for stage in PRE_WRITE_STAGES {
        assert_eq!(
            reconcile_interrupted(stage, &Checkpoint::BeforeWrite, None, 0),
            Reconciliation::NewAttempt
        );
    }
    // A write stage without a checkpoint can never be judged: escalate at once and
    // release the locks instead of holding them forever.
    for stage in [
        "transfer_lfs",
        "push",
        "verify",
        "workspace",
        "finalize",
        "unknown",
    ] {
        assert_eq!(
            reconcile_interrupted(stage, &Checkpoint::BeforeWrite, None, 0),
            Reconciliation::Uncertain,
            "{stage}"
        );
    }
}

#[test]
fn interrupted_pushes_are_decided_by_the_observed_destination() -> TestResult {
    // TEST-CRASH: killed before / after / during the push checkpoint.
    let before = map(&[("refs/heads/main", 'a'), ("refs/heads/dev", 'b')])?;
    let snapshot = manifest(
        "snap-2",
        map(&[("refs/heads/main", '1'), ("refs/heads/dev", '2')])?,
    );
    let plan = plan("plan-1", &snapshot, &before)?;
    let checkpoint = Checkpoint::Push {
        plan: &plan,
        snapshot_refs: &snapshot.refs,
    };
    // Killed before the push reached the server.
    assert_eq!(
        reconcile_interrupted("push", &checkpoint, Some(&before), 0),
        Reconciliation::NewAttempt
    );
    // Killed after the push but before persisting the result.
    assert_eq!(
        reconcile_interrupted("push", &checkpoint, Some(&snapshot.refs), 0),
        Reconciliation::VerifiedComplete
    );
    // Killed while a non-atomic push was half done.
    let half = map(&[("refs/heads/main", '1'), ("refs/heads/dev", 'b')])?;
    assert_eq!(
        reconcile_interrupted("verify", &checkpoint, Some(&half), 0),
        Reconciliation::PartialNeedsReview
    );
    // A third party changed other refs meanwhile: partial, never success.
    let foreign = map(&[
        ("refs/heads/main", '1'),
        ("refs/heads/dev", '2'),
        ("refs/heads/x", 'c'),
    ])?;
    assert_eq!(
        reconcile_interrupted("verify", &checkpoint, Some(&foreign), 0),
        Reconciliation::PartialNeedsReview
    );
    // Not observable now: nothing is assumed, the job keeps its locks — but only until
    // the deadline, then the owner decides (locks released).
    assert_eq!(
        reconcile_interrupted("push", &checkpoint, None, RECONCILE_DEADLINE_MS - 1),
        Reconciliation::KeepInterrupted
    );
    assert_eq!(
        reconcile_interrupted("push", &checkpoint, None, RECONCILE_DEADLINE_MS),
        Reconciliation::Uncertain
    );
    Ok(())
}

struct NoStore;

impl SnapshotStore for NoStore {
    fn create(&self, _: &RecoveryRequest) -> Result<RecoverySnapshot, StoreError> {
        Err(StoreError::Failed)
    }
    fn verify(&self, _: &str) -> Result<RecoverySnapshot, StoreError> {
        Err(StoreError::NotFound)
    }
}

/// Decision-level half of "never resume a stale approval": the new attempt's plan has
/// a new id, so the old approval cannot authorize it. The store half (the old plan and
/// approval expire when the attempt is closed, and the new attempt has no plan) is in
/// infrastructure `sqlite::tests::reconcile_store`.
#[test]
fn a_stale_approval_never_authorizes_the_new_attempt() -> TestResult {
    let before = map(&[("refs/heads/main", 'a')])?;
    let snapshot = manifest("snap-2", map(&[("refs/heads/main", '1')])?);
    let old = plan("plan-1", &snapshot, &before)?;
    let approval = approve_plan(&old, "ap-1", &old.plan_digest, 1, 1, 2_000, 60_000)?;
    // After the restart the new attempt plans again (new id).
    let new = plan("plan-2", &snapshot, &before)?;
    let context = ApplyContext {
        source_now: Some(&snapshot.refs),
        destination_now: Some(&before),
        destination_identity: &identity(),
        last_verified_now: Some(&before),
        policy: POLICY,
        now_ms: 3_000,
    };
    assert_eq!(
        authorize_apply(&new, Some(&approval), &context, &NoStore, None),
        Err(AuthorizeError::ApprovalInvalid)
    );
    assert_eq!(new.operations[0].kind, OperationKind::FastForward);
    Ok(())
}

fn run<T>(future: impl std::future::Future<Output = T>) -> Option<T> {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// GitLab double: `create` stores the project (the remote success happened).
struct GitLab {
    projects: RefCell<Vec<ProjectView>>,
    creates: Cell<u32>,
}

impl DestinationApi for GitLab {
    async fn find_by_path(&self, path: &str) -> Result<Option<ProjectView>, ProviderError> {
        Ok(self
            .projects
            .borrow()
            .iter()
            .find(|p| p.path_with_namespace == path)
            .cloned())
    }
    async fn get_by_id(&self, _: u64) -> Result<Option<ProjectView>, ProviderError> {
        Ok(None)
    }
    async fn create(&self, intent: &CreateIntent) -> Result<ProjectView, ProviderError> {
        self.creates.set(self.creates.get() + 1);
        let project = ProjectView {
            project_id: 500,
            namespace_id: intent.namespace_id,
            path_with_namespace: intent.full_path(),
            visibility: intent.visibility,
            clone_url: format!("https://gitlab.com/{}.git", intent.full_path()),
            description: intent.description(),
            empty_repo: true,
            access_level: Some(50),
        };
        self.projects.borrow_mut().push(project.clone());
        Ok(project)
    }
}

#[test]
fn a_crash_after_remote_project_creation_never_duplicates_the_project() -> TestResult {
    // TEST-CRASH: the app dies after GitLab created the project but before the
    // project id was persisted. Restart re-runs the same persisted intent.
    let gitlab = GitLab {
        projects: RefCell::new(Vec::new()),
        creates: Cell::new(0),
    };
    let intent = intent();
    // The intent is persisted before the create is sent.
    let stored = CheckpointRecord::ProjectCreate(intent.clone());
    let (kind, payload) = (stored.kind(), stored.payload());
    let first = run(ensure_project(&gitlab, &intent)).ok_or("pending")??;
    assert!(matches!(first, EnsureOutcome::Created(_)));
    // …crash; the project id was not persisted. On restart the checkpoint is loaded:
    let Some(CheckpointRecord::ProjectCreate(loaded)) = CheckpointRecord::decode(kind, &payload)
    else {
        return Err("checkpoint did not round-trip".into());
    };
    assert_eq!(loaded, intent, "same marker after restart");
    assert_eq!(
        reconcile_interrupted("preflight", &Checkpoint::ProjectCreate(&loaded), None, 0),
        Reconciliation::RerunCreateIntent
    );
    let again = run(ensure_project(&gitlab, &loaded)).ok_or("pending")??;
    assert!(matches!(again, EnsureOutcome::Reconciled(ref p) if p.project_id == 500));
    assert_eq!(gitlab.creates.get(), 1, "no duplicate project");
    Ok(())
}

fn intent() -> CreateIntent {
    CreateIntent {
        namespace_id: 7,
        namespace_full_path: "backups".to_owned(),
        path: "tool-gh42".to_owned(),
        visibility: Visibility::Private,
        source_private: true,
        marker: "f3a9c1d2e4b5a6978877".to_owned(),
    }
}

#[test]
fn checkpoints_decode_strictly() {
    let push = CheckpointRecord::Push {
        plan_id: "plan-1".to_owned(),
        snapshot_id: "snap-2".to_owned(),
    };
    assert_eq!(
        CheckpointRecord::decode(push.kind(), &push.payload()),
        Some(push.clone())
    );
    for (kind, payload) in [
        ("push", r#"{"planId":"plan-1"}"#),
        ("push", r#"{"planId":"plan-1","snapshotId":"s","argv":"x"}"#),
        ("push", r#"{"planId":1,"snapshotId":"s"}"#),
        ("project_create", r#"{"planId":"plan-1","snapshotId":"s"}"#),
        ("other", r#"{"planId":"plan-1","snapshotId":"s"}"#),
        ("push", "not json"),
    ] {
        assert_eq!(CheckpointRecord::decode(kind, payload), None, "{payload}");
    }
    let create = CheckpointRecord::ProjectCreate(intent());
    let mut value: serde_json::Value = serde_json::from_str(&create.payload()).unwrap_or_default();
    value["visibility"] = "secret".into();
    assert_eq!(
        CheckpointRecord::decode("project_create", &value.to_string()),
        None
    );
}

#[test]
fn every_decision_closes_the_attempt_or_keeps_it_explicitly() {
    use ClosedAs::*;
    let cases = [
        (
            Reconciliation::NewAttempt,
            Some((Failed, Some(ErrorCode::PlanStale), true)),
        ),
        (
            Reconciliation::RerunCreateIntent,
            Some((Failed, Some(ErrorCode::PlanStale), true)),
        ),
        (
            Reconciliation::VerifiedComplete,
            Some((Succeeded, None, false)),
        ),
        (
            Reconciliation::PartialNeedsReview,
            Some((Partial, Some(ErrorCode::PushPartial), false)),
        ),
        (
            Reconciliation::Uncertain,
            Some((Failed, Some(ErrorCode::VerifyFailed), false)),
        ),
        (Reconciliation::KeepInterrupted, None),
    ];
    for (decision, expected) in cases {
        assert_eq!(
            decision
                .closing()
                .map(|c| (c.state, c.error, c.new_attempt)),
            expected,
            "{decision:?}"
        );
    }
}

#[test]
fn cancel_is_reported_honestly() {
    let none = Checkpoint::BeforeWrite;
    assert_eq!(
        cancel_result("push", &none, CancelObservation::NotConfirmed),
        CancelResult::StillCanceling,
        "never 'Canceled' before the process is confirmed stopped"
    );
    for stage in PRE_WRITE_STAGES {
        for observation in [CancelObservation::Stopped, CancelObservation::BeforeStart] {
            assert_eq!(
                cancel_result(stage, &none, observation),
                CancelResult::Canceled,
                "{stage}"
            );
        }
    }
    let uncertain = CancelResult::CanceledUncertain(ErrorCode::CancelUncertain);
    // A write stage: an earlier child may already have written, even if the next one
    // never started.
    for stage in [
        "transfer_lfs",
        "push",
        "verify",
        "workspace",
        "finalize",
        "unknown",
    ] {
        for observation in [CancelObservation::Stopped, CancelObservation::BeforeStart] {
            assert_eq!(
                cancel_result(stage, &none, observation),
                uncertain,
                "{stage}"
            );
        }
    }
    // A create intent was persisted (maybe sent) during preflight.
    let intent = intent();
    let creating = Checkpoint::ProjectCreate(&intent);
    for observation in [CancelObservation::Stopped, CancelObservation::BeforeStart] {
        assert_eq!(
            cancel_result("preflight", &creating, observation),
            uncertain
        );
    }
    assert_eq!(
        cancel_result("push", &none, CancelObservation::Finished),
        CancelResult::CompletedAnyway
    );
}

#[test]
fn resume_after_a_long_suspend_yields_one_scan() {
    let hour = 3_600_000;
    assert!(scan_due(None, hour, 0));
    assert!(!scan_due(Some(0), hour, hour - 1));
    // Ten intervals missed while suspended: still exactly one due scan.
    let mut last = Some(0);
    let mut scans = 0;
    for _tick in 0..5 {
        if scan_due(last, hour, 10 * hour) {
            scans += 1;
            last = Some(10 * hour);
        }
    }
    assert_eq!(scans, 1);
    // Wall clock moved backwards (or a future timestamp was stored): scan now rather
    // than suppress scans until the clock catches up.
    assert!(scan_due(Some(10 * hour), hour, 9 * hour));
}
