//! Git argv/env for authenticated operations. Contains the helper location and socket
//! path only; never a token, never a credential-bearing URL.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

/// The helper program Git runs, e.g. the app binary with a helper subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperCommand {
    /// Absolute path of shell-inert characters: Git then runs the verbatim string
    /// (no `git credential-` lookup). Arguments and the socket path are quoted.
    pub program: PathBuf,
    pub args: Vec<String>,
    pub socket: PathBuf,
}

/// The value is executed by Git through the shell, so every word is single-quoted and
/// words that cannot be quoted safely are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsafeHelperPath;

impl fmt::Display for UnsafeHelperPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the credential helper path cannot be passed to Git safely")
    }
}

impl std::error::Error for UnsafeHelperPath {}

fn quote(word: &str) -> Result<String, UnsafeHelperPath> {
    if word.is_empty() || word.contains(['\'', '\n', '\r', '\0']) {
        return Err(UnsafeHelperPath);
    }
    Ok(format!("'{word}'"))
}

fn path_word(path: &Path) -> Result<String, UnsafeHelperPath> {
    if !path.is_absolute() {
        return Err(UnsafeHelperPath);
    }
    quote(path.to_str().ok_or(UnsafeHelperPath)?)
}

/// Git treats the helper as a program path only when the string itself starts with
/// `/`, so the program word cannot be quoted: it must consist of shell-inert bytes.
fn program_word(path: &Path) -> Result<String, UnsafeHelperPath> {
    let text = path.to_str().ok_or(UnsafeHelperPath)?;
    let inert = text
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b'+'));
    if !path.is_absolute() || !inert || text.split('/').any(|s| s == "..") {
        return Err(UnsafeHelperPath);
    }
    Ok(text.to_owned())
}

/// `-c` options for every authenticated Git invocation:
/// - drop every inherited credential helper (an empty value resets the list, blocking
///   e.g. a global `store` helper), install the job helper, send the repository path
///   to it and forbid interactive prompts;
/// - because any program in Git's process tree could ask the broker, disable code that
///   repository config could make Git run: hooks, fsmonitor and askpass, and allow only
///   the HTTPS transport (no `ext::`, `file`, `ssh` or `git` protocols), with TLS
///   verification on, no proxy, no extra headers and no redirects.
///
/// Filter/textconv drivers are not listed here: the managed mirror is bare and these
/// operations never check out files. Workspace updates (P05-T04) need their own review.
pub fn git_auth_args(helper: &HelperCommand) -> Result<Vec<String>, UnsafeHelperPath> {
    let mut words = vec![program_word(&helper.program)?];
    for arg in &helper.args {
        words.push(quote(arg)?);
    }
    words.push(quote("--socket")?);
    words.push(path_word(&helper.socket)?);
    let options = [
        "credential.helper=".to_owned(),
        format!("credential.helper={}", words.join(" ")),
        "credential.useHttpPath=true".to_owned(),
        "credential.interactive=false".to_owned(),
        "core.hooksPath=/dev/null".to_owned(),
        "core.fsmonitor=false".to_owned(),
        "core.askPass=".to_owned(),
        "protocol.allow=never".to_owned(),
        "protocol.https.allow=always".to_owned(),
        // Transport settings a repository config could otherwise weaken.
        "http.sslVerify=true".to_owned(),
        "http.proxy=".to_owned(),
        "http.extraHeader=".to_owned(),
        "http.followRedirects=false".to_owned(),
    ];
    Ok(options
        .into_iter()
        .flat_map(|option| ["-c".to_owned(), option])
        .collect())
}

/// Complete environment for a Git child (start from `env_clear()`): no inherited
/// variables, no system/global config, no terminal or askpass prompts.
pub fn git_child_env(path_var: &OsStr, home: &Path) -> Vec<(OsString, OsString)> {
    vec![
        ("PATH".into(), path_var.to_owned()),
        ("HOME".into(), home.as_os_str().to_owned()),
        ("LC_ALL".into(), "C".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
    ]
}
