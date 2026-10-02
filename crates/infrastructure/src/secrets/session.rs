//! Session-only credentials: process memory only, gone on restart (explicit user choice).

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use gbm_application::credentials::{CredentialError, CredentialRef, CredentialStore, StorageMode};
use gbm_domain::contract::secret::SecretString;

/// In-memory store. Nothing is written to disk; `Debug` shows only the entry count.
#[derive(Default)]
pub struct SessionStore {
    items: Mutex<HashMap<String, SecretString>>,
}

impl fmt::Debug for SessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.items.lock().map(|m| m.len()).unwrap_or(0);
        write!(f, "SessionStore({count} entries)")
    }
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for SessionStore {
    fn mode(&self) -> StorageMode {
        StorageMode::SessionOnly
    }

    fn put(&self, reference: &CredentialRef, secret: &SecretString) -> Result<(), CredentialError> {
        let mut items = self.items.lock().map_err(|_| CredentialError::Backend)?;
        items.insert(
            reference.as_str().to_owned(),
            SecretString::new(secret.expose().to_owned()),
        );
        Ok(())
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretString, CredentialError> {
        let items = self.items.lock().map_err(|_| CredentialError::Backend)?;
        items
            .get(reference.as_str())
            .map(|secret| SecretString::new(secret.expose().to_owned()))
            .ok_or(CredentialError::NotFound)
    }

    fn delete(&self, reference: &CredentialRef) -> Result<(), CredentialError> {
        let mut items = self.items.lock().map_err(|_| CredentialError::Backend)?;
        items.remove(reference.as_str());
        Ok(())
    }
}
