//! Job admission, deduplication and bounded retry (REQ-09, REQ-19;
//! SCHEDULER_AND_STATE.md "Persistent job state machine", "Retry and restart").
//!
//! Pure decisions over durable job rows: which queued jobs may start now (global
//! concurrency, one active mutating job per repository and per destination mapping,
//! FIFO, retry not-before times, pause), how a logical job is identified for
//! deduplication, and whether a failed attempt gets a new attempt and when. A retry is
//! a new attempt row; the failed attempt and its evidence are kept.

use std::collections::HashSet;

use gbm_domain::contract::ErrorCode;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Network, timeout, rate limit, provider 5xx: retried with backoff.
    Transient,
    /// Needs the owner (re-authenticate, grant access, install Git/LFS, free space).
    Actionable,
    /// A write may have happened or safety rules apply: new plan/review, no retry.
    Safety,
    Unsupported,
}

pub fn classify(code: ErrorCode) -> ErrorClass {
    use ErrorCode::*;
    match code {
        NetworkUnavailable | Timeout | RateLimited => ErrorClass::Transient,
        AuthRequired | AccessDenied | KeyringUnavailable | GitUnavailable | GitLfsUnavailable
        | DiskFull | PathRejected | SourceUnavailable | PartialDiscovery => ErrorClass::Actionable,
        PlanStale | ApprovalRequired | DestinationDiverged | ProtectedRef | PushPartial
        | VerifyFailed | MirrorInvalid | CancelUncertain | WorkspaceSkipped | LfsIncomplete
        | SubmoduleUncovered => ErrorClass::Safety,
        RootRelocationUnsupported | ContractMismatch | Unsupported | Internal => {
            ErrorClass::Unsupported
        }
    }
}

/// Native retry budget (not user-unbounded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts including the first.
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    /// No retry may start later than this after the first attempt.
    pub deadline_ms: u64,
}

pub const DEFAULT_RETRY: RetryPolicy = RetryPolicy {
    max_attempts: 4,
    base_delay_ms: 30_000,
    max_delay_ms: 30 * 60_000,
    deadline_ms: 6 * 60 * 60_000,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Enqueue attempt `attempt + 1` not before this instant (ms).
    RetryAt(i64),
    /// Budget or deadline exhausted.
    GiveUp,
    /// Not retried automatically: the owner must act (auth, review, unsupported).
    NeedsOwner,
}

/// Source of jitter in `[0, 1)`. Production uses OS randomness; tests use fixed values.
pub trait Jitter {
    fn unit(&self) -> f64;
}

/// Production jitter: std's `RandomState` keys come from the OS on creation; each call
/// hashes a fresh counter value with them.
pub struct SystemJitter {
    state: std::hash::RandomState,
    counter: std::sync::atomic::AtomicU64,
}

