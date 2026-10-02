//! Live command handler (CR-BE-EARLY-2026-10-02): wires the capabilities that already
//! exist (SQLite stores, OS keyring/session store, PAT validation, backup-root
//! ownership, GitLab namespace listing) to the IPC contract. Every other command still
//! answers `UNSUPPORTED` through `NotYetImplemented`; nothing here fabricates data.
//!
//! Each call runs on a Tauri blocking worker (see `commands`), so async adapters are
//! driven with `block_on` and blocking work (keyring, dialog, filesystem) is allowed.

mod diagnostics;
mod time;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use gbm_application::credentials::{
    CredentialError, CredentialRef, CredentialStore, store_credential,
};
use gbm_application::ipc::{CommandHandler, NotYetImplemented};
use gbm_application::providers::{
    Assessment, ProviderError, TokenIdentity, assess_github_source, assess_gitlab_destination,
};
use gbm_domain::contract::envelope::app_error;
use gbm_domain::contract::secret::SecretString;
use gbm_domain::contract::*;
use gbm_infrastructure::github::GitHubClient;
use gbm_infrastructure::gitlab::GitLabClient;
use gbm_infrastructure::secrets::{KeyringStore, SessionStore};
use gbm_infrastructure::sqlite::{
    AccountRecord, Database, NewAccount, NewActivity, RootRecord, Severity, StoreError,
};
use gbm_infrastructure::storage::{self, RootCandidate, RootError};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use time::now_rfc3339;

/// Picks a folder with the native dialog; `None` when the user cancels.
pub type FolderPicker = Box<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// A folder chosen in the picker, waiting for `gbm_storage_commit_root`.
#[derive(Clone)]
enum Candidate {
    New(RootCandidate),
    /// The root already registered in the database, chosen again (e.g. after a restart).
    Registered(PathBuf),
}

/// What the last successful validation reported; expiry/capabilities are not stored
/// in SQLite, so after a restart they are unknown until the next verification.
#[derive(Clone)]
struct Verified {
    expires_at: Option<String>,
    capability_summary: Vec<String>,
}

pub struct LiveHandler {
    db: Database,
    app_data: PathBuf,
    home: Option<PathBuf>,
    session: SessionStore,
    picker: FolderPicker,
    candidates: Mutex<HashMap<String, Candidate>>,
    verified: Mutex<HashMap<String, Verified>>,
    counter: AtomicU64,
}

type Reply = Result<Value, AppError>;

fn fail(code: ErrorCode, message: &str, request_id: &str) -> AppError {
    app_error(code, message, request_id, None)
}

fn store_error(error: &StoreError, request_id: &str) -> AppError {
    match error {
        StoreError::StaleRevision { .. } => fail(
            ErrorCode::PlanStale,
            "Settings changed elsewhere. Reload to see the latest values.",
            request_id,
        ),
        StoreError::NotFound => fail(
            ErrorCode::Internal,
            "The record no longer exists.",
            request_id,
        ),
        _ => fail(
            ErrorCode::Internal,
            "The application database reported an error.",
            request_id,
        ),
    }
}

fn credential_error(error: CredentialError, request_id: &str) -> AppError {
    let code = match error {
        CredentialError::KeyringUnavailable | CredentialError::KeyringLocked => {
            ErrorCode::KeyringUnavailable
        }
        CredentialError::NotFound => ErrorCode::AuthRequired,
        CredentialError::InvalidSecret => ErrorCode::AuthRequired,
        CredentialError::InvalidReference | CredentialError::Backend => ErrorCode::Internal,
    };
    fail(code, &error.to_string(), request_id)
}

fn provider_error(error: &ProviderError, request_id: &str) -> AppError {
    let mut out = fail(error.error_code(), &error.to_string(), request_id);
    out.retryable = error.retryable();
    out
}

fn parse<T: DeserializeOwned>(args: Value, request_id: &str) -> Result<T, AppError> {
    serde_json::from_value(args).map_err(|_| {
        fail(
            ErrorCode::ContractMismatch,
            "Invalid request arguments.",
            request_id,
        )
    })
}

