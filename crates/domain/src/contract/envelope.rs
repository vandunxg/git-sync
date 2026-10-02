//! Request/result envelopes from contracts/ipc.v1.json.
//!
//! Request: `{schemaVersion, requestId, args}`, exactly these keys, `schemaVersion == 1`,
//! `requestId` an `Id`, `args` valid for the command's request DTO.
//! Result: `{ok: true, requestId, data}` or `{ok: false, requestId, error}`, never both.
//! `requestId` is `null` only on failure when the request carried no valid `Id`.

use serde_json::{Value, json};

use super::schema::{SchemaError, is_id, validate_definition};
use super::{AppError, CommandSpec, ErrorCode, SCHEMA_VERSION};

const REQUEST_KEYS: [&str; 3] = ["schemaVersion", "requestId", "args"];

/// An envelope whose shape, version, request ID and args passed validation.
/// `args` may hold a secret (credential command), so this type is neither `Debug`
/// nor `Clone`; handlers must deserialize it immediately and never format it.
pub struct ValidatedRequest {
    pub request_id: String,
    pub args: Value,
}

/// A rejected request. `request_id` is the caller's ID when it was itself valid.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedRequest {
    pub request_id: Option<String>,
    pub code: ErrorCode,
    pub message: &'static str,
    pub details: Option<String>,
}

/// Validates a raw request envelope for `spec`. For sensitive commands the rejection
/// never carries argument-level details, only a generic message.
/// Returns the envelope's `requestId` when it is a valid `Id`, for correlating failures.
pub fn valid_request_id(raw: &Value) -> Option<&str> {
    raw.get("requestId")
        .and_then(Value::as_str)
        .filter(|id| is_id(id))
}

pub fn parse_request(spec: &CommandSpec, raw: &Value) -> Result<ValidatedRequest, RejectedRequest> {
    let reject = |request_id: Option<String>, message: &'static str, details: Option<String>| {
        RejectedRequest {
            request_id,
            code: ErrorCode::ContractMismatch,
            message,
            details,
        }
    };
    let Some(map) = raw.as_object() else {
        return Err(reject(None, "Request envelope must be an object.", None));
    };
    let request_id = valid_request_id(raw).map(str::to_owned);
    if let Some(extra) = map.keys().find(|key| !REQUEST_KEYS.contains(&key.as_str())) {
        let details = (!spec.sensitive).then(|| format!("unexpected envelope field `{extra}`"));
        return Err(reject(
            request_id,
            "Request envelope has unexpected fields.",
            details,
        ));
    }
    let Some(request_id) = request_id else {
        return Err(reject(
            None,
            "Request envelope requires a valid requestId.",
            None,
        ));
    };
    match map.get("schemaVersion") {
        Some(version) if version.as_i64() == Some(SCHEMA_VERSION) && version.is_i64() => {}
        _ => {
            return Err(reject(
                Some(request_id),
                "Unsupported request schemaVersion.",
                None,
            ));
        }
    }
    let Some(args) = map.get("args") else {
        return Err(reject(
            Some(request_id),
            "Request envelope requires args.",
            None,
        ));
    };
    match validate_definition(spec.request_type, args) {
        Ok(violations) if violations.is_empty() => Ok(ValidatedRequest {
            request_id,
            args: args.clone(),
        }),
        Ok(violations) => {
            let details = (!spec.sensitive).then(|| {
                violations
                    .iter()
                    .take(8)
                    .map(|v| format!("{}: {}", v.path.replacen('$', "args", 1), v.reason))
                    .collect::<Vec<_>>()
                    .join("; ")
            });
            Err(reject(
                Some(request_id),
                "Request arguments do not match the contract.",
                details,
            ))
        }
        Err(SchemaError::Unavailable | SchemaError::UnknownDefinition) => Err(RejectedRequest {
            request_id: Some(request_id),
            code: ErrorCode::Internal,
            message: "Contract schema unavailable.",
            details: None,
        }),
    }
}

/// Builds `{ok: true, requestId, data}`.
pub fn success(request_id: &str, data: Value) -> Value {
    json!({ "ok": true, "requestId": request_id, "data": data })
}

/// Builds `{ok: false, requestId, error}`.
pub fn failure(request_id: Option<&str>, error: &AppError) -> Value {
    json!({ "ok": false, "requestId": request_id, "error": error })
}

