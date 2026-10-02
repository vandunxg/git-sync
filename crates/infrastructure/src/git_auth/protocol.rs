//! Bounded parser/formatter for Git's credential `key=value` protocol (gitcredentials(7)).

use std::fmt;

/// Upper bound for one request or response; Git's real messages are far smaller.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const MAX_LINES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    TooLarge,
    NotUtf8,
    /// Missing `=`, empty key, or a control character (NUL/CR) inside a line.
    Malformed,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "credential message too large",
            Self::NotUtf8 => "credential message is not UTF-8",
            Self::Malformed => "malformed credential message",
        })
    }
}

impl std::error::Error for ProtocolError {}

/// Ordered attributes of one message. Repeated keys keep the last value, except that
/// array keys (`key[]`) are ignored: the broker never needs them.
#[derive(Default, PartialEq, Eq)]
pub struct Attributes(Vec<(String, String)>);

impl fmt::Debug for Attributes {
    // Values can include a password in `store` input; never print them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|(k, _)| k))
            .finish()
    }
}

impl Attributes {
    pub fn parse(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::NotUtf8)?;
        let mut attributes = Self::default();
        for (index, line) in text.split('\n').enumerate() {
            if line.is_empty() {
                break;
            }
            if index >= MAX_LINES || line.contains(['\0', '\r']) {
                return Err(ProtocolError::Malformed);
            }
            let (key, value) = line.split_once('=').ok_or(ProtocolError::Malformed)?;
            if key.is_empty() {
                return Err(ProtocolError::Malformed);
            }
            if key.ends_with("[]") {
                continue;
            }
            attributes.set(key, value);
        }
        Ok(attributes)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => value.clone_into(&mut entry.1),
            None => self.0.push((key.to_owned(), value.to_owned())),
        }
    }

    /// Serialized message terminated by a blank line. Callers guarantee values contain
    /// no newline/NUL (validated when a grant is created).
    pub fn to_message(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.0 {
            out.push_str(key);
            out.push('=');
            out.push_str(value);
            out.push('\n');
        }
        out.push('\n');
        out
    }
}

/// True when `value` can be embedded in one protocol line.
pub(crate) fn is_line_safe(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\n', '\r', '\0'])
}
