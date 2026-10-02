//! TEST-SNAPSHOT (gate logic) with a test-only store; the real bundle store is tested
//! in infrastructure/src/snapshots.

use std::collections::BTreeMap;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn oid(fill: char) -> ObjectId {
    match ObjectId::parse(&fill.to_string().repeat(40)) {
        Some(value) => value,
        None => oid('a'),
    }
}

fn name(text: &str) -> Result<RefName, String> {
    RefName::parse(text).ok_or_else(|| format!("bad name {text}"))
}

/// Test-only store: snapshot id -> verification result.
struct FakeStore(BTreeMap<String, Result<RecoverySnapshot, StoreError>>);

impl SnapshotStore for FakeStore {
    fn create(&self, request: &RecoveryRequest) -> Result<RecoverySnapshot, StoreError> {
        self.0
            .get(&request.snapshot_id)
            .cloned()
            .unwrap_or(Err(StoreError::Failed))
    }

    fn verify(&self, snapshot_id: &str) -> Result<RecoverySnapshot, StoreError> {
        self.0
            .get(snapshot_id)
            .cloned()
            .unwrap_or(Err(StoreError::NotFound))
    }
}

fn old_state() -> Result<RefMap, Box<dyn std::error::Error>> {
    Ok(RefMap::from_entries([
        (name("refs/heads/main")?, oid('1')),
        (name("refs/heads/feature")?, oid('2')),
        (name("refs/tags/v1")?, oid('3')),
    ])?)
}

fn snapshot(refs: RefMap) -> RecoverySnapshot {
    RecoverySnapshot {
        repository_id: "repo-1".to_owned(),
        snapshot_id: "r1".to_owned(),
        object_format: refs.format(),
        refs,
        bundle_sha256: Some("ab".repeat(32)),
        bundle_bytes: 1,
    }
}

fn destructive_plan() -> Result<Vec<PlannedChange>, Box<dyn std::error::Error>> {
    Ok(vec![
        PlannedChange {
            name: name("refs/heads/feature")?,
            old: Some(oid('2')),
            new: Some(oid('7')),
            kind: ChangeKind::Rewrite,
        },
        PlannedChange {
            name: name("refs/tags/v1")?,
            old: Some(oid('3')),
            new: Some(oid('8')),
            kind: ChangeKind::TagMove,
        },
        PlannedChange {
            name: name("refs/heads/main")?,
            old: Some(oid('1')),
            new: None,
            kind: ChangeKind::Delete,
        },
    ])
}

#[test]
fn destructive_plans_pass_only_with_a_verified_covering_snapshot() -> TestResult {
    // TEST-SNAPSHOT: good snapshot -> allowed.
    let store = FakeStore(BTreeMap::from([(
        "r1".to_owned(),
        Ok(snapshot(old_state()?)),
    )]));
    let used = require_recovery(&destructive_plan()?, &store, Some("r1"))?;
    assert_eq!(used.map(|s| s.snapshot_id), Some("r1".to_owned()));
    Ok(())
}

#[test]
fn corrupt_missing_or_full_storage_blocks_destructive_plans() -> TestResult {
    // TEST-SNAPSHOT: corrupt, disk-full and absent recovery material all block.
    let plan = destructive_plan()?;
    for failure in [
        StoreError::Corrupt,
        StoreError::NoSpace,
        StoreError::NotFound,
        StoreError::Failed,
    ] {
        let store = FakeStore(BTreeMap::from([("r1".to_owned(), Err(failure))]));
        assert_eq!(
            require_recovery(&plan, &store, Some("r1")),
            Err(GateError::RecoveryUnverified(failure))
        );
    }
    let store = FakeStore(BTreeMap::new());
    assert_eq!(
        require_recovery(&plan, &store, None),
        Err(GateError::RecoveryMissing)
    );
    Ok(())
}

#[test]
fn a_snapshot_must_hold_every_old_oid_exactly() -> TestResult {
    // The snapshot predates a later change of the tag: it does not cover the old state.
    let stale = RefMap::from_entries([
        (name("refs/heads/main")?, oid('1')),
        (name("refs/heads/feature")?, oid('2')),
        (name("refs/tags/v1")?, oid('9')),
    ])?;
    let store = FakeStore(BTreeMap::from([("r1".to_owned(), Ok(snapshot(stale)))]));
    assert_eq!(
        require_recovery(&destructive_plan()?, &store, Some("r1")),
        Err(GateError::OldStateNotCovered {
            name: "refs/tags/v1".to_owned()
        })
    );
    // A snapshot missing a deleted ref entirely is also not coverage.
    let partial = RefMap::from_entries([(name("refs/heads/feature")?, oid('2'))])?;
    let store = FakeStore(BTreeMap::from([("r1".to_owned(), Ok(snapshot(partial)))]));
    assert!(matches!(
        require_recovery(&destructive_plan()?, &store, Some("r1")),
        Err(GateError::OldStateNotCovered { .. })
    ));
    Ok(())
}

#[test]
fn non_destructive_plans_need_no_snapshot_and_inconsistent_ones_are_rejected() -> TestResult {
    let store = FakeStore(BTreeMap::new());
    let safe = vec![
        PlannedChange {
            name: name("refs/heads/new")?,
            old: None,
            new: Some(oid('4')),
            kind: ChangeKind::Create,
        },
        PlannedChange {
            name: name("refs/heads/main")?,
            old: Some(oid('1')),
            new: Some(oid('5')),
            kind: ChangeKind::FastForward,
        },
    ];
    assert_eq!(require_recovery(&safe, &store, None), Ok(None));
    // A "delete" that still has a new value, or a "create" with an old value, is not
    // allowed to dodge the gate by mislabelling.
    for (kind, old, new) in [
        (ChangeKind::Delete, Some(oid('1')), Some(oid('2'))),
        (ChangeKind::Create, Some(oid('1')), Some(oid('2'))),
        (ChangeKind::FastForward, Some(oid('1')), None),
        (ChangeKind::Rewrite, Some(oid('1')), Some(oid('1'))),
    ] {
        let change = PlannedChange {
            name: name("refs/heads/main")?,
            old,
            new,
            kind,
        };
        assert!(matches!(
            require_recovery(&[change], &store, None),
            Err(GateError::InvalidChange { .. })
        ));
    }
    Ok(())
}

#[test]
fn sha256_matches_a_known_vector() -> TestResult {
    assert_eq!(
        sha256_hex("abc".as_bytes())?,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    Ok(())
}