/// Creates a non-retryable `AppError` with a sanitized, bounded message.
pub fn app_error(
    code: ErrorCode,
    message: &str,
    correlation_id: &str,
    details: Option<String>,
) -> AppError {
    AppError {
        code,
        message: bounded(message, 256),
        retryable: false,
        retry_at: None,
        correlation_id: correlation_id.to_owned(),
        details: details.map(|d| bounded(&d, 1024)),
    }
}

fn bounded(text: &str, max_chars: usize) -> String {
    let trimmed: String = text.chars().take(max_chars).collect();
    if trimmed.is_empty() {
        "error".to_owned()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::command_spec;

    fn spec(name: &str) -> &'static CommandSpec {
        match command_spec(name) {
            Some(spec) => spec,
            None => &crate::contract::COMMANDS[0],
        }
    }

    fn rejected(name: &str, raw: Value) -> Option<RejectedRequest> {
        parse_request(spec(name), &raw).err()
    }

    #[test]
    fn valid_empty_request_is_accepted() {
        let raw = json!({"schemaVersion": 1, "requestId": "req-1", "args": {}});
        let parsed = parse_request(spec("gbm_app_bootstrap"), &raw);
        assert!(parsed.is_ok_and(|r| r.request_id == "req-1" && r.args == json!({})));
    }

    #[test]
    fn malformed_envelopes_are_rejected() {
        let cases = [
            json!(null),
            json!([]),
            json!("x"),
            json!({}),
            json!({"schemaVersion": 1, "args": {}}),
            json!({"schemaVersion": 1, "requestId": "", "args": {}}),
            json!({"schemaVersion": 1, "requestId": "../etc", "args": {}}),
            json!({"schemaVersion": 2, "requestId": "r", "args": {}}),
            json!({"schemaVersion": 1.0, "requestId": "r", "args": {}}),
            json!({"schemaVersion": "1", "requestId": "r", "args": {}}),
            json!({"requestId": "r", "args": {}}),
            json!({"schemaVersion": 1, "requestId": "r"}),
            json!({"schemaVersion": 1, "requestId": "r", "args": {}, "extra": true}),
            json!({"schemaVersion": 1, "requestId": "r", "args": {"unexpected": 1}}),
            json!({"schemaVersion": 1, "requestId": "r", "args": null}),
        ];
        for raw in cases {
            let outcome = rejected("gbm_app_bootstrap", raw.clone());
            assert!(outcome.is_some(), "accepted malformed envelope {raw}");
        }
    }

    #[test]
    fn arbitrary_ids_and_paths_are_rejected() {
        for id in [
            "",
            "../../etc/passwd",
            "/home/user",
            "a b",
            "id;rm -rf",
            "x\n",
        ] {
            let raw = json!({"schemaVersion": 1, "requestId": "r",
                             "args": {"repositoryId": id}});
            assert!(
                rejected("gbm_repository_get", raw).is_some(),
                "accepted id {id:?}"
            );
        }
    }

    #[test]
    fn sensitive_rejections_carry_no_details() {
        let raw = json!({"schemaVersion": 1, "requestId": "r",
                         "args": {"provider": "github", "token": "", "storageMode": "keyring"}});
        let outcome = rejected("gbm_credentials_validate_store", raw);
        assert!(outcome.is_some_and(|r| r.details.is_none()));
        let raw = json!({"schemaVersion": 1, "requestId": "r", "args": {}, "token": "secret"});
        let outcome = rejected("gbm_credentials_validate_store", raw);
        assert!(outcome.is_some_and(|r| r.details.is_none()));
    }

    #[test]
    fn result_envelopes_have_exactly_one_payload() {
        let ok = success("r", json!({}));
        assert_eq!(ok, json!({"ok": true, "requestId": "r", "data": {}}));
        let error = app_error(ErrorCode::Unsupported, "Not available.", "r", None);
        let failed = failure(None, &error);
        let keys: Vec<_> = failed
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        assert_eq!(keys.len(), 3);
        assert_eq!(failed.get("requestId"), Some(&Value::Null));
        assert!(failed.get("data").is_none());
        let violations =
            validate_definition("AppError", failed.get("error").unwrap_or(&Value::Null));
        assert_eq!(violations, Ok(vec![]));
    }
}
