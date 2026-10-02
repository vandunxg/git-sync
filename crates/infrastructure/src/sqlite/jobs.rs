//! Durable jobs: idempotent enqueue, compare-and-set transitions, one active
//! mutating job per repository (unique partial index) and restart recovery.

use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use gbm_application::time::rfc3339_to_ms;
use sqlx::{Sqlite, Transaction};

use super::{Database, StoreError, bump_revision, finish, require_id, require_timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    Scan,
    Plan,
    Sync,
    RestoreTest,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Plan => "plan",
            Self::Sync => "sync",
            Self::RestoreTest => "restore_test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobState {
    Queued,
    Running,
    WaitingForApproval,
    WaitingForRetry,
    CancelRequested,
    Succeeded,
    Partial,
    Failed,
    Canceled,
    Interrupted,
}

const ALL_STATES: [JobState; 10] = [
    JobState::Queued,
    JobState::Running,
    JobState::WaitingForApproval,
    JobState::WaitingForRetry,
    JobState::CancelRequested,
    JobState::Succeeded,
    JobState::Partial,
    JobState::Failed,
    JobState::Canceled,
    JobState::Interrupted,
];

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::WaitingForRetry => "waiting_for_retry",
            Self::CancelRequested => "cancel_requested",
            Self::Succeeded => "succeeded",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Interrupted => "interrupted",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        ALL_STATES.into_iter().find(|s| s.as_str() == text)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Partial | Self::Failed | Self::Canceled
        )
    }
}

