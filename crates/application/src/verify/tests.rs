//! TEST-VERIFY with a test-only result store.

use std::cell::RefCell;

use gbm_domain::refs::ObjectId;

use super::*;
use crate::apply::PushReport;
use crate::snapshot::Coverage;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn map(entries: &[(&str, char)]) -> Result<RefMap, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for (name, fill) in entries {
        out.push((
            RefName::parse(name).ok_or("name")?,
            ObjectId::parse(&fill.to_string().repeat(40)).ok_or("oid")?,
        ));
    }
    Ok(RefMap::from_entries(out)?)
}

fn manifest(refs: RefMap) -> SnapshotManifest {
    SnapshotManifest {
        repository_id: "repo-1".to_owned(),
        snapshot_id: "snap-2".to_owned(),
        object_format: refs.format(),
        ref_set_digest: ref_set_digest(&refs),
        refs,
        head_target: None,
        observed_at_ms: 1,
        ref_namespace: None,
        git_refs: Coverage::Complete,
        lfs: Coverage::NotChecked,
        submodules: Coverage::NotChecked,
    }
}

fn applied() -> ApplyOutcome {
    ApplyOutcome::Applied(PushReport { refs: Vec::new() })
}

fn input<'a>(
    snapshot: &'a SnapshotManifest,
    outcome: &'a ApplyOutcome,
    after: Option<&'a RefMap>,
    source_after: Option<&'a RefMap>,
    last_verified: Option<&'a RefMap>,
) -> VerifyInput<'a> {
    VerifyInput {
        repository_id: "repo-1",
        plan_id: "plan-1",
        snapshot,
        outcome,
        destination_after: after,
        source_after,
        last_verified,
        now_ms: 5_000,
    }
}

#[test]
fn exit_success_with_a_differing_ref_is_a_failed_verification() -> TestResult {
    // TEST-VERIFY (first half).
    let snapshot = manifest(map(&[("refs/heads/main", '1'), ("refs/tags/v1", '2')])?);
    let after = map(&[("refs/heads/main", '1'), ("refs/tags/v1", '9')])?;
    let outcome = applied();
    let record = record_attempt(&input(&snapshot, &outcome, Some(&after), None, None));
    assert_eq!(record.git_result, JobResult::Failed);
    assert_eq!(record.git_error, Some(ErrorCode::VerifyFailed));
    assert_eq!(record.git_refs, CoverageGitRefs::Failed);
    assert_eq!(record.verified_at_ms, None);
    assert_eq!(record.mismatched_refs, vec!["refs/tags/v1".to_owned()]);

    let unobserved = record_attempt(&input(&snapshot, &outcome, None, None, None));
    assert_eq!(unobserved.git_refs, CoverageGitRefs::NotChecked);
    assert_eq!(unobserved.verified_at_ms, None);
    assert_eq!(unobserved.git_result, JobResult::Failed);
    Ok(())
}

#[test]
fn a_source_that_moved_again_is_an_update_not_synced_forever() -> TestResult {
    // TEST-VERIFY (second half).
    let snapshot = manifest(map(&[("refs/heads/main", '1')])?);
    let newer = map(&[("refs/heads/main", '3')])?;
    let outcome = applied();
    let record = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        Some(&newer),
        None,
    ));
    assert_eq!(record.git_result, JobResult::Succeeded);
    assert_eq!(record.verified_at_ms, Some(5_000));
    assert_eq!(record.source_recheck, SourceRecheck::Newer);
    let same = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        Some(&snapshot.refs),
        None,
    ));
    assert_eq!(same.source_recheck, SourceRecheck::Same);
    Ok(())
}

#[test]
fn partial_outcomes_reconcile_the_managed_state_without_adopting_drift() -> TestResult {
    let old = map(&[
        ("refs/heads/a", '1'),
        ("refs/heads/b", '2'),
        ("refs/heads/c", '3'),
    ])?;
    let snapshot = manifest(map(&[
        ("refs/heads/a", '4'),
        ("refs/heads/b", '5'),
        ("refs/heads/new", '6'),
    ])?);
    // a: written by us; b: not written (still old); c: deleted by us; new: written;
    // x: created by a third party; nothing else.
    let after = map(&[
        ("refs/heads/a", '4'),
        ("refs/heads/b", '2'),
        ("refs/heads/new", '6'),
        ("refs/heads/x", '7'),
    ])?;
    let outcome = ApplyOutcome::Partial(PushReport { refs: Vec::new() });
    let record = record_attempt(&input(&snapshot, &outcome, Some(&after), None, Some(&old)));
    assert_eq!(record.git_result, JobResult::Partial);
    assert_eq!(record.git_error, Some(ErrorCode::PushPartial));
    let managed = record.managed_state.ok_or("managed")?;
    assert_eq!(
        managed,
        map(&[
            ("refs/heads/a", '4'),
            ("refs/heads/b", '2'),
            ("refs/heads/new", '6'),
        ])?,
        "x stays foreign (drift next time); c's deletion is ours"
    );
    // A third party moved b to an unknown value: keep the old value so it is drift.
    let moved = map(&[("refs/heads/a", '4'), ("refs/heads/b", '8')])?;
    let managed =
        reconciled_managed_state(Some(&old), &snapshot.refs, Some(&moved)).ok_or("managed")?;
    assert_eq!(
        managed.get(&RefName::parse("refs/heads/b").ok_or("name")?),
        old.get(&RefName::parse("refs/heads/b").ok_or("name")?)
    );
    // No observation: nothing changes.
    assert_eq!(
        reconciled_managed_state(Some(&old), &snapshot.refs, None),
        Some(old)
    );
    Ok(())
}

