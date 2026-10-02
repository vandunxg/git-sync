//! TEST-PLAN-LEASE, TEST-SNAPSHOT (gate) and TEST-NO-DELETE-ON-404 (planning) with a
//! test-only recovery store and ancestry oracle.

use std::collections::{BTreeMap, HashSet};

use gbm_domain::refs::{ObjectId, RefName};

use super::*;
use crate::recovery::{RecoveryRequest, StoreError};
use crate::snapshot::Coverage;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn oid(fill: char) -> Result<ObjectId, String> {
    ObjectId::parse(&fill.to_string().repeat(40)).ok_or_else(|| format!("oid {fill}"))
}

fn map(entries: &[(&str, char)]) -> Result<RefMap, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for (name, fill) in entries {
        out.push((RefName::parse(name).ok_or("name")?, oid(*fill)?));
    }
    Ok(RefMap::from_entries(out)?)
}

fn manifest(refs: RefMap) -> SnapshotManifest {
    SnapshotManifest {
        repository_id: "repo-1".to_owned(),
        snapshot_id: "snap-1".to_owned(),
        object_format: refs.format(),
        ref_set_digest: ref_set_digest(&refs),
        refs,
        head_target: Some("refs/heads/main".to_owned()),
        observed_at_ms: 1,
        ref_namespace: Some("refs/gbm/snapshots/snap-1/".to_owned()),
        git_refs: Coverage::Complete,
        lfs: Coverage::NotChecked,
        submodules: Coverage::NotChecked,
    }
}

struct Graph(HashSet<(char, char)>);

impl AncestryOracle for Graph {
    fn is_ancestor(&self, a: &ObjectId, d: &ObjectId) -> Option<bool> {
        let key = (a.as_str().chars().next()?, d.as_str().chars().next()?);
        Some(key.0 == key.1 || self.0.contains(&key))
    }
}

fn graph() -> Graph {
    Graph([('a', '1')].into_iter().collect())
}

struct Store(BTreeMap<String, Result<RecoverySnapshot, StoreError>>);

impl SnapshotStore for Store {
    fn create(&self, _: &RecoveryRequest) -> Result<RecoverySnapshot, StoreError> {
        Err(StoreError::Failed)
    }
    fn verify(&self, id: &str) -> Result<RecoverySnapshot, StoreError> {
        self.0.get(id).cloned().unwrap_or(Err(StoreError::NotFound))
    }
}

fn store_with(id: &str, result: Result<RecoverySnapshot, StoreError>) -> Store {
    Store([(id.to_owned(), result)].into_iter().collect())
}

fn recovery(refs: RefMap) -> RecoverySnapshot {
    RecoverySnapshot {
        repository_id: "repo-1".to_owned(),
        snapshot_id: "rec-1".to_owned(),
        object_format: refs.format(),
        refs,
        bundle_sha256: Some("ab".repeat(32)),
        bundle_bytes: 10,
    }
}

const AUTO: SyncPolicy = SyncPolicy {
    revision: 3,
    auto_apply_non_destructive: true,
};

struct Fixture {
    snapshot: SnapshotManifest,
    destination: RefMap,
    identity: DestinationIdentity,
}

fn identity(project_id: u64) -> DestinationIdentity {
    DestinationIdentity {
        gitlab_account_id: "acc-gl".to_owned(),
        project_id,
    }
}

impl Fixture {
    /// Destination at last verified state; source fast-forwards main and adds a branch.
    fn non_destructive() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            snapshot: manifest(map(&[("refs/heads/main", '1'), ("refs/heads/new", '2')])?),
            destination: map(&[("refs/heads/main", 'a')])?,
            identity: identity(42),
        })
    }

    /// Source rewrites `feature`, moves tag `v1` and drops `old`.
    fn destructive() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            snapshot: manifest(map(&[("refs/heads/feature", '7'), ("refs/tags/v1", '8')])?),
            destination: map(&[
                ("refs/heads/feature", 'b'),
                ("refs/heads/old", 'c'),
                ("refs/tags/v1", 'd'),
            ])?,
            identity: identity(42),
        })
    }

    fn request(&self, state: SourceState, policy: SyncPolicy) -> PlanRequest<'_> {
        PlanRequest {
            plan_id: "plan-1",
            mapping_id: "map-1",
            snapshot: &self.snapshot,
            source_state: state,
            destination: &self.destination,
            destination_identity: &self.identity,
            last_verified: Some(&self.destination),
            policy,
            now_ms: 1_000,
            ttl_ms: 600_000,
        }
    }

    fn plan(&self, policy: SyncPolicy) -> Result<SyncPlan, Box<dyn std::error::Error>> {
        match build_plan(&self.request(SourceState::Current, policy), &graph()) {
            PlanOutcome::Planned(plan) => Ok(*plan),
            PlanOutcome::Blocked(block) => Err(format!("blocked: {block:?}").into()),
        }
    }

    fn context(&self, policy: SyncPolicy, now_ms: i64) -> ApplyContext<'_> {
        ApplyContext {
            source_now: Some(&self.snapshot.refs),
            destination_now: Some(&self.destination),
            destination_identity: &self.identity,
            last_verified_now: Some(&self.destination),
            policy,
            now_ms,
        }
    }
}

