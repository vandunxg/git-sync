//! Cancel, shutdown and restart reconciliation (REQ-10, REQ-19, REQ-44;
//! SCHEDULER_AND_STATE.md "Retry and restart", AUTH_AND_LIFECYCLE.md).
//!
//! After a restart every `running`/`cancel_requested` job is `interrupted` (process IDs
//! are never trusted, none are stored). Its possible effects are reconciled from its
//! durable checkpoint (`job_checkpoints`, written before each effect starts) and fresh
//! observations only:
//! - interrupted before any destination write → a new attempt with a new plan;
//! - interrupted with a persisted project-create intent → a new attempt re-runs that
//!   same intent (same marker), which looks up the exact path first (no duplicate);
//! - interrupted at or after the push → the destination is observed: equal to the
//!   snapshot = verified success; equal to the planned starting state = nothing was
//!   written (new attempt, new plan); anything else = partial, owner review;
//! - a write stage without a checkpoint, or a destination still unobservable after
//!   `RECONCILE_DEADLINE_MS`, is uncertain: the attempt fails (`VERIFY_FAILED`), its
//!   locks are released and the owner decides. The owner may also choose this at any
//!   time for a job that stays interrupted.
//!
//! The interrupted attempt is always *closed* (store: `resolve_interrupted`), never
//! re-queued: closing it expires its plan and approvals, and the next attempt starts
//! without a plan, so a previous approval is never resumed.

use gbm_domain::contract::ErrorCode;
use gbm_domain::refs::RefMap;
use serde_json::{Value, json};

use crate::destination::{CreateIntent, Visibility};
use crate::plan::SyncPlan;
use crate::scheduler::PRE_WRITE_STAGES;
use crate::snapshot::ref_set_digest;

/// A job that stays unobservable this long after it was interrupted is escalated to
/// the owner instead of holding its repository and mapping locks indefinitely.
pub const RECONCILE_DEADLINE_MS: i64 = 24 * 60 * 60_000;

/// Durable checkpoint as stored (`kind`, JSON `payload`). It holds identifiers and the
/// create intent only — never command argv, URLs with credentials or tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointRecord {
    ProjectCreate(CreateIntent),
    Push {
        plan_id: String,
        snapshot_id: String,
    },
}

impl CheckpointRecord {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ProjectCreate(_) => "project_create",
            Self::Push { .. } => "push",
        }
    }

    pub fn payload(&self) -> String {
        match self {
            Self::ProjectCreate(intent) => json!({
                "namespaceId": intent.namespace_id,
                "namespaceFullPath": intent.namespace_full_path,
                "path": intent.path,
                "visibility": intent.visibility.as_str(),
                "sourcePrivate": intent.source_private,
                "marker": intent.marker,
            }),
            Self::Push {
                plan_id,
                snapshot_id,
            } => json!({ "planId": plan_id, "snapshotId": snapshot_id }),
        }
        .to_string()
    }

    /// Strict decode: unknown kinds, missing or extra fields are rejected (`None`), and
    /// the caller then treats the job as having no readable checkpoint.
    pub fn decode(kind: &str, payload: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(payload).ok()?;
        let object = value.as_object()?;
        let text = |key: &str| object.get(key)?.as_str().map(str::to_owned);
        match kind {
            "project_create" if object.len() == 6 => Some(Self::ProjectCreate(CreateIntent {
                namespace_id: object.get("namespaceId")?.as_u64()?,
                namespace_full_path: text("namespaceFullPath")?,
                path: text("path")?,
                visibility: Visibility::parse(object.get("visibility")?.as_str()?)?,
                source_private: object.get("sourcePrivate")?.as_bool()?,
                marker: text("marker")?,
            })),
            "push" if object.len() == 2 => Some(Self::Push {
                plan_id: text("planId")?,
                snapshot_id: text("snapshotId")?,
            }),
            _ => None,
        }
    }
}

/// What the interrupted job had reached, from its durable checkpoint (loaded and
/// resolved by the caller).
pub enum Checkpoint<'a> {
    /// No checkpoint: no destination effect was recorded as started.
    BeforeWrite,
    /// A GitLab project create intent was persisted (and maybe sent).
    ProjectCreate(&'a CreateIntent),
    /// A push of `plan` may have started.
    Push {
        plan: &'a SyncPlan,
        snapshot_refs: &'a RefMap,
    },
}

