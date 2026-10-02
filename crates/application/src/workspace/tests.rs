//! Policy tests with a test-only workspace double; real Git is exercised in
//! infrastructure/src/workspace.

use std::cell::{Cell, RefCell};

use super::*;

fn oid(fill: char) -> ObjectId {
    match ObjectId::parse(&fill.to_string().repeat(40)) {
        Some(value) => value,
        None => oid('a'),
    }
}

fn clean(head: HeadState, fill: char) -> WorkspaceInspection {
    WorkspaceInspection {
        head,
        head_oid: Some(oid(fill)),
        tracked_changes: false,
        untracked: false,
        operation_in_progress: false,
    }
}

struct Fake {
    states: RefCell<Vec<Option<WorkspaceInspection>>>,
    ancestry: Ancestry,
    filters: bool,
    overwrites: bool,
    forwarded: Cell<bool>,
    dropped: Cell<bool>,
    brought: Cell<bool>,
}

impl Fake {
    /// `states` are returned by successive `inspect` calls (last one repeats).
    fn new(states: Vec<Option<WorkspaceInspection>>, ancestry: Ancestry) -> Self {
        Self {
            states: RefCell::new(states),
            ancestry,
            filters: false,
            overwrites: false,
            forwarded: Cell::new(false),
            dropped: Cell::new(false),
            brought: Cell::new(false),
        }
    }
}

impl WorkspaceGit for Fake {
    fn inspect(&self) -> Result<Option<WorkspaceInspection>, WorkspaceError> {
        let mut states = self.states.borrow_mut();
        if states.len() > 1 {
            Ok(states.remove(0))
        } else {
            Ok(states.first().cloned().flatten())
        }
    }
    fn bring_target(&self, _: &ObjectId) -> Result<(), WorkspaceError> {
        self.brought.set(true);
        Ok(())
    }
    fn ancestry(&self, _: &ObjectId, _: &ObjectId) -> Result<Ancestry, WorkspaceError> {
        Ok(self.ancestry)
    }
    fn target_uses_filters(&self, _: &ObjectId) -> Result<bool, WorkspaceError> {
        Ok(self.filters)
    }
    fn target_overwrites_local_files(
        &self,
        _: &ObjectId,
        _: &ObjectId,
    ) -> Result<bool, WorkspaceError> {
        Ok(self.overwrites)
    }
    fn fast_forward(&self, _: &ObjectId) -> Result<(), WorkspaceError> {
        self.forwarded.set(true);
        Ok(())
    }
    fn drop_target(&self) -> Result<(), WorkspaceError> {
        self.dropped.set(true);
        Ok(())
    }
}

fn main() -> HeadState {
    HeadState::Branch("main".to_owned())
}

#[test]
fn only_a_clean_known_branch_fast_forward_is_applied() -> Result<(), WorkspaceError> {
    let before = clean(main(), '1');
    let after = clean(main(), '2');
    let fake = Fake::new(
        vec![Some(before.clone()), Some(before), Some(after)],
        Ancestry::Behind,
    );
    assert_eq!(
        update_workspace(&fake, "main", &oid('2'))?,
        WorkspaceOutcome::Updated {
            from: oid('1'),
            to: oid('2')
        }
    );
    assert!(fake.forwarded.get() && fake.dropped.get());
    Ok(())
}

#[test]
fn unsafe_states_are_skipped_before_anything_is_fetched() -> Result<(), WorkspaceError> {
    let mut dirty = clean(main(), '1');
    dirty.tracked_changes = true;
    let mut untracked = clean(main(), '1');
    untracked.untracked = true;
    let mut busy = clean(main(), '1');
    busy.operation_in_progress = true;
    let cases = [
        (None, SkipReason::Missing),
        (Some(clean(HeadState::Detached, '1')), SkipReason::Detached),
        (
            Some(WorkspaceInspection {
                head_oid: None,
                ..clean(HeadState::Unborn, '1')
            }),
            SkipReason::Unborn,
        ),
        (
            Some(clean(HeadState::Branch("feature".to_owned()), '1')),
            SkipReason::OtherBranch,
        ),
        (Some(dirty), SkipReason::Dirty),
        (Some(untracked), SkipReason::Untracked),
        (Some(busy), SkipReason::OperationInProgress),
    ];
    for (state, reason) in cases {
        let fake = Fake::new(vec![state], Ancestry::Behind);
        assert_eq!(
            update_workspace(&fake, "main", &oid('2'))?,
            WorkspaceOutcome::Skipped(reason)
        );
        assert!(!fake.brought.get() && !fake.forwarded.get(), "{reason:?}");
    }
    Ok(())
}

#[test]
fn ahead_diverged_and_filtered_targets_are_never_written() -> Result<(), WorkspaceError> {
    for (ancestry, filters, expected) in [
        (
            Ancestry::Ahead,
            false,
            WorkspaceOutcome::Skipped(SkipReason::Ahead),
        ),
        (
            Ancestry::Diverged,
            false,
            WorkspaceOutcome::Skipped(SkipReason::Diverged),
        ),
        (
            Ancestry::Behind,
            true,
            WorkspaceOutcome::Skipped(SkipReason::FiltersUnsupported),
        ),
        (Ancestry::Equal, false, WorkspaceOutcome::Current),
    ] {
        let mut fake = Fake::new(vec![Some(clean(main(), '1'))], ancestry);
        fake.filters = filters;
        assert_eq!(update_workspace(&fake, "main", &oid('2'))?, expected);
        assert!(!fake.forwarded.get());
        assert!(fake.dropped.get(), "temporary ref always removed");
    }
    Ok(())
}

#[test]
fn a_target_that_would_overwrite_local_files_is_skipped() -> Result<(), WorkspaceError> {
    let mut fake = Fake::new(vec![Some(clean(main(), '1'))], Ancestry::Behind);
    fake.overwrites = true;
    assert_eq!(
        update_workspace(&fake, "main", &oid('2'))?,
        WorkspaceOutcome::Skipped(SkipReason::WouldOverwriteLocalFiles)
    );
    assert!(!fake.forwarded.get() && fake.dropped.get());
    Ok(())
}

#[test]
fn a_change_during_preparation_wins_over_the_update() -> Result<(), WorkspaceError> {
    let before = clean(main(), '1');
    let mut edited = before.clone();
    edited.tracked_changes = true;
    let fake = Fake::new(vec![Some(before), Some(edited)], Ancestry::Behind);
    assert_eq!(
        update_workspace(&fake, "main", &oid('2'))?,
        WorkspaceOutcome::Skipped(SkipReason::ChangedDuringUpdate)
    );
    assert!(!fake.forwarded.get());
    Ok(())
}

#[test]
fn an_unexpected_post_state_is_an_error_not_a_success() {
    let before = clean(main(), '1');
    let mut wrong = clean(main(), '3');
    wrong.untracked = true;
    let fake = Fake::new(
        vec![Some(before.clone()), Some(before), Some(wrong)],
        Ancestry::Behind,
    );
    assert_eq!(
        update_workspace(&fake, "main", &oid('2')),
        Err(WorkspaceError::Failed)
    );
}

#[test]
fn already_current_needs_no_git_work() -> Result<(), WorkspaceError> {
    let fake = Fake::new(vec![Some(clean(main(), '2'))], Ancestry::Behind);
    assert_eq!(
        update_workspace(&fake, "main", &oid('2'))?,
        WorkspaceOutcome::Current
    );
    assert!(!fake.brought.get());
    Ok(())
}
