//! Secret Service store. Blocking calls: run on a bounded blocking worker.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use gbm_application::credentials::{CredentialError, CredentialRef, CredentialStore, StorageMode};
use gbm_domain::contract::secret::SecretString;
use keyring_core::api::CredentialStoreApi;
use zbus_secret_service_keyring_store::Store;

/// Secret Service `service` attribute for every item this application owns.
pub const KEYRING_SERVICE: &str = "git-backup-manager";

/// Maps keyring errors without ever formatting them: `BadEncoding`/`BadDataFormat`
/// carry the raw secret bytes and platform errors may carry backend text.
///
/// The Locked/Unavailable split is best-effort guidance for the UI, not a diagnosis:
/// `NoStorageAccess` also covers a dismissed unlock prompt or a missing default
/// collection, and the store reports some item-creation lock/prompt failures as
/// `PlatformFailure`. `Ambiguous` (several items match service+user, e.g. created by
/// another tool) fails closed as `Backend` for get, put and delete.
pub(crate) fn map_error(error: keyring_core::Error) -> CredentialError {
    use keyring_core::Error as E;
    match error {
        E::NoEntry => CredentialError::NotFound,
        E::NoStorageAccess(_) => CredentialError::KeyringLocked,
        E::PlatformFailure(_) | E::NoDefaultStore => CredentialError::KeyringUnavailable,
        E::TooLong(..) => CredentialError::InvalidSecret,
        _ => CredentialError::Backend,
    }
}

/// The OS keyring via the Secret Service D-Bus API (gnome-keyring, KWallet bridge, ...).
pub struct KeyringStore {
    store: Arc<Store>,
}

impl fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyringStore(secret-service)")
    }
}

impl KeyringStore {
    /// Connects to the session bus Secret Service. Unavailability is an explicit error
    /// for the UI to explain; callers must not fall back to any other storage.
    pub fn connect() -> Result<Self, CredentialError> {
        Store::new().map(|store| Self { store }).map_err(map_error)
    }

    fn entry(&self, reference: &CredentialRef) -> Result<keyring_core::Entry, CredentialError> {
        let modifiers: HashMap<&str, &str> =
            HashMap::from([("label", "Git Backup Manager credential")]);
        self.store
            .build(KEYRING_SERVICE, reference.as_str(), Some(&modifiers))
            .map_err(map_error)
    }
}

impl CredentialStore for KeyringStore {
    fn mode(&self) -> StorageMode {
        StorageMode::Keyring
    }

    fn put(&self, reference: &CredentialRef, secret: &SecretString) -> Result<(), CredentialError> {
        self.entry(reference)?
            .set_password(secret.expose())
            .map_err(map_error)
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretString, CredentialError> {
        self.entry(reference)?
            .get_password()
            .map(SecretString::new)
            .map_err(map_error)
    }

    fn delete(&self, reference: &CredentialRef) -> Result<(), CredentialError> {
        match self.entry(reference)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(other) => Err(map_error(other)),
        }
    }
}