#[test]
fn no_plan_unless_the_source_is_current() -> TestResult {
    // TEST-NO-DELETE-ON-404: a source that is unavailable (404/changed token), only
    // partially observed, changed or stale never yields a plan, so never a delete.
    let fixture = Fixture::destructive()?;
    for state in [
        SourceState::Unavailable,
        SourceState::Unknown,
        SourceState::Changed,
        SourceState::Stale,
    ] {
        assert_eq!(
            build_plan(&fixture.request(state, AUTO), &graph()),
            PlanOutcome::Blocked(PlanBlock::SourceNotCurrent(state))
        );
    }
    Ok(())
}

#[test]
fn plans_carry_exact_leases_and_require_approval_when_destructive() -> TestResult {
    let plan = Fixture::non_destructive()?.plan(AUTO)?;
    assert!(!plan.requires_approval && !plan.pre_change_snapshot_required);
    assert_eq!(plan.operations.len(), 2);
    assert_eq!(plan.operations[0].kind, OperationKind::FastForward);
    assert_eq!(plan.operations[0].expected_destination, Some(oid('a')?));
    assert_eq!(plan.operations[1].kind, OperationKind::Create);
    assert_eq!(plan.operations[1].expected_destination, None);

    let manual = SyncPolicy {
        auto_apply_non_destructive: false,
        ..AUTO
    };
    assert!(Fixture::non_destructive()?.plan(manual)?.requires_approval);

    let destructive = Fixture::destructive()?.plan(AUTO)?;
    assert!(destructive.requires_approval && destructive.pre_change_snapshot_required);
    let kinds: Vec<OperationKind> = destructive.operations.iter().map(|o| o.kind).collect();
    assert_eq!(
        kinds,
        vec![
            OperationKind::Rewrite,
            OperationKind::DeleteRef,
            OperationKind::MoveTag
        ]
    );
    Ok(())
}

#[test]
fn drift_and_inconsistent_snapshots_block_planning() -> TestResult {
    let fixture = Fixture::non_destructive()?;
    let verified = map(&[("refs/heads/main", 'f')])?;
    let mut request = fixture.request(SourceState::Current, AUTO);
    request.last_verified = Some(&verified);
    assert!(matches!(
        build_plan(&request, &graph()),
        PlanOutcome::Blocked(PlanBlock::DestinationDrift(_))
    ));
    let mut tampered = Fixture::non_destructive()?;
    tampered.snapshot.ref_set_digest = "0".repeat(64);
    assert_eq!(
        build_plan(&tampered.request(SourceState::Current, AUTO), &graph()),
        PlanOutcome::Blocked(PlanBlock::InvalidRequest)
    );
    Ok(())
}

#[test]
fn approval_binds_digest_policy_and_expiry() -> TestResult {
    let plan = Fixture::destructive()?.plan(AUTO)?;
    let digest = plan.plan_digest.clone();
    assert_eq!(
        approve_plan(&plan, "ap-1", "stale-digest", 3, 3, 2_000, 60_000),
        Err(ApproveError::DigestMismatch)
    );
    assert_eq!(
        approve_plan(&plan, "ap-1", &digest, 3, 4, 2_000, 60_000),
        Err(ApproveError::PolicyChanged)
    );
    assert_eq!(
        approve_plan(&plan, "ap-1", &digest, 2, 3, 2_000, 60_000),
        Err(ApproveError::PolicyChanged)
    );
    assert_eq!(
        approve_plan(&plan, "ap-1", &digest, 3, 3, plan.expires_at_ms, 60_000),
        Err(ApproveError::PlanExpired)
    );
    let approval = approve_plan(&plan, "ap-1", &digest, 3, 3, 2_000, 10_000_000)?;
    assert_eq!(
        approval.expires_at_ms, plan.expires_at_ms,
        "never outlives the plan"
    );
    let mut altered = plan.clone();
    altered.operations.pop();
    assert_eq!(
        approve_plan(&altered, "ap-1", &digest, 3, 3, 2_000, 60_000),
        Err(ApproveError::PlanTampered)
    );
    Ok(())
}

