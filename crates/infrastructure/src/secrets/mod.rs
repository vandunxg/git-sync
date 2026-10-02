//! Credential stores: the OS keyring (Secret Service) and an explicit, in-memory
//! session-only store. There is no file or SQLite secret store (REQ-12, R-SEC).

mod keyring;
mod session;

pub use keyring::{KEYRING_SERVICE, KeyringStore};
pub use session::SessionStore;

#[cfg(test)]
mod tests;
