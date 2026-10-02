//! Complete local source snapshot capture (REQ-05, REQ-07, REQ-42; SYNC_ENGINE.md
//! "Managed mirror layout" and "One job, in order" steps 2–4).
//!
//! 1. Observe the complete selected ref map. Unavailable/partial: stop, touch nothing.
//! 2. An empty repository yields an explicit empty manifest: no fetch, no bundle, no
//!    invented HEAD.
//! 3. Fetch every selected ref by explicit refspec into a fresh, job-owned staging
//!    namespace `refs/gbm/staging/<job>/...`.
//! 4. Read the staged refs back and require them to equal an observation exactly. If
//!    the source moved during the fetch, re-observe; a staged map that matches no
//!    complete observation is never promoted (bounded retries).
//! 5. Check object integrity, copy staging to `refs/gbm/snapshots/<snapshot>/...`
//!    atomically, read it back, remove the exact staging refs, and record the manifest
//!    with a ref-set digest. Previous snapshots are never touched.
//! 6. Re-observe once more: a newer source is reported as changed, not as current.

use std::fmt;

use gbm_domain::observation::{ObservationOutcome, UnavailableReason};
use gbm_domain::refs::{Advertisement, ObjectFormat, ObjectId, RefMap, RefName};
use sha2::{Digest, Sha256};

use crate::observation::{ReadError, SourceRefReader, outcome_from};

/// Capture attempts before giving up on a source that keeps changing.
pub const MAX_CAPTURE_ATTEMPTS: usize = 3;
const MAX_ID_LEN: usize = 64;

/// Sanitized mirror adapter failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorError {
    /// The existing mirror uses another object format than the source.
    ObjectFormatMismatch,
    Cancelled,
    /// Git failed, timed out or produced unusable output.
    Failed,
}

impl fmt::Display for MirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ObjectFormatMismatch => "the mirror uses another object format",
            Self::Cancelled => "the mirror operation was cancelled",
            Self::Failed => "the mirror operation failed",
        })
    }
}

impl std::error::Error for MirrorError {}

/// Git operations on one app-owned bare mirror, bound to one job's source.
pub trait MirrorGit {
    /// Creates the bare mirror if it does not exist yet; an existing one must use
    /// `format`.
    fn prepare(&self, format: ObjectFormat) -> Result<(), MirrorError>;
    /// Fetches explicit `(source ref, local ref)` pairs from the job's source (forced,
    /// no tags, no submodules).
    fn fetch_from_source(&self, pairs: &[(RefName, String)]) -> Result<(), MirrorError>;
    /// Copies `(local ref, local ref)` pairs inside the mirror.
    fn copy_refs(&self, pairs: &[(String, String)]) -> Result<(), MirrorError>;
    /// Every ref under `prefix` (ending in `/`) with its object ID.
    fn read_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, MirrorError>;
    /// Full object connectivity/integrity check of the mirror.
    fn check_integrity(&self) -> Result<(), MirrorError>;
    /// Deletes exactly these refs, each only if it still has the given object ID.
    fn delete_refs(&self, refs: &[(String, ObjectId)]) -> Result<(), MirrorError>;
    /// Removes every ref under an app-owned `prefix` (staging or a failed snapshot).
    /// Runs even after the job was cancelled, so cleanup is not skipped.
    fn remove_namespace(&self, prefix: &str) -> Result<(), MirrorError>;
}

