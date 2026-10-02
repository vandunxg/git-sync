//! Immutable sync plans and approval validation (REQ-20, REQ-21; SYNC_ENGINE.md
//! "Approval contract"; IPC.md "Job submission and approval").
//!
//! A plan is built only from a captured snapshot whose source is *current* (latest
//! observation complete and equal to the snapshot), plus a complete destination
//! observation. It binds the source digest, destination digest, policy revision,
//! mapping, expiry and the exact operations with their leases into `plan_digest`.
//! An approval binds that digest and policy revision with its own expiry.
//!
//! `authorize_apply` is the last check before any write: it recomputes the plan
//! digest, re-checks expiry, policy revision, a fresh source and destination digest,
//! the approval (or the still-enabled automatic policy for non-destructive plans), and
//! for destructive plans a recovery snapshot that verifies now and holds every old
//! destination value. Any drift invalidates the plan; nothing is "refreshed" in place.

use std::fmt;

use gbm_domain::contract::RepositorySummarySourceState as SourceState;
use gbm_domain::plan::{
    AncestryOracle, Classification, Drift, OperationKind, RefOperation, classify,
};
use gbm_domain::refs::{RefError, RefMap};
use sha2::{Digest, Sha256};

use crate::recovery::{
    ChangeKind, GateError, PlannedChange, RecoverySnapshot, SnapshotStore, require_recovery,
};
use crate::snapshot::{SnapshotManifest, ref_set_digest};

/// The owner's sync policy. Any change bumps `revision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncPolicy {
    pub revision: u64,
    /// Create/fast-forward plans on a managed destination may run without a manual
    /// approval. Destructive plans always need one.
    pub auto_apply_non_destructive: bool,
}

/// Upper bound for plan and approval lifetimes.
pub const MAX_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// The exact destination a plan writes to. A re-pointed mapping (other project or
/// account) invalidates the plan even if the ref maps happen to be identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationIdentity {
    pub gitlab_account_id: String,
    pub project_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub id: String,
    pub repository_id: String,
    pub mapping_id: String,
    pub destination: DestinationIdentity,
    /// Digest of the destination state last verified after the app's own write
    /// (`"none"` when never written). A later re-verification changes it, so an
    /// approval cannot be replayed after the plan was applied.
    pub last_verified_digest: String,
    pub source_snapshot_id: String,
    pub source_digest: String,
    pub destination_digest: String,
    pub policy_revision: u64,
    pub operations: Vec<RefOperation>,
    pub requires_approval: bool,
    pub pre_change_snapshot_required: bool,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub plan_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanBlock {
    /// Source not observed completely and equal to the snapshot (unknown, changed,
    /// stale, or unavailable — e.g. 404 under a changed token). Never a delete.
    SourceNotCurrent(SourceState),
    /// Destination differs from what the app last verified there.
    DestinationDrift(Vec<Drift>),
    ObjectFormatMismatch,
    InvalidRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanOutcome {
    Planned(Box<SyncPlan>),
    Blocked(PlanBlock),
}

pub struct PlanRequest<'a> {
    pub plan_id: &'a str,
    pub mapping_id: &'a str,
    pub snapshot: &'a SnapshotManifest,
    /// `SourceView::source_state` against `snapshot.refs`, computed by the caller.
    pub source_state: SourceState,
    /// A complete observation of the destination's selected refs.
    pub destination: &'a RefMap,
    pub destination_identity: &'a DestinationIdentity,
    /// Destination refs as last verified after the app's own write (`None` = never).
    pub last_verified: Option<&'a RefMap>,
    pub policy: SyncPolicy,
    pub now_ms: i64,
    pub ttl_ms: i64,
}

/// Contract `Id` pattern: `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$`.
fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    id.len() <= 128
        && bytes.next().is_some_and(|b| b.is_ascii_alphanumeric())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

