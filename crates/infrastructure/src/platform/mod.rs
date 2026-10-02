//! Linux platform primitives for the credential broker: a private runtime directory,
//! the current user ID and process-ancestry checks read from `/proc`. Nothing here is
//! claimed for other platforms; callers on non-Linux targets get `Unsupported`.

use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Directory name created under the session runtime directory (e.g. `$XDG_RUNTIME_DIR`).
pub const RUNTIME_SUBDIR: &str = "git-backup-manager";

/// Sanitized platform errors; no variant carries paths of other users or secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformError {
    /// The base runtime directory is missing, relative, or not a directory.
    RuntimeDirUnavailable,
    /// The directory exists but is a symlink, owned by another user, or not mode 0700.
    RuntimeDirInsecure,
    Unsupported,
    Io,
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RuntimeDirUnavailable => "the session runtime directory is unavailable",
            Self::RuntimeDirInsecure => "the runtime directory is not private to this user",
            Self::Unsupported => "this platform is not supported by the credential broker",
            Self::Io => "a filesystem error occurred",
        })
    }
}

impl std::error::Error for PlatformError {}

/// Real user ID of this process, read from the owner of `/proc/self`.
pub fn current_uid() -> Result<u32, PlatformError> {
    fs::metadata("/proc/self")
        .map(|m| m.uid())
        .map_err(|_| PlatformError::Unsupported)
}

/// A directory that exists, is not a symlink, is owned by the current user and has
/// mode 0700. Only such a directory may hold broker sockets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateRuntimeDir(PathBuf);

impl PrivateRuntimeDir {
    /// Creates or validates `<base>/git-backup-manager`. `base` must be an absolute
    /// private directory such as `$XDG_RUNTIME_DIR`; there is no `/tmp` fallback.
    pub fn under(base: &Path) -> Result<Self, PlatformError> {
        if !base.is_absolute() {
            return Err(PlatformError::RuntimeDirUnavailable);
        }
        let base_meta =
            fs::symlink_metadata(base).map_err(|_| PlatformError::RuntimeDirUnavailable)?;
        if !base_meta.is_dir() {
            return Err(PlatformError::RuntimeDirUnavailable);
        }
        let uid = current_uid()?;
        if base_meta.uid() != uid {
            return Err(PlatformError::RuntimeDirInsecure);
        }
        let dir = base.join(RUNTIME_SUBDIR);
        match fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(PlatformError::Io),
        }
        Self::validate(dir, uid)
    }

    fn validate(dir: PathBuf, uid: u32) -> Result<Self, PlatformError> {
        let meta = fs::symlink_metadata(&dir).map_err(|_| PlatformError::Io)?;
        let mode = meta.permissions().mode() & 0o7777;
        if meta.file_type().is_symlink() || !meta.is_dir() || meta.uid() != uid || mode != 0o700 {
            return Err(PlatformError::RuntimeDirInsecure);
        }
        Ok(Self(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Parent PID of `pid` from `/proc/<pid>/stat`, or `None` when the process is gone.
pub fn parent_pid(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 2 (comm) is parenthesized and may contain spaces or ')'; parse after the last ')'.
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    let mut fields = rest.split_ascii_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

/// True while `pid` exists and is not a zombie (state `Z`) or dead (`X`).
pub fn is_running(pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let state = stat
        .rfind(')')
        .and_then(|i| stat.get(i + 1..))
        .and_then(|rest| rest.split_ascii_whitespace().next());
    matches!(state, Some(s) if s != "Z" && s != "X" && s != "x")
}

/// Maximum process-tree depth searched; Git -> shell -> helper is three levels.
const MAX_ANCESTRY_DEPTH: usize = 32;

/// True when `ancestor` is `pid` itself or one of its ancestors in the live process
/// tree. A PID that has exited (and could be reused) is not trusted: callers must bind
/// the PID of a child they still own (unreaped) for the whole broker lifetime.
pub fn is_same_or_descendant(pid: u32, ancestor: u32) -> bool {
    if ancestor <= 1 {
        return false;
    }
    let mut current = pid;
    for _ in 0..MAX_ANCESTRY_DEPTH {
        if current == ancestor {
            return true;
        }
        match parent_pid(current) {
            Some(parent) if parent > 1 && parent != current => current = parent,
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests;
