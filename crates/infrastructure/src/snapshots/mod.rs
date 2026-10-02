//! Recovery snapshot store: complete Git bundle + manifest under
//! `<root>/.gbm/snapshots/gh-<id>/<snapshot-id>/` (P05-T03; RETENTION_RESTORE.md).
//!
//! Create: write into `<snapshot-id>.staging/` (owned, 0700), `git bundle create` from the
//! mirror namespace, check the bundle heads, restore it into a fresh bare repository
//! inside staging (`init` + `fetch` from the bundle file only, `fsck`, exact ref
//! comparison), hash it, write the manifest (0600, fsync), then rename staging to the
//! final name. Any failure removes exactly that staging directory and returns an error;
//! nothing partial becomes visible. Verify repeats the checksum, head and restore
//! checks. Restoring never contacts a source: the only Git location used is the bundle.
//! LFS payloads and submodule contents are recorded as not covered.
//!
//! Invariants and limits: one writer per repository creates snapshots (the job queue
//! serializes jobs per repository); verification uses a unique temp dir per call.
//! Manifests and checksums detect corruption, not a local attacker who can rewrite the
//! backup root (they are not signed). A failed `restore` may leave a partial bare
//! repository in the caller's fresh destination directory.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gbm_application::recovery::{
    RecoveryRequest, RecoverySnapshot, SnapshotStore, StoreError, sha256_hex,
};
use gbm_application::snapshot::is_namespace_id;
use gbm_domain::identity::{GitHubRepoId, RelativePath, snapshots_relative_dir};
use gbm_domain::refs::{ObjectFormat, ObjectId, RefMap, RefName};
use serde_json::{Value, json};

use crate::filesystem::{FsError, OwnedRoot};
use crate::git::runner::{
    Arg, BundleFile, CancelToken, GitInvocation, GitOutput, GitRunner, Location, RefPrefix,
    RefSpec, RunError, RunLimits, Subcommand, Termination, WorkDir,
};

const MANIFEST_FILE: &str = "manifest.json";
const BUNDLE_FILE: &str = "repo.bundle";
const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
const MAX_REASON_CHARS: usize = 200;
const REFSPECS_PER_FETCH: usize = 200;
const REF_FORMAT: &str = "--format=%(objectname)%09%(objecttype)%09%(refname)";
const LIMITATIONS: [&str; 2] = [
    "Git LFS payloads are not included in this snapshot.",
    "Submodule contents are not included; only gitlink commit IDs are.",
];

fn format_word(format: ObjectFormat) -> &'static str {
    match format {
        ObjectFormat::Sha1 => "sha1",
        ObjectFormat::Sha256 => "sha256",
    }
}

fn parse_format(word: &str) -> Option<ObjectFormat> {
    match word {
        "sha1" => Some(ObjectFormat::Sha1),
        "sha256" => Some(ObjectFormat::Sha256),
        _ => None,
    }
}

/// Classifies a failed write: "No space left on device" is reported as such.
fn write_failure(stderr: &str) -> StoreError {
    if stderr
        .to_ascii_lowercase()
        .contains("no space left on device")
    {
        StoreError::NoSpace
    } else {
        StoreError::Failed
    }
}

fn io_failure(error: &std::io::Error) -> StoreError {
    if error.raw_os_error() == Some(28) {
        StoreError::NoSpace // ENOSPC
    } else {
        StoreError::Failed
    }
}

/// The result of restoring a snapshot into a fresh repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub object_format: Option<ObjectFormat>,
    pub refs: RefMap,
}

/// Parsed manifest of a ready snapshot.
struct Manifest {
    snapshot: RecoverySnapshot,
    namespace: String,
}

pub struct BundleStore {
    runner: GitRunner,
    root: OwnedRoot,
    mirror_dir: PathBuf,
    repository: GitHubRepoId,
    limits: RunLimits,
    cancel: CancelToken,
}

impl BundleStore {
    pub fn new(
        runner: GitRunner,
        root: OwnedRoot,
        mirror_dir: PathBuf,
        repository: GitHubRepoId,
        limits: RunLimits,
        cancel: CancelToken,
    ) -> Self {
        Self {
            runner,
            root,
            mirror_dir,
            repository,
            limits,
            cancel,
        }
    }

    fn relative(&self, name: &str) -> Result<RelativePath, StoreError> {
        RelativePath::parse(&format!(
            "{}/{name}",
            snapshots_relative_dir(self.repository)
        ))
        .map_err(|_| StoreError::InvalidRequest)
    }

