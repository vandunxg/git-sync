//! Typed IPC facade: validates envelopes, routes allowlisted commands to a handler and
//! validates every response against the contract before it reaches the renderer.

use std::sync::atomic::{AtomicU64, Ordering};

use gbm_domain::contract::envelope::{
    app_error, failure, parse_request, success, valid_request_id,
};
use gbm_domain::contract::schema::validate_definition;
use gbm_domain::contract::{AppError, CommandSpec, ErrorCode, command_spec};
use serde_json::Value;

/// Implements app commands. `args` has already passed contract validation for
/// `spec.request_type`; the returned value must match `spec.response_type`.
pub trait CommandHandler: Send + Sync {
    fn handle(&self, spec: &CommandSpec, request_id: &str, args: Value) -> Result<Value, AppError>;
}

/// Handler for commands whose implementing task has not landed yet. It answers
/// `UNSUPPORTED` honestly instead of returning placeholder data (IPC.md).
#[derive(Debug, Default, Clone, Copy)]
pub struct NotYetImplemented;

impl CommandHandler for NotYetImplemented {
    fn handle(
        &self,
        spec: &CommandSpec,
        request_id: &str,
        _args: Value,
    ) -> Result<Value, AppError> {
        Err(app_error(
            ErrorCode::Unsupported,
            "This operation is not available in this build.",
            request_id,
            Some(format!("{} is not implemented yet", spec.name)),
        ))
    }
}

fn native_correlation_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("native-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Executes one command invocation and always returns a result envelope.
pub fn dispatch(handler: &dyn CommandHandler, command: &str, raw_request: &Value) -> Value {
    let Some(spec) = command_spec(command) else {
        let request_id = valid_request_id(raw_request);
        let id = request_id.map_or_else(native_correlation_id, str::to_owned);
        let error = app_error(ErrorCode::Unsupported, "Unknown command.", &id, None);
        return failure(request_id, &error);
    };
    let request = match parse_request(spec, raw_request) {
        Ok(request) => request,
        Err(rejected) => {
            let correlation = rejected
                .request_id
                .clone()
                .unwrap_or_else(native_correlation_id);
            let error = app_error(
                rejected.code,
                rejected.message,
                &correlation,
                rejected.details,
            );
            return failure(rejected.request_id.as_deref(), &error);
        }
    };
    let request_id = request.request_id;
    match handler.handle(spec, &request_id, request.args) {
        Ok(data) => match validate_definition(spec.response_type, &data) {
            Ok(violations) if violations.is_empty() => success(&request_id, data),
            _ => {
                let error = app_error(
                    ErrorCode::Internal,
                    "The application produced a response that does not match the contract.",
                    &request_id,
                    None,
                );
                failure(Some(&request_id), &error)
            }
        },
        Err(error) => {
            let error = if validate_definition(
                "AppError",
                &serde_json::to_value(&error).unwrap_or(Value::Null),
            )
            .is_ok_and(|v| v.is_empty())
            {
                error
            } else {
                app_error(
                    ErrorCode::Internal,
                    "Invalid error from handler.",
                    &request_id,
                    None,
                )
            };
            failure(Some(&request_id), &error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gbm_domain::contract::COMMANDS;
    use serde_json::json;

    struct Fixed(Result<Value, AppError>);

    impl CommandHandler for Fixed {
        fn handle(&self, _: &CommandSpec, _: &str, _: Value) -> Result<Value, AppError> {
            self.0.clone()
        }
    }

    fn request(args: Value) -> Value {
        json!({"schemaVersion": 1, "requestId": "req-7", "args": args})
    }

    fn error_code(envelope: &Value) -> Option<&str> {
        envelope.pointer("/error/code").and_then(Value::as_str)
    }

    #[test]
    fn every_contract_command_answers_unsupported_honestly() {
        for spec in &COMMANDS {
            let out = dispatch(&NotYetImplemented, spec.name, &json!({}));
            // Even an invalid envelope must produce a well-formed failure envelope.
            assert_eq!(out.get("ok"), Some(&Value::Bool(false)), "{}", spec.name);
            assert!(out.get("data").is_none());
        }
        let out = dispatch(&NotYetImplemented, "gbm_app_bootstrap", &request(json!({})));
        assert_eq!(error_code(&out), Some("UNSUPPORTED"));
        assert_eq!(out.get("requestId"), Some(&json!("req-7")));
    }

    #[test]
    fn forbidden_and_unknown_commands_are_rejected() {
        for name in [
            "run_shell",
            "execute_git_arbitrary",
            "read_file_arbitrary",
            "fetch_url_arbitrary",
            "get_stored_token",
            "gbm_unknown",
            "",
        ] {
            let out = dispatch(&Fixed(Ok(json!({}))), name, &request(json!({})));
            assert_eq!(error_code(&out), Some("UNSUPPORTED"), "{name}");
            assert_eq!(out.get("requestId"), Some(&json!("req-7")));
        }
        let out = dispatch(
            &Fixed(Ok(json!({}))),
            "run_shell",
            &json!({"requestId": "../x"}),
        );
        assert_eq!(out.get("requestId"), Some(&Value::Null));
    }

    #[test]
    fn handler_is_not_called_for_invalid_args() {
        struct MustNotRun;
        impl CommandHandler for MustNotRun {
            fn handle(&self, _: &CommandSpec, _: &str, _: Value) -> Result<Value, AppError> {
                Ok(json!("handler ran"))
            }
        }
        let out = dispatch(
            &MustNotRun,
            "gbm_repository_get",
            &request(json!({"repositoryId": "../x"})),
        );
        assert_eq!(error_code(&out), Some("CONTRACT_MISMATCH"));
    }

    #[test]
    fn invalid_handler_output_never_reaches_the_renderer() {
        let out = dispatch(
            &Fixed(Ok(json!({"unexpected": true}))),
            "gbm_app_bootstrap",
            &request(json!({})),
        );
        assert_eq!(error_code(&out), Some("INTERNAL"));
        assert!(out.get("data").is_none());
    }

    #[test]
    fn valid_handler_output_is_wrapped_as_success() {
        let ack = json!({"revision": 3});
        let out = dispatch(
            &Fixed(Ok(ack.clone())),
            "gbm_queue_set_paused",
            &request(json!({"paused": true})),
        );
        assert_eq!(out, json!({"ok": true, "requestId": "req-7", "data": ack}));
    }

    #[test]
    fn invalid_handler_error_is_replaced() {
        let mut bad = app_error(ErrorCode::Timeout, "x", "req-7", None);
        bad.correlation_id = "not valid id".into();
        let out = dispatch(&Fixed(Err(bad)), "gbm_app_bootstrap", &request(json!({})));
        assert_eq!(error_code(&out), Some("INTERNAL"));
    }
}
