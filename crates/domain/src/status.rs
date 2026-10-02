//! Post-write verification and repository status aggregation (REQ-07, REQ-08,
//! REQ-42, REQ-43; SCHEDULER_AND_STATE.md "Status dimensions"; SYNC_ENGINE.md step 10).
//!
//! Verification compares the destination's complete selected refs after a push with
//! the captured source snapshot, ref by ref (full name + full OID). A Git exit code is
//! never evidence. Dimensions stay independent; the primary badge follows one fixed
//! precedence, and a workspace state can never change it.

use crate::contract::{
    Coverage, CoverageGitRefs, CoverageLfs, CoverageLocalObjects, CoverageSubmodules, ErrorCode,
    RepositorySummaryDestinationState as DestinationState,
    RepositorySummaryMirrorState as MirrorState, RepositorySummaryPrimaryStatus as PrimaryStatus,
    RepositorySummarySourceState as SourceState,
};
use crate::refs::{ObjectId, RefMap, RefName};

/// One ref whose destination value is not the snapshot value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefMismatch {
    pub name: RefName,
    pub expected: Option<ObjectId>,
    pub actual: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verification {
    /// The destination's selected refs equal the snapshot exactly.
    Verified,
    Mismatch(Vec<RefMismatch>),
    /// The destination could not be observed completely: nothing is claimed.
    Unverified,
}

/// Compares a complete destination observation with the snapshot refs.
pub fn verify_destination(snapshot: &RefMap, destination: Option<&RefMap>) -> Verification {
    let Some(destination) = destination else {
        return Verification::Unverified;
    };
    let mut names: Vec<&RefName> = snapshot
        .iter()
        .chain(destination.iter())
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names.dedup();
    let mismatches: Vec<RefMismatch> = names
        .into_iter()
        .filter(|name| snapshot.get(name) != destination.get(name))
        .map(|name| RefMismatch {
            name: name.clone(),
            expected: snapshot.get(name).cloned(),
            actual: destination.get(name).cloned(),
        })
        .collect();
    if mismatches.is_empty() {
        Verification::Verified
    } else {
        Verification::Mismatch(mismatches)
    }
}

/// What the writer reported (mirrors the application's apply outcome, without data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    Applied,
    Rejected {
        stale_lease: bool,
        remote_rejected: bool,
    },
    Partial,
    Blocked,
    NotStarted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobResult {
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncResult {
    pub result: JobResult,
    pub error: Option<ErrorCode>,
    /// The destination equals the snapshot (by observation, not by exit code).
    pub destination_verified: bool,
    /// The source was observed again after the write and differs from the snapshot:
    /// the backup of the snapshot may be correct, but the repository needs an update.
    pub newer_source: bool,
}

/// Combines the writer's report with the post-write observation. Success requires a
/// verified destination; a newer source never turns a verified result into "synced
/// forever" but is reported separately.
pub fn sync_result(
    push: PushResult,
    verification: &Verification,
    newer_source: bool,
) -> SyncResult {
    let verified = *verification == Verification::Verified;
    let (result, error) = match (push, verified) {
        // Whatever Git said, an exact match is a verified backup of the snapshot.
        (_, true) => (JobResult::Succeeded, None),
        (PushResult::Applied, false) => (JobResult::Failed, Some(ErrorCode::VerifyFailed)),
        (PushResult::Partial, false) => (JobResult::Partial, Some(ErrorCode::PushPartial)),
        (PushResult::Unknown, false) => (JobResult::Failed, Some(ErrorCode::VerifyFailed)),
        (
            PushResult::Rejected {
                stale_lease: true, ..
            },
            false,
        ) => (JobResult::Failed, Some(ErrorCode::PlanStale)),
        (
            PushResult::Rejected {
                remote_rejected: true,
                ..
            },
            false,
        ) => (JobResult::Failed, Some(ErrorCode::ProtectedRef)),
        (PushResult::Rejected { .. }, false) => (JobResult::Failed, Some(ErrorCode::VerifyFailed)),
        (PushResult::Blocked, false) => (JobResult::Failed, Some(ErrorCode::Unsupported)),
        (PushResult::NotStarted, false) => (JobResult::Failed, Some(ErrorCode::Internal)),
    };
    SyncResult {
        result,
        error,
        destination_verified: verified,
        newer_source,
    }
}

/// Destination dimension from a verification (`None` = not observable).
pub fn destination_state(
    verification: &Verification,
    project_exists: Option<bool>,
    drift_or_rewrite: bool,
) -> DestinationState {
    match (project_exists, verification) {
        (Some(false), _) => DestinationState::Missing,
        (None, _) | (_, Verification::Unverified) => DestinationState::Unknown,
        (Some(true), Verification::Verified) => DestinationState::Current,
        (Some(true), Verification::Mismatch(_)) if drift_or_rewrite => DestinationState::Diverged,
        (Some(true), Verification::Mismatch(_)) => DestinationState::Behind,
    }
}

/// Every independent dimension of one repository at one revision.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusInputs {
    pub source: SourceState,
    pub mirror: MirrorState,
    pub destination: DestinationState,
    /// Shown separately; never part of the primary precedence.
    pub workspace_dirty_or_skipped: bool,
    pub coverage: Coverage,
    /// A job for this repository is queued or running.
    pub job_active: bool,
    /// The last attempt failed and the destination was not verified afterwards.
    pub last_attempt_failed_unverified: bool,
    /// An actionable blocker (auth, access, approval, protected ref, drift, LFS...).
    pub blocker: Option<ErrorCode>,
}

fn lfs_complete(lfs: CoverageLfs) -> bool {
    matches!(
        lfs,
        CoverageLfs::NotApplicable | CoverageLfs::Uploaded | CoverageLfs::RestoreVerified
    )
}

/// Primary badge precedence: running → blocked → failed (unverified) → missing →
/// update → stale/unknown → synced. Workspace state is ignored here.
pub fn primary_status(inputs: &StatusInputs) -> PrimaryStatus {
    if inputs.job_active {
        return PrimaryStatus::Running;
    }
    let coverage_blocker = inputs.coverage.lfs == CoverageLfs::Incomplete
        || inputs.coverage.submodules == CoverageSubmodules::Incomplete
        || inputs.mirror == MirrorState::Invalid;
    if inputs.blocker.is_some()
        || coverage_blocker
        || inputs.destination == DestinationState::Diverged
        || inputs.destination == DestinationState::Inaccessible
        || inputs.source == SourceState::Unavailable
    {
        return PrimaryStatus::Blocked;
    }
    if inputs.last_attempt_failed_unverified
        || inputs.coverage.git_refs == CoverageGitRefs::Failed
        || inputs.coverage.local_objects == CoverageLocalObjects::Failed
    {
        return PrimaryStatus::Failed;
    }
    if inputs.mirror == MirrorState::Missing || inputs.destination == DestinationState::Missing {
        return PrimaryStatus::Missing;
    }
    if inputs.source == SourceState::Changed || inputs.destination == DestinationState::Behind {
        return PrimaryStatus::Update;
    }
    if inputs.source == SourceState::Stale {
        return PrimaryStatus::Stale;
    }
    let all_verified = inputs.source == SourceState::Current
        && inputs.mirror == MirrorState::Current
        && inputs.destination == DestinationState::Current
        && inputs.coverage.git_refs == CoverageGitRefs::Verified
        && inputs.coverage.local_objects == CoverageLocalObjects::Verified
        && lfs_complete(inputs.coverage.lfs);
    if all_verified {
        PrimaryStatus::Synced
    } else {
        PrimaryStatus::Unknown
    }
}

#[cfg(test)]
mod tests;