    fn git(&self, invocation: &GitInvocation, work: &WorkDir) -> Result<GitOutput, StoreError> {
        let out = self
            .runner
            .run(invocation, work, self.limits, &self.cancel, &[])
            .map_err(|error| match error {
                RunError::Cancelled => StoreError::Cancelled,
                _ => StoreError::Failed,
            })?;
        if out.termination == Termination::Cancelled {
            return Err(StoreError::Cancelled);
        }
        Ok(out)
    }

    fn git_ok(
        &self,
        subcommand: Subcommand,
        args: Vec<Arg>,
        work: &WorkDir,
    ) -> Result<String, StoreError> {
        let invocation =
            GitInvocation::new(subcommand, args).map_err(|_| StoreError::InvalidRequest)?;
        let out = self.git(&invocation, work)?;
        match out.success_stdout() {
            Ok(text) => Ok(text.to_owned()),
            Err(RunError::Failed(Termination::Exited(_))) => Err(write_failure(&out.stderr)),
            Err(_) => Err(StoreError::Failed),
        }
    }

    /// Removes an exact app-owned temporary directory (staging/verify) if present.
    fn remove_temp(&self, name: &str) -> Result<(), StoreError> {
        match self.root.open_dir(&self.relative(name)?) {
            Ok(path) => fs::remove_dir_all(path).map_err(|_| StoreError::Failed),
            Err(FsError::Missing) => Ok(()),
            Err(_) => Err(StoreError::Failed),
        }
    }

    fn fresh_dir(&self, name: &str) -> Result<PathBuf, StoreError> {
        self.remove_temp(name)?;
        self.root
            .create_dir(&self.relative(name)?)
            .map_err(|_| StoreError::Failed)
    }

    /// Bundle heads mapped from `namespace` back to logical names (fail-closed).
    fn bundle_refs(&self, bundle: &BundleFile, namespace: &str) -> Result<RefMap, StoreError> {
        let text = self.git_ok(
            Subcommand::Bundle,
            vec![Arg::Flag("list-heads"), Arg::Bundle(bundle.clone())],
            &WorkDir::GitDir(self.mirror_dir.clone()),
        )?;
        let mut entries = Vec::new();
        for line in text.lines() {
            let (oid, name) = line.split_once(' ').ok_or(StoreError::Corrupt)?;
            let oid = ObjectId::parse(oid).ok_or(StoreError::Corrupt)?;
            let logical = name
                .strip_prefix(namespace)
                .and_then(|rest| RefName::parse(&format!("refs/{rest}")))
                .ok_or(StoreError::Corrupt)?;
            entries.push((logical, oid));
        }
        RefMap::from_entries(entries).map_err(|_| StoreError::Corrupt)
    }