fn to_value<T: serde::Serialize>(value: &T, request_id: &str) -> Reply {
    serde_json::to_value(value).map_err(|_| {
        fail(
            ErrorCode::Internal,
            "Could not encode the response.",
            request_id,
        )
    })
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "github",
        Provider::Gitlab => "gitlab",
    }
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "GitHub",
        Provider::Gitlab => "GitLab",
    }
}

/// PRODUCT.md default policies.
pub fn default_settings() -> Settings {
    Settings {
        scan_interval_minutes: 30,
        auto_scan: true,
        auto_sync: false,
        max_concurrent_jobs: 3,
        include_private: true,
        include_forks: true,
        include_archived: true,
        include_collaborations: false,
        include_organizations: false,
        workspace_enabled: false,
        notify_errors: true,
        notify_success: false,
        start_minimized: false,
        close_behavior: SettingsCloseBehavior::Ask,
        theme: SettingsTheme::Light,
        retention_cleanup: SettingsRetentionCleanup::Manual,
        telemetry: false,
    }
}

impl LiveHandler {
    pub fn new(db: Database, app_data: PathBuf, picker: FolderPicker) -> Self {
        Self {
            db,
            app_data,
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute()),
            session: SessionStore::new(),
            picker,
            candidates: Mutex::new(HashMap::new()),
            verified: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }

    /// Unique within this database: nanosecond clock plus a process-wide counter.
    fn next_id(&self, prefix: &str) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        format!("{prefix}-{nanos}-{n}")
    }

    fn revision(&self, request_id: &str) -> Result<i64, AppError> {
        Self::block_on(self.db.revision()).map_err(|e| store_error(&e, request_id))
    }

    fn record_activity(
        &self,
        severity: Severity,
        event_type: &str,
        message: &str,
        error_code: Option<&str>,
    ) {
        let id = self.next_id("act");
        let now = now_rfc3339();
        // Activity is best effort; a failure must not undo the user's action.
        let _ = Self::block_on(self.db.append_activity(&NewActivity {
            id: &id,
            repository_id: None,
            job_id: None,
            occurred_at: &now,
            severity,
            event_type,
            safe_message: message,
            error_code,
        }));
    }

    fn summarize(&self, record: &AccountRecord) -> AccountSummary {
        let provider = if record.provider == "gitlab" {
            Provider::Gitlab
        } else {
            Provider::Github
        };
        let mut storage_mode = match record.storage_mode.as_str() {
            "keyring" => AccountSummaryStorageMode::Keyring,
            "session_only" => AccountSummaryStorageMode::SessionOnly,
            _ => AccountSummaryStorageMode::None,
        };
        let mut auth_state = match record.auth_state.as_str() {
            "connected" => AccountSummaryAuthState::Connected,
            "expired" => AccountSummaryAuthState::Expired,
            "forbidden" => AccountSummaryAuthState::Forbidden,
            "disconnected" => AccountSummaryAuthState::Disconnected,
            _ => AccountSummaryAuthState::Unknown,
        };
        // A session-only secret does not survive a restart: report it as gone.
        if storage_mode == AccountSummaryStorageMode::SessionOnly {
            let present = record
                .credential_ref
                .as_deref()
                .and_then(|r| CredentialRef::parse(r).ok())
                .is_some_and(|r| self.session.get(&r).is_ok());
            if !present {
                auth_state = AccountSummaryAuthState::Disconnected;
                storage_mode = AccountSummaryStorageMode::None;
            }
        }
        let verified = self
            .verified
            .lock()
            .ok()
            .and_then(|v| v.get(&record.id).cloned());
        AccountSummary {
            id: record.id.clone(),
            provider,
            username: record.username.clone(),
            auth_state,
            storage_mode,
            expires_at: verified.as_ref().and_then(|v| v.expires_at.clone()),
            capability_summary: verified.map(|v| v.capability_summary).unwrap_or_default(),
            verified_at: record.verified_at.clone(),
        }
    }

    fn accounts(&self, request_id: &str) -> Result<Vec<AccountSummary>, AppError> {
        let records =
            Self::block_on(self.db.list_accounts()).map_err(|e| store_error(&e, request_id))?;
        Ok(records.iter().map(|r| self.summarize(r)).collect())
    }

    fn settings(&self, request_id: &str) -> Result<(Settings, i64), AppError> {
        match Self::block_on(self.db.get_settings()).map_err(|e| store_error(&e, request_id))? {
            Some(record) => {
                let settings = serde_json::from_value(record.document).map_err(|_| {
                    fail(
                        ErrorCode::Internal,
                        "Stored settings are unreadable.",
                        request_id,
                    )
                })?;
                Ok((settings, record.revision))
            }
            None => Ok((default_settings(), 0)),
        }
    }

    fn root(&self, request_id: &str) -> Result<Option<RootRecord>, AppError> {
        let roots =
            Self::block_on(self.db.list_roots()).map_err(|e| store_error(&e, request_id))?;
        Ok(roots.into_iter().next())
    }

    fn display(&self, path: &Path) -> String {
        match self.home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
            Some(rest) => format!("~/{}", rest.display()),
            None => path.display().to_string(),
        }
    }

    fn diagnostics(&self, request_id: &str) -> Result<Diagnostics, AppError> {
        let root = self
            .root(request_id)?
            .map(|r| self.display(Path::new(&r.canonical_path)));
        Ok(Diagnostics {
            items: diagnostics::collect(&self.app_data, root.as_deref()),
            observed_at: now_rfc3339(),
        })
    }

    // --- commands ---------------------------------------------------------------

    fn bootstrap(&self, request_id: &str) -> Reply {
        let (settings, _) = self.settings(request_id)?;
        let bootstrap = Bootstrap {
            // Completion requires a committed destination mapping (gbm_destination_commit,
            // P06-T01), which is not implemented yet; the app stays in onboarding.
            onboarding_complete: false,
            accounts: self.accounts(request_id)?,
            settings,
            diagnostics: self.diagnostics(request_id)?,
            revision: self.revision(request_id)?,
        };
        to_value(&bootstrap, request_id)
    }

    fn accounts_list(&self, request_id: &str) -> Reply {
        let list = AccountList {
            items: self.accounts(request_id)?,
            revision: self.revision(request_id)?,
        };
        to_value(&list, request_id)
    }

    fn validate(
        &self,
        provider: Provider,
        token: &SecretString,
    ) -> Result<(TokenIdentity, Assessment), ProviderError> {
        match provider {
            Provider::Github => {
                let identity = Self::block_on(GitHubClient::new()?.validate_token(token))?;
                let assessment = assess_github_source(&identity)?;
                Ok((identity, assessment))
            }
            Provider::Gitlab => {
                let identity = Self::block_on(GitLabClient::new()?.validate_token(token))?;
                let assessment = assess_gitlab_destination(&identity)?;
                Ok((identity, assessment))
            }
        }
    }

    fn credentials_validate_store(&self, args: Value, request_id: &str) -> Reply {
        let request: CredentialRequest = parse(args, request_id)?;
        let provider = request.provider;
        let (identity, assessment) = self.validate(provider, &request.token).map_err(|e| {
            let code = serde_json::to_value(e.error_code()).ok();
            self.record_activity(
                Severity::Warning,
                "auth",
                &format!("{} token was not accepted: {e}", provider_label(provider)),
                code.as_ref().and_then(Value::as_str),
            );
            provider_error(&e, request_id)
        })?;
        let account_id = format!(
            "acc-{}-{}",
            provider_name(provider),
            identity.provider_user_id
        );
        // The reference is deterministic per account, so a reconnect overwrites the same
        // slot; remember the previous mode to clean the *other* store afterwards.
        let previous = Self::block_on(self.db.list_accounts())
            .map_err(|e| store_error(&e, request_id))?
            .into_iter()
            .find(|r| r.id == account_id);
        let (reference, mode) = match request.storage_mode {
            CredentialRequestStorageMode::Keyring => {
                let keyring =
                    KeyringStore::connect().map_err(|e| credential_error(e, request_id))?;
                (
                    store_credential(&keyring, &account_id, &request.token)
                        .map_err(|e| credential_error(e, request_id))?,
                    "keyring",
                )
            }
            CredentialRequestStorageMode::SessionOnly => (
                store_credential(&self.session, &account_id, &request.token)
                    .map_err(|e| credential_error(e, request_id))?,
                "session_only",
            ),
        };
        drop(request.token);
        let verified_at = now_rfc3339();
        let saved = Self::block_on(self.db.upsert_account(&NewAccount {
            id: &account_id,
            provider: provider_name(provider),
            provider_user_id: &identity.provider_user_id,
            username: &identity.username,
            credential_ref: Some(reference.as_str()),
            auth_state: "connected",
            storage_mode: mode,
            verified_at: Some(&verified_at),
        }));
        let revision = match saved {
            Ok(revision) => revision,
            Err(error) => {
                // Remove the new secret only when no existing account row still refers to
                // this slot in this store; otherwise it replaced an equivalent secret.
                let slot_was_referenced = previous
                    .as_ref()
                    .is_some_and(|p| p.storage_mode == mode && p.credential_ref.is_some());
                if !slot_was_referenced {
                    let _ = delete_secret(&self.session, mode, &reference);
                }
                return Err(store_error(&error, request_id));
            }
        };
        // Switching keyring <-> session-only must not leave the old secret behind.
        if let Some(old) = previous.as_ref()
            && old.storage_mode != mode
            && let Some(old_ref) = old
                .credential_ref
                .as_deref()
                .and_then(|r| CredentialRef::parse(r).ok())
        {
            let _ = delete_secret(&self.session, &old.storage_mode, &old_ref);
        }
        let mut capability_summary = assessment.capability_summary;
        capability_summary.extend(assessment.warnings);
        if let Ok(mut verified) = self.verified.lock() {
            verified.insert(
                account_id.clone(),
                Verified {
                    expires_at: identity.expires_at.clone(),
                    capability_summary,
                },
            );
        }
        self.record_activity(
            Severity::Info,
            "auth",
            &format!(
                "{} account {} connected",
                provider_label(provider),
                identity.username
            ),
            None,
        );
        let record = Self::block_on(self.db.list_accounts())
            .map_err(|e| store_error(&e, request_id))?
            .into_iter()
            .find(|r| r.id == account_id)
            .ok_or_else(|| {
                fail(
                    ErrorCode::Internal,
                    "The account was not saved.",
                    request_id,
                )
            })?;
        to_value(
            &AccountResult {
                account: self.summarize(&record),
                revision,
            },
            request_id,
        )
    }

    fn find_account(&self, account_id: &str, request_id: &str) -> Result<AccountRecord, AppError> {
        Self::block_on(self.db.list_accounts())
            .map_err(|e| store_error(&e, request_id))?
            .into_iter()
            .find(|r| r.id == account_id)
            .ok_or_else(|| {
                fail(
                    ErrorCode::AuthRequired,
                    "This account is not connected.",
                    request_id,
                )
            })
    }

    fn accounts_disconnect(&self, args: Value, request_id: &str) -> Reply {
        let request: AccountRequest = parse(args, request_id)?;
        let record = self.find_account(&request.account_id, request_id)?;
        if let Some(reference) = record
            .credential_ref
            .as_deref()
            .and_then(|r| CredentialRef::parse(r).ok())
        {
            // Delete the secret first so a failure leaves the account intact and retryable.
            delete_secret(&self.session, &record.storage_mode, &reference)
                .map_err(|e| credential_error(e, request_id))?;
        }
        let revision = Self::block_on(self.db.disconnect_account(&record.id))
            .map_err(|e| store_error(&e, request_id))?;
        if let Ok(mut verified) = self.verified.lock() {
            verified.remove(&record.id);
        }
        let label = if record.provider == "gitlab" {
            "GitLab"
        } else {
            "GitHub"
        };
        self.record_activity(
            Severity::Info,
            "auth",
            &format!(
                "{label} account {} disconnected; backups kept",
                record.username
            ),
            None,
        );
        to_value(&Ack { revision }, request_id)
    }

    fn settings_get(&self, request_id: &str) -> Reply {
        let (settings, revision) = self.settings(request_id)?;
        to_value(&SettingsResult { settings, revision }, request_id)
    }

    fn settings_update(&self, args: Value, request_id: &str) -> Reply {
        let update: SettingsUpdate = parse(args, request_id)?;
        let document = serde_json::to_value(&update.settings).map_err(|_| {
            fail(
                ErrorCode::Internal,
                "Could not encode settings.",
                request_id,
            )
        })?;
        let expected = (update.expected_revision > 0).then_some(update.expected_revision);
        let revision = Self::block_on(self.db.put_settings(&document, expected))
            .map_err(|e| store_error(&e, request_id))?;
        to_value(
            &SettingsResult {
                settings: update.settings,
                revision,
            },
            request_id,
        )
    }

    fn activity_list(&self, args: Value, request_id: &str) -> Reply {
        let request: ActivityRequest = parse(args, request_id)?;
        let records =
            Self::block_on(self.db.list_activity(1000)).map_err(|e| store_error(&e, request_id))?;
        let needle = request.search.trim().to_lowercase();
        let wanted = match request.severity {
            ActivityRequestSeverity::All => None,
            ActivityRequestSeverity::Info => Some("info"),
            ActivityRequestSeverity::Warning => Some("warning"),
            ActivityRequestSeverity::Error => Some("error"),
        };
        // Stored activity is not linked to repositories yet; a repository filter matches none.
        let any_repository = request.repository_id.is_none();
        let matching: Vec<_> = records
            .into_iter()
            .filter(|_| any_repository)
            .filter(|r| wanted.is_none_or(|w| r.severity == w))
            .filter(|r| needle.is_empty() || r.safe_message.to_lowercase().contains(&needle))
            .collect();
        let start = request
            .cursor
            .as_deref()
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0);
        let limit = usize::try_from(request.limit.clamp(1, 200)).unwrap_or(50);
        let items = matching
            .iter()
            .skip(start)
            .take(limit)
            .map(|r| ActivityEntry {
                id: r.id.clone(),
                repository_id: None,
                job_id: None,
                occurred_at: r.occurred_at.clone(),
                severity: match r.severity.as_str() {
                    "error" => ActivityEntrySeverity::Error,
                    "warning" => ActivityEntrySeverity::Warning,
                    _ => ActivityEntrySeverity::Info,
                },
                r#type: match r.event_type.as_str() {
                    "scan" => ActivityEntryType::Scan,
                    "sync" => ActivityEntryType::Sync,
                    "auth" => ActivityEntryType::Auth,
                    "project" => ActivityEntryType::Project,
                    _ => ActivityEntryType::System,
                },
                message: r.safe_message.clone(),
                error_code: None,
            })
            .collect();
        let next = (start + limit < matching.len()).then(|| (start + limit).to_string());
        to_value(
            &ActivityList {
                items,
                next_cursor: next,
                revision: self.revision(request_id)?,
            },
            request_id,
        )
    }

    fn storage_pick_root(&self, request_id: &str) -> Reply {
        let Some(path) = (self.picker)() else {
            return to_value(
                &StoragePickResult {
                    outcome: StoragePickResultOutcome::Canceled,
                    candidate: None,
                },
                request_id,
            );
        };
        let candidate_id = self.next_id("cand");
        let existing = self.root(request_id)?;
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let candidate = match existing {
            // The already registered root may be chosen again (e.g. after a restart).
            Some(root) if Path::new(&root.canonical_path) == canonical => Ok(None),
            Some(_) => Err(fail(
                ErrorCode::RootRelocationUnsupported,
                "A backup folder is already set. Moving it is not supported in this version.",
                request_id,
            )),
            None if self.home.as_deref() == Some(canonical.as_path())
                || canonical.starts_with(&self.app_data)
                || self.app_data.starts_with(&canonical) =>
            {
                Err(fail(
                    ErrorCode::PathRejected,
                    "Choose a dedicated folder, not your home folder or the application data folder.",
                    request_id,
                ))
            }
            None => storage::validate_candidate(&path)
                .map(Some)
                .map_err(|e| fail(ErrorCode::PathRejected, &root_error_text(e), request_id)),
        };
        let (valid, errors) = match candidate {
            Ok(found) => {
                let entry =
                    found.map_or_else(|| Candidate::Registered(canonical.clone()), Candidate::New);
                if let Ok(mut map) = self.candidates.lock() {
                    map.insert(candidate_id.clone(), entry);
                }
                (true, vec![])
            }
            Err(error) => (false, vec![error]),
        };
        let result = StoragePickResult {
            outcome: StoragePickResultOutcome::Selected,
            candidate: Some(StorageCandidate {
                candidate_id,
                display_path: self.display(&canonical),
                valid,
                errors,
            }),
        };
        to_value(&result, request_id)
    }

    fn storage_commit_root(&self, args: Value, request_id: &str) -> Reply {
        let request: RootCommitRequest = parse(args, request_id)?;
        let candidate = self
            .candidates
            .lock()
            .ok()
            .and_then(|map| map.get(&request.candidate_id).cloned())
            .ok_or_else(|| {
                fail(
                    ErrorCode::PathRejected,
                    "Choose the folder again.",
                    request_id,
                )
            })?;
        let existing = self.root(request_id)?;
        let candidate = match (candidate, existing) {
            (Candidate::Registered(path), Some(root))
                if Path::new(&root.canonical_path) == path =>
            {
                return to_value(
                    &self.root_summary(&root, request.workspace_enabled),
                    request_id,
                );
            }
            (Candidate::New(candidate), None) => candidate,
            _ => {
                return Err(fail(
                    ErrorCode::RootRelocationUnsupported,
                    "A backup folder is already set. Moving it is not supported in this version.",
                    request_id,
                ));
            }
        };
        let root_id = self.next_id("root");
        let created_at = now_rfc3339();
        let established = storage::establish(&candidate, &root_id, &created_at)
            .map_err(|e| fail(ErrorCode::PathRejected, &root_error_text(e), request_id))?;
        let established_path = established.root().path().to_owned();
        let record = RootRecord {
            id: root_id.clone(),
            canonical_path: established_path.display().to_string(),
            ownership_marker: root_id,
            created_at,
        };
        if let Err(error) = Self::block_on(self.db.insert_root(&record)) {
            // Undo only what this call created, so the folder can be chosen again.
            established.undo();
            return Err(store_error(&error, request_id));
        }
        if let Ok(mut map) = self.candidates.lock() {
            map.remove(&request.candidate_id);
        }
        self.record_activity(
            Severity::Info,
            "system",
            &format!("Backup folder set to {}", self.display(&established_path)),
            None,
        );
        to_value(
            &self.root_summary(&record, request.workspace_enabled),
            request_id,
        )
    }

    fn root_summary(&self, root: &RootRecord, workspace_enabled: bool) -> RootSummary {
        RootSummary {
            id: root.id.clone(),
            display_path: self.display(Path::new(&root.canonical_path)),
            ownership_verified: Path::new(&root.canonical_path)
                .join(storage::APP_DIR)
                .join(storage::MANIFEST_FILE)
                .is_file(),
            // Free space needs a statvfs probe that this build does not include: unknown.
            available_bytes: None,
            workspace_enabled,
        }
    }

    fn secret_for(
        &self,
        record: &AccountRecord,
        request_id: &str,
    ) -> Result<SecretString, AppError> {
        let reference = record
            .credential_ref
            .as_deref()
            .and_then(|r| CredentialRef::parse(r).ok())
            .ok_or_else(|| {
                fail(
                    ErrorCode::AuthRequired,
                    "Connect this account again.",
                    request_id,
                )
            })?;
        match record.storage_mode.as_str() {
            "keyring" => KeyringStore::connect().and_then(|k| k.get(&reference)),
            _ => self.session.get(&reference),
        }
        .map_err(|e| credential_error(e, request_id))
    }

    fn namespaces_list(&self, args: Value, request_id: &str) -> Reply {
        let request: NamespaceRequest = parse(args, request_id)?;
        let record = self.find_account(&request.account_id, request_id)?;
        if record.provider != "gitlab" {
            return Err(fail(
                ErrorCode::AccessDenied,
                "Namespaces are listed for GitLab accounts only.",
                request_id,
            ));
        }
        let token = self.secret_for(&record, request_id)?;
        let page = request
            .cursor
            .as_deref()
            .and_then(|c| c.parse::<u32>().ok())
            .unwrap_or(1);
        let per_page = u32::try_from(request.limit.clamp(1, 100)).unwrap_or(20);
        let client = GitLabClient::new().map_err(|e| provider_error(&e, request_id))?;
        let (items, next) = Self::block_on(client.list_namespaces(&token, page, per_page))
            .map_err(|e| provider_error(&e, request_id))?;
        let list = NamespaceList {
            items: items
                .into_iter()
                // Create permission is confirmed by the destination preview, not here.
                .map(|ns| Namespace {
                    id: ns.id,
                    full_path: ns.full_path,
                    can_create_project: None,
                })
                .collect(),
            next_cursor: next.map(|n| n.to_string()),
        };
        to_value(&list, request_id)
    }
}

