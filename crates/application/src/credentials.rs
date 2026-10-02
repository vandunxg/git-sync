//! Credential storage ports and the connect/disconnect use cases (REQ-12).
//!
//! Rules: the OS keyring is the only persistent secret store; session-only storage is
//! an explicit user choice, never an automatic fallback; there is no plaintext path.
//! Disconnect removes the local secret and marks the account disconnected; it never
//! deletes backups, mappings or history, and it does not revoke the provider token.
//! Ports are synchronous: callers run them on a bounded blocking worker (R-RUST).

use std::fmt;

use gbm_domain::contract::schema::is_id;
use gbm_domain::contract::secret::SecretString;

/// Where a credential is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    Keyring,
    SessionOnly,
}

impl StorageMode {
    /// Value persisted in `accounts.storage_mode`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyring => "keyring",
            Self::SessionOnly => "session_only",
        }
    }
}

/// Opaque `cred:<id>` handle; the only credential data SQLite ever stores.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CredentialRef(String);

impl CredentialRef {
    pub fn for_account(account_id: &str) -> Result<Self, CredentialError> {
        Self::parse(&format!("cred:{account_id}"))
    }

    pub fn parse(value: &str) -> Result<Self, CredentialError> {
        // Same rules as the SQLite adapter's `validate_credential_ref`.
        const TOKEN_PREFIXES: [&str; 8] = [
            "ghp_",
            "gho_",
            "ghu_",
            "ghs_",
            "ghr_",
            "github_pat_",
            "glpat-",
            "gloas-",
        ];
        let valid = value.strip_prefix("cred:").is_some_and(|rest| {
            let lowered = rest.to_ascii_lowercase();
            is_id(rest)
                && rest.len() <= 64
                && !TOKEN_PREFIXES.iter().any(|p| lowered.starts_with(p))
                && !(rest.len() >= 32 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
        });
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(CredentialError::InvalidReference)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Sanitized credential errors. No variant can carry secret bytes or backend text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    /// No usable Secret Service (not running, not installed, no session bus).
    KeyringUnavailable,
    /// The keyring exists but is locked and was not unlocked (or the prompt failed).
    KeyringLocked,
    NotFound,
    InvalidReference,
    /// The secret itself is unusable (empty or too long); nothing was stored.
    InvalidSecret,
    /// Any other backend failure; details are intentionally dropped.
    Backend,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::KeyringUnavailable => "the system keyring is not available",
            Self::KeyringLocked => "the system keyring is locked",
            Self::NotFound => "no stored credential for this account",
            Self::InvalidReference => "invalid credential reference",
            Self::InvalidSecret => "the credential is empty or too long",
            Self::Backend => "the credential store reported an error",
        })
    }
}

impl std::error::Error for CredentialError {}

/// A secret store. Implementations must not log, serialize or cache secrets elsewhere.
pub trait CredentialStore: Send + Sync {
    fn mode(&self) -> StorageMode;
    fn put(&self, reference: &CredentialRef, secret: &SecretString) -> Result<(), CredentialError>;
    fn get(&self, reference: &CredentialRef) -> Result<SecretString, CredentialError>;
    /// Idempotent: deleting a missing credential succeeds.
    fn delete(&self, reference: &CredentialRef) -> Result<(), CredentialError>;
}

/// Account persistence needed by the use cases (implemented by the SQLite adapter).
pub trait AccountDirectory: Send + Sync {
    /// The stored reference and mode, or `None` when the account has no credential.
    fn credential_of(
        &self,
        account_id: &str,
    ) -> Result<Option<(CredentialRef, StorageMode)>, CredentialError>;
    /// Clears the reference and marks the account disconnected; keeps the row.
    fn mark_disconnected(&self, account_id: &str) -> Result<(), CredentialError>;
}

/// Matches `CredentialRequest.token.maxLength` in contracts/dtos.schema.json.
const MAX_SECRET_CHARS: usize = 8192;

/// Stores `secret` for `account_id` in the store the user explicitly chose. A keyring
/// failure is returned as-is; it is never retried in session or plaintext storage.
pub fn store_credential(
    store: &dyn CredentialStore,
    account_id: &str,
    secret: &SecretString,
) -> Result<CredentialRef, CredentialError> {
    let chars = secret.expose().chars().count();
    if chars == 0 || chars > MAX_SECRET_CHARS {
        return Err(CredentialError::InvalidSecret);
    }
    let reference = CredentialRef::for_account(account_id)?;
    store.put(&reference, secret)?;
    Ok(reference)
}