#[test]
fn an_uncertain_push_is_resolved_by_observation() -> TestResult {
    let snapshot = manifest(map(&[("refs/heads/main", '1')])?);
    let outcome = ApplyOutcome::Unknown(PushReport { refs: Vec::new() });
    let ok = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        None,
        None,
    ));
    assert_eq!(ok.git_result, JobResult::Succeeded);
    let unknown = record_attempt(&input(&snapshot, &outcome, None, None, None));
    assert_eq!(unknown.git_result, JobResult::Failed);
    Ok(())
}

struct Store {
    revision: RefCell<u64>,
    saved: RefCell<Vec<SyncRecord>>,
}

impl ResultStore for Store {
    fn commit(&self, record: &SyncRecord, expected: u64) -> Result<u64, StoreError> {
        let mut revision = self.revision.borrow_mut();
        if *revision != expected {
            return Err(StoreError::RevisionConflict);
        }
        *revision += 1;
        self.saved.borrow_mut().push(record.clone());
        Ok(*revision)
    }
}

#[test]
fn results_are_persisted_with_revision_checks() -> TestResult {
    let snapshot = manifest(map(&[("refs/heads/main", '1')])?);
    let outcome = applied();
    let store = Store {
        revision: RefCell::new(7),
        saved: RefCell::new(Vec::new()),
    };
    let attempt = input(&snapshot, &outcome, Some(&snapshot.refs), None, None);
    let (record, revision) = persist_attempt(&store, &attempt, 7).map_err(|e| format!("{e:?}"))?;
    assert_eq!(revision, 8);
    assert_eq!(store.saved.borrow().as_slice(), &[record]);
    assert_eq!(
        persist_attempt(&store, &attempt, 7),
        Err(StoreError::RevisionConflict)
    );
    assert_eq!(store.saved.borrow().len(), 1);
    Ok(())
}

#[test]
fn an_unobserved_source_is_not_reported_as_unchanged() -> TestResult {
    let snapshot = manifest(map(&[("refs/heads/main", '1')])?);
    let outcome = applied();
    let record = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        None,
        None,
    ));
    assert_eq!(record.source_recheck, SourceRecheck::NotObserved);
    Ok(())
}

#[test]
fn a_verified_git_backup_is_partial_until_lfs_and_submodules_are_proven() -> TestResult {
    // P06 "LFS release boundary": never a complete job for pointer-only scope.
    let mut snapshot = manifest(map(&[("refs/heads/main", '1')])?);
    let outcome = applied();
    let record = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        None,
        None,
    ));
    assert_eq!(record.git_result, JobResult::Succeeded);
    assert_eq!(record.lfs, Coverage::NotChecked);
    assert_eq!(
        job_result(&record),
        (JobResult::Partial, Some(ErrorCode::LfsIncomplete))
    );
    snapshot.lfs = Coverage::Complete;
    let record = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        None,
        None,
    ));
    assert_eq!(
        job_result(&record),
        (JobResult::Partial, Some(ErrorCode::SubmoduleUncovered))
    );
    snapshot.submodules = Coverage::Complete;
    let record = record_attempt(&input(
        &snapshot,
        &outcome,
        Some(&snapshot.refs),
        None,
        None,
    ));
    assert_eq!(job_result(&record), (JobResult::Succeeded, None));
    // A Git failure is never upgraded by complete scope.
    let after = map(&[("refs/heads/main", '9')])?;
    let failed = record_attempt(&input(&snapshot, &outcome, Some(&after), None, None));
    assert_eq!(
        job_result(&failed),
        (JobResult::Failed, Some(ErrorCode::VerifyFailed))
    );
    Ok(())
}
