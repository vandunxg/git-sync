//! Credential-helper side: relays one Git `get` request to the job broker.
//!
//! Git runs the helper as `<helper> --socket <path> <operation>` (gitcredentials(7)).
//! `store` and `erase` are acknowledged without contacting the broker, so the password
//! Git passes on `store` is never forwarded, written or kept.

use std::fmt;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use super::protocol::{Attributes, MAX_MESSAGE_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperError {
    UnknownOperation,
    InvalidInput,
    BrokerUnavailable,
    InvalidResponse,
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownOperation => "unknown credential operation",
            Self::InvalidInput => "invalid credential request",
            Self::BrokerUnavailable => "the credential broker is not available",
            Self::InvalidResponse => "invalid broker response",
        })
    }
}

impl std::error::Error for HelperError {}

const IO_TIMEOUT: Duration = Duration::from_secs(10);

fn read_bounded(mut input: impl Read) -> Result<Vec<u8>, HelperError> {
    let mut buffer = Vec::with_capacity(512);
    input
        .by_ref()
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_to_end(&mut buffer)
        .map_err(|_| HelperError::InvalidInput)?;
    if buffer.len() > MAX_MESSAGE_BYTES {
        Err(HelperError::InvalidInput)
    } else {
        Ok(buffer)
    }
}

/// Blocking relay used by the helper executable. Writes nothing on denial, so Git
/// treats the credential as unavailable (and, with prompts disabled, fails).
pub fn relay(
    socket: &Path,
    operation: &str,
    input: impl Read,
    mut output: impl Write,
) -> Result<(), HelperError> {
    match operation {
        "get" => {}
        "store" | "erase" => return Ok(()),
        _ => return Err(HelperError::UnknownOperation),
    }
    let mut request =
        Attributes::parse(&read_bounded(input)?).map_err(|_| HelperError::InvalidInput)?;
    request.set("operation", "get");

    let mut stream = UnixStream::connect(socket).map_err(|_| HelperError::BrokerUnavailable)?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|_| HelperError::BrokerUnavailable)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|_| HelperError::BrokerUnavailable)?;
    stream
        .write_all(request.to_message().as_bytes())
        .map_err(|_| HelperError::BrokerUnavailable)?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|_| HelperError::BrokerUnavailable)?;

    let raw = read_bounded(&mut stream).map_err(|_| HelperError::BrokerUnavailable)?;
    let response = Attributes::parse(&raw).map_err(|_| HelperError::InvalidResponse)?;
    if let (Some(username), Some(password)) = (response.get("username"), response.get("password")) {
        let mut answer = Attributes::default();
        answer.set("username", username);
        answer.set("password", password);
        output
            .write_all(answer.to_message().as_bytes())
            .map_err(|_| HelperError::InvalidResponse)?;
    }
    Ok(())
}
