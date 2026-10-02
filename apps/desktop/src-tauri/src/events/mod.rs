//! `gbm://state` emitter (contracts/ipc.v1.json `events`): wraps the command handler so
//! every committed state change is followed by one invalidation event. The renderer
//! listens through `core:event:default` only; it cannot emit app state events.

use std::hash::BuildHasher;

use gbm_application::events::{Emitting, EventPublisher, EventSink, RevisionSource};
use gbm_application::ipc::CommandHandler;
use gbm_infrastructure::sqlite::Database;
use serde_json::Value;
use tauri::Emitter;

/// Event channel from the IPC contract.
pub const STATE_CHANNEL: &str = "gbm://state";

/// Emits to all webviews of the app. A failed emit (e.g. no window yet) is dropped:
/// the renderer re-queries on subscription and on any sequence gap.
pub struct TauriSink<R: tauri::Runtime>(tauri::AppHandle<R>);

impl<R: tauri::Runtime> EventSink for TauriSink<R> {
    fn emit(&self, event: &Value) {
        let _ = self.0.emit(STATE_CHANNEL, event);
    }
}

/// Reads the committed global revision after a command returned.
pub struct StoreRevisions(Database);

impl RevisionSource for StoreRevisions {
    fn committed_revision(&self) -> Option<i64> {
        tauri::async_runtime::block_on(self.0.revision()).ok()
    }
}

/// A fresh identifier for this app run (OS-seeded hash keys), so the renderer can tell
/// a restarted backend from a gap in the same run.
fn run_id() -> String {
    let state = std::hash::RandomState::new();
    format!("{:016x}", state.hash_one(std::process::id()))
}

/// Wraps `handler` so committed changes are announced on `gbm://state`.
pub fn emitting<R: tauri::Runtime>(
    handler: impl CommandHandler + 'static,
    app: tauri::AppHandle<R>,
    db: Database,
) -> Option<impl CommandHandler + 'static> {
    let publisher = EventPublisher::new(&run_id(), TauriSink(app))?;
    Some(Emitting::new(handler, publisher, StoreRevisions(db)))
}

#[cfg(test)]
mod tests;
