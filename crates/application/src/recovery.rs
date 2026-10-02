//! Pre-change recovery snapshots and the destructive-change gate (REQ-18, REQ-20;
//! RETENTION_RESTORE.md).
//!
//! Before any branch rewrite, tag move or ref deletion, the previous state of every
//! affected ref must be held in a *verified* recovery snapshot: a complete,
//! self-contained Git bundle plus a manifest with checksums, verified by restoring it
//! into a fresh repository. Missing, unverified, corrupt or incomplete recovery
//! material blocks the change; there is no "proceed anyway" path here. LFS payloads and
//! submodule contents are explicitly *not covered* by these snapshots yet.

use std::fmt;
use std::io::{self, Read};

use gbm_domain::refs::{ObjectFormat, ObjectId, RefMap, RefName};
use sha2::{Digest, Sha256};

/// Why recovery material could not be produced or trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    InvalidRequest,
    /// The snapshot id is already used (snapshots are append-only).
    Exists,
    NotFound,
    /// The storage ran out of space while writing.
    NoSpace,
    /// Checksums, bundle contents or restored refs do not match the manifest.
    Corrupt,
    Cancelled,
    Failed,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "invalid recovery snapshot request",
            Self::Exists => "the recovery snapshot already exists",
            Self::NotFound => "the recovery snapshot does not exist",
            Self::NoSpace => "not enough free space for the recovery snapshot",
            Self::Corrupt => "the recovery snapshot failed verification",
            Self::Cancelled => "the recovery snapshot was cancelled",
            Self::Failed => "the recovery snapshot could not be written",
        })
    }
}

impl std::error::Error for StoreError {}

/// What to retain: `refs` (logical names) as held in the mirror under `namespace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRequest {
    pub repository_id: String,
    pub snapshot_id: String,
    /// Mirror namespace holding the refs, e.g. `refs/gbm/snapshots/<capture>/`.
    pub namespace: String,
    pub refs: RefMap,
    pub reason: String,
    pub created_at_ms: i64,
}

/// A recovery snapshot that passed verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverySnapshot {
    pub repository_id: String,
    pub snapshot_id: String,
    pub object_format: Option<ObjectFormat>,
    pub refs: RefMap,
    /// SHA-256 of the bundle file; `None` for an explicit empty snapshot.
    pub bundle_sha256: Option<String>,
    pub bundle_bytes: u64,
}

/// Storage of recovery snapshots (bundle + manifest), implemented in infrastructure.
pub trait SnapshotStore {
    /// Writes a complete bundle and manifest for `request`, verifies it by restoring
    /// into a fresh repository, and only then makes it visible as ready.
    fn create(&self, request: &RecoveryRequest) -> Result<RecoverySnapshot, StoreError>;
    /// Re-verifies a ready snapshot (checksums, bundle heads, full restore + fsck).
    fn verify(&self, snapshot_id: &str) -> Result<RecoverySnapshot, StoreError>;
}

/// How a planned ref update relates to the previous state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Create,
    FastForward,
    Rewrite,
    TagMove,
    Delete,
}

impl ChangeKind {
    pub fn is_destructive(self) -> bool {
        matches!(self, Self::Rewrite | Self::TagMove | Self::Delete)
    }
}

/// A planned ref update. CONTRACT: `kind` must be derived by the planner from observed
/// destination state and commit ancestry (P06), never from labels supplied by a UI or
/// a remote. The gate checks only that `kind` agrees with old/new presence; it cannot
/// see ancestry, so a rewrite mislabelled `FastForward` would not be caught here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChange {
    pub name: RefName,
    pub old: Option<ObjectId>,
    pub new: Option<ObjectId>,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    /// Kind and old/new values disagree (e.g. a "delete" with a new value).
    InvalidChange { name: String },
    /// Destructive changes exist but no recovery snapshot was named.
    RecoveryMissing,
    /// The named snapshot could not be verified now.
    RecoveryUnverified(StoreError),
    /// The verified snapshot does not hold this ref at its old object ID.
    OldStateNotCovered { name: String },
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidChange { name } => write!(f, "inconsistent planned change for {name}"),
            Self::RecoveryMissing => f.write_str("a verified recovery snapshot is required"),
            Self::RecoveryUnverified(error) => write!(f, "recovery snapshot not verified: {error}"),
            Self::OldStateNotCovered { name } => {
                write!(
                    f,
                    "the recovery snapshot does not cover the old state of {name}"
                )
            }
        }
    }
}

impl std::error::Error for GateError {}

fn consistent(change: &PlannedChange) -> bool {
    match change.kind {
        ChangeKind::Create => change.old.is_none() && change.new.is_some(),
        ChangeKind::Delete => change.old.is_some() && change.new.is_none(),
        ChangeKind::FastForward | ChangeKind::Rewrite | ChangeKind::TagMove => {
            change.old.is_some() && change.new.is_some() && change.old != change.new
        }
    }
}

/// Allows a plan only if every destructive change's old state is held, at the exact old
/// object ID, by a recovery snapshot that verifies *now*. Non-destructive plans need no
/// snapshot. Returns the verified snapshot used, if any.
pub fn require_recovery(
    changes: &[PlannedChange],
    store: &dyn SnapshotStore,
    recovery_snapshot_id: Option<&str>,
) -> Result<Option<RecoverySnapshot>, GateError> {
    if let Some(bad) = changes.iter().find(|change| !consistent(change)) {
        return Err(GateError::InvalidChange {
            name: bad.name.to_string(),
        });
    }
    let destructive: Vec<&PlannedChange> = changes
        .iter()
        .filter(|change| change.kind.is_destructive())
        .collect();
    if destructive.is_empty() {
        return Ok(None);
    }
    let id = recovery_snapshot_id.ok_or(GateError::RecoveryMissing)?;
    let verified = store.verify(id).map_err(GateError::RecoveryUnverified)?;
    for change in destructive {
        if verified.refs.get(&change.name) != change.old.as_ref() {
            return Err(GateError::OldStateNotCovered {
                name: change.name.to_string(),
            });
        }
    }
    Ok(Some(verified))
}

/// Streams `reader` through SHA-256 and returns lower-case hex.
pub fn sha256_hex(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(buffer.get(..read).unwrap_or_default());
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests;