fn verified_digest(last_verified: Option<&RefMap>) -> String {
    last_verified.map_or_else(|| "none".to_owned(), ref_set_digest)
}

/// Builds a plan, or explains why none may be built. Pure apart from the ancestry
/// oracle (read-only Git queries in production).
pub fn build_plan(request: &PlanRequest<'_>, ancestry: &dyn AncestryOracle) -> PlanOutcome {
    if !valid_id(request.plan_id)
        || !valid_id(request.mapping_id)
        || !valid_id(&request.destination_identity.gitlab_account_id)
        || request.ttl_ms <= 0
        || request.ttl_ms > MAX_TTL_MS
        || request.snapshot.ref_set_digest != ref_set_digest(&request.snapshot.refs)
    {
        return PlanOutcome::Blocked(PlanBlock::InvalidRequest);
    }
    if request.source_state != SourceState::Current {
        return PlanOutcome::Blocked(PlanBlock::SourceNotCurrent(request.source_state));
    }
    let operations = match classify(
        &request.snapshot.refs,
        request.destination,
        request.last_verified,
        ancestry,
    ) {
        Ok(Classification::Operations(operations)) => operations,
        Ok(Classification::DestinationDrift(drift)) => {
            return PlanOutcome::Blocked(PlanBlock::DestinationDrift(drift));
        }
        Err(RefError::MixedObjectFormat) => {
            return PlanOutcome::Blocked(PlanBlock::ObjectFormatMismatch);
        }
        Err(_) => return PlanOutcome::Blocked(PlanBlock::InvalidRequest),
    };
    let destructive = operations.iter().any(|op| op.kind.is_destructive());
    let mut plan = SyncPlan {
        id: request.plan_id.to_owned(),
        repository_id: request.snapshot.repository_id.clone(),
        mapping_id: request.mapping_id.to_owned(),
        destination: request.destination_identity.clone(),
        last_verified_digest: verified_digest(request.last_verified),
        source_snapshot_id: request.snapshot.snapshot_id.clone(),
        source_digest: request.snapshot.ref_set_digest.clone(),
        destination_digest: ref_set_digest(request.destination),
        policy_revision: request.policy.revision,
        requires_approval: destructive || !request.policy.auto_apply_non_destructive,
        pre_change_snapshot_required: destructive,
        operations,
        created_at_ms: request.now_ms,
        expires_at_ms: request.now_ms.saturating_add(request.ttl_ms),
        plan_digest: String::new(),
    };
    plan.plan_digest = plan_digest(&plan);
    PlanOutcome::Planned(Box::new(plan))
}

/// SHA-256 over a length-prefixed encoding of every field except the digest itself.
pub fn plan_digest(plan: &SyncPlan) -> String {
    let mut hasher = Sha256::new();
    let mut field = |value: &str| {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    };
    field("gbm-sync-plan-v1");
    field(&plan.id);
    field(&plan.repository_id);
    field(&plan.mapping_id);
    field(&plan.destination.gitlab_account_id);
    field(&plan.destination.project_id.to_string());
    field(&plan.last_verified_digest);
    field(&plan.source_snapshot_id);
    field(&plan.source_digest);
    field(&plan.destination_digest);
    field(&plan.policy_revision.to_string());
    field(if plan.requires_approval {
        "approval"
    } else {
        "auto"
    });
    field(if plan.pre_change_snapshot_required {
        "snapshot"
    } else {
        "no-snapshot"
    });
    field(&plan.created_at_ms.to_string());
    field(&plan.expires_at_ms.to_string());
    field(&plan.operations.len().to_string());
    for op in &plan.operations {
        field(op.kind.as_str());
        field(op.name.as_str());
        field(op.expected_destination.as_ref().map_or("-", |o| o.as_str()));
        field(op.source.as_ref().map_or("-", |o| o.as_str()));
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Native approval record (never a renderer boolean).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub policy_revision: u64,
    pub approved_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApproveError {
    InvalidRequest,
    PlanTampered,
    PlanExpired,
    /// The renderer approved a different plan than the stored one.
    DigestMismatch,
    /// The policy changed since the plan (or the renderer's view) was made.
    PolicyChanged,
}

impl fmt::Display for ApproveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "invalid approval request",
            Self::PlanTampered => "the stored plan does not match its digest",
            Self::PlanExpired => "the plan has expired; create a new plan",
            Self::DigestMismatch => "the reviewed plan is not the current plan",
            Self::PolicyChanged => "the sync policy changed; create a new plan",
        })
    }
}

