//! `WorkspaceGit` over the runner for one working clone (P05-T04).
//!
//! The target commit is brought from the app's mirror snapshot into a temporary
//! `refs/gbm/incoming/<job>` ref of the workspace repository (local transport, the
//! mirror path only), then removed. Status uses `git status --porcelain=v2 -z`;
//! in-progress operations are detected from Git's own state files. The only write to
//! the working tree is `git merge --ff-only`, with hooks, autostash and global config
//! disabled by the runner.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use gbm_application::workspace::{
    Ancestry, HeadState, WorkspaceError, WorkspaceGit, WorkspaceInspection,
};
use gbm_domain::refs::ObjectId;

use crate::git::runner::{
    Arg, CancelToken, FullRef, GitInvocation, GitOutput, GitRunner, Location, RefSpec, RunError,
    RunLimits, Subcommand, Termination, WorkDir,
};

/// Git state files/directories that mean an operation is in progress.
const IN_PROGRESS: [&str; 10] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "REBASE_HEAD",
    "BISECT_START",
    "BISECT_LOG",
    "rebase-merge",
    "rebase-apply",
    "sequencer",
    "AUTO_MERGE",
];

pub struct GitWorkspace {
    runner: GitRunner,
    worktree: PathBuf,
    mirror: PathBuf,
    /// Full mirror ref holding the target, e.g. `refs/gbm/snapshots/<id>/heads/main`.
    source_ref: String,
    /// Temporary ref in the workspace repository, e.g. `refs/gbm/incoming/<job>`.
    incoming_ref: String,
    limits: RunLimits,
    cancel: CancelToken,
}

impl GitWorkspace {
    pub fn new(
        runner: GitRunner,
        worktree: PathBuf,
        mirror: PathBuf,
        source_ref: String,
        job_id: &str,
        limits: RunLimits,
        cancel: CancelToken,
    ) -> Self {
        Self {
            runner,
            worktree,
            mirror,
            source_ref,
            incoming_ref: format!("refs/gbm/incoming/{job_id}"),
            limits,
            cancel,
        }
    }

    fn run(&self, subcommand: Subcommand, args: Vec<Arg>) -> Result<GitOutput, WorkspaceError> {
        let invocation =
            GitInvocation::new(subcommand, args).map_err(|_| WorkspaceError::Failed)?;
        let out = self
            .runner
            .run(
                &invocation,
                &WorkDir::WorkTree(self.worktree.clone()),
                self.limits,
                &self.cancel,
                &[],
            )
            .map_err(|error| match error {
                RunError::Cancelled => WorkspaceError::Cancelled,
                RunError::InvalidWorkDir => WorkspaceError::Unsupported,
                _ => WorkspaceError::Failed,
            })?;
        if out.termination == Termination::Cancelled {
            return Err(WorkspaceError::Cancelled);
        }
        Ok(out)
    }

    fn stdout(&self, subcommand: Subcommand, args: Vec<Arg>) -> Result<String, WorkspaceError> {
        let out = self.run(subcommand, args)?;
        out.success_stdout()
            .map(str::to_owned)
            .map_err(|_| WorkspaceError::Failed)
    }

    fn is_ancestor(&self, a: &ObjectId, b: &ObjectId) -> Result<bool, WorkspaceError> {
        let out = self.run(
            Subcommand::MergeBase,
            vec![
                Arg::Flag("--is-ancestor"),
                Arg::Oid(a.clone()),
                Arg::Oid(b.clone()),
            ],
        )?;
        match (out.termination, out.reaped, out.output_complete) {
            (Termination::Exited(0), true, true) => Ok(true),
            (Termination::Exited(1), true, true) => Ok(false),
            _ => Err(WorkspaceError::Failed),
        }
    }
}

