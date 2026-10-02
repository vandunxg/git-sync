//! Backup-root validation and ownership manifest (WORKSPACE_AND_FILESYSTEM.md).
//!
//! A candidate must be an existing absolute directory owned by the current user, not
//! the filesystem root or a system directory, not inside or equal to a Git work tree,
//! and not already claimed by another manifest. Committing creates `<root>/.gbm/`
//! (mode 0700) and writes `root-manifest.json` with create-new semantics.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::platform::current_uid;

pub const APP_DIR: &str = ".gbm";
pub const MANIFEST_FILE: &str = "root-manifest.json";

/// Top-level system trees a backup root may not live in.
const SYSTEM_PREFIXES: [&str; 12] = [
    "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/run", "/sbin", "/sys", "/usr",
    "/var",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootError {
    NotAbsolute,
    NotADirectory,
    NotOwnedByUser,
    SystemLocation,
    InsideGitRepository,
    AlreadyClaimed,
    NotWritable,
    Io,
}

impl fmt::Display for RootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAbsolute => "choose an absolute folder path",
            Self::NotADirectory => "the selected path is not an existing folder",
            Self::NotOwnedByUser => "the folder is not owned by your user account",
            Self::SystemLocation => "system folders cannot hold backups",
            Self::InsideGitRepository => "the folder is inside a Git repository",
            Self::AlreadyClaimed => "the folder already contains a backup manifest",
            Self::NotWritable => "the folder is not writable",
            Self::Io => "the folder could not be checked",
        })
    }
}

impl std::error::Error for RootError {}

/// A validated, canonical candidate (no symlinks in the stored path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCandidate(PathBuf);

impl RootCandidate {
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Display form with the home directory abbreviated to `~`.
    pub fn display_path(&self, home: Option<&Path>) -> String {
        match home.and_then(|h| self.0.strip_prefix(h).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
            Some(rest) => format!("~/{}", rest.display()),
            None => self.0.display().to_string(),
        }
    }
}

pub fn validate_candidate(path: &Path) -> Result<RootCandidate, RootError> {
    if !path.is_absolute() {
        return Err(RootError::NotAbsolute);
    }
    let canonical = fs::canonicalize(path).map_err(|_| RootError::NotADirectory)?;
    let meta = fs::metadata(&canonical).map_err(|_| RootError::NotADirectory)?;
    if !meta.is_dir() {
        return Err(RootError::NotADirectory);
    }
    if canonical == Path::new("/")
        || SYSTEM_PREFIXES
            .iter()
            .any(|prefix| canonical.starts_with(prefix))
    {
        return Err(RootError::SystemLocation);
    }
    let uid = current_uid().map_err(|_| RootError::Io)?;
    if meta.uid() != uid {
        return Err(RootError::NotOwnedByUser);
    }
    if canonical.ancestors().any(|dir| dir.join(".git").exists()) {
        return Err(RootError::InsideGitRepository);
    }
    if canonical.join(APP_DIR).join(MANIFEST_FILE).exists() {
        return Err(RootError::AlreadyClaimed);
    }
    Ok(RootCandidate(canonical))
}

/// Result of `establish`: the root plus what this call created, so a failed database
/// write can be rolled back without touching anything that existed before.
#[derive(Debug)]
pub struct Established {
    root: RootCandidate,
    created_dir: bool,
}

impl Established {
    pub fn root(&self) -> &RootCandidate {
        &self.root
    }

    /// Removes the manifest written by this call, and `.gbm/` if this call created it
    /// (`remove_dir` only succeeds when it is empty again).
    pub fn undo(self) {
        let app_dir = self.root.path().join(APP_DIR);
        let _ = fs::remove_file(app_dir.join(MANIFEST_FILE));
        if self.created_dir {
            let _ = fs::remove_dir(&app_dir);
        }
    }
}

/// Creates `.gbm/` and the manifest naming `root_id`. Re-validates first, so a folder
/// changed since selection is refused. Never overwrites an existing manifest.
pub fn establish(
    candidate: &RootCandidate,
    root_id: &str,
    created_at: &str,
) -> Result<Established, RootError> {
    let checked = validate_candidate(candidate.path())?;
    if checked != *candidate {
        return Err(RootError::NotADirectory);
    }
    let app_dir = checked.path().join(APP_DIR);
    let created_dir = match fs::DirBuilder::new().mode(0o700).create(&app_dir) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // An existing `.gbm/` must be a private directory of this user.
            let meta = fs::symlink_metadata(&app_dir).map_err(|_| RootError::Io)?;
            let uid = current_uid().map_err(|_| RootError::Io)?;
            if !meta.is_dir()
                || meta.file_type().is_symlink()
                || meta.uid() != uid
                || meta.mode() & 0o077 != 0
            {
                return Err(RootError::AlreadyClaimed);
            }
            false
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(RootError::NotWritable);
        }
        Err(_) => return Err(RootError::Io),
    };
    let manifest = serde_json::json!({
        "app": "git-backup-manager",
        "manifestVersion": 1,
        "rootId": root_id,
        "createdAt": created_at,
    });
    let opened = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(app_dir.join(MANIFEST_FILE));
    let established = Established {
        root: checked,
        created_dir,
    };
    let mut file = match opened {
        Ok(file) => file,
        Err(error) => {
            if established.created_dir {
                let _ = fs::remove_dir(&app_dir);
            }
            return Err(match error.kind() {
                io::ErrorKind::AlreadyExists => RootError::AlreadyClaimed,
                io::ErrorKind::PermissionDenied => RootError::NotWritable,
                _ => RootError::Io,
            });
        }
    };
    if file
        .write_all(manifest.to_string().as_bytes())
        .and_then(|()| file.sync_all())
        .is_err()
    {
        established.undo();
        return Err(RootError::Io);
    }
    Ok(established)
}

#[cfg(test)]
mod tests;