    /// Restores `bundle` into the empty canonical directory `dest` and returns the
    /// restored logical refs after a full fsck. Only the bundle file is a Git location.
    fn restore_into(
        &self,
        bundle: &Path,
        namespace: &str,
        refs: &RefMap,
        format: ObjectFormat,
        dest: &Path,
    ) -> Result<RefMap, StoreError> {
        let work = WorkDir::GitDir(dest.to_path_buf());
        let format_flag = match format {
            ObjectFormat::Sha1 => "--object-format=sha1",
            ObjectFormat::Sha256 => "--object-format=sha256",
        };
        self.git_ok(
            Subcommand::Init,
            vec![
                Arg::Flag("--bare"),
                Arg::Flag("--quiet"),
                Arg::Flag(format_flag),
            ],
            &work,
        )?;
        let source = Location::local(bundle).map_err(|_| StoreError::Corrupt)?;
        let pairs: Vec<(String, String)> = refs
            .iter()
            .map(|(name, _)| {
                let rest = name.as_str().strip_prefix("refs/").unwrap_or(name.as_str());
                (format!("{namespace}{rest}"), name.as_str().to_owned())
            })
            .collect();
        for chunk in pairs.chunks(REFSPECS_PER_FETCH) {
            let mut args: Vec<Arg> = [
                "--no-tags",
                "--no-write-fetch-head",
                "--no-recurse-submodules",
                "--no-auto-gc",
                "--no-auto-maintenance",
                "--quiet",
            ]
            .iter()
            .map(|flag| Arg::Flag(flag))
            .collect();
            args.push(Arg::EndOfOptions);
            args.push(Arg::Location(source.clone()));
            for (from, to) in chunk {
                args.push(Arg::RefSpec(
                    RefSpec::new(from, to, true).map_err(|_| StoreError::Corrupt)?,
                ));
            }
            self.git_ok(Subcommand::Fetch, args, &work)
                .map_err(|error| {
                    if error == StoreError::Failed {
                        StoreError::Corrupt
                    } else {
                        error
                    }
                })?;
        }
        self.git_ok(
            Subcommand::Fsck,
            vec![Arg::Flag("--no-dangling"), Arg::Flag("--no-progress")],
            &work,
        )
        .map_err(|error| {
            if error == StoreError::Failed {
                StoreError::Corrupt
            } else {
                error
            }
        })?;
        let mut entries = Vec::new();
        for prefix in ["refs/heads/", "refs/tags/"] {
            let pattern = RefPrefix::new(prefix).map_err(|_| StoreError::Failed)?;
            let text = self.git_ok(
                Subcommand::ForEachRef,
                vec![Arg::Flag(REF_FORMAT), Arg::RefPrefix(pattern)],
                &work,
            )?;
            for line in text.lines() {
                let mut fields = line.split('\t');
                let (Some(oid), Some(_), Some(name), None) =
                    (fields.next(), fields.next(), fields.next(), fields.next())
                else {
                    return Err(StoreError::Corrupt);
                };
                let oid = ObjectId::parse(oid).ok_or(StoreError::Corrupt)?;
                let name = RefName::parse(name).ok_or(StoreError::Corrupt)?;
                entries.push((name, oid));
            }
        }
        RefMap::from_entries(entries).map_err(|_| StoreError::Corrupt)
    }

    /// Full verification of bundle + expected refs via a restore inside a temp dir.
    fn verify_bundle(
        &self,
        snapshot_id: &str,
        bundle_path: &Path,
        namespace: &str,
        refs: &RefMap,
    ) -> Result<(), StoreError> {
        let bundle = BundleFile::new(bundle_path).map_err(|_| StoreError::Corrupt)?;
        if self.bundle_refs(&bundle, namespace)? != *refs {
            return Err(StoreError::Corrupt);
        }
        let format = refs.format().ok_or(StoreError::Corrupt)?;
        // Unique per call, so concurrent verifications never remove each other's dir.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = format!(
            "{snapshot_id}.verify-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dest = self.fresh_dir(&temp)?;
        let restored = self.restore_into(bundle_path, namespace, refs, format, &dest);
        let cleaned = self.remove_temp(&temp);
        if restored? != *refs {
            return Err(StoreError::Corrupt);
        }
        cleaned
    }