impl std::error::Error for ApproveError {}

/// Records an approval for the stored `plan`, given what the renderer showed.
/// Expires at the earlier of `now + ttl` and the plan's own expiry.
pub fn approve_plan(
    plan: &SyncPlan,
    approval_id: &str,
    expected_plan_digest: &str,
    expected_policy_revision: u64,
    current_policy_revision: u64,
    now_ms: i64,
    ttl_ms: i64,
) -> Result<Approval, ApproveError> {
    if !valid_id(approval_id) || ttl_ms <= 0 || ttl_ms > MAX_TTL_MS {
        return Err(ApproveError::InvalidRequest);
    }
    if plan_digest(plan) != plan.plan_digest {
        return Err(ApproveError::PlanTampered);
    }
    if now_ms >= plan.expires_at_ms || now_ms < plan.created_at_ms {
        return Err(ApproveError::PlanExpired);
    }
    if expected_plan_digest != plan.plan_digest {
        return Err(ApproveError::DigestMismatch);
    }
    if expected_policy_revision != plan.policy_revision
        || current_policy_revision != plan.policy_revision
    {
        return Err(ApproveError::PolicyChanged);
    }
    Ok(Approval {
        id: approval_id.to_owned(),
        plan_id: plan.id.clone(),
        plan_digest: plan.plan_digest.clone(),
        policy_revision: plan.policy_revision,
        approved_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms).min(plan.expires_at_ms),
    })
}

/// State observed immediately before applying.
pub struct ApplyContext<'a> {
    /// A fresh complete source observation, or `None` if it is not complete/available.
    pub source_now: Option<&'a RefMap>,
    /// A fresh complete destination observation, or `None`.
    pub destination_now: Option<&'a RefMap>,
    /// The destination the mapping points to now.
    pub destination_identity: &'a DestinationIdentity,
    /// The destination state last verified by the app, as stored now.
    pub last_verified_now: Option<&'a RefMap>,
    pub policy: SyncPolicy,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizeError {
    PlanTampered,
    PlanExpired,
    PolicyChanged,
    SourceUnavailable,
    SourceChanged,
    DestinationUnavailable,
    DestinationChanged,
    /// The mapping now points to another project or account.
    DestinationIdentityChanged,
    /// The app verified a newer destination state since the plan (e.g. it was applied).
    ManagedStateChanged,
    ApprovalRequired,
    ApprovalInvalid,
    ApprovalExpired,
    Recovery(GateError),
    /// The recovery snapshot belongs to another repository.
    RecoveryForeign,
}

impl fmt::Display for AuthorizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PlanTampered => "the stored plan does not match its digest",
            Self::PlanExpired => "the plan has expired; create a new plan",
            Self::PolicyChanged => "the sync policy changed; create a new plan",
            Self::SourceUnavailable => "the source could not be observed completely",
            Self::SourceChanged => "the source changed since the plan was made",
            Self::DestinationUnavailable => "the destination could not be observed completely",
            Self::DestinationChanged => "the destination changed since the plan was made",
            Self::DestinationIdentityChanged => "the plan was made for another destination",
            Self::ManagedStateChanged => "the destination was updated since the plan was made",
            Self::ApprovalRequired => "this plan needs an approval",
            Self::ApprovalInvalid => "the approval does not match this plan",
            Self::ApprovalExpired => "the approval has expired",
            Self::Recovery(_) => "verified recovery material is missing for this change",
            Self::RecoveryForeign => "the recovery snapshot belongs to another repository",
        })
    }
}