/// Persistence-level transition table (SCHEDULER_AND_STATE.md "Persistent job state
/// machine"). Only the spec's listed edges plus the cancel request are allowed; edges
/// into/out of `waiting_for_approval` / `waiting_for_retry` are deliberately absent until
/// the scheduler task defines them with owner ratification (P03-T01 review F6).
pub fn allowed_transition(from: JobState, to: JobState) -> bool {
    use JobState::*;
    matches!(
        (from, to),
        (Queued, Running | Canceled)
            | (Running, Succeeded | Partial | Failed | Canceled | Interrupted)
            // "cancel_requested is a request state, not a claim the process stopped":
            // the job may still end in any outcome, or be interrupted by a restart.
            | (Running, CancelRequested)
            | (CancelRequested, Canceled | Succeeded | Partial | Failed | Interrupted)
            // Only through `resolve_interrupted`, after reconciliation: it closes the
            // attempt with its reconciled outcome and revokes its plan's approvals. The
            // spec's "interrupted -> queued" is a *new* linked attempt row with no plan
            // (the stale plan/approval is never resumed), never this row re-queued.
            | (Interrupted, Succeeded | Partial | Failed)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJob<'a> {
    pub id: &'a str,
    pub repository_id: Option<&'a str>,
    pub account_id: Option<&'a str>,
    pub plan_id: Option<&'a str>,
    /// Destination mapping a mutating job writes to (shared destination lock).
    pub mapping_id: Option<&'a str>,
    /// Earliest start of a retry attempt (RFC 3339 UTC).
    pub not_before: Option<&'a str>,
    pub kind: JobKind,
    pub stage: &'a str,
    pub attempt: i64,
    pub idempotency_key: &'a str,
    pub now: &'a str,
    pub coverage_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enqueued {
    /// New job; the committed revision.
    New(i64),
    /// The same logical job is already pending or running.
    Existing(String),
    /// The latest attempt failed or partially pushed: no automatic re-run.
    NeedsOwner(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedRow {
    pub id: String,
    pub repository_id: Option<String>,
    pub mapping_id: Option<String>,
    /// Earliest start in Unix ms (`None` = now). An unreadable stored value is
    /// reported as `Corrupt`, never as "now".
    pub not_before_ms: Option<i64>,
    pub sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRow {
    pub id: String,
    pub repository_id: Option<String>,
    pub mapping_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecord {
    pub id: String,
    pub repository_id: Option<String>,
    pub kind: String,
    pub state: JobState,
    pub stage: String,
    pub attempt: i64,
    pub idempotency_key: String,
    pub error_code: Option<String>,
    pub updated_at: String,
    pub revision: i64,
}

const STAGES: [&str; 10] = [
    "preflight",
    "observe_source",
    "fetch",
    "plan",
    "snapshot",
    "transfer_lfs",
    "push",
    "verify",
    "workspace",
    "finalize",
];

fn require_stage(stage: &str) -> Result<(), StoreError> {
    if STAGES.contains(&stage) {
        Ok(())
    } else {
        Err(StoreError::InvalidInput("stage"))
    }
}

pub(crate) fn require_error_code(code: Option<&str>) -> Result<(), StoreError> {
    match code {
        None => Ok(()),
        Some(c)
            if !c.is_empty()
                && c.len() <= 64
                && c.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') =>
        {
            Ok(())
        }
        Some(_) => Err(StoreError::InvalidInput("error_code")),
    }
}

fn record(row: &SqliteRow) -> Result<JobRecord, StoreError> {
    let state: String = row.try_get("state").map_err(StoreError::from_sqlx)?;
    Ok(JobRecord {
        id: row.try_get("id").map_err(StoreError::from_sqlx)?,
        repository_id: row
            .try_get("repository_id")
            .map_err(StoreError::from_sqlx)?,
        kind: row.try_get("kind").map_err(StoreError::from_sqlx)?,
        state: JobState::parse(&state).ok_or(StoreError::Corrupt)?,
        stage: row.try_get("stage").map_err(StoreError::from_sqlx)?,
        attempt: row.try_get("attempt").map_err(StoreError::from_sqlx)?,
        idempotency_key: row
            .try_get("idempotency_key")
            .map_err(StoreError::from_sqlx)?,
        error_code: row.try_get("error_code").map_err(StoreError::from_sqlx)?,
        updated_at: row.try_get("updated_at").map_err(StoreError::from_sqlx)?,
        revision: row.try_get("revision").map_err(StoreError::from_sqlx)?,
    })
}

fn validate_new_job(job: &NewJob<'_>) -> Result<(), StoreError> {
    require_id("id", job.id)?;
    for (field, value) in [
        ("repository_id", job.repository_id),
        ("account_id", job.account_id),
        ("plan_id", job.plan_id),
        ("mapping_id", job.mapping_id),
    ] {
        if let Some(v) = value {
            require_id(field, v)?;
        }
    }
    require_id("idempotency_key", job.idempotency_key)?;
    require_stage(job.stage)?;
    require_timestamp("now", job.now)?;
    if let Some(t) = job.not_before {
        // Canonical UTC only, so the scheduler can always read it back (fail closed).
        if rfc3339_to_ms(t).is_none() {
            return Err(StoreError::InvalidInput("not_before"));
        }
    }
    if job.attempt < 1 {
        return Err(StoreError::InvalidInput("attempt"));
    }
    let coverage: serde_json::Value = serde_json::from_str(job.coverage_json)
        .map_err(|_| StoreError::InvalidInput("coverage_json"))?;
    if !matches!(
        gbm_domain::contract::schema::validate_definition("Coverage", &coverage),
        Ok(ref v) if v.is_empty()
    ) {
        return Err(StoreError::InvalidInput("coverage_json"));
    }
    Ok(())
}

async fn insert_job(
    tx: &mut Transaction<'static, Sqlite>,
    job: &NewJob<'_>,
    attempt: i64,
) -> Result<i64, StoreError> {
    let revision = bump_revision(tx).await?;
    sqlx::query(
        "INSERT INTO jobs (id, repository_id, account_id, plan_id, mapping_id, not_before, \
         kind, state, stage, attempt, idempotency_key, created_at, updated_at, error_code, \
         coverage_json, revision) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(job.id)
    .bind(job.repository_id)
    .bind(job.account_id)
    .bind(job.plan_id)
    .bind(job.mapping_id)
    .bind(job.not_before)
    .bind(job.kind.as_str())
    .bind(job.stage)
    .bind(attempt)
    .bind(job.idempotency_key)
    .bind(job.now)
    .bind(job.now)
    .bind(job.coverage_json)
    .bind(revision)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::from_sqlx)?;
    Ok(revision)
}

/// Stages before any destination write; only failures here may be retried
/// automatically (allow-list: an unknown stage is never retried).
const PRE_WRITE_STAGES: [&str; 5] = ["preflight", "observe_source", "fetch", "plan", "snapshot"];
/// Error codes a failed attempt may be retried for automatically.
const RETRYABLE_CODES: [&str; 3] = ["TIMEOUT", "NETWORK_UNAVAILABLE", "RATE_LIMITED"];

impl Database {
    /// Persists a queued job (intent before any external effect). The same
    /// (idempotency_key, attempt) cannot be enqueued twice.
    pub async fn enqueue_job(&self, job: &NewJob<'_>) -> Result<i64, StoreError> {
        validate_new_job(job)?;
        let mut tx = self.begin().await?;
        let result = insert_job(&mut tx, job, job.attempt).await;
        finish(tx, result).await
    }

    /// Atomically moves a job from its current state to `to` if the edge is allowed.
    /// Runs in one IMMEDIATE transaction; the UPDATE also re-checks the expected state.
    /// `error_code` replaces the stored code (pass the previous code to keep it).
    pub async fn transition_job(
        &self,
        id: &str,
        to: JobState,
        stage: &str,
        error_code: Option<&str>,
        now: &str,
    ) -> Result<i64, StoreError> {
        self.transition_job_checked(id, to, stage, error_code, now, None)
            .await
    }

    async fn transition_job_checked(
        &self,
        id: &str,
        to: JobState,
        stage: &str,
        error_code: Option<&str>,
        now: &str,
        max_concurrent: Option<u8>,
    ) -> Result<i64, StoreError> {
        require_id("id", id)?;
        require_stage(stage)?;
        require_error_code(error_code)?;
        require_timestamp("now", now)?;
        let mut tx = self.begin().await?;
        let result = async {
            let row = sqlx::query("SELECT state, repository_id, mapping_id FROM jobs WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?
                .ok_or(StoreError::NotFound)?;
            let current_text: String = row.try_get("state").map_err(StoreError::from_sqlx)?;
            let current = JobState::parse(&current_text).ok_or(StoreError::Corrupt)?;
            if current == JobState::Interrupted || !allowed_transition(current, to) {
                return Err(StoreError::InvalidTransition {
                    current: current_text,
                });
            }
            let repository: Option<String> = row
                .try_get("repository_id")
                .map_err(StoreError::from_sqlx)?;
            let mapping: Option<String> =
                row.try_get("mapping_id").map_err(StoreError::from_sqlx)?;
            if let (Some(mapping), JobState::Running | JobState::CancelRequested) = (mapping, to) {
                let (active,): (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM jobs WHERE mapping_id = ? AND id != ? \
                 AND state IN ('running', 'cancel_requested', 'interrupted')",
                )
                .bind(&mapping)
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?;
                if active > 0 {
                    return Err(StoreError::ActiveJobExists);
                }
            }
            if let (Some(repository), JobState::Running | JobState::CancelRequested) =
                (repository, to)
            {
                // Explicit check inside the write transaction; the unique partial index
                // `one_active_repo_job` remains the backstop.
                let (active,): (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM jobs WHERE repository_id = ? AND id != ? \
                 AND state IN ('running', 'cancel_requested', 'interrupted')",
                )
                .bind(&repository)
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?;
                if active > 0 {
                    return Err(StoreError::ActiveJobExists);
                }
            }
            if let (Some(max), JobState::Running) = (max_concurrent, to) {
                let (active,): (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM jobs WHERE id != ? \
                     AND state IN ('running', 'cancel_requested')",
                )
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?;
                if active >= i64::from(max.clamp(1, 8)) {
                    return Err(StoreError::CapacityReached);
                }
            }
            let revision = bump_revision(&mut tx).await?;
            let updated = sqlx::query(
            "UPDATE jobs SET state = ?, stage = ?, error_code = ?, updated_at = ?, revision = ? \
             WHERE id = ? AND state = ?",
        )
        .bind(to.as_str())
        .bind(stage)
        .bind(error_code)
        .bind(now)
        .bind(revision)
        .bind(id)
        .bind(current.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from_sqlx)?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::InvalidTransition {
                    current: current_text,
                });
            }
            Ok(revision)
        }
        .await;
        finish(tx, result).await
    }

    /// Startup recovery (REQ-10): a previous process cannot still own `running` or
    /// `cancel_requested` jobs (PIDs are not trusted), so they become `interrupted` and
    /// must be reconciled before any retry. A pending cancel additionally records
    /// `CANCEL_UNCERTAIN`; other error codes are left as they were.
    pub async fn recover_interrupted_jobs(&self, now: &str) -> Result<Vec<String>, StoreError> {
        require_timestamp("now", now)?;
        let mut tx = self.begin().await?;
        let result = async {
            let ids: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM jobs WHERE state IN ('running', 'cancel_requested') ORDER BY id",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            let revision = bump_revision(&mut tx).await?;
            sqlx::query(
                "UPDATE jobs SET error_code = CASE WHEN state = 'cancel_requested' \
             THEN 'CANCEL_UNCERTAIN' ELSE error_code END, state = 'interrupted', \
             updated_at = ?, revision = ? WHERE state IN ('running', 'cancel_requested')",
            )
            .bind(now)
            .bind(revision)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            Ok(ids.into_iter().map(|(id,)| id).collect())
        }
        .await;
        finish(tx, result).await
    }

    /// Deduplicated enqueue in one IMMEDIATE transaction: if a non-terminal job with the
    /// same idempotency key exists, returns it; otherwise inserts a new attempt linked
    /// by the key (`max(attempt) + 1` after earlier terminal attempts).
    pub async fn enqueue_or_existing(&self, job: &NewJob<'_>) -> Result<Enqueued, StoreError> {
        validate_new_job(job)?;
        let mut tx = self.begin().await?;
        let result = async {
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM jobs WHERE idempotency_key = ? AND state NOT IN \
                 ('succeeded', 'partial', 'failed', 'canceled') ORDER BY attempt DESC LIMIT 1",
            )
            .bind(job.idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            if let Some((id,)) = existing {
                return Ok(Enqueued::Existing(id));
            }
            let latest: Option<(String, String, i64, Option<String>)> = sqlx::query_as(
                "SELECT id, state, attempt, error_code FROM jobs WHERE idempotency_key = ? \
                 ORDER BY attempt DESC LIMIT 1",
            )
            .bind(job.idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            // A failed logical job is never re-run by a plain request: retries go
            // through `enqueue_retry` (transient, pre-write only) or an owner action.
            // Neither is a partially pushed one: it needs owner review first.
            if let Some((id, state, _, code)) = &latest
                && (state == JobState::Failed.as_str()
                    || (state == JobState::Partial.as_str()
                        && matches!(code.as_deref(), Some("PUSH_PARTIAL" | "CANCEL_UNCERTAIN"))))
            {
                return Ok(Enqueued::NeedsOwner(id.clone()));
            }
            let attempt = latest.map_or(job.attempt, |(_, _, n, _)| n + 1);
            insert_job(&mut tx, job, attempt).await.map(Enqueued::New)
        }
        .await;
        finish(tx, result).await
    }

    /// Starts a queued job only if the queue is not paused and fewer than
    /// `max_concurrent` jobs are active; the limit is checked in the same IMMEDIATE
    /// transaction as the per-repository and per-mapping locks.
    pub async fn start_job(
        &self,
        id: &str,
        stage: &str,
        max_concurrent: u8,
        paused: bool,
        now: &str,
    ) -> Result<i64, StoreError> {
        if paused {
            return Err(StoreError::QueuePaused);
        }
        self.transition_job_checked(
            id,
            JobState::Running,
            stage,
            None,
            now,
            Some(max_concurrent),
        )
        .await
    }

    /// Queued jobs in enqueue order with their lock keys and not-before times.
    pub async fn queued_jobs(&self) -> Result<Vec<QueuedRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, repository_id, mapping_id, not_before, revision FROM jobs \
             WHERE state = 'queued' ORDER BY revision, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from_sqlx)?;
        rows.iter()
            .map(|row| {
                let not_before: Option<String> =
                    row.try_get("not_before").map_err(StoreError::from_sqlx)?;
                let not_before_ms = match not_before {
                    Some(text) => Some(rfc3339_to_ms(&text).ok_or(StoreError::Corrupt)?),
                    None => None,
                };
                Ok(QueuedRow {
                    id: row.try_get("id").map_err(StoreError::from_sqlx)?,
                    repository_id: row
                        .try_get("repository_id")
                        .map_err(StoreError::from_sqlx)?,
                    mapping_id: row.try_get("mapping_id").map_err(StoreError::from_sqlx)?,
                    not_before_ms,
                    sequence: row.try_get("revision").map_err(StoreError::from_sqlx)?,
                })
            })
            .collect()
    }

    /// Jobs that currently hold a repository/mapping lock (incl. interrupted jobs that
    /// were not reconciled yet).
    pub async fn active_jobs(&self) -> Result<Vec<ActiveRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, repository_id, mapping_id FROM jobs \
             WHERE state IN ('running', 'cancel_requested', 'interrupted') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from_sqlx)?;
        rows.iter()
            .map(|row| {
                Ok(ActiveRow {
                    id: row.try_get("id").map_err(StoreError::from_sqlx)?,
                    repository_id: row
                        .try_get("repository_id")
                        .map_err(StoreError::from_sqlx)?,
                    mapping_id: row.try_get("mapping_id").map_err(StoreError::from_sqlx)?,
                })
            })
            .collect()
    }

    /// Enqueues the next attempt of a failed job (same key, attempt + 1, same scope),
    /// not before `not_before`. The failed attempt and its evidence stay as they are.
    pub async fn enqueue_retry(
        &self,
        failed_id: &str,
        new_id: &str,
        not_before: &str,
        now: &str,
    ) -> Result<i64, StoreError> {
        require_id("id", failed_id)?;
        let row = sqlx::query(
            "SELECT repository_id, account_id, plan_id, mapping_id, kind, state, stage, \
             error_code, attempt, idempotency_key, coverage_json FROM jobs WHERE id = ?",
        )
        .bind(failed_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::from_sqlx)?
        .ok_or(StoreError::NotFound)?;
        let state: String = row.try_get("state").map_err(StoreError::from_sqlx)?;
        if state != JobState::Failed.as_str() {
            return Err(StoreError::InvalidTransition { current: state });
        }
        // Second line of defence: only transient failures before any write.
        let stage: String = row.try_get("stage").map_err(StoreError::from_sqlx)?;
        let code: Option<String> = row.try_get("error_code").map_err(StoreError::from_sqlx)?;
        if !PRE_WRITE_STAGES.contains(&stage.as_str())
            || !code
                .as_deref()
                .is_some_and(|c| RETRYABLE_CODES.contains(&c))
        {
            return Err(StoreError::InvalidInput("retry"));
        }
        let kind_text: String = row.try_get("kind").map_err(StoreError::from_sqlx)?;
        let kind = [
            JobKind::Scan,
            JobKind::Plan,
            JobKind::Sync,
            JobKind::RestoreTest,
        ]
        .into_iter()
        .find(|k| k.as_str() == kind_text)
        .ok_or(StoreError::Corrupt)?;
        let repository_id: Option<String> = row
            .try_get("repository_id")
            .map_err(StoreError::from_sqlx)?;
        let account_id: Option<String> =
            row.try_get("account_id").map_err(StoreError::from_sqlx)?;
        let plan_id: Option<String> = row.try_get("plan_id").map_err(StoreError::from_sqlx)?;
        let mapping_id: Option<String> =
            row.try_get("mapping_id").map_err(StoreError::from_sqlx)?;
        let attempt: i64 = row.try_get("attempt").map_err(StoreError::from_sqlx)?;
        let key: String = row
            .try_get("idempotency_key")
            .map_err(StoreError::from_sqlx)?;
        let coverage: String = row
            .try_get("coverage_json")
            .map_err(StoreError::from_sqlx)?;
        self.enqueue_job(&NewJob {
            id: new_id,
            repository_id: repository_id.as_deref(),
            account_id: account_id.as_deref(),
            plan_id: plan_id.as_deref(),
            mapping_id: mapping_id.as_deref(),
            not_before: Some(not_before),
            kind,
            stage: "preflight",
            attempt: attempt + 1,
            idempotency_key: &key,
            now,
            coverage_json: &coverage,
        })
        .await
    }

    pub async fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError> {
        require_id("id", id)?;
        let row = sqlx::query(
            "SELECT id, repository_id, kind, state, stage, attempt, idempotency_key, \
             error_code, updated_at, revision FROM jobs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::from_sqlx)?;
        row.as_ref().map(record).transpose()
    }
}

/// Kinds of durable checkpoint (`job_checkpoints.kind`).
const CHECKPOINT_KINDS: [&str; 2] = ["project_create", "push"];
const MAX_CHECKPOINT_PAYLOAD: usize = 4096;

/// How a reconciled interrupted attempt is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptedClose {
    Succeeded,
    Partial,
    Failed,
}

impl InterruptedClose {
    fn state(self) -> JobState {
        match self {
            Self::Succeeded => JobState::Succeeded,
            Self::Partial => JobState::Partial,
            Self::Failed => JobState::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCheckpoint {
    pub kind: String,
    pub payload: String,
}

impl Database {
    /// Records the running attempt's safe checkpoint (replacing the previous one) before
    /// the effect it describes starts. The payload is the caller's encoded intent; it
    /// must never contain command argv or secrets.
    pub async fn save_checkpoint(
        &self,
        job_id: &str,
        kind: &str,
        payload: &str,
        now: &str,
    ) -> Result<(), StoreError> {
        require_id("id", job_id)?;
        require_timestamp("now", now)?;
        if !CHECKPOINT_KINDS.contains(&kind) {
            return Err(StoreError::InvalidInput("checkpoint_kind"));
        }
        if payload.is_empty() || payload.len() > MAX_CHECKPOINT_PAYLOAD {
            return Err(StoreError::InvalidInput("checkpoint_payload"));
        }
        let mut tx = self.begin().await?;
        let result = async {
            let state: Option<(String,)> = sqlx::query_as("SELECT state FROM jobs WHERE id = ?")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?;
            let (state,) = state.ok_or(StoreError::NotFound)?;
            // Only the attempt that is actually running records its progress.
            if state != JobState::Running.as_str() {
                return Err(StoreError::InvalidTransition { current: state });
            }
            sqlx::query(
                "INSERT INTO job_checkpoints (job_id, kind, payload, updated_at) \
                 VALUES (?, ?, ?, ?) ON CONFLICT(job_id) DO UPDATE SET \
                 kind = excluded.kind, payload = excluded.payload, \
                 updated_at = excluded.updated_at",
            )
            .bind(job_id)
            .bind(kind)
            .bind(payload)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            Ok(())
        }
        .await;
        finish(tx, result).await
    }

    pub async fn checkpoint(&self, job_id: &str) -> Result<Option<StoredCheckpoint>, StoreError> {
        require_id("id", job_id)?;
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT kind, payload FROM job_checkpoints WHERE job_id = ?")
                .bind(job_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::from_sqlx)?;
        Ok(row.map(|(kind, payload)| StoredCheckpoint { kind, payload }))
    }

    /// Closes a reconciled `interrupted` attempt in one IMMEDIATE transaction (REQ-10):
    /// - the attempt gets its reconciled outcome and releases its locks;
    /// - the approvals and the plan it ran expire now, so they can never be resumed;
    /// - with `new_attempt_id` (only for a `Failed` close), the next attempt of the same
    ///   logical job is queued without a plan (it must plan again from fresh state); a
    ///   persisted project-create intent is carried over so the create is re-run with
    ///   the same marker (lookup first, no duplicate project).
    pub async fn resolve_interrupted(
        &self,
        id: &str,
        close: InterruptedClose,
        error_code: Option<&str>,
        new_attempt_id: Option<&str>,
        now: &str,
    ) -> Result<i64, StoreError> {
        require_id("id", id)?;
        require_error_code(error_code)?;
        require_timestamp("now", now)?;
        if let Some(new_id) = new_attempt_id {
            require_id("id", new_id)?;
            if close != InterruptedClose::Failed {
                return Err(StoreError::InvalidInput("new_attempt"));
            }
        }
        let mut tx = self.begin().await?;
        let result = async {
            let row = sqlx::query(
                "SELECT state, repository_id, account_id, plan_id, mapping_id, kind, attempt, \
                 idempotency_key, coverage_json FROM jobs WHERE id = ?",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?
            .ok_or(StoreError::NotFound)?;
            let state: String = row.try_get("state").map_err(StoreError::from_sqlx)?;
            if state != JobState::Interrupted.as_str() {
                return Err(StoreError::InvalidTransition { current: state });
            }
            let plan_id: Option<String> = row.try_get("plan_id").map_err(StoreError::from_sqlx)?;
            // Every plan this attempt may have run: the job's own plan and the plan named
            // by its push checkpoint (in-job planning leaves `jobs.plan_id` empty).
            let mut plans: Vec<String> = plan_id.into_iter().collect();
            let push: Option<(String,)> = sqlx::query_as(
                "SELECT payload FROM job_checkpoints WHERE job_id = ? AND kind = 'push'",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            if let Some((payload,)) = push {
                let value: serde_json::Value =
                    serde_json::from_str(&payload).map_err(|_| StoreError::Corrupt)?;
                let planned = value
                    .get("planId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(StoreError::Corrupt)?;
                plans.push(planned.to_owned());
            }
            let revision = bump_revision(&mut tx).await?;
            // A recorded CANCEL_UNCERTAIN is kept unless the destination was verified.
            let updated = sqlx::query(
                "UPDATE jobs SET state = ?, error_code = CASE WHEN error_code = \
                 'CANCEL_UNCERTAIN' AND ? != 'succeeded' THEN error_code ELSE ? END, \
                 updated_at = ?, revision = ? WHERE id = ? AND state = 'interrupted'",
            )
            .bind(close.state().as_str())
            .bind(close.state().as_str())
            .bind(error_code)
            .bind(now)
            .bind(revision)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::InvalidTransition { current: state });
            }
            for plan_id in &plans {
                for sql in [
                    "UPDATE approvals SET expires_at = ? WHERE plan_id = ?",
                    "UPDATE plans SET expires_at = ? WHERE id = ?",
                ] {
                    sqlx::query(sql)
                        .bind(now)
                        .bind(plan_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(StoreError::from_sqlx)?;
                }
            }
            let Some(new_id) = new_attempt_id else {
                return Ok(revision);
            };
            let kind_text: String = row.try_get("kind").map_err(StoreError::from_sqlx)?;
            let kind = [
                JobKind::Scan,
                JobKind::Plan,
                JobKind::Sync,
                JobKind::RestoreTest,
            ]
            .into_iter()
            .find(|k| k.as_str() == kind_text)
            .ok_or(StoreError::Corrupt)?;
            let repository_id: Option<String> = row
                .try_get("repository_id")
                .map_err(StoreError::from_sqlx)?;
            let account_id: Option<String> =
                row.try_get("account_id").map_err(StoreError::from_sqlx)?;
            let mapping_id: Option<String> =
                row.try_get("mapping_id").map_err(StoreError::from_sqlx)?;
            let key: String = row
                .try_get("idempotency_key")
                .map_err(StoreError::from_sqlx)?;
            let coverage: String = row
                .try_get("coverage_json")
                .map_err(StoreError::from_sqlx)?;
            let (pending,): (i64,) = sqlx::query_as(
                "SELECT count(*) FROM jobs WHERE idempotency_key = ? AND state NOT IN \
                 ('succeeded', 'partial', 'failed', 'canceled')",
            )
            .bind(&key)
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            if pending > 0 {
                // The logical job already has a pending attempt: no duplicate.
                return Ok(revision);
            }
            let (latest,): (i64,) =
                sqlx::query_as("SELECT max(attempt) FROM jobs WHERE idempotency_key = ?")
                    .bind(&key)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(StoreError::from_sqlx)?;
            let next = NewJob {
                id: new_id,
                repository_id: repository_id.as_deref(),
                account_id: account_id.as_deref(),
                plan_id: None,
                mapping_id: mapping_id.as_deref(),
                not_before: None,
                kind,
                stage: "preflight",
                attempt: latest + 1,
                idempotency_key: &key,
                now,
                coverage_json: &coverage,
            };
            validate_new_job(&next)?;
            let revision = insert_job(&mut tx, &next, next.attempt).await?;
            sqlx::query(
                "INSERT INTO job_checkpoints (job_id, kind, payload, updated_at) \
                 SELECT ?, kind, payload, ? FROM job_checkpoints \
                 WHERE job_id = ? AND kind = 'project_create'",
            )
            .bind(new_id)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::from_sqlx)?;
            Ok(revision)
        }
        .await;
        finish(tx, result).await
    }
}