/// Monotonic observation sequence plus start time (see `observe_source`).
pub trait ObservationClock {
    fn next(&self) -> (u64, i64);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureError {
    InvalidId,
    Cancelled,
    SourceUnavailable(UnavailableReason),
    SourcePartial,
    /// The staged refs never matched a complete observation within the budget.
    SourceKeptChanging,
    /// Staged refs had names outside the selected namespaces.
    UnexpectedStagedRef,
    IntegrityFailed,
    /// The promoted snapshot refs did not equal the captured map.
    PromotionMismatch,
    /// The snapshot namespace already holds refs; snapshots are immutable.
    SnapshotExists,
    Mirror(MirrorError),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => f.write_str("invalid job or snapshot id"),
            Self::Cancelled => f.write_str("the capture was cancelled"),
            Self::SourceUnavailable(reason) => write!(f, "source unavailable: {reason:?}"),
            Self::SourcePartial => f.write_str("source observation was incomplete"),
            Self::SourceKeptChanging => f.write_str("the source kept changing during capture"),
            Self::UnexpectedStagedRef => f.write_str("unexpected ref in staging"),
            Self::IntegrityFailed => f.write_str("mirror integrity check failed"),
            Self::PromotionMismatch => f.write_str("snapshot refs did not match the capture"),
            Self::SnapshotExists => f.write_str("the snapshot id is already used"),
            Self::Mirror(error) => write!(f, "mirror operation failed: {error:?}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<MirrorError> for CaptureError {
    fn from(error: MirrorError) -> Self {
        match error {
            MirrorError::Cancelled => Self::Cancelled,
            other => Self::Mirror(other),
        }
    }
}

/// What the source looked like when re-checked right after capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRecheck {
    Unchanged,
    /// New changes exist: the snapshot is correct for its observation, not current.
    Changed,
    Unknown,
}

/// Independent coverage dimensions (REQ-42). This task certifies Git refs only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Complete,
    NotChecked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub repository_id: String,
    pub snapshot_id: String,
    /// `None` only for an empty repository.
    pub object_format: Option<ObjectFormat>,
    pub refs: RefMap,
    /// Advertised symbolic HEAD target (metadata; may name an unborn branch).
    pub head_target: Option<String>,
    pub observed_at_ms: i64,
    /// SHA-256 over format + sorted `oid name` lines (no HEAD, no peeled entries).
    pub ref_set_digest: String,
    /// Mirror namespace holding the refs; `None` for an empty snapshot.
    pub ref_namespace: Option<String>,
    pub git_refs: Coverage,
    pub lfs: Coverage,
    pub submodules: Coverage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    pub manifest: SnapshotManifest,
    pub recheck: SourceRecheck,
    /// False when the job's staging refs could not be removed (reported, not hidden).
    pub staging_removed: bool,
}

/// IDs used inside ref names: `[A-Za-z0-9_-]`, 1..=64, not starting with `-`.
pub fn is_namespace_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

pub fn staging_prefix(job_id: &str) -> String {
    format!("refs/gbm/staging/{job_id}/")
}

pub fn snapshot_prefix(snapshot_id: &str) -> String {
    format!("refs/gbm/snapshots/{snapshot_id}/")
}

/// `refs/heads/x` under `prefix` -> `<prefix>heads/x`.
fn under(prefix: &str, name: &RefName) -> String {
    let rest = name.as_str().strip_prefix("refs/").unwrap_or(name.as_str());
    format!("{prefix}{rest}")
}

/// Maps refs read under `prefix` back to logical names; anything not a selected
/// head/tag fails closed.
fn logical_map(prefix: &str, refs: &[(String, ObjectId)]) -> Result<RefMap, CaptureError> {
    let mut entries = Vec::with_capacity(refs.len());
    for (name, oid) in refs {
        let logical = name
            .strip_prefix(prefix)
            .and_then(|rest| RefName::parse(&format!("refs/{rest}")))
            .ok_or(CaptureError::UnexpectedStagedRef)?;
        entries.push((logical, oid.clone()));
    }
    RefMap::from_entries(entries).map_err(|_| CaptureError::UnexpectedStagedRef)
}

pub fn ref_set_digest(refs: &RefMap) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gbm-refset-v1\n");
    hasher.update(match refs.format() {
        Some(ObjectFormat::Sha1) => b"sha1\n".as_slice(),
        Some(ObjectFormat::Sha256) => b"sha256\n".as_slice(),
        None => b"empty\n".as_slice(),
    });
    for (name, oid) in refs.iter() {
        hasher.update(oid.as_str().as_bytes());
        hasher.update(b" ");
        hasher.update(name.as_str().as_bytes());
        hasher.update(b"\n");
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn observe(
    reader: &dyn SourceRefReader,
    clock: &dyn ObservationClock,
    repository_id: &str,
) -> Result<(Advertisement, i64), CaptureError> {
    // The sequence orders observations for a `SourceView`; capture only needs the time.
    let (_sequence, at) = clock.next();
    let result = reader.read_advertisement(repository_id);
    if result == Err(ReadError::Cancelled) {
        return Err(CaptureError::Cancelled);
    }
    match outcome_from(result) {
        ObservationOutcome::Complete(advertisement) => Ok((advertisement, at)),
        ObservationOutcome::Partial => Err(CaptureError::SourcePartial),
        ObservationOutcome::Unavailable(reason) => Err(CaptureError::SourceUnavailable(reason)),
    }
}

pub struct CaptureRequest<'a> {
    pub repository_id: &'a str,
    pub job_id: &'a str,
    pub snapshot_id: &'a str,
}

pub fn capture_snapshot(
    reader: &dyn SourceRefReader,
    mirror: &dyn MirrorGit,
    clock: &dyn ObservationClock,
    request: &CaptureRequest<'_>,
) -> Result<CaptureOutcome, CaptureError> {
    if !is_namespace_id(request.job_id) || !is_namespace_id(request.snapshot_id) {
        return Err(CaptureError::InvalidId);
    }
    let staging = staging_prefix(request.job_id);
    let result = capture_inner(reader, mirror, clock, request, &staging);
    // Job-owned staging refs are removed on every path (success, failure, cancel).
    let staging_removed = mirror.remove_namespace(&staging).is_ok();
    result.map(|mut outcome| {
        outcome.staging_removed = staging_removed;
        outcome
    })
}