/// Disconnect: delete the local secret first (so a failure leaves the account intact
/// and retryable, never a "disconnected" account with an orphaned secret), then mark
/// the account disconnected. Backups and mappings are untouched by design.
pub fn disconnect_account(
    keyring: &dyn CredentialStore,
    session: &dyn CredentialStore,
    directory: &dyn AccountDirectory,
    account_id: &str,
) -> Result<(), CredentialError> {
    if let Some((reference, mode)) = directory.credential_of(account_id)? {
        match mode {
            StorageMode::Keyring => keyring.delete(&reference)?,
            StorageMode::SessionOnly => session.delete(&reference)?,
        }
    }
    directory.mark_disconnected(account_id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    const SENTINEL: &str = "glpat-TESTSENTINEL-0000";

    /// Test-only fake store; it is never compiled into production builds.
    struct FakeStore {
        mode: StorageMode,
        fail: Option<CredentialError>,
        items: Mutex<HashMap<String, String>>,
    }

    impl FakeStore {
        fn new(mode: StorageMode, fail: Option<CredentialError>) -> Self {
            Self {
                mode,
                fail,
                items: Mutex::new(HashMap::new()),
            }
        }
        fn len(&self) -> usize {
            self.items.lock().map(|m| m.len()).unwrap_or(usize::MAX)
        }
    }

    impl CredentialStore for FakeStore {
        fn mode(&self) -> StorageMode {
            self.mode
        }
        fn put(&self, r: &CredentialRef, s: &SecretString) -> Result<(), CredentialError> {
            if let Some(e) = self.fail {
                return Err(e);
            }
            let mut items = self.items.lock().map_err(|_| CredentialError::Backend)?;
            items.insert(r.as_str().to_owned(), s.expose().to_owned());
            Ok(())
        }
        fn get(&self, r: &CredentialRef) -> Result<SecretString, CredentialError> {
            let items = self.items.lock().map_err(|_| CredentialError::Backend)?;
            items
                .get(r.as_str())
                .map(|s| SecretString::new(s.clone()))
                .ok_or(CredentialError::NotFound)
        }
        fn delete(&self, r: &CredentialRef) -> Result<(), CredentialError> {
            if let Some(e) = self.fail {
                return Err(e);
            }
            let mut items = self.items.lock().map_err(|_| CredentialError::Backend)?;
            items.remove(r.as_str());
            Ok(())
        }
    }

    struct FakeDirectory {
        entry: Option<(CredentialRef, StorageMode)>,
        disconnected: Mutex<bool>,
    }

    impl AccountDirectory for FakeDirectory {
        fn credential_of(
            &self,
            _: &str,
        ) -> Result<Option<(CredentialRef, StorageMode)>, CredentialError> {
            Ok(self.entry.clone())
        }
        fn mark_disconnected(&self, _: &str) -> Result<(), CredentialError> {
            let mut flag = self
                .disconnected
                .lock()
                .map_err(|_| CredentialError::Backend)?;
            *flag = true;
            Ok(())
        }
    }

    fn secret() -> SecretString {
        SecretString::new(SENTINEL.to_owned())
    }

    #[test]
    fn keyring_failures_never_fall_back() {
        for failure in [
            CredentialError::KeyringUnavailable,
            CredentialError::KeyringLocked,
        ] {
            let keyring = FakeStore::new(StorageMode::Keyring, Some(failure));
            assert_eq!(store_credential(&keyring, "acc-1", &secret()), Err(failure));
            assert_eq!(keyring.len(), 0);
        }
    }

    #[test]
    fn session_only_is_an_explicit_store() -> Result<(), CredentialError> {
        let session = FakeStore::new(StorageMode::SessionOnly, None);
        let reference = store_credential(&session, "acc-1", &secret())?;
        assert_eq!(reference.as_str(), "cred:acc-1");
        assert_eq!(session.get(&reference)?.expose(), SENTINEL);
        assert_eq!(StorageMode::SessionOnly.as_str(), "session_only");
        Ok(())
    }

    #[test]
    fn invalid_secrets_and_references_are_rejected() {
        let store = FakeStore::new(StorageMode::Keyring, None);
        let empty = SecretString::new(String::new());
        let huge = SecretString::new("x".repeat(MAX_SECRET_CHARS + 1));
        assert_eq!(
            store_credential(&store, "acc-1", &empty),
            Err(CredentialError::InvalidSecret)
        );
        assert_eq!(
            store_credential(&store, "acc-1", &huge),
            Err(CredentialError::InvalidSecret)
        );
        assert_eq!(
            store_credential(&store, "../x", &secret()),
            Err(CredentialError::InvalidReference)
        );
        for bad in [
            "acc-1",
            "cred:",
            "cred:a b",
            "cred:../x",
            "cred:glpat-x",
            "cred:0123456789abcdef0123456789abcdef01234567",
        ] {
            assert_eq!(
                CredentialRef::parse(bad),
                Err(CredentialError::InvalidReference)
            );
        }
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn disconnect_deletes_secret_then_marks_account() -> Result<(), CredentialError> {
        let keyring = FakeStore::new(StorageMode::Keyring, None);
        let session = FakeStore::new(StorageMode::SessionOnly, None);
        let reference = store_credential(&keyring, "acc-1", &secret())?;
        let directory = FakeDirectory {
            entry: Some((reference, StorageMode::Keyring)),
            disconnected: Mutex::new(false),
        };
        disconnect_account(&keyring, &session, &directory, "acc-1")?;
        assert_eq!(keyring.len(), 0);
        assert!(directory.disconnected.lock().map(|g| *g).unwrap_or(false));
        Ok(())
    }

    #[test]
    fn failed_secret_deletion_leaves_account_connected() -> Result<(), CredentialError> {
        let keyring = FakeStore::new(StorageMode::Keyring, Some(CredentialError::KeyringLocked));
        let session = FakeStore::new(StorageMode::SessionOnly, None);
        let directory = FakeDirectory {
            entry: Some((CredentialRef::for_account("acc-1")?, StorageMode::Keyring)),
            disconnected: Mutex::new(false),
        };
        let outcome = disconnect_account(&keyring, &session, &directory, "acc-1");
        assert_eq!(outcome, Err(CredentialError::KeyringLocked));
        assert!(!directory.disconnected.lock().map(|g| *g).unwrap_or(true));
        Ok(())
    }

    #[test]
    fn errors_and_references_never_render_secrets() -> Result<(), CredentialError> {
        let reference = CredentialRef::for_account("acc-1")?;
        let rendered = [
            format!("{reference:?}"),
            format!("{:?}", secret()),
            CredentialError::Backend.to_string(),
            format!("{:?}", CredentialError::InvalidSecret),
        ];
        assert!(
            rendered.iter().all(|text| !text.contains(SENTINEL)),
            "{rendered:?}"
        );
        Ok(())
    }
}