/// Deletes a secret from the store named by an account's `storage_mode`.
fn delete_secret(
    session: &SessionStore,
    mode: &str,
    reference: &CredentialRef,
) -> Result<(), CredentialError> {
    match mode {
        "keyring" => KeyringStore::connect().and_then(|k| k.delete(reference)),
        "session_only" => session.delete(reference),
        _ => Ok(()),
    }
}

fn root_error_text(error: RootError) -> String {
    let mut text = error.to_string();
    if let Some(first) = text.get(..1) {
        text = format!("{}{}.", first.to_uppercase(), &text[1..]);
    }
    text
}

impl CommandHandler for LiveHandler {
    fn handle(&self, spec: &CommandSpec, request_id: &str, args: Value) -> Reply {
        match spec.name {
            "gbm_app_bootstrap" => self.bootstrap(request_id),
            "gbm_accounts_list" => self.accounts_list(request_id),
            "gbm_credentials_validate_store" => self.credentials_validate_store(args, request_id),
            "gbm_accounts_disconnect" => self.accounts_disconnect(args, request_id),
            "gbm_settings_get" => self.settings_get(request_id),
            "gbm_settings_update" => self.settings_update(args, request_id),
            "gbm_environment_diagnostics" => to_value(&self.diagnostics(request_id)?, request_id),
            "gbm_activity_list" => self.activity_list(args, request_id),
            "gbm_storage_pick_root" => self.storage_pick_root(request_id),
            "gbm_storage_commit_root" => self.storage_commit_root(args, request_id),
            "gbm_namespaces_list" => self.namespaces_list(args, request_id),
            _ => NotYetImplemented.handle(spec, request_id, args),
        }
    }
}

/// Used when the database could not be opened at startup: every command reports it.
pub struct StartupFailed;

impl CommandHandler for StartupFailed {
    fn handle(&self, _spec: &CommandSpec, request_id: &str, _args: Value) -> Reply {
        Err(app_error(
            ErrorCode::Internal,
            "The application database could not be opened.",
            request_id,
            Some("Check that the application data folder is writable, then restart.".to_owned()),
        ))
    }
}

#[cfg(test)]
mod tests;