#[test]
fn any_drift_after_approval_invalidates_the_apply() -> TestResult {
    // TEST-PLAN-LEASE.
    let fixture = Fixture::destructive()?;
    let plan = fixture.plan(AUTO)?;
    let approval = approve_plan(&plan, "ap-1", &plan.plan_digest, 3, 3, 2_000, 60_000)?;
    let good = store_with("rec-1", Ok(recovery(fixture.destination.clone())));
    let authorize = |context: &ApplyContext<'_>, approval: Option<&Approval>| {
        authorize_apply(&plan, approval, context, &good, Some("rec-1"))
    };
    assert!(authorize(&fixture.context(AUTO, 3_000), Some(&approval)).is_ok());

    let moved_source = map(&[("refs/heads/feature", '9'), ("refs/tags/v1", '8')])?;
    let mut context = fixture.context(AUTO, 3_000);
    context.source_now = Some(&moved_source);
    assert_eq!(
        authorize(&context, Some(&approval)),
        Err(AuthorizeError::SourceChanged)
    );
    context.source_now = None;
    assert_eq!(
        authorize(&context, Some(&approval)),
        Err(AuthorizeError::SourceUnavailable)
    );

    let moved_destination = map(&[
        ("refs/heads/feature", 'e'),
        ("refs/heads/old", 'c'),
        ("refs/tags/v1", 'd'),
    ])?;
    let mut context = fixture.context(AUTO, 3_000);
    context.destination_now = Some(&moved_destination);
    assert_eq!(
        authorize(&context, Some(&approval)),
        Err(AuthorizeError::DestinationChanged)
    );
    context.destination_now = None;
    assert_eq!(
        authorize(&context, Some(&approval)),
        Err(AuthorizeError::DestinationUnavailable)
    );

    let changed_policy = SyncPolicy {
        revision: 4,
        ..AUTO
    };
    assert_eq!(
        authorize(&fixture.context(changed_policy, 3_000), Some(&approval)),
        Err(AuthorizeError::PolicyChanged)
    );
    assert_eq!(
        authorize(&fixture.context(AUTO, plan.expires_at_ms), Some(&approval)),
        Err(AuthorizeError::PlanExpired)
    );

    let mut other = approval.clone();
    other.plan_digest = "0".repeat(64);
    assert_eq!(
        authorize(&fixture.context(AUTO, 3_000), Some(&other)),
        Err(AuthorizeError::ApprovalInvalid)
    );
    let mut short = approval.clone();
    short.expires_at_ms = 2_500;
    assert_eq!(
        authorize(&fixture.context(AUTO, 3_000), Some(&short)),
        Err(AuthorizeError::ApprovalExpired)
    );
    assert_eq!(
        authorize(&fixture.context(AUTO, 3_000), None),
        Err(AuthorizeError::ApprovalRequired),
        "destructive plans never run on the automatic policy"
    );

    let mut tampered = plan.clone();
    tampered.operations[0].source = Some(oid('5')?);
    assert_eq!(
        authorize_apply(
            &tampered,
            Some(&approval),
            &fixture.context(AUTO, 3_000),
            &good,
            Some("rec-1")
        ),
        Err(AuthorizeError::PlanTampered)
    );
    Ok(())
}

