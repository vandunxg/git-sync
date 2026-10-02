//! App-owned storage paths under a verified backup root (REQ-05, REQ-40;
//! WORKSPACE_AND_FILESYSTEM.md).
//!
//! Only a root whose `.gbm/root-manifest.json` names the expected root ID, and is a
//! private regular file of the current user, can be mutated. Paths are walked one
//! component at a time without following symlinks; a missing component is created
//! with mode 0700, an existing one must be a real directory of the current user.
//! A new leaf is created with create-new semantics, so an existing folder, even one
//! with the exact expected name, is never adopted. A case-variant sibling is refused
//! so a case-insensitive filesystem cannot merge two paths.
//!
//! Residual risk: std has no `openat`, so a process of the *same user* racing the walk
//! could swap a component for a symlink between checks. The final canonical path is
//! re-checked against the root, which detects (but cannot prevent) such a swap; in
//! that case an empty 0700 directory may already have been created at the symlink
//! target. The returned `PathBuf` is not a long-lived proof of containment: callers
//! re-resolve through `open_dir` before each mutating operation.

use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use gbm_domain::identity::RelativePath;

use crate::platform::current_uid;
use crate::storage::{APP_DIR, MANIFEST_FILE};

/// Upper bound for a manifest read; the real file is under 200 bytes.
const MAX_MANIFEST_BYTES: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// The root is missing, not canonical, or its manifest is absent, foreign or unsafe.
    NotOwnedRoot,
    /// A path component is a symlink or the result resolves outside the root.
    Escape,
    /// The target exists but was not created for this mapping, or is not ours.
    ForeignPath,
    /// A sibling differs only by letter case.
    CaseCollision,
    /// An already-mapped folder no longer exists (show missing/review; never recreate
    /// silently).
    Missing,
    NotADirectory,
    NotWritable,
    Io,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotOwnedRoot => "the backup folder is not owned by this app",
            Self::Escape => "the path leaves the backup folder",
            Self::ForeignPath => "the folder already exists and is not managed by this app",
            Self::CaseCollision => "a folder with the same name in different case exists",
            Self::Missing => "the mapped folder is missing",
            Self::NotADirectory => "the path is not a folder",
            Self::NotWritable => "the backup folder is not writable",
            Self::Io => "the backup folder could not be accessed",
        })
    }
}

impl std::error::Error for FsError {}

/// A backup root verified against its manifest. Holds no open handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedRoot {
    path: PathBuf,
    uid: u32,
}

impl OwnedRoot {
    /// Verifies `root` is canonical, ours, and carries a manifest for `root_id`.
    pub fn verify(root: &Path, root_id: &str) -> Result<Self, FsError> {
        if !root.is_absolute() {
            return Err(FsError::NotOwnedRoot);
        }
        let canonical = fs::canonicalize(root).map_err(|_| FsError::NotOwnedRoot)?;
        if canonical != root {
            return Err(FsError::NotOwnedRoot);
        }
        let uid = current_uid().map_err(|_| FsError::Io)?;
        let owned_dir =
            |path: &Path| fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && m.uid() == uid);
        let app_dir = root.join(APP_DIR);
        let private = fs::symlink_metadata(&app_dir).is_ok_and(|m| m.mode() & 0o077 == 0);
        // A world-writable root lets any local user plant entries next to ours.
        let not_world_writable = fs::symlink_metadata(root).is_ok_and(|m| m.mode() & 0o002 == 0);
        if !owned_dir(root) || !owned_dir(&app_dir) || !private || !not_world_writable {
            return Err(FsError::NotOwnedRoot);
        }
        let manifest = read_manifest(&app_dir.join(MANIFEST_FILE), uid)?;
        let matches = manifest.get("app").and_then(|v| v.as_str()) == Some("git-backup-manager")
            && manifest.get("rootId").and_then(|v| v.as_str()) == Some(root_id);
        if !matches {
            return Err(FsError::NotOwnedRoot);
        }
        Ok(Self {
            path: canonical,
            uid,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Creates `rel` (and missing parents, 0700). The leaf must not exist yet.
    pub fn create_dir(&self, rel: &RelativePath) -> Result<PathBuf, FsError> {
        self.walk(rel, true)
    }

    /// Resolves an already-mapped `rel`: every component must exist as a real
    /// directory of the current user. Nothing is created.
    pub fn open_dir(&self, rel: &RelativePath) -> Result<PathBuf, FsError> {
        self.walk(rel, false)
    }

    fn walk(&self, rel: &RelativePath, create_leaf: bool) -> Result<PathBuf, FsError> {
        let components = rel.components();
        let last = components.len().saturating_sub(1);
        let mut current = self.path.clone();
        for (index, component) in components.iter().enumerate() {
            let next = current.join(component);
            let is_leaf = index == last;
            match fs::symlink_metadata(&next) {
                Ok(meta) => {
                    if meta.file_type().is_symlink() {
                        return Err(FsError::Escape);
                    }
                    if !meta.is_dir() {
                        return Err(FsError::NotADirectory);
                    }
                    if meta.uid() != self.uid || (is_leaf && create_leaf) {
                        return Err(FsError::ForeignPath);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if !create_leaf {
                        return Err(FsError::Missing);
                    }
                    if has_case_variant(&current, component)? {
                        return Err(FsError::CaseCollision);
                    }
                    make_private_dir(&next)?;
                }
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    return Err(FsError::NotWritable);
                }
                Err(_) => return Err(FsError::Io),
            }
            current = next;
        }
        let resolved = fs::canonicalize(&current).map_err(|_| FsError::Io)?;
        if resolved != current || !resolved.starts_with(&self.path) {
            return Err(FsError::Escape);
        }
        Ok(resolved)
    }
}

fn read_manifest(path: &Path, uid: u32) -> Result<serde_json::Value, FsError> {
    let meta = fs::symlink_metadata(path).map_err(|_| FsError::NotOwnedRoot)?;
    if !meta.is_file()
        || meta.uid() != uid
        || meta.mode() & 0o022 != 0
        || meta.len() > MAX_MANIFEST_BYTES
    {
        return Err(FsError::NotOwnedRoot);
    }
    let mut text = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_MANIFEST_BYTES).read_to_string(&mut text))
        .map_err(|_| FsError::NotOwnedRoot)?;
    serde_json::from_str(&text).map_err(|_| FsError::NotOwnedRoot)
}

/// True when `dir` holds an entry whose name equals `name` ignoring ASCII case but
/// not exactly (an exact match was already handled by the caller).
fn has_case_variant(dir: &Path, name: &str) -> Result<bool, FsError> {
    let entries = fs::read_dir(dir).map_err(|error| match error.kind() {
        io::ErrorKind::PermissionDenied => FsError::NotWritable,
        _ => FsError::Io,
    })?;
    for entry in entries {
        let entry = entry.map_err(|_| FsError::Io)?;
        let other = entry.file_name();
        if other
            .to_str()
            .is_some_and(|other| other != name && other.eq_ignore_ascii_case(name))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn make_private_dir(path: &Path) -> Result<(), FsError> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|error| match error.kind() {
            // Appeared between the check and the create: never adopt it.
            io::ErrorKind::AlreadyExists => FsError::ForeignPath,
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
                FsError::NotWritable
            }
            _ => FsError::Io,
        })
}

#[cfg(test)]
mod tests;
