//! Optional workspace updates (REQ-17; WORKSPACE_AND_FILESYSTEM.md "Workspace update
//! policy", ADR-006).
//!
//! A workspace is updated only when it is on the selected working branch, has no
//! staged, unstaged, unmerged or untracked changes, no operation in progress, and its
//! HEAD is a strict ancestor of the captured target (fast-forward). Everything else is
//! reported with an explicit status and left untouched: no stash, merge commit, rebase,
//! reset, clean, checkout of another branch, or overwrite of untracked or ignored files. Targets
//! whose `.gitattributes` select filter drivers are skipped (the app does not run the
//! user's filter configuration). A skipped workspace never affects the mirror result.

use std::fmt;

use gbm_domain::refs::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadState {
    Branch(String),
    Detached,
    /// A branch with no commits yet.
    Unborn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInspection {
    pub head: HeadState,
    pub head_oid: Option<ObjectId>,
    /// Staged, unstaged or unmerged changes to tracked files.
    pub tracked_changes: bool,
    pub untracked: bool,
    /// Merge, rebase, cherry-pick, revert, bisect or sequencer state present.
    pub operation_in_progress: bool,
}

impl WorkspaceInspection {
    fn same_state(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ancestry {
    Equal,
    /// HEAD is a strict ancestor of the target: fast-forward possible.
    Behind,
    /// The target is a strict ancestor of HEAD: local commits.
    Ahead,
    Diverged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceError {
    /// `.git` is a file or symlink (worktree/submodule/gitdir link): not managed here.
    Unsupported,
    Cancelled,
    Failed,
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "this working copy layout is not supported",
            Self::Cancelled => "the workspace update was cancelled",
            Self::Failed => "the workspace could not be inspected or updated",
        })
    }
}

impl std::error::Error for WorkspaceError {}

/// Git operations on one workspace, bound to one job's target.
pub trait WorkspaceGit {
    /// `None` when the workspace folder does not exist.
    fn inspect(&self) -> Result<Option<WorkspaceInspection>, WorkspaceError>;
    /// Makes the target commit available in the workspace repository (temporary ref).
    fn bring_target(&self, target: &ObjectId) -> Result<(), WorkspaceError>;
    fn ancestry(&self, head: &ObjectId, target: &ObjectId) -> Result<Ancestry, WorkspaceError>;
    /// True when any `.gitattributes` in the target tree (or the repository's own
    /// attributes file) selects a filter driver.
    fn target_uses_filters(&self, target: &ObjectId) -> Result<bool, WorkspaceError>;
    /// True when a path the target adds (or a parent of it) already exists in the
    /// working tree. Status does not report ignored files, and `merge --ff-only`
    /// would overwrite them.
    fn target_overwrites_local_files(
        &self,
        head: &ObjectId,
        target: &ObjectId,
    ) -> Result<bool, WorkspaceError>;
    /// `merge --ff-only <target>`.
    fn fast_forward(&self, target: &ObjectId) -> Result<(), WorkspaceError>;
    /// Removes the temporary ref created by `bring_target` (idempotent).
    fn drop_target(&self) -> Result<(), WorkspaceError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Missing,
    Detached,
    Unborn,
    OtherBranch,
    Dirty,
    Untracked,
    OperationInProgress,
    Ahead,
    Diverged,
    FiltersUnsupported,
    /// The target adds a path that already exists locally (e.g. an ignored file).
    WouldOverwriteLocalFiles,
    /// The workspace changed while the update was being prepared.
    ChangedDuringUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceOutcome {
    Updated { from: ObjectId, to: ObjectId },
    Current,
    Skipped(SkipReason),
}

fn precheck(
    inspection: &WorkspaceInspection,
    working_branch: &str,
) -> Result<ObjectId, SkipReason> {
    let branch = match &inspection.head {
        HeadState::Detached => return Err(SkipReason::Detached),
        HeadState::Unborn => return Err(SkipReason::Unborn),
        HeadState::Branch(name) => name,
    };
    if branch != working_branch {
        return Err(SkipReason::OtherBranch);
    }
    if inspection.operation_in_progress {
        return Err(SkipReason::OperationInProgress);
    }
    if inspection.tracked_changes {
        return Err(SkipReason::Dirty);
    }
    if inspection.untracked {
        return Err(SkipReason::Untracked);
    }
    inspection.head_oid.clone().ok_or(SkipReason::Unborn)
}

/// Updates the workspace to `target` only by a safe fast-forward of `working_branch`
/// (short branch name). See the module documentation for the policy.
pub fn update_workspace(
    git: &dyn WorkspaceGit,
    working_branch: &str,
    target: &ObjectId,
) -> Result<WorkspaceOutcome, WorkspaceError> {
    let Some(before) = git.inspect()? else {
        return Ok(WorkspaceOutcome::Skipped(SkipReason::Missing));
    };
    let head = match precheck(&before, working_branch) {
        Ok(head) => head,
        Err(reason) => return Ok(WorkspaceOutcome::Skipped(reason)),
    };
    if head == *target {
        return Ok(WorkspaceOutcome::Current);
    }
    let result = fast_forward_checked(git, &before, &head, target);
    let dropped = git.drop_target();
    let outcome = result?;
    dropped?;
    Ok(outcome)
}

fn fast_forward_checked(
    git: &dyn WorkspaceGit,
    before: &WorkspaceInspection,
    head: &ObjectId,
    target: &ObjectId,
) -> Result<WorkspaceOutcome, WorkspaceError> {
    git.bring_target(target)?;
    match git.ancestry(head, target)? {
        Ancestry::Equal => return Ok(WorkspaceOutcome::Current),
        Ancestry::Ahead => return Ok(WorkspaceOutcome::Skipped(SkipReason::Ahead)),
        Ancestry::Diverged => return Ok(WorkspaceOutcome::Skipped(SkipReason::Diverged)),
        Ancestry::Behind => {}
    }
    if git.target_uses_filters(target)? {
        return Ok(WorkspaceOutcome::Skipped(SkipReason::FiltersUnsupported));
    }
    if git.target_overwrites_local_files(head, target)? {
        return Ok(WorkspaceOutcome::Skipped(
            SkipReason::WouldOverwriteLocalFiles,
        ));
    }
    // Re-check immediately before writing: anything the user changed meanwhile wins.
    match git.inspect()? {
        Some(now) if now.same_state(before) => {}
        _ => return Ok(WorkspaceOutcome::Skipped(SkipReason::ChangedDuringUpdate)),
    }
    git.fast_forward(target)?;
    let after = git.inspect()?.ok_or(WorkspaceError::Failed)?;
    let clean = !after.tracked_changes && !after.untracked && !after.operation_in_progress;
    if after.head_oid.as_ref() != Some(target) || after.head != before.head || !clean {
        return Err(WorkspaceError::Failed);
    }
    Ok(WorkspaceOutcome::Updated {
        from: head.clone(),
        to: target.clone(),
    })
}

#[cfg(test)]
mod tests;