#[test]
fn destructive_apply_needs_verified_recovery_of_the_old_destination() -> TestResult {
    // TEST-SNAPSHOT (gate): good, missing, corrupt, disk-full and incomplete material.
    let fixture = Fixture::destructive()?;
    let plan = fixture.plan(AUTO)?;
    let approval = approve_plan(&plan, "ap-1", &plan.plan_digest, 3, 3, 2_000, 60_000)?;
    let context = fixture.context(AUTO, 3_000);
    let good = store_with("rec-1", Ok(recovery(fixture.destination.clone())));
    let authority = authorize_apply(&plan, Some(&approval), &context, &good, Some("rec-1"))?;
    assert_eq!(authority.operations, plan.operations);
    assert!(authority.recovery.is_some());

    assert_eq!(
        authorize_apply(&plan, Some(&approval), &context, &good, None),
        Err(AuthorizeError::Recovery(GateError::RecoveryMissing))
    );
    for error in [
        StoreError::Corrupt,
        StoreError::NoSpace,
        StoreError::NotFound,
    ] {
        let bad = store_with("rec-1", Err(error));
        assert_eq!(
            authorize_apply(&plan, Some(&approval), &context, &bad, Some("rec-1")),
            Err(AuthorizeError::Recovery(GateError::RecoveryUnverified(
                error
            )))
        );
    }
    // A verified snapshot that lacks the old value of one destructive ref.
    let partial = store_with(
        "rec-1",
        Ok(recovery(map(&[
            ("refs/heads/feature", 'b'),
            ("refs/tags/v1", 'd'),
        ])?)),
    );
    assert!(matches!(
        authorize_apply(&plan, Some(&approval), &context, &partial, Some("rec-1")),
        Err(AuthorizeError::Recovery(
            GateError::OldStateNotCovered { .. }
        ))
    ));
    Ok(())
}

#[test]
fn non_destructive_plans_run_automatically_only_while_the_policy_allows() -> TestResult {
    let fixture = Fixture::non_destructive()?;
    let plan = fixture.plan(AUTO)?;
    let empty = Store(BTreeMap::new());
    let authority = authorize_apply(&plan, None, &fixture.context(AUTO, 3_000), &empty, None)?;
    assert!(authority.recovery.is_none());
    // Same revision but automatic apply switched off (a policy change must bump the
    // revision; even if it did not, the flag is honoured).
    let off = SyncPolicy {
        auto_apply_non_destructive: false,
        ..AUTO
    };
    assert_eq!(
        authorize_apply(&plan, None, &fixture.context(off, 3_000), &empty, None),
        Err(AuthorizeError::ApprovalRequired)
    );
    Ok(())
}

type Edit = Box<dyn Fn(&mut SyncPlan)>;

#[test]
fn the_plan_digest_covers_every_bound_field() -> TestResult {
    let plan = Fixture::destructive()?.plan(AUTO)?;
    let original = plan_digest(&plan);
    let mut variants: Vec<SyncPlan> = Vec::new();
    let edits: Vec<Edit> = vec![
        Box::new(|p| p.id.push('x')),
        Box::new(|p| p.repository_id.push('x')),
        Box::new(|p| p.mapping_id.push('x')),
        Box::new(|p| p.destination.gitlab_account_id.push('x')),
        Box::new(|p| p.destination.project_id += 1),
        Box::new(|p| p.last_verified_digest.push('x')),
        Box::new(|p| p.source_snapshot_id.push('x')),
        Box::new(|p| p.source_digest.push('x')),
        Box::new(|p| p.destination_digest.push('x')),
        Box::new(|p| p.policy_revision += 1),
        Box::new(|p| p.requires_approval = !p.requires_approval),
        Box::new(|p| p.pre_change_snapshot_required = !p.pre_change_snapshot_required),
        Box::new(|p| p.created_at_ms += 1),
        Box::new(|p| p.expires_at_ms += 1),
        Box::new(|p| p.operations[0].kind = OperationKind::FastForward),
        Box::new(|p| p.operations[0].expected_destination = None),
        Box::new(|p| p.operations[0].source = None),
        Box::new(|p| {
            p.operations.swap(0, 1);
        }),
    ];
    for edit in &edits {
        let mut variant = plan.clone();
        edit(&mut variant);
        variants.push(variant);
    }
    for (index, variant) in variants.iter().enumerate() {
        assert_ne!(plan_digest(variant), original, "edit {index}");
    }
    Ok(())
}

