//! Git source observation adapter pieces (REQ-06, REQ-07).
//!
//! Only the read-only `ls-remote` argument vector lives here. Executing it (bounded
//! output, timeouts, cleared environment, credential broker) is the secure process
//! runner's job (P05-T01); no ad-hoc transport is added before that review. Parsing and
//! comparison are in `gbm_domain::refs`.

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