impl Checkpoint<'_> {
    /// Whether a destination effect may have started in `stage`.
    fn write_may_have_started(&self, stage: &str) -> bool {
        !matches!(self, Self::BeforeWrite) || !PRE_WRITE_STAGES.contains(&stage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciliation {
    /// Close the attempt; a new attempt must build a new plan.
    NewAttempt,
    /// Close the attempt; a new attempt re-runs `ensure_project` with the same
    /// persisted intent (lookup first).
    RerunCreateIntent,
    /// The destination equals the snapshot. The caller records it through
    /// `verify::record_attempt` (push outcome unknown, the fresh observation and source
    /// recheck), which decides synced vs. update; only then is the attempt closed.
    VerifiedComplete,
    /// Some refs changed, not all: record partial and require review.
    PartialNeedsReview,
    /// Cannot be decided and will not become decidable: fail, release, owner decides.
    Uncertain,
    /// The destination could not be observed yet: keep the job interrupted (it keeps
    /// its locks) until `RECONCILE_DEADLINE_MS` or an owner decision.
    KeepInterrupted,
}

/// Decides how an interrupted job is reconciled. `stage` is the stored stage;
/// `destination_now` is a fresh, complete destination observation, if available;
/// `interrupted_for_ms` is the time since the job was marked interrupted.
pub fn reconcile_interrupted(
    stage: &str,
    checkpoint: &Checkpoint<'_>,
    destination_now: Option<&RefMap>,
    interrupted_for_ms: i64,
) -> Reconciliation {
    match checkpoint {
        Checkpoint::ProjectCreate(_) => Reconciliation::RerunCreateIntent,
        Checkpoint::BeforeWrite if PRE_WRITE_STAGES.contains(&stage) => Reconciliation::NewAttempt,
        // A stage past the pre-write list without a push checkpoint cannot be judged,
        // now or later.
        Checkpoint::BeforeWrite => Reconciliation::Uncertain,
        Checkpoint::Push {
            plan,
            snapshot_refs,
        } => {
            let Some(destination) = destination_now else {
                return if interrupted_for_ms >= RECONCILE_DEADLINE_MS {
                    Reconciliation::Uncertain
                } else {
                    Reconciliation::KeepInterrupted
                };
            };
            let now = ref_set_digest(destination);
            if now == ref_set_digest(snapshot_refs) {
                Reconciliation::VerifiedComplete
            } else if now == plan.destination_digest {
                Reconciliation::NewAttempt
            } else {
                Reconciliation::PartialNeedsReview
            }
        }
    }
}

/// Outcome the interrupted attempt is closed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedAs {
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closing {
    pub state: ClosedAs,
    pub error: Option<ErrorCode>,
    /// Queue the next attempt of the same logical job (without a plan).
    pub new_attempt: bool,
}

impl Reconciliation {
    /// How the store closes the attempt (`None`: leave it interrupted).
    pub fn closing(&self) -> Option<Closing> {
        let close = |state, error, new_attempt| {
            Some(Closing {
                state,
                error,
                new_attempt,
            })
        };
        match self {
            Self::NewAttempt | Self::RerunCreateIntent => {
                close(ClosedAs::Failed, Some(ErrorCode::PlanStale), true)
            }
            Self::VerifiedComplete => close(ClosedAs::Succeeded, None, false),
            Self::PartialNeedsReview => {
                close(ClosedAs::Partial, Some(ErrorCode::PushPartial), false)
            }
            Self::Uncertain => close(ClosedAs::Failed, Some(ErrorCode::VerifyFailed), false),
            Self::KeepInterrupted => None,
        }
    }
}

/// How the job behaved after a cancel request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelObservation {
    /// The token was cancelled before the next child was spawned.
    BeforeStart,
    /// The child tree was stopped and reaped.
    Stopped,
    /// The child exited on its own (finished before the signal took effect).
    Finished,
    /// The child could not be confirmed stopped/reaped.
    NotConfirmed,
}

/// Terminal result of a cancelled job. A write that may have started — in this or an
/// earlier child of the attempt, or a persisted create intent — makes the outcome
/// uncertain (`CANCEL_UNCERTAIN`): the destination must be reconciled before anything
/// else runs against it. "Canceled" is only reported once the process is confirmed
/// stopped; otherwise the job stays `cancel_requested` (UI: "Canceling").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelResult {
    Canceled,
    CanceledUncertain(ErrorCode),
    /// Not confirmed stopped: keep `cancel_requested`.
    StillCanceling,
    /// The work completed before the cancel took effect: keep its real result.
    CompletedAnyway,
}

pub fn cancel_result(
    stage: &str,
    checkpoint: &Checkpoint<'_>,
    observation: CancelObservation,
) -> CancelResult {
    match observation {
        CancelObservation::NotConfirmed => CancelResult::StillCanceling,
        CancelObservation::Finished => CancelResult::CompletedAnyway,
        CancelObservation::BeforeStart | CancelObservation::Stopped
            if checkpoint.write_may_have_started(stage) =>
        {
            CancelResult::CanceledUncertain(ErrorCode::CancelUncertain)
        }
        CancelObservation::BeforeStart | CancelObservation::Stopped => CancelResult::Canceled,
    }
}

/// Coalesces scans after suspend/resume or offline time: at most one scan is due,
/// however many intervals were missed. A last scan "in the future" (wall clock moved
/// backwards) makes a scan due instead of suppressing scans until the clock catches up.
pub fn scan_due(last_scan_ms: Option<i64>, interval_ms: i64, now_ms: i64) -> bool {
    match last_scan_ms {
        None => true,
        Some(last) if now_ms < last => true,
        Some(last) => now_ms.saturating_sub(last) >= interval_ms.max(1),
    }
}

#[cfg(test)]
mod tests;
