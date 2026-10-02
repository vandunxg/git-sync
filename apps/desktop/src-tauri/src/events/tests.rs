//! TEST-EVENTS (shell half): a committed command reaches a `gbm://state` listener as a
//! contract-valid event; a failed command emits nothing.

use std::sync::{Arc, Mutex};

use gbm_domain::contract::schema::validate_definition;
use gbm_domain::contract::{AppError, CommandSpec, ErrorCode};
use serde_json::json;
use tauri::Listener;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Canned(bool);

impl CommandHandler for Canned {
    fn handle(&self, _: &CommandSpec, request_id: &str, _: Value) -> Result<Value, AppError> {
        if self.0 {
            Ok(json!({ "revision": 7 }))
        } else {
            Err(AppError {
                code: ErrorCode::Internal,
                message: "failed".to_owned(),
                retryable: false,
                retry_at: None,
                correlation_id: request_id.to_owned(),
                details: None,
            })
        }
    }
}

#[test]
fn committed_changes_reach_state_listeners() -> TestResult {
    let dir = std::env::temp_dir().join(format!("gbm-events-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let (db, _) = tauri::async_runtime::block_on(Database::open(&dir.join("state.sqlite")))?;
    let app = tauri::test::mock_app();
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&received);
    app.listen(STATE_CHANNEL, move |event| {
        if let Ok(mut all) = sink.lock() {
            all.push(event.payload().to_owned());
        }
    });
    let request = json!({ "schemaVersion": 1, "requestId": "req-1", "args": { "paused": true } });
    let ok = emitting(Canned(true), app.handle().clone(), db.clone()).ok_or("run id")?;
    let failing = emitting(Canned(false), app.handle().clone(), db).ok_or("run id")?;
    gbm_application::ipc::dispatch(&failing, "gbm_queue_set_paused", &request);
    gbm_application::ipc::dispatch(&ok, "gbm_queue_set_paused", &request);
    let payloads = received.lock().map(|p| p.clone()).unwrap_or_default();
    assert_eq!(payloads.len(), 1, "only the committed change is announced");
    let event: Value = serde_json::from_str(&payloads[0])?;
    assert!(matches!(validate_definition("Event", &event), Ok(v) if v.is_empty()));
    assert_eq!(event["type"], json!("queue_changed"));
    assert_eq!(event["revision"], json!(7));
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