/// Parses `status --porcelain=v2 --branch -z` output.
fn parse_status(text: &str) -> Result<(HeadState, Option<ObjectId>, bool, bool), WorkspaceError> {
    let mut head_name: Option<String> = None;
    let mut head_oid: Option<ObjectId> = None;
    let mut initial = false;
    let mut tracked = false;
    let mut untracked = false;
    let mut skip_next = false;
    for record in text.split('\0').filter(|r| !r.is_empty()) {
        if skip_next {
            // The original path of a rename/copy record ("2 ...") is its own field.
            skip_next = false;
            continue;
        }
        if let Some(value) = record.strip_prefix("# branch.oid ") {
            if value == "(initial)" {
                initial = true;
            } else {
                head_oid = Some(ObjectId::parse(value).ok_or(WorkspaceError::Failed)?);
            }
        } else if let Some(value) = record.strip_prefix("# branch.head ") {
            head_name = Some(value.to_owned());
        } else if record.starts_with("# ") {
            // branch.upstream / branch.ab: informational only.
        } else if record.starts_with("1 ") || record.starts_with("u ") {
            tracked = true;
        } else if record.starts_with("2 ") {
            tracked = true;
            skip_next = true;
        } else if record.starts_with("? ") {
            untracked = true;
        } else {
            return Err(WorkspaceError::Failed);
        }
    }
    let head = match head_name.as_deref() {
        Some("(detached)") => HeadState::Detached,
        Some(_) if initial => HeadState::Unborn,
        Some(name) => HeadState::Branch(name.to_owned()),
        None => return Err(WorkspaceError::Failed),
    };
    Ok((head, head_oid, tracked, untracked))
}

/// Parses `diff-tree -r -z --name-status --diff-filter=AD` output into
/// (added, deleted) paths. Paths with unsafe components fail closed.
fn parse_added_deleted(text: &str) -> Result<(Vec<String>, BTreeSet<String>), WorkspaceError> {
    let mut added = Vec::new();
    let mut deleted = BTreeSet::new();
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    while let Some(status) = fields.next() {
        let path = fields.next().ok_or(WorkspaceError::Failed)?;
        let safe = path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && !c.eq_ignore_ascii_case(".git"));
        if !safe {
            return Err(WorkspaceError::Failed);
        }
        match status {
            "A" => added.push(path.to_owned()),
            "D" => {
                deleted.insert(path.to_owned());
            }
            _ => return Err(WorkspaceError::Failed),
        }
    }
    Ok((added, deleted))
}

/// True when writing `added` paths would replace something already on disk: the path
/// itself exists (in any form), or a parent exists as a non-directory that HEAD does
/// not track (a tracked file being deleted by the same change is expected).
fn collides(worktree: &Path, added: &[String], deleted: &BTreeSet<String>) -> bool {
    added.iter().any(|path| {
        if fs::symlink_metadata(worktree.join(path)).is_ok() {
            return true;
        }
        let parts: Vec<&str> = path.split('/').collect();
        (1..parts.len()).any(|n| {
            let prefix = parts[..n].join("/");
            match fs::symlink_metadata(worktree.join(&prefix)) {
                Ok(meta) => !meta.is_dir() && !deleted.contains(&prefix),
                Err(_) => false,
            }
        })
    })
}

