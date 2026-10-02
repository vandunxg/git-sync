//! Durable jobs: idempotent enqueue, compare-and-set transitions, one active
//! mutating job per repository (unique partial index) and restart recovery.

use sqlx::Row;
use sqlx::sqlite::SqliteRow;

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
            | (Interrupted, Queued) // only after reconciliation (caller's obligation)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJob<'a> {
    pub id: &'a str,
    pub repository_id: Option<&'a str>,
    pub account_id: Option<&'a str>,
    pub plan_id: Option<&'a str>,
    pub kind: JobKind,
    pub stage: &'a str,
    pub attempt: i64,
    pub idempotency_key: &'a str,
    pub now: &'a str,
    pub coverage_json: &'a str,
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

impl Database {
    /// Persists a queued job (intent before any external effect). The same
    /// (idempotency_key, attempt) cannot be enqueued twice.
    pub async fn enqueue_job(&self, job: &NewJob<'_>) -> Result<i64, StoreError> {
        require_id("id", job.id)?;
        for (field, value) in [
            ("repository_id", job.repository_id),
            ("account_id", job.account_id),
            ("plan_id", job.plan_id),
        ] {
            if let Some(v) = value {
                require_id(field, v)?;
            }
        }
        require_id("idempotency_key", job.idempotency_key)?;
        require_stage(job.stage)?;
        require_timestamp("now", job.now)?;
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
        let mut tx = self.begin().await?;
        let result = async {
        let revision = bump_revision(&mut tx).await?;
        sqlx::query(
            "INSERT INTO jobs (id, repository_id, account_id, plan_id, kind, state, stage, \
             attempt, idempotency_key, created_at, updated_at, error_code, coverage_json, revision) \
             VALUES (?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(job.id)
        .bind(job.repository_id)
        .bind(job.account_id)
        .bind(job.plan_id)
        .bind(job.kind.as_str())
        .bind(job.stage)
        .bind(job.attempt)
        .bind(job.idempotency_key)
        .bind(job.now)
        .bind(job.now)
        .bind(job.coverage_json)
        .bind(revision)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::from_sqlx)?;
        Ok(revision)
        }
        .await;
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
        require_id("id", id)?;
        require_stage(stage)?;
        require_error_code(error_code)?;
        require_timestamp("now", now)?;
        let mut tx = self.begin().await?;
        let result = async {
            let row = sqlx::query("SELECT state, repository_id FROM jobs WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::from_sqlx)?
                .ok_or(StoreError::NotFound)?;
            let current_text: String = row.try_get("state").map_err(StoreError::from_sqlx)?;
            let current = JobState::parse(&current_text).ok_or(StoreError::Corrupt)?;
            if !allowed_transition(current, to) {
                return Err(StoreError::InvalidTransition {
                    current: current_text,
                });
            }
            let repository: Option<String> = row
                .try_get("repository_id")
                .map_err(StoreError::from_sqlx)?;
            if let (Some(repository), JobState::Running | JobState::CancelRequested) =
                (repository, to)
            {
                // Explicit check inside the write transaction; the unique partial index
                // `one_active_repo_job` remains the backstop.
                let (active,): (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM jobs WHERE repository_id = ? AND id != ? \
                 AND state IN ('running', 'cancel_requested')",
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
