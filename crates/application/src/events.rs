//! Committed-state invalidation events on `gbm://state` (IPC.md "Event semantics",
//! SCHEDULER_AND_STATE.md "Events"; REQ-08, REQ-36, REQ-43).
//!
//! Events are invalidations, not the source of truth: they carry the committed
//! revision, a type and affected IDs only — never tokens, paths or process output. They
//! are emitted strictly after the native state they describe was committed (a command
//! handler has returned success). Every event of one app run carries a strictly
//! increasing sequence in its id (`e-<run>.<seq>`), so the renderer can detect a lost
//! or reordered event and re-query instead of trusting stale data.

use std::sync::atomic::{AtomicU64, Ordering};

use gbm_domain::contract::schema::validate_definition;
use gbm_domain::contract::{AppError, CommandSpec, Event, EventType};
use serde_json::Value;

use crate::ipc::CommandHandler;
use crate::time::ms_to_rfc3339;

/// Delivers a serialized, contract-valid `Event` to the renderer. Delivery may fail
/// silently (no window): the renderer re-queries on the next gap or start.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &Value);
}

/// The committed global revision, read after a command succeeded.
pub trait RevisionSource: Send + Sync {
    fn committed_revision(&self) -> Option<i64>;
}

/// What a successful command changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalidation {
    pub kind: EventType,
    pub repository_id: Option<String>,
    pub job_id: Option<String>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

/// Maps a successful app command to the state it changed (`None`: read-only, or an
/// external effect that changes no native state).
pub fn invalidation(command: &str, args: &Value, data: &Value) -> Option<Invalidation> {
    let of = |kind, repository_id, job_id| {
        Some(Invalidation {
            kind,
            repository_id,
            job_id,
        })
    };
    match command {
        "gbm_credentials_validate_store" | "gbm_accounts_disconnect" => {
            of(EventType::AccountChanged, None, None)
        }
        "gbm_storage_commit_root" | "gbm_settings_update" => {
            of(EventType::SettingsChanged, None, None)
        }
        "gbm_repositories_select" | "gbm_destination_commit" | "gbm_sync_plan_approve" => {
            of(EventType::CatalogChanged, None, None)
        }
        "gbm_queue_set_paused" => of(EventType::QueueChanged, None, None),
        "gbm_source_scan_enqueue"
        | "gbm_sync_plan_create"
        | "gbm_sync_apply_enqueue"
        | "gbm_job_retry" => of(
            EventType::JobChanged,
            text(args, "repositoryId"),
            text(data, "jobId"),
        ),
        "gbm_job_cancel" => of(
            EventType::JobChanged,
            text(data, "repositoryId"),
            text(data, "id"),
        ),
        _ => None,
    }
}

/// Builds and emits contract-valid events with a per-run strictly increasing sequence.
pub struct EventPublisher<S> {
    run: String,
    sequence: AtomicU64,
    sink: S,
}

impl<S: EventSink> EventPublisher<S> {
    /// `run` identifies this app run (`[A-Za-z0-9]{1,32}`); a new run makes the
    /// renderer re-query everything.
    pub fn new(run: &str, sink: S) -> Option<Self> {
        let valid = (1..=32).contains(&run.len()) && run.bytes().all(|b| b.is_ascii_alphanumeric());
        valid.then(|| Self {
            run: run.to_owned(),
            sequence: AtomicU64::new(0),
            sink,
        })
    }

    /// Consumes a sequence number without emitting: a committed change whose revision
    /// cannot be read is still visible to the renderer as a gap (full re-query).
    pub fn skip(&self) {
        self.sequence.fetch_add(1, Ordering::SeqCst);
    }

    /// Emits one event for a committed `revision`. Returns false (nothing emitted) if
    /// the event would not match the contract; the renderer then sees a sequence gap.
    pub fn publish(&self, change: &Invalidation, revision: i64, now_ms: i64) -> bool {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(occurred_at) = ms_to_rfc3339(now_ms) else {
            return false;
        };
        let event = Event {
            schema_version: 1,
            event_id: format!("e-{}.{sequence}", self.run),
            revision,
            occurred_at,
            r#type: change.kind,
            repository_id: change.repository_id.clone(),
            job_id: change.job_id.clone(),
        };
        let Ok(value) = serde_json::to_value(&event) else {
            return false;
        };
        if !matches!(validate_definition("Event", &value), Ok(ref v) if v.is_empty()) {
            return false;
        }
        self.sink.emit(&value);
        true
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Command handler decorator: after the inner handler returned success (its state is
/// committed), publishes the matching invalidation with the committed revision — the
/// response's own `revision` when it has one, else the store's current revision.
pub struct Emitting<H, S, R> {
    inner: H,
    publisher: EventPublisher<S>,
    revisions: R,
}

impl<H, S, R> Emitting<H, S, R> {
    pub fn new(inner: H, publisher: EventPublisher<S>, revisions: R) -> Self {
        Self {
            inner,
            publisher,
            revisions,
        }
    }
}

impl<H: CommandHandler, S: EventSink, R: RevisionSource> CommandHandler for Emitting<H, S, R> {
    fn handle(&self, spec: &CommandSpec, request_id: &str, args: Value) -> Result<Value, AppError> {
        let change_args = args.clone();
        let data = self.inner.handle(spec, request_id, args)?;
        if let Some(change) = invalidation(spec.name, &change_args, &data) {
            let revision = data
                .get("revision")
                .and_then(Value::as_i64)
                .or_else(|| self.revisions.committed_revision());
            match revision {
                Some(revision) => {
                    self.publisher.publish(&change, revision, now_ms());
                }
                None => self.publisher.skip(),
            }
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests;