#[test]
fn a_repointed_or_already_applied_destination_invalidates_the_approval() -> TestResult {
    // Review finding 1 and 2: same refs, other project; and replay after apply.
    let fixture = Fixture::destructive()?;
    let plan = fixture.plan(AUTO)?;
    let approval = approve_plan(&plan, "ap-1", &plan.plan_digest, 3, 3, 2_000, 60_000)?;
    let good = store_with("rec-1", Ok(recovery(fixture.destination.clone())));
    let other = identity(77);
    let mut context = fixture.context(AUTO, 3_000);
    context.destination_identity = &other;
    assert_eq!(
        authorize_apply(&plan, Some(&approval), &context, &good, Some("rec-1")),
        Err(AuthorizeError::DestinationIdentityChanged)
    );
    // After the apply the app records a new verified state; someone then restores the
    // old refs outside the app. The ref digest matches again, the approval does not.
    let applied = map(&[("refs/heads/feature", '7'), ("refs/tags/v1", '8')])?;
    let mut context = fixture.context(AUTO, 3_000);
    context.last_verified_now = Some(&applied);
    assert_eq!(
        authorize_apply(&plan, Some(&approval), &context, &good, Some("rec-1")),
        Err(AuthorizeError::ManagedStateChanged)
    );
    Ok(())
}

#[test]
fn destructive_operations_need_approval_whatever_the_stored_flags_say() -> TestResult {
    let fixture = Fixture::destructive()?;
    let mut plan = fixture.plan(AUTO)?;
    plan.requires_approval = false;
    plan.pre_change_snapshot_required = false;
    plan.plan_digest = plan_digest(&plan);
    let good = store_with("rec-1", Ok(recovery(fixture.destination.clone())));
    assert_eq!(
        authorize_apply(
            &plan,
            None,
            &fixture.context(AUTO, 3_000),
            &good,
            Some("rec-1")
        ),
        Err(AuthorizeError::ApprovalRequired)
    );
    Ok(())
}

#[test]
fn lifetimes_ids_and_clock_edges_are_bounded() -> TestResult {
    let fixture = Fixture::non_destructive()?;
    let mut request = fixture.request(SourceState::Current, AUTO);
    request.ttl_ms = MAX_TTL_MS + 1;
    assert_eq!(
        build_plan(&request, &graph()),
        PlanOutcome::Blocked(PlanBlock::InvalidRequest)
    );
    for bad in ["-x", "..", ":a", ".hidden", ""] {
        let mut request = fixture.request(SourceState::Current, AUTO);
        request.plan_id = bad;
        assert_eq!(
            build_plan(&request, &graph()),
            PlanOutcome::Blocked(PlanBlock::InvalidRequest),
            "{bad:?}"
        );
    }
    let plan = fixture.plan(AUTO)?;
    let digest = plan.plan_digest.clone();
    assert_eq!(
        approve_plan(&plan, "ap-1", &digest, 3, 3, 2_000, MAX_TTL_MS + 1),
        Err(ApproveError::InvalidRequest)
    );
    assert_eq!(
        approve_plan(&plan, "-ap", &digest, 3, 3, 2_000, 60_000),
        Err(ApproveError::InvalidRequest)
    );
    // A clock before the plan was created (moved backwards) is not trusted.
    assert_eq!(
        approve_plan(&plan, "ap-1", &digest, 3, 3, 999, 60_000),
        Err(ApproveError::PlanExpired)
    );
    let approval = approve_plan(&plan, "ap-1", &digest, 3, 3, 2_000, 60_000)?;
    let empty = Store(BTreeMap::new());
    assert_eq!(
        authorize_apply(
            &plan,
            Some(&approval),
            &fixture.context(AUTO, 1_500),
            &empty,
            None
        ),
        Err(AuthorizeError::ApprovalExpired)
    );
    assert_eq!(
        authorize_apply(
            &plan,
            Some(&approval),
            &fixture.context(AUTO, 500),
            &empty,
            None
        ),
        Err(AuthorizeError::PlanExpired)
    );
    Ok(())
}

#[test]
fn a_recovery_snapshot_of_another_repository_is_rejected() -> TestResult {
    let fixture = Fixture::destructive()?;
    let plan = fixture.plan(AUTO)?;
    let approval = approve_plan(&plan, "ap-1", &plan.plan_digest, 3, 3, 2_000, 60_000)?;
    let mut foreign = recovery(fixture.destination.clone());
    foreign.repository_id = "repo-2".to_owned();
    let store = store_with("rec-1", Ok(foreign));
    assert_eq!(
        authorize_apply(
            &plan,
            Some(&approval),
            &fixture.context(AUTO, 3_000),
            &store,
            Some("rec-1")
        ),
        Err(AuthorizeError::RecoveryForeign)
    );
    Ok(())
}