impl WorkspaceGit for GitWorkspace {
    fn inspect(&self) -> Result<Option<WorkspaceInspection>, WorkspaceError> {
        if !self.worktree.exists() {
            return Ok(None);
        }
        let git_dir = self.worktree.join(".git");
        let real = fs::symlink_metadata(&git_dir)
            .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink());
        if !real {
            return Err(WorkspaceError::Unsupported);
        }
        let operation_in_progress = IN_PROGRESS.iter().any(|name| git_dir.join(name).exists());
        let text = self.stdout(
            Subcommand::Status,
            vec![
                Arg::Flag("--porcelain=v2"),
                Arg::Flag("--branch"),
                Arg::Flag("--untracked-files=all"),
                Arg::Flag("--ignore-submodules=none"),
                Arg::Flag("-z"),
            ],
        )?;
        let (head, head_oid, tracked_changes, untracked) = parse_status(&text)?;
        Ok(Some(WorkspaceInspection {
            head,
            head_oid,
            tracked_changes,
            untracked,
            operation_in_progress,
        }))
    }

    fn bring_target(&self, target: &ObjectId) -> Result<(), WorkspaceError> {
        let mirror = Location::local(&self.mirror).map_err(|_| WorkspaceError::Failed)?;
        let spec = RefSpec::new(&self.source_ref, &self.incoming_ref, true)
            .map_err(|_| WorkspaceError::Failed)?;
        self.stdout(
            Subcommand::Fetch,
            vec![
                Arg::Flag("--no-tags"),
                Arg::Flag("--no-write-fetch-head"),
                Arg::Flag("--no-recurse-submodules"),
                Arg::Flag("--no-auto-gc"),
                Arg::Flag("--no-auto-maintenance"),
                Arg::Flag("--quiet"),
                Arg::EndOfOptions,
                Arg::Location(mirror),
                Arg::RefSpec(spec),
            ],
        )?;
        // The fetched ref must be exactly the target captured in the snapshot.
        let name = FullRef::new(&self.incoming_ref).map_err(|_| WorkspaceError::Failed)?;
        let text = self.stdout(
            Subcommand::RevParse,
            vec![Arg::Flag("--verify"), Arg::Flag("--quiet"), Arg::Ref(name)],
        )?;
        if text.trim_end() != target.as_str() {
            return Err(WorkspaceError::Failed);
        }
        Ok(())
    }

    fn ancestry(&self, head: &ObjectId, target: &ObjectId) -> Result<Ancestry, WorkspaceError> {
        if head == target {
            return Ok(Ancestry::Equal);
        }
        if self.is_ancestor(head, target)? {
            Ok(Ancestry::Behind)
        } else if self.is_ancestor(target, head)? {
            Ok(Ancestry::Ahead)
        } else {
            Ok(Ancestry::Diverged)
        }
    }

    fn target_uses_filters(&self, target: &ObjectId) -> Result<bool, WorkspaceError> {
        let local = self.worktree.join(".git").join("info").join("attributes");
        if fs::read_to_string(&local).is_ok_and(|text| text.contains("filter")) {
            return Ok(true);
        }
        let listing = self.stdout(
            Subcommand::LsTree,
            vec![
                Arg::Flag("-r"),
                Arg::Flag("-z"),
                Arg::Flag("--full-tree"),
                Arg::Oid(target.clone()),
            ],
        )?;
        for entry in listing.split('\0').filter(|e| !e.is_empty()) {
            let (meta, path) = entry.split_once('\t').ok_or(WorkspaceError::Failed)?;
            let is_attributes = path == ".gitattributes" || path.ends_with("/.gitattributes");
            if !is_attributes {
                continue;
            }
            let mut fields = meta.split(' ');
            let (Some(_mode), Some(kind), Some(oid)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(WorkspaceError::Failed);
            };
            if kind != "blob" {
                continue;
            }
            let blob = ObjectId::parse(oid).ok_or(WorkspaceError::Failed)?;
            let text = self.stdout(Subcommand::CatFile, vec![Arg::Flag("blob"), Arg::Oid(blob)])?;
            if text.contains("filter") {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn target_overwrites_local_files(
        &self,
        head: &ObjectId,
        target: &ObjectId,
    ) -> Result<bool, WorkspaceError> {
        let text = self.stdout(
            Subcommand::DiffTree,
            vec![
                Arg::Flag("-r"),
                Arg::Flag("-z"),
                Arg::Flag("--name-status"),
                Arg::Flag("--no-renames"),
                Arg::Flag("--diff-filter=AD"),
                Arg::Oid(head.clone()),
                Arg::Oid(target.clone()),
            ],
        )?;
        let (added, deleted) = parse_added_deleted(&text)?;
        Ok(collides(&self.worktree, &added, &deleted))
    }

    fn fast_forward(&self, target: &ObjectId) -> Result<(), WorkspaceError> {
        self.stdout(
            Subcommand::Merge,
            vec![
                Arg::Flag("--ff-only"),
                Arg::Flag("--quiet"),
                Arg::Flag("--no-stat"),
                Arg::Flag("--no-autostash"),
                Arg::Oid(target.clone()),
            ],
        )
        .map(|_| ())
    }

    fn drop_target(&self) -> Result<(), WorkspaceError> {
        let name = FullRef::new(&self.incoming_ref).map_err(|_| WorkspaceError::Failed)?;
        let current = self.run(
            Subcommand::RevParse,
            vec![
                Arg::Flag("--verify"),
                Arg::Flag("--quiet"),
                Arg::Ref(name.clone()),
            ],
        )?;
        let text = match (current.termination, current.reaped, current.output_complete) {
            // `rev-parse --verify --quiet` exits 1 only when the ref does not exist.
            (Termination::Exited(1), true, true) => return Ok(()),
            _ => current
                .success_stdout()
                .map_err(|_| WorkspaceError::Failed)?,
        };
        let oid = ObjectId::parse(text.trim_end()).ok_or(WorkspaceError::Failed)?;
        self.stdout(
            Subcommand::UpdateRef,
            vec![
                Arg::Flag("--no-deref"),
                Arg::Flag("-d"),
                Arg::Ref(name),
                Arg::Oid(oid),
            ],
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests;