impl std::error::Error for AuthorizeError {}

/// What the writer may do: exactly these operations, each with its lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyAuthority {
    pub plan_id: String,
    pub plan_digest: String,
    pub operations: Vec<RefOperation>,
    pub recovery: Option<RecoverySnapshot>,
}

fn change_kind(kind: OperationKind) -> ChangeKind {
    match kind {
        OperationKind::Create => ChangeKind::Create,
        OperationKind::FastForward => ChangeKind::FastForward,
        OperationKind::Rewrite => ChangeKind::Rewrite,
        OperationKind::MoveTag => ChangeKind::TagMove,
        OperationKind::DeleteRef => ChangeKind::Delete,
    }
}

/// The final gate before any destination write. See the module documentation.
pub fn authorize_apply(
    plan: &SyncPlan,
    approval: Option<&Approval>,
    context: &ApplyContext<'_>,
    store: &dyn SnapshotStore,
    recovery_snapshot_id: Option<&str>,
) -> Result<ApplyAuthority, AuthorizeError> {
    if plan_digest(plan) != plan.plan_digest {
        return Err(AuthorizeError::PlanTampered);
    }
    if context.now_ms >= plan.expires_at_ms || context.now_ms < plan.created_at_ms {
        return Err(AuthorizeError::PlanExpired);
    }
    if *context.destination_identity != plan.destination {
        return Err(AuthorizeError::DestinationIdentityChanged);
    }
    if verified_digest(context.last_verified_now) != plan.last_verified_digest {
        return Err(AuthorizeError::ManagedStateChanged);
    }
    if context.policy.revision != plan.policy_revision {
        return Err(AuthorizeError::PolicyChanged);
    }
    let source = context
        .source_now
        .ok_or(AuthorizeError::SourceUnavailable)?;
    if ref_set_digest(source) != plan.source_digest {
        return Err(AuthorizeError::SourceChanged);
    }
    let destination = context
        .destination_now
        .ok_or(AuthorizeError::DestinationUnavailable)?;
    if ref_set_digest(destination) != plan.destination_digest {
        return Err(AuthorizeError::DestinationChanged);
    }
    let destructive = plan.operations.iter().any(|op| op.kind.is_destructive());
    let needs_approval =
        plan.requires_approval || destructive || !context.policy.auto_apply_non_destructive;
    match approval {
        Some(approval) => {
            if approval.plan_id != plan.id
                || approval.plan_digest != plan.plan_digest
                || approval.policy_revision != plan.policy_revision
            {
                return Err(AuthorizeError::ApprovalInvalid);
            }
            if context.now_ms >= approval.expires_at_ms || context.now_ms < approval.approved_at_ms
            {
                return Err(AuthorizeError::ApprovalExpired);
            }
        }
        None if needs_approval => return Err(AuthorizeError::ApprovalRequired),
        None => {}
    }
    let changes: Vec<PlannedChange> = plan
        .operations
        .iter()
        .map(|op| PlannedChange {
            name: op.name.clone(),
            old: op.expected_destination.clone(),
            new: op.source.clone(),
            kind: change_kind(op.kind),
        })
        .collect();
    let recovery = require_recovery(&changes, store, recovery_snapshot_id)
        .map_err(AuthorizeError::Recovery)?;
    if recovery
        .as_ref()
        .is_some_and(|snapshot| snapshot.repository_id != plan.repository_id)
    {
        return Err(AuthorizeError::RecoveryForeign);
    }
    Ok(ApplyAuthority {
        plan_id: plan.id.clone(),
        plan_digest: plan.plan_digest.clone(),
        operations: plan.operations.clone(),
        recovery,
    })
}

#[cfg(test)]
mod tests;