fn capture_inner(
    reader: &dyn SourceRefReader,
    mirror: &dyn MirrorGit,
    clock: &dyn ObservationClock,
    request: &CaptureRequest<'_>,
    staging: &str,
) -> Result<CaptureOutcome, CaptureError> {
    let mut captured: Option<(Advertisement, i64)> = None;
    let mut last_fetch_error: Option<MirrorError> = None;
    for _ in 0..MAX_CAPTURE_ATTEMPTS {
        let (advertisement, at) = observe(reader, clock, request.repository_id)?;
        let Some(format) = advertisement.refs.format() else {
            // Empty repository: explicit empty manifest, nothing fetched.
            return Ok(finish(reader, clock, request, advertisement, at, None));
        };
        mirror.prepare(format)?;
        // Drop staging refs of an earlier attempt so deleted source refs cannot linger.
        let stale = mirror.read_refs(staging)?;
        mirror.delete_refs(&stale)?;
        let pairs: Vec<(RefName, String)> = advertisement
            .refs
            .iter()
            .map(|(name, _)| (name.clone(), under(staging, name)))
            .collect();
        // A ref deleted between observation and fetch makes Git fail: re-observe.
        match mirror.fetch_from_source(&pairs) {
            Ok(()) => {}
            Err(MirrorError::Cancelled) => return Err(CaptureError::Cancelled),
            Err(error) => {
                last_fetch_error = Some(error);
                continue;
            }
        }
        last_fetch_error = None;
        let staged = logical_map(staging, &mirror.read_refs(staging)?)?;
        if staged == advertisement.refs {
            captured = Some((advertisement, at));
            break;
        }
        // The source moved during the fetch (or Git reported success wrongly): accept
        // the staged map only if a new complete observation equals it.
        let (again, again_at) = observe(reader, clock, request.repository_id)?;
        if staged == again.refs {
            captured = Some((again, again_at));
            break;
        }
    }
    let Some((advertisement, at)) = captured else {
        return Err(match last_fetch_error {
            Some(error) => CaptureError::from(error),
            None => CaptureError::SourceKeptChanging,
        });
    };
    mirror
        .check_integrity()
        .map_err(|_| CaptureError::IntegrityFailed)?;
    let target = snapshot_prefix(request.snapshot_id);
    if !mirror.read_refs(&target)?.is_empty() {
        return Err(CaptureError::SnapshotExists);
    }
    let pairs: Vec<(String, String)> = advertisement
        .refs
        .iter()
        .map(|(name, _)| (under(staging, name), under(&target, name)))
        .collect();
    // From here on the namespace is ours: any failure removes the partial snapshot.
    let promoted = mirror
        .copy_refs(&pairs)
        .map_err(CaptureError::from)
        .and_then(|()| mirror.read_refs(&target).map_err(CaptureError::from))
        .and_then(|refs| logical_map(&target, &refs))
        .and_then(|map| {
            if map == advertisement.refs {
                Ok(())
            } else {
                Err(CaptureError::PromotionMismatch)
            }
        });
    if let Err(error) = promoted {
        let _ = mirror.remove_namespace(&target);
        return Err(error);
    }
    Ok(finish(
        reader,
        clock,
        request,
        advertisement,
        at,
        Some(target),
    ))
}

fn finish(
    reader: &dyn SourceRefReader,
    clock: &dyn ObservationClock,
    request: &CaptureRequest<'_>,
    advertisement: Advertisement,
    observed_at_ms: i64,
    ref_namespace: Option<String>,
) -> CaptureOutcome {
    let recheck = match observe(reader, clock, request.repository_id) {
        Ok((now, _)) if now.refs == advertisement.refs => SourceRecheck::Unchanged,
        Ok(_) => SourceRecheck::Changed,
        Err(_) => SourceRecheck::Unknown,
    };
    let manifest = SnapshotManifest {
        repository_id: request.repository_id.to_owned(),
        snapshot_id: request.snapshot_id.to_owned(),
        object_format: advertisement.refs.format(),
        ref_set_digest: ref_set_digest(&advertisement.refs),
        refs: advertisement.refs,
        head_target: advertisement.head_target,
        observed_at_ms,
        ref_namespace,
        git_refs: Coverage::Complete,
        lfs: Coverage::NotChecked,
        submodules: Coverage::NotChecked,
    };
    CaptureOutcome {
        manifest,
        recheck,
        staging_removed: true,
    }
}

#[cfg(test)]
mod tests;
