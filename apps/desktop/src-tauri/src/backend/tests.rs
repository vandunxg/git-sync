// Handler tests against a real temporary SQLite database and the in-memory session
// store. No network: provider validation is exercised in the infrastructure tests.

use std::path::PathBuf;
use std::sync::Arc;

use gbm_application::ipc::dispatch;
use serde_json::{Value, json};

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    dir: PathBuf,
    handler: Arc<LiveHandler>,
}

fn fixture(name: &str, picked: Option<PathBuf>) -> Result<Fixture, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("gbm-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let (db, _) = tauri::async_runtime::block_on(Database::open(&dir.join("state.sqlite")))?;
    let picker: FolderPicker = Box::new(move || picked.clone());
    Ok(Fixture {
        handler: Arc::new(LiveHandler::new(db, dir.clone(), picker)),
        dir,
    })
}

fn call(handler: &LiveHandler, command: &str, args: Value) -> Value {
    dispatch(
        handler,
        command,
        &json!({"schemaVersion": 1, "requestId": "req-1", "args": args}),
    )
}

fn data(envelope: &Value) -> Result<&Value, String> {
    if envelope.get("ok") == Some(&json!(true)) {
        envelope.get("data").ok_or_else(|| "no data".to_owned())
    } else {
        Err(format!("failure: {envelope}"))
    }
}

#[test]
fn bootstrap_is_real_and_stays_in_onboarding() -> TestResult {
    let f = fixture("boot", None)?;
    let out = call(&f.handler, "gbm_app_bootstrap", json!({}));
    let body = data(&out)?;
    assert_eq!(body["onboardingComplete"], json!(false));
    assert_eq!(body["accounts"], json!([]));
    assert_eq!(body["settings"]["maxConcurrentJobs"], json!(3));
    assert_eq!(body["settings"]["autoSync"], json!(false));
    let keys: Vec<_> = body["diagnostics"]["items"]
        .as_array()
        .ok_or("items")?
        .iter()
        .filter_map(|i| i["key"].as_str())
        .collect();
    assert!(keys.contains(&"git") && keys.contains(&"keyring") && keys.contains(&"storage-root"));
    std::fs::remove_dir_all(&f.dir)?;
    Ok(())
}

#[test]
fn settings_round_trip_with_revision_conflict() -> TestResult {
    let f = fixture("settings", None)?;
    let got = call(&f.handler, "gbm_settings_get", json!({}));
    let mut settings = data(&got)?["settings"].clone();
    assert_eq!(data(&got)?["revision"], json!(0));
    settings["maxConcurrentJobs"] = json!(5);
    let saved = call(
        &f.handler,
        "gbm_settings_update",
        json!({"settings": settings, "expectedRevision": 0}),
    );
    let revision = data(&saved)?["revision"].as_i64().ok_or("revision")?;
    assert!(revision > 0);
    let again = call(&f.handler, "gbm_settings_get", json!({}));
    assert_eq!(data(&again)?["settings"]["maxConcurrentJobs"], json!(5));
    let stale = call(
        &f.handler,
        "gbm_settings_update",
        json!({"settings": settings, "expectedRevision": 0}),
    );
    assert_eq!(stale.pointer("/error/code"), Some(&json!("PLAN_STALE")));
    let invalid = {
        let mut bad = settings.clone();
        bad["maxConcurrentJobs"] = json!(42);
        call(
            &f.handler,
            "gbm_settings_update",
            json!({"settings": bad, "expectedRevision": revision}),
        )
    };
    assert_eq!(
        invalid.pointer("/error/code"),
        Some(&json!("CONTRACT_MISMATCH"))
    );
    std::fs::remove_dir_all(&f.dir)?;
    Ok(())
}