    fn write_snapshot(
        &self,
        staging: &Path,
        request: &RecoveryRequest,
    ) -> Result<RecoverySnapshot, StoreError> {
        let mut snapshot = RecoverySnapshot {
            repository_id: request.repository_id.clone(),
            snapshot_id: request.snapshot_id.clone(),
            object_format: request.refs.format(),
            refs: request.refs.clone(),
            bundle_sha256: None,
            bundle_bytes: 0,
        };
        if !request.refs.is_empty() {
            let bundle_path = staging.join(BUNDLE_FILE);
            let bundle = BundleFile::new(&bundle_path).map_err(|_| StoreError::Failed)?;
            let prefix =
                RefPrefix::new(&request.namespace).map_err(|_| StoreError::InvalidRequest)?;
            self.git_ok(
                Subcommand::Bundle,
                vec![
                    Arg::Flag("create"),
                    Arg::Bundle(bundle),
                    Arg::RefGlob(prefix),
                ],
                &WorkDir::GitDir(self.mirror_dir.clone()),
            )?;
            self.verify_bundle(
                &request.snapshot_id,
                &bundle_path,
                &request.namespace,
                &request.refs,
            )?;
            let file = File::open(&bundle_path).map_err(|e| io_failure(&e))?;
            snapshot.bundle_bytes = file.metadata().map_err(|e| io_failure(&e))?.len();
            snapshot.bundle_sha256 = Some(sha256_hex(file).map_err(|e| io_failure(&e))?);
        }
        let manifest = json!({
            "app": "git-backup-manager",
            "manifestVersion": 1,
            "kind": "recovery",
            "repositoryId": request.repository_id,
            "sourceRepositoryId": self.repository.to_string(),
            "snapshotId": request.snapshot_id,
            "reason": request.reason,
            "createdAtMs": request.created_at_ms,
            "objectFormat": request.refs.format().map(format_word),
            "bundleRefNamespace": request.namespace,
            "refs": request.refs.iter().map(|(name, oid)| json!({"name": name.as_str(), "oid": oid.as_str()})).collect::<Vec<_>>(),
            "bundle": snapshot.bundle_sha256.as_ref().map(|sha| json!({
                "file": BUNDLE_FILE,
                "sha256": sha,
                "bytes": snapshot.bundle_bytes,
                "selfContained": true,
            })),
            "coverage": {"gitRefs": "complete", "lfs": "not_covered", "submodules": "not_covered"},
            "limitations": LIMITATIONS,
        });
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(staging.join(MANIFEST_FILE))
            .map_err(|e| io_failure(&e))?;
        file.write_all(manifest.to_string().as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| io_failure(&e))?;
        File::open(staging)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| io_failure(&e))?;
        Ok(snapshot)
    }

    fn read_manifest(&self, dir: &Path) -> Result<Manifest, StoreError> {
        let path = dir.join(MANIFEST_FILE);
        let meta = fs::symlink_metadata(&path).map_err(|_| StoreError::Corrupt)?;
        if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
            return Err(StoreError::Corrupt);
        }
        let mut text = String::new();
        File::open(&path)
            .and_then(|file| file.take(MAX_MANIFEST_BYTES).read_to_string(&mut text))
            .map_err(|_| StoreError::Corrupt)?;
        let value: Value = serde_json::from_str(&text).map_err(|_| StoreError::Corrupt)?;
        let text_field = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        let mut entries = Vec::new();
        for entry in value
            .get("refs")
            .and_then(Value::as_array)
            .ok_or(StoreError::Corrupt)?
        {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .and_then(RefName::parse);
            let oid = entry
                .get("oid")
                .and_then(Value::as_str)
                .and_then(ObjectId::parse);
            entries.push((
                name.ok_or(StoreError::Corrupt)?,
                oid.ok_or(StoreError::Corrupt)?,
            ));
        }
        let refs = RefMap::from_entries(entries).map_err(|_| StoreError::Corrupt)?;
        let format = match value.get("objectFormat") {
            Some(Value::Null) | None => None,
            Some(Value::String(word)) => Some(parse_format(word).ok_or(StoreError::Corrupt)?),
            Some(_) => return Err(StoreError::Corrupt),
        };
        let namespace = text_field("bundleRefNamespace").ok_or(StoreError::Corrupt)?;
        let namespace_ok =
            RefPrefix::new(&namespace).is_ok() && namespace.starts_with("refs/gbm/snapshots/");
        if format != refs.format()
            || text_field("app").as_deref() != Some("git-backup-manager")
            || text_field("kind").as_deref() != Some("recovery")
            || value.get("manifestVersion").and_then(Value::as_u64) != Some(1)
            || text_field("sourceRepositoryId") != Some(self.repository.to_string())
            || !namespace_ok
        {
            return Err(StoreError::Corrupt);
        }
        let (bundle_sha256, bundle_bytes) = match value.get("bundle") {
            Some(Value::Null) | None => (None, 0),
            Some(bundle) => (
                Some(
                    bundle
                        .get("sha256")
                        .and_then(Value::as_str)
                        .ok_or(StoreError::Corrupt)?
                        .to_owned(),
                ),
                bundle
                    .get("bytes")
                    .and_then(Value::as_u64)
                    .ok_or(StoreError::Corrupt)?,
            ),
        };
        Ok(Manifest {
            snapshot: RecoverySnapshot {
                repository_id: text_field("repositoryId").ok_or(StoreError::Corrupt)?,
                snapshot_id: text_field("snapshotId").ok_or(StoreError::Corrupt)?,
                object_format: format,
                refs,
                bundle_sha256,
                bundle_bytes,
            },
            namespace,
        })
    }

    /// Verifies checksums and heads of a ready snapshot (no restore).
    fn check_files(&self, snapshot_id: &str) -> Result<(PathBuf, Manifest), StoreError> {
        if !is_namespace_id(snapshot_id) {
            return Err(StoreError::InvalidRequest);
        }
        let dir = match self.root.open_dir(&self.relative(snapshot_id)?) {
            Ok(dir) => dir,
            Err(FsError::Missing) => return Err(StoreError::NotFound),
            Err(_) => return Err(StoreError::Corrupt),
        };
        let manifest = self.read_manifest(&dir)?;
        if manifest.snapshot.snapshot_id != snapshot_id {
            return Err(StoreError::Corrupt);
        }
        let bundle_path = dir.join(BUNDLE_FILE);
        match &manifest.snapshot.bundle_sha256 {
            None => {
                if !manifest.snapshot.refs.is_empty() || bundle_path.exists() {
                    return Err(StoreError::Corrupt);
                }
            }
            Some(expected) => {
                let meta = fs::symlink_metadata(&bundle_path).map_err(|_| StoreError::Corrupt)?;
                if !meta.is_file() || meta.len() != manifest.snapshot.bundle_bytes {
                    return Err(StoreError::Corrupt);
                }
                let file = File::open(&bundle_path).map_err(|_| StoreError::Corrupt)?;
                if sha256_hex(file).map_err(|_| StoreError::Corrupt)? != *expected {
                    return Err(StoreError::Corrupt);
                }
            }
        }
        Ok((dir, manifest))
    }

    /// Restore drill / runbook: restores a verified snapshot into `destination`, which
    /// must be an existing, empty, canonical directory outside the backup root. Never
    /// contacts any source; never writes anywhere else.
    pub fn restore(
        &self,
        snapshot_id: &str,
        destination: &Path,
    ) -> Result<RestoreReport, StoreError> {
        let (dir, manifest) = self.check_files(snapshot_id)?;
        let canonical = fs::canonicalize(destination).map_err(|_| StoreError::InvalidRequest)?;
        let empty = fs::read_dir(&canonical)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if canonical != destination || !empty || canonical.starts_with(self.root.path()) {
            return Err(StoreError::InvalidRequest);
        }
        let Some(format) = manifest.snapshot.object_format else {
            return Ok(RestoreReport {
                object_format: None,
                refs: RefMap::default(),
            });
        };
        let refs = self.restore_into(
            &dir.join(BUNDLE_FILE),
            &manifest.namespace,
            &manifest.snapshot.refs,
            format,
            &canonical,
        )?;
        if refs != manifest.snapshot.refs {
            return Err(StoreError::Corrupt);
        }
        Ok(RestoreReport {
            object_format: Some(format),
            refs,
        })
    }
}

