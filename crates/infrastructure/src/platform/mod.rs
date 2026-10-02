//! Linux platform primitives for the credential broker and the Git runner: a private
//! runtime directory, the current user ID, process-ancestry checks read from `/proc`
//! and process-tree signalling (PID + start time, so reused PIDs are never signalled).
//! Nothing here is claimed for other platforms; callers on non-Linux targets get
//! `Unsupported`.

use std::collections::{HashMap, HashSet};
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

/// Start time of `pid` in clock ticks since boot (`/proc/<pid>/stat` field 22). With
/// the PID it identifies one process instance, so a reused PID is never mistaken for it.
pub fn start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    // After comm, field 3 (state) is index 0, so field 22 is index 19.
    rest.split_ascii_whitespace().nth(19)?.parse().ok()
}

/// A process instance: PID plus start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessId {
    pub pid: u32,
    pub start_time: u64,
}

impl ProcessId {
    pub fn of(pid: u32) -> Option<Self> {
        start_time(pid).map(|start_time| Self { pid, start_time })
    }

    /// True while the same instance (not a reused PID) is alive and not a zombie.
    pub fn is_alive(self) -> bool {
        start_time(self.pid) == Some(self.start_time) && is_running(self.pid)
    }
}

/// Upper bound on processes scanned in `/proc`, so a hostile fork bomb cannot make
/// the scan unbounded.
const MAX_SCANNED_PROCESSES: usize = 65_536;

/// Live descendants of `root` (excluding `root`), found by scanning `/proc` parent links
/// once and walking a parent -> children map (linear in the number of processes).
pub fn descendants(root: u32) -> Vec<ProcessId> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for entry in entries.flatten().take(MAX_SCANNED_PROCESSES) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if let Some(parent) = parent_pid(pid) {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut seen = HashSet::from([root]);
    let mut found = Vec::new();
    let mut frontier = vec![root];
    while let Some(current) = frontier.pop() {
        for &pid in children
            .get(&current)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if seen.insert(pid)
                && let Some(id) = ProcessId::of(pid)
            {
                found.push(id);
                frontier.push(pid);
            }
        }
    }
    found
}

/// Process group ID of `pid` (`/proc/<pid>/stat` field 5).
pub fn process_group(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    rest.split_ascii_whitespace().nth(2)?.parse().ok()
}

/// Live members of process group `pgid`, found by scanning `/proc`.
pub fn group_members(pgid: u32) -> Vec<ProcessId> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .take(MAX_SCANNED_PROCESSES)
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
        })
        .filter(|&pid| process_group(pid) == Some(pgid))
        .filter_map(ProcessId::of)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopSignal {
    Terminate,
    Kill,
}

impl StopSignal {
    fn raw(self) -> rustix::process::Signal {
        match self {
            Self::Terminate => rustix::process::Signal::TERM,
            Self::Kill => rustix::process::Signal::KILL,
        }
    }
}

/// Signals the process group led by `leader` (spawned with its own group). Errors such
/// as "no such group" are expected after exit and ignored.
pub fn signal_group(leader: u32, signal: StopSignal) {
    if let Some(pid) = i32::try_from(leader)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        && leader > 1
    {
        let _ = rustix::process::kill_process_group(pid, signal.raw());
    }
}

/// Signals one process instance, only if its start time still matches (no PID reuse).
pub fn signal_process(id: ProcessId, signal: StopSignal) {
    if id.pid <= 1 || start_time(id.pid) != Some(id.start_time) {
        return;
    }
    if let Some(pid) = i32::try_from(id.pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process(pid, signal.raw());
    }
}

#[cfg(test)]
mod tests;