impl SystemJitter {
    pub fn new() -> Self {
        Self {
            state: std::hash::RandomState::new(),
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl Default for SystemJitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Jitter for SystemJitter {
    fn unit(&self) -> f64 {
        use std::hash::BuildHasher;
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let bits = self.state.hash_one(n) >> 11; // 53 random bits
        bits as f64 / (1_u64 << 53) as f64
    }
}

/// Stages before any destination write. Only failures in these stages may be retried
/// automatically; any other stage (incl. an unknown one) needs reconciliation first.
pub const PRE_WRITE_STAGES: [&str; 5] =
    ["preflight", "observe_source", "fetch", "plan", "snapshot"];

/// Stages after which a destination write may already have happened.
pub const MUTATING_STAGES: [&str; 4] = ["transfer_lfs", "push", "verify", "finalize"];

/// Exponential backoff with "equal jitter", honouring provider `Retry-After`.
/// A failure outside `PRE_WRITE_STAGES` is never retried here: the destination must be
/// reconciled first (P07-T02).
#[allow(clippy::too_many_arguments)]
pub fn retry_decision(
    policy: &RetryPolicy,
    code: ErrorCode,
    stage: &str,
    attempt: u32,
    first_attempt_at_ms: i64,
    now_ms: i64,
    retry_after_seconds: Option<u64>,
    jitter: &dyn Jitter,
) -> RetryDecision {
    if classify(code) != ErrorClass::Transient || !PRE_WRITE_STAGES.contains(&stage) {
        return RetryDecision::NeedsOwner;
    }
    if attempt >= policy.max_attempts {
        return RetryDecision::GiveUp;
    }
    let exponent = attempt.saturating_sub(1).min(32);
    let delay = policy
        .base_delay_ms
        .saturating_mul(1_u64 << exponent)
        .min(policy.max_delay_ms);
    let unit = jitter.unit().clamp(0.0, 1.0);
    // delay/2 + [0, delay/2): never zero, never a synchronized storm.
    let jittered = delay / 2 + ((delay / 2) as f64 * unit) as u64;
    let floor = retry_after_seconds.map_or(0, |s| s.saturating_mul(1000));
    let wait = jittered.max(floor);
    let at = now_ms.saturating_add(i64::try_from(wait).unwrap_or(i64::MAX));
    let deadline =
        first_attempt_at_ms.saturating_add(i64::try_from(policy.deadline_ms).unwrap_or(i64::MAX));
    if at > deadline {
        RetryDecision::GiveUp
    } else {
        RetryDecision::RetryAt(at)
    }
}

/// A queued job as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedJob {
    pub id: String,
    pub repository_id: Option<String>,
    pub mapping_id: Option<String>,
    /// Earliest start (ms); `None` = now.
    pub not_before_ms: Option<i64>,
    /// Enqueue order (durable revision or sequence).
    pub sequence: i64,
}

/// A running (or cancel-requested) job as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveJob {
    pub id: String,
    pub repository_id: Option<String>,
    pub mapping_id: Option<String>,
}

/// Jobs to start now, in order. Pause starts nothing (running jobs are untouched).
pub fn select_starts(
    queued: &[QueuedJob],
    active: &[ActiveJob],
    paused: bool,
    max_concurrent: u8,
    now_ms: i64,
) -> Vec<String> {
    if paused {
        return Vec::new();
    }
    let capacity = usize::from(max_concurrent.clamp(1, 8)).saturating_sub(active.len());
    let mut busy_repos: HashSet<&str> = active
        .iter()
        .filter_map(|j| j.repository_id.as_deref())
        .collect();
    let mut busy_mappings: HashSet<&str> = active
        .iter()
        .filter_map(|j| j.mapping_id.as_deref())
        .collect();
    let mut ordered: Vec<&QueuedJob> = queued.iter().collect();
    ordered.sort_by_key(|j| (j.sequence, j.id.clone()));
    let mut starts = Vec::new();
    for job in ordered {
        if starts.len() >= capacity {
            break;
        }
        if job.not_before_ms.is_some_and(|t| t > now_ms) {
            continue;
        }
        if job
            .repository_id
            .as_deref()
            .is_some_and(|r| busy_repos.contains(r))
            || job
                .mapping_id
                .as_deref()
                .is_some_and(|m| busy_mappings.contains(m))
        {
            continue;
        }
        if let Some(r) = job.repository_id.as_deref() {
            busy_repos.insert(r);
        }
        if let Some(m) = job.mapping_id.as_deref() {
            busy_mappings.insert(m);
        }
        starts.push(job.id.clone());
    }
    starts
}

/// What makes two requests the same logical job (spec: repo, source snapshot,
/// destination mapping, policy revision; plus the kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobIdentity<'a> {
    pub kind: &'a str,
    pub repository_id: Option<&'a str>,
    /// Snapshot or plan the job works from (`None` for a fresh scan).
    pub basis: Option<&'a str>,
    pub mapping_id: Option<&'a str>,
    pub policy_revision: u64,
}

/// Stable idempotency key (`k-` + 40 hex chars, a valid contract Id).
pub fn idempotency_key(identity: &JobIdentity<'_>) -> String {
    let mut hasher = Sha256::new();
    for value in [
        "gbm-job-v1",
        identity.kind,
        identity.repository_id.unwrap_or("-"),
        identity.basis.unwrap_or("-"),
        identity.mapping_id.unwrap_or("-"),
        &identity.policy_revision.to_string(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(20).map(|b| format!("{b:02x}")).collect();
    format!("k-{hex}")
}

#[cfg(test)]
mod tests;
