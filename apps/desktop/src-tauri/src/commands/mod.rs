//! Tauri registration of the app-owned command allowlist (contracts/ipc.v1.json).
//!
//! Each command takes one `request` envelope and always resolves with a result
//! envelope; validation and routing live in `gbm_application::ipc::dispatch`.
//! Nothing here executes shell, Git, filesystem or URL operations for the renderer.

use gbm_application::ipc::{CommandHandler, dispatch};
use serde_json::Value;

/// Managed state holding the command implementation used by every IPC command.
pub struct Facade(Box<dyn CommandHandler>);

impl Facade {
    pub fn new(handler: impl CommandHandler + 'static) -> Self {
        Self(Box::new(handler))
    }
}

macro_rules! app_commands {
    ($($name:ident),+ $(,)?) => {
        $(
            #[tauri::command(rename_all = "camelCase")]
            async fn $name(facade: tauri::State<'_, Facade>, request: Value) -> Result<Value, ()> {
                Ok(dispatch(facade.0.as_ref(), stringify!($name), &request))
            }
        )+

        /// Names registered with Tauri, for the parity check against the contract.
        #[cfg(test)]
        pub const REGISTERED: &[&str] = &[$(stringify!($name)),+];

        /// The invoke handler containing exactly the allowlisted commands.
        pub fn invoke_handler<R: tauri::Runtime>()
            -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($name),+]
        }
    };
}

app_commands!(
    gbm_app_bootstrap,
    gbm_accounts_list,
    gbm_credentials_validate_store,
    gbm_accounts_disconnect,
    gbm_storage_pick_root,
    gbm_storage_commit_root,
    gbm_repositories_list,
    gbm_repository_get,
    gbm_repositories_select,
    gbm_source_scan_enqueue,
    gbm_namespaces_list,
    gbm_destination_preview,
    gbm_destination_commit,
    gbm_sync_plan_create,
    gbm_sync_plan_approve,
    gbm_sync_apply_enqueue,
    gbm_jobs_list,
    gbm_job_cancel,
    gbm_job_retry,
    gbm_queue_set_paused,
    gbm_dashboard_get,
    gbm_repository_refs,
    gbm_activity_list,
    gbm_logs_read,
    gbm_activity_export,
    gbm_settings_get,
    gbm_settings_update,
    gbm_repository_open_workspace,
    gbm_repository_open_provider,
    gbm_environment_diagnostics,
    gbm_sync_plan_get,
);

#[cfg(test)]
mod tests {
    use super::*;
    use gbm_application::ipc::NotYetImplemented;
    use gbm_domain::contract::{COMMANDS, FORBIDDEN_COMMANDS};
    use serde_json::json;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn registered_commands_equal_contract_allowlist() {
        let mut registered: Vec<_> = REGISTERED.to_vec();
        let mut contract: Vec<_> = COMMANDS.iter().map(|c| c.name).collect();
        registered.sort_unstable();
        contract.sort_unstable();
        assert_eq!(registered, contract);
        assert!(
            REGISTERED
                .iter()
                .all(|name| !FORBIDDEN_COMMANDS.contains(name))
        );
    }

    fn invoke(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        cmd: &str,
        body: Value,
    ) -> Result<Result<Value, Value>, Box<dyn std::error::Error>> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse()?,
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        Ok(match get_ipc_response(webview, request) {
            Ok(response) => Ok(response.deserialize::<Value>()?),
            Err(error) => Err(error),
        })
    }

    /// TEST-IPC through Tauri's real IPC router on the mock runtime.
    #[test]
    fn tauri_router_enforces_the_allowlist() -> TestResult {
        let app = mock_builder()
            .manage(Facade::new(NotYetImplemented))
            .invoke_handler(invoke_handler())
            .build(mock_context(noop_assets()))?;
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;

        let envelope = json!({"schemaVersion": 1, "requestId": "req-1", "args": {}});
        let ok = invoke(&webview, "gbm_app_bootstrap", json!({"request": envelope}))?;
        let body = ok.map_err(|e| format!("transport error {e}"))?;
        assert_eq!(body.get("ok"), Some(&json!(false)));
        assert_eq!(body.pointer("/error/code"), Some(&json!("UNSUPPORTED")));
        assert_eq!(body.get("requestId"), Some(&json!("req-1")));

        let bad = json!({"schemaVersion": 9, "requestId": "req-2", "args": {}});
        let rejected = invoke(&webview, "gbm_app_bootstrap", json!({"request": bad}))?;
        let body = rejected.map_err(|e| format!("transport error {e}"))?;
        assert_eq!(
            body.pointer("/error/code"),
            Some(&json!("CONTRACT_MISMATCH"))
        );

        for name in FORBIDDEN_COMMANDS
            .iter()
            .chain(["plugin:opener|open_url", "greet"].iter())
        {
            let outcome = invoke(&webview, name, json!({"request": envelope}))?;
            assert!(outcome.is_err(), "{name} must not be routable");
        }
        let missing = invoke(&webview, "gbm_app_bootstrap", json!({}))?;
        assert!(
            missing.is_err(),
            "missing request argument must be a transport error"
        );
        Ok(())
    }
}