#[test]
fn root_pick_cancel_reject_and_commit() -> TestResult {
    let canceled = fixture("cancel", None)?;
    let out = call(&canceled.handler, "gbm_storage_pick_root", json!({}));
    assert_eq!(data(&out)?["outcome"], json!("canceled"));
    std::fs::remove_dir_all(&canceled.dir)?;

    let rejected = fixture("reject", Some(PathBuf::from("/etc")))?;
    let out = call(&rejected.handler, "gbm_storage_pick_root", json!({}));
    assert_eq!(data(&out)?["candidate"]["valid"], json!(false));
    assert_eq!(
        data(&out)?["candidate"]["errors"][0]["code"],
        json!("PATH_REJECTED")
    );
    std::fs::remove_dir_all(&rejected.dir)?;

    let base = std::env::temp_dir().join(format!("gbm-live-rootdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base)?;
    let f = fixture("commit", Some(base.clone()))?;
    let picked = call(&f.handler, "gbm_storage_pick_root", json!({}));
    let candidate = data(&picked)?["candidate"].clone();
    assert_eq!(candidate["valid"], json!(true));
    let committed = call(
        &f.handler,
        "gbm_storage_commit_root",
        json!({"candidateId": candidate["candidateId"], "workspaceEnabled": false}),
    );
    let root = data(&committed)?;
    assert_eq!(root["ownershipVerified"], json!(true));
    assert_eq!(
        root["availableBytes"],
        Value::Null,
        "free space is not invented"
    );
    assert!(base.join(".gbm/root-manifest.json").is_file());

    // Choosing the same folder again (e.g. after a restart) is idempotent.
    let again = call(&f.handler, "gbm_storage_pick_root", json!({}));
    let id = data(&again)?["candidate"]["candidateId"].clone();
    let recommit = call(
        &f.handler,
        "gbm_storage_commit_root",
        json!({"candidateId": id, "workspaceEnabled": true}),
    );
    assert_eq!(data(&recommit)?["id"], root["id"]);
    let unknown = call(
        &f.handler,
        "gbm_storage_commit_root",
        json!({"candidateId": "cand-nope", "workspaceEnabled": false}),
    );
    assert_eq!(
        unknown.pointer("/error/code"),
        Some(&json!("PATH_REJECTED"))
    );
    std::fs::remove_dir_all(&f.dir)?;
    std::fs::remove_dir_all(&base)?;
    Ok(())
}

#[test]
fn unimplemented_commands_stay_unsupported_and_unknown_account_is_rejected() -> TestResult {
    let f = fixture("unsupported", None)?;
    for command in [
        "gbm_dashboard_get",
        "gbm_jobs_list",
        "gbm_destination_preview",
    ] {
        let out = call(&f.handler, command, json!({}));
        assert_eq!(out.get("ok"), Some(&json!(false)), "{command}");
    }
    let out = call(
        &f.handler,
        "gbm_accounts_disconnect",
        json!({"accountId": "acc-github-1"}),
    );
    assert_eq!(out.pointer("/error/code"), Some(&json!("AUTH_REQUIRED")));
    let out = call(
        &f.handler,
        "gbm_namespaces_list",
        json!({"accountId": "acc-gitlab-1", "cursor": null, "limit": 20}),
    );
    assert_eq!(out.pointer("/error/code"), Some(&json!("AUTH_REQUIRED")));
    std::fs::remove_dir_all(&f.dir)?;
    Ok(())
}

#[test]
fn session_only_account_disconnects_and_never_echoes_the_token() -> TestResult {
    let f = fixture("session", None)?;
    let reference = store_credential(
        &f.handler.session,
        "acc-github-42",
        &SecretString::new("ghp_SESSIONSENTINEL123456".to_owned()),
    )?;
    tauri::async_runtime::block_on(f.handler.db.upsert_account(&NewAccount {
        id: "acc-github-42",
        provider: "github",
        provider_user_id: "42",
        username: "octo",
        credential_ref: Some(reference.as_str()),
        auth_state: "connected",
        storage_mode: "session_only",
        verified_at: Some("2026-10-02T00:00:00Z"),
    }))?;
    let listed = call(&f.handler, "gbm_accounts_list", json!({}));
    assert_eq!(data(&listed)?["items"][0]["authState"], json!("connected"));
    assert!(!listed.to_string().contains("SESSIONSENTINEL"));
    let out = call(
        &f.handler,
        "gbm_accounts_disconnect",
        json!({"accountId": "acc-github-42"}),
    );
    data(&out)?;
    let listed = call(&f.handler, "gbm_accounts_list", json!({}));
    assert_eq!(
        data(&listed)?["items"][0]["authState"],
        json!("disconnected")
    );
    assert!(f.handler.session.get(&reference).is_err(), "secret removed");
    let activity = call(
        &f.handler,
        "gbm_activity_list",
        json!({"cursor": null, "limit": 20, "repositoryId": null, "severity": "all", "search": ""}),
    );
    assert!(
        data(&activity)?["items"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("disconnected; backups kept")
    );
    std::fs::remove_dir_all(&f.dir)?;
    Ok(())
}

#[test]
fn app_data_folder_cannot_be_the_backup_root() -> TestResult {
    let f = fixture("appdata", None)?;
    let inside = f.dir.join("inner");
    std::fs::create_dir_all(&inside)?;
    let handler = LiveHandler::new(
        tauri::async_runtime::block_on(Database::open(&f.dir.join("other.sqlite")))?.0,
        f.dir.clone(),
        Box::new(move || Some(inside.clone())),
    );
    let out = call(&handler, "gbm_storage_pick_root", json!({}));
    assert_eq!(data(&out)?["candidate"]["valid"], json!(false));
    std::fs::remove_dir_all(&f.dir)?;
    Ok(())
}