impl SnapshotStore for BundleStore {
    fn create(&self, request: &RecoveryRequest) -> Result<RecoverySnapshot, StoreError> {
        let id = request.snapshot_id.as_str();
        let namespace_ok = RefPrefix::new(&request.namespace).is_ok()
            && request.namespace.starts_with("refs/gbm/snapshots/");
        let reason_ok = request.reason.chars().count() <= MAX_REASON_CHARS
            && !request.reason.chars().any(char::is_control);
        if !is_namespace_id(id) || request.repository_id.is_empty() || !namespace_ok || !reason_ok {
            return Err(StoreError::InvalidRequest);
        }
        match self.root.open_dir(&self.relative(id)?) {
            Ok(_) => return Err(StoreError::Exists),
            Err(FsError::Missing) => {}
            Err(_) => return Err(StoreError::Failed),
        }
        let staging_name = format!("{id}.staging");
        let staging = self.fresh_dir(&staging_name)?;
        let written = self.write_snapshot(&staging, request);
        let snapshot = match written {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = self.remove_temp(&staging_name);
                return Err(error);
            }
        };
        let final_path = staging
            .parent()
            .map(|parent| parent.join(id))
            .ok_or(StoreError::Failed)?;
        if final_path.exists() {
            let _ = self.remove_temp(&staging_name);
            return Err(StoreError::Exists);
        }
        if let Err(error) = fs::rename(&staging, &final_path) {
            let _ = self.remove_temp(&staging_name);
            return Err(io_failure(&error));
        }
        Ok(snapshot)
    }

    fn verify(&self, snapshot_id: &str) -> Result<RecoverySnapshot, StoreError> {
        let (dir, manifest) = self.check_files(snapshot_id)?;
        if manifest.snapshot.bundle_sha256.is_some() {
            self.verify_bundle(
                snapshot_id,
                &dir.join(BUNDLE_FILE),
                &manifest.namespace,
                &manifest.snapshot.refs,
            )?;
        }
        Ok(manifest.snapshot)
    }
}

#[cfg(test)]
mod tests;
