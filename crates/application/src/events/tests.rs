//! TEST-EVENTS (native half): events are emitted only after a committed success, carry
//! the committed revision, validate against the contract, never carry secrets and have a
//! strictly increasing per-run sequence.

use std::sync::Mutex;

use gbm_domain::contract::{ErrorCode, command_spec};
use serde_json::json;

use super::*;
use crate::ipc::{NotYetImplemented, dispatch};

#[derive(Default)]
struct Recorder(Mutex<Vec<Value>>);

impl EventSink for &Recorder {
    fn emit(&self, event: &Value) {
        if let Ok(mut events) = self.0.lock() {
            events.push(event.clone());
        }
    }
}

impl Recorder {
    fn take(&self) -> Vec<Value> {
        self.0
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }
}

struct Fixed(Option<i64>);

impl RevisionSource for Fixed {
    fn committed_revision(&self) -> Option<i64> {
        self.0
    }
}

/// Returns a canned, contract-valid response per command (or a failure).
struct Canned(Result<Value, ErrorCode>);

impl CommandHandler for Canned {
    fn handle(&self, _: &CommandSpec, request_id: &str, _: Value) -> Result<Value, AppError> {
        self.0.clone().map_err(|code| AppError {
            code,
            message: "failed".to_owned(),
            retryable: false,
            retry_at: None,
            correlation_id: request_id.to_owned(),
            details: None,
        })
    }
}

fn request(args: Value) -> Value {
    json!({ "schemaVersion": 1, "requestId": "req-1", "args": args })
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn a_committed_mutation_emits_one_valid_event_with_its_revision() -> TestResult {
    let recorder = Recorder::default();
    let handler = Emitting::new(
        Canned(Ok(
            json!({ "jobId": "job-7", "state": "queued", "revision": 41 }),
        )),
        EventPublisher::new("run1", &recorder).ok_or("run")?,
        Fixed(Some(99)),
    );
    let response = dispatch(
        &handler,
        "gbm_sync_plan_create",
        &request(json!({ "repositoryId": "repo-3" })),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    let events = recorder.take();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert!(matches!(validate_definition("Event", event), Ok(v) if v.is_empty()));
    assert_eq!(
        event["revision"],
        json!(41),
        "the response's committed revision"
    );
    assert_eq!(event["type"], json!("job_changed"));
    assert_eq!(event["repositoryId"], json!("repo-3"));
    assert_eq!(event["jobId"], json!("job-7"));
    assert_eq!(event["eventId"], json!("e-run1.1"));
    Ok(())
}

#[test]
fn failures_and_reads_emit_nothing() -> TestResult {
    let recorder = Recorder::default();
    let failing = Emitting::new(
        Canned(Err(ErrorCode::AuthRequired)),
        EventPublisher::new("run1", &recorder).ok_or("run")?,
        Fixed(Some(5)),
    );
    let response = dispatch(
        &failing,
        "gbm_queue_set_paused",
        &request(json!({ "paused": true })),
    );
    assert_eq!(response["ok"], json!(false));
    // An unimplemented command is a failure too.
    let unsupported = Emitting::new(
        NotYetImplemented,
        EventPublisher::new("run1", &recorder).ok_or("run")?,
        Fixed(Some(5)),
    );
    dispatch(
        &unsupported,
        "gbm_queue_set_paused",
        &request(json!({ "paused": true })),
    );
    for command in [
        "gbm_settings_get",
        "gbm_dashboard_get",
        "gbm_logs_read",
        "gbm_activity_export",
    ] {
        assert_eq!(
            invalidation(command, &json!({}), &json!({})),
            None,
            "{command}"
        );
    }
    assert_eq!(recorder.take(), Vec::<Value>::new());
    Ok(())
}

#[test]
fn exactly_the_state_changing_commands_are_mapped() {
    // Contract effects that change no native state (reads, dialogs that only return a
    // candidate, safe openers, probes). Every other command must invalidate something.
    const NO_STATE_CHANGE: [&str; 7] = [
        "read",
        "read_network",
        "redacted_read",
        "native_dialog",
        "native_save_dialog",
        "safe_opener",
        "bounded_probe",
    ];
    for spec in gbm_domain::contract::COMMANDS {
        let mapped = invalidation(spec.name, &json!({}), &json!({})).is_some();
        assert_eq!(
            mapped,
            !NO_STATE_CHANGE.contains(&spec.effect),
            "{} ({})",
            spec.name,
            spec.effect
        );
    }
    assert!(command_spec("gbm_settings_update").is_some());
}

#[test]
fn without_a_response_revision_the_committed_store_revision_is_used() -> TestResult {
    let recorder = Recorder::default();
    let root = json!({
        "id": "root-1",
        "displayPath": "~/Backups",
        "ownershipVerified": true,
        "availableBytes": null,
        "workspaceEnabled": false
    });
    let handler = Emitting::new(
        Canned(Ok(root.clone())),
        EventPublisher::new("run1", &recorder).ok_or("run")?,
        Fixed(Some(12)),
    );
    handler
        .handle(
            command_spec("gbm_storage_commit_root").ok_or("spec")?,
            "req-1",
            json!({}),
        )
        .map_err(|e| e.message)?;
    let events = recorder.take();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["revision"], json!(12));
    assert_eq!(events[0]["type"], json!("settings_changed"));
    // No revision known at all: nothing is emitted rather than a made-up one, but the
    // sequence is consumed so the renderer sees a gap and re-queries everything.
    let publisher = EventPublisher::new("run2", &recorder).ok_or("run")?;
    publisher.skip();
    let change = Invalidation {
        kind: EventType::SettingsChanged,
        repository_id: None,
        job_id: None,
    };
    assert!(publisher.publish(&change, 14, 1_790_944_496_789));
    assert_eq!(recorder.take()[0]["eventId"], json!("e-run2.2"));
    let unknown = Emitting::new(
        Canned(Ok(root)),
        EventPublisher::new("run1", &recorder).ok_or("run")?,
        Fixed(None),
    );
    unknown
        .handle(
            command_spec("gbm_storage_commit_root").ok_or("spec")?,
            "req-1",
            json!({}),
        )
        .map_err(|e| e.message)?;
    assert_eq!(recorder.take(), Vec::<Value>::new());
    Ok(())
}

#[test]
fn sequences_increase_and_invalid_events_are_not_emitted() -> TestResult {
    let recorder = Recorder::default();
    let publisher = EventPublisher::new("abc", &recorder).ok_or("run")?;
    let change = Invalidation {
        kind: EventType::RepositoryChanged,
        repository_id: Some("repo-1".to_owned()),
        job_id: None,
    };
    assert!(publisher.publish(&change, 3, 1_790_944_496_789));
    // Not contract-valid: a negative revision and an ID with a path separator.
    assert!(!publisher.publish(&change, -1, 1_790_944_496_789));
    let bad = Invalidation {
        repository_id: Some("../etc".to_owned()),
        ..change.clone()
    };
    assert!(!publisher.publish(&bad, 4, 1_790_944_496_789));
    assert!(publisher.publish(&change, 5, 1_790_944_496_789));
    let ids: Vec<Value> = recorder
        .take()
        .iter()
        .map(|e| e["eventId"].clone())
        .collect();
    // The rejected events still consumed sequence numbers: the renderer sees a gap
    // and re-queries instead of silently missing a change.
    assert_eq!(ids, vec![json!("e-abc.1"), json!("e-abc.4")]);
    for run in ["", "a.b", "x/y", &"r".repeat(33)] {
        assert!(EventPublisher::new(run, &recorder).is_none(), "{run}");
    }
    Ok(())
}
