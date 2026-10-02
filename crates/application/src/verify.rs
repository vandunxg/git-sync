//! Post-write verification and scoped result persistence (REQ-07, REQ-08, REQ-42,
//! REQ-43; SYNC_ENGINE.md step 10).
//!
//! After any push attempt (applied, partial, rejected or unknown) the destination is
//! observed again and compared with the snapshot. The recorded result, the new
//! "last verified" destination state and the coverage of Git refs come only from that
//! observation. LFS, submodule and workspace coverage are separate dimensions and are
//! left untouched here. The source is re-observed cheaply: a newer source is reported
//! as "update", never as a perpetual "synced".

use gbm_domain::contract::{CoverageGitRefs, ErrorCode};
use gbm_domain::refs::{RefMap, RefName};
use gbm_domain::status::{
    JobResult, PushResult, SyncResult, Verification, sync_result, verify_destination,
};

use crate::apply::{ApplyOutcome, RefPushStatus, RejectReason};
use crate::snapshot::{Coverage, SnapshotManifest, ref_set_digest};

/// The cheap source re-observation after the write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRecheck {
    Same,
    /// The source moved on: the repository needs an update.
    Newer,
    /// Not observed (failed or skipped): freshness is unknown, never "same".
    NotObserved,
}

/// The writer's report reduced to what the verification needs.
pub fn push_result(outcome: &ApplyOutcome) -> PushResult {
    match outcome {
        ApplyOutcome::Applied(_) => PushResult::Applied,
        ApplyOutcome::Partial(_) => PushResult::Partial,
        ApplyOutcome::Rejected(report) => {
            let has = |reason| {
                report
                    .refs
                    .iter()
                    .any(|(_, s)| *s == RefPushStatus::Rejected(reason))
            };
            PushResult::Rejected {
                stale_lease: has(RejectReason::StaleLease),
                remote_rejected: has(RejectReason::RemoteRejected),
            }
        }
        ApplyOutcome::Blocked(_) => PushResult::Blocked,
        ApplyOutcome::NotStarted => PushResult::NotStarted,
        ApplyOutcome::Unknown(_) => PushResult::Unknown,
    }
}

/// The destination state the app may treat as its own after an attempt: per ref, the
/// observed value if it is the old managed value or the snapshot value (written by
/// this job), otherwise the old managed value (so a third party's change stays drift
/// for the next plan). `None` observation keeps the old state.
pub fn reconciled_managed_state(
    last_verified: Option<&RefMap>,
    snapshot: &RefMap,
    observed: Option<&RefMap>,
) -> Option<RefMap> {
    let Some(observed) = observed else {
        return last_verified.cloned();
    };
    let empty = RefMap::default();
    let old = last_verified.unwrap_or(&empty);
    let mut names: Vec<&RefName> = old
        .iter()
        .chain(snapshot.iter())
        .chain(observed.iter())
        .map(|(n, _)| n)
        .collect();
    names.sort();
    names.dedup();
    let mut entries = Vec::new();
    for name in names {
        let now = observed.get(name);
        let ours = now == old.get(name) || now == snapshot.get(name);
        let keep = if ours { now } else { old.get(name) };
        if let Some(oid) = keep {
            entries.push((name.clone(), oid.clone()));
        }
    }
    RefMap::from_entries(entries).ok()
}

/// One persisted, scoped result of a sync attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRecord {
    pub repository_id: String,
    pub plan_id: String,
    pub snapshot_id: String,
    pub snapshot_digest: String,
    /// Scoped to Git refs only. See `job_result` for the whole job.
    pub git_result: JobResult,
    pub git_error: Option<ErrorCode>,
    /// LFS and submodule scope of the snapshot (independent of Git refs).
    pub lfs: Coverage,
    pub submodules: Coverage,
    pub git_refs: CoverageGitRefs,
    /// Set only when the destination equals the snapshot.
    pub verified_at_ms: Option<i64>,
    pub observed_at_ms: i64,
    pub source_recheck: SourceRecheck,
    /// Refs that differ from the snapshot after the attempt.
    pub mismatched_refs: Vec<String>,
    /// New "last verified" destination state for the next plan.
    pub managed_state: Option<RefMap>,
}

