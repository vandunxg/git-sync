//! Secret input wrapper for write-only DTO fields (e.g. `CredentialRequest.token`).

use std::fmt;

use serde::{Deserialize, Deserializer};

/// Holds a secret received from the renderer. It deliberately implements neither
/// `Serialize` nor `Clone`, and its `Debug` output is redacted, so it cannot be echoed
/// back over IPC, logged verbatim or copied implicitly (R-RUST, R-SEC).
pub struct SecretString(String);

impl SecretString {
    /// Exposes the secret for the single native call that needs it (keyring / provider).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(SecretString)
    }
}

#[cfg(test)]
mod tests {
    use super::SecretString;

    #[test]
    fn debug_output_is_redacted() -> Result<(), serde_json::Error> {
        let secret: SecretString = serde_json::from_str("\"glpat-example\"")?;
        assert_eq!(format!("{secret:?}"), "SecretString(<redacted>)");
        assert_eq!(secret.expose(), "glpat-example");
        Ok(())
    }
}
