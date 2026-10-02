//! Git credential bridge (REQ-45, REQ-22, ADR-011). Tokens never enter Git argv, the
//! child environment, remote URLs or files. A per-job native broker listens on a Unix
//! socket inside a private runtime directory; a credential helper started by Git relays
//! `get` requests to it. The broker answers only when the peer is the job's own Git
//! process tree, the request is HTTPS to the job's host and repository path, and the
//! grant has not expired. `store`/`erase` are never forwarded and never persist.

mod broker;
mod command;
mod helper;
mod protocol;
mod redact;
mod remote;

pub use broker::{BrokerError, BrokerHandle, Denial, JobGrant, start_broker, sweep_stale_sockets};
pub use command::{HelperCommand, UnsafeHelperPath, git_auth_args, git_child_env};
pub use helper::{HelperError, relay};
pub use protocol::{Attributes, MAX_MESSAGE_BYTES, ProtocolError};
pub use redact::{REDACTED, redact};
pub use remote::{AllowedHost, RemoteError, RemoteTarget};

#[cfg(test)]
mod tests;
