//! Git source observation adapter pieces (REQ-06, REQ-07).
//!
//! `runner` is the only way the app executes Git (bounded output, timeouts, process-tree
//! cleanup, cleared environment, hardening config, redaction). `ls_remote_args` is the
//! read-only observation argv. Parsing and comparison are in `gbm_domain::refs`.

pub mod runner;

use crate::git_auth::RemoteTarget;

/// `git ls-remote --symref -- <url>`: lists every advertised ref plus the symbolic
/// HEAD target. It cannot write to the remote. `--` ends option parsing, so the URL
/// can never be read as an option.
pub fn ls_remote_args(remote: &RemoteTarget) -> Vec<String> {
    args_for(&remote.url())
}

fn args_for(location: &str) -> Vec<String> {
    ["ls-remote", "--symref", "--", location]
        .map(str::to_owned)
        .to_vec()
}

#[cfg(test)]
mod tests;