pub struct VerifyInput<'a> {
    pub repository_id: &'a str,
    pub plan_id: &'a str,
    pub snapshot: &'a SnapshotManifest,
    pub outcome: &'a ApplyOutcome,
    /// Complete destination observation after the attempt (`None` if not complete).
    pub destination_after: Option<&'a RefMap>,
    /// Complete source observation after the attempt (`None` if not available).
    pub source_after: Option<&'a RefMap>,
    pub last_verified: Option<&'a RefMap>,
    pub now_ms: i64,
}

/// Builds the record for one attempt.
pub fn record_attempt(input: &VerifyInput<'_>) -> SyncRecord {
    let verification = verify_destination(&input.snapshot.refs, input.destination_after);
    let source_recheck = match input.source_after {
        None => SourceRecheck::NotObserved,
        Some(refs) if ref_set_digest(refs) == input.snapshot.ref_set_digest => SourceRecheck::Same,
        Some(_) => SourceRecheck::Newer,
    };
    let newer_source = source_recheck == SourceRecheck::Newer;
    let SyncResult {
        result,
        error,
        destination_verified,
        ..
    } = sync_result(push_result(input.outcome), &verification, newer_source);
    let mismatched_refs = match &verification {
        Verification::Mismatch(list) => list.iter().map(|m| m.name.to_string()).collect(),
        _ => Vec::new(),
    };
    SyncRecord {
        repository_id: input.repository_id.to_owned(),
        plan_id: input.plan_id.to_owned(),
        snapshot_id: input.snapshot.snapshot_id.clone(),
        snapshot_digest: input.snapshot.ref_set_digest.clone(),
        git_result: result,
        git_error: error,
        lfs: input.snapshot.lfs,
        submodules: input.snapshot.submodules,
        git_refs: match verification {
            Verification::Verified => CoverageGitRefs::Verified,
            Verification::Mismatch(_) => CoverageGitRefs::Failed,
            Verification::Unverified => CoverageGitRefs::NotChecked,
        },
        verified_at_ms: destination_verified.then_some(input.now_ms),
        observed_at_ms: input.now_ms,
        source_recheck,
        mismatched_refs,
        managed_state: reconciled_managed_state(
            input.last_verified,
            &input.snapshot.refs,
            input.destination_after,
        ),
    }
}

/// The whole job's terminal result: a verified Git backup is only complete when LFS
/// and submodule scope are also proven (P06 "LFS release boundary"); until then it is
/// partial with the uncovered scope named.
pub fn job_result(record: &SyncRecord) -> (JobResult, Option<ErrorCode>) {
    if record.git_result != JobResult::Succeeded {
        return (record.git_result, record.git_error);
    }
    if record.lfs != Coverage::Complete {
        return (JobResult::Partial, Some(ErrorCode::LfsIncomplete));
    }
    if record.submodules != Coverage::Complete {
        return (JobResult::Partial, Some(ErrorCode::SubmoduleUncovered));
    }
    (JobResult::Succeeded, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// Another writer committed a newer revision first.
    RevisionConflict,
    Failed,
}

/// Durable, revisioned repository state (SQLite in infrastructure).
pub trait ResultStore {
    /// Commits `record` only if the repository is still at `expected_revision`;
    /// returns the new revision. Events are emitted only after this succeeds.
    /// On `RevisionConflict` the caller re-reads the repository (incl. its last
    /// verified state), rebuilds the record with `record_attempt` and retries; a
    /// record is never dropped silently after a write.
    fn commit(&self, record: &SyncRecord, expected_revision: u64) -> Result<u64, StoreError>;
}

/// Records the attempt; success is reported to callers only after it is persisted.
pub fn persist_attempt(
    store: &dyn ResultStore,
    input: &VerifyInput<'_>,
    expected_revision: u64,
) -> Result<(SyncRecord, u64), StoreError> {
    let record = record_attempt(input);
    let revision = store.commit(&record, expected_revision)?;
    Ok((record, revision))
}

#[cfg(test)]
mod tests;
