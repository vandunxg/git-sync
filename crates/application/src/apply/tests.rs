//! TEST-ATOMIC-PUSH policy with a scripted test-only writer; real Git pushes are tested
//! in infrastructure/src/git/push.

use std::cell::RefCell;

use gbm_domain::plan::OperationKind;
use gbm_domain::refs::ObjectId;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn op(name: &str, kind: OperationKind) -> Result<RefOperation, Box<dyn std::error::Error>> {
    let oid = ObjectId::parse(&"a".repeat(40)).ok_or("oid")?;
    let old = ObjectId::parse(&"b".repeat(40)).ok_or("oid")?;
    Ok(RefOperation {
        kind,
        name: RefName::parse(name).ok_or("name")?,
        expected_destination: (kind != OperationKind::Create).then_some(old),
        source: (kind != OperationKind::DeleteRef).then_some(oid),
    })
}

fn authority(operations: Vec<RefOperation>) -> ApplyAuthority {
    ApplyAuthority {
        plan_id: "plan-1".to_owned(),
        plan_digest: "d".repeat(64),
        operations,
        recovery: None,
    }
}

type Script = Box<dyn Fn(&[RefOperation], bool) -> Result<PushReport, PushError>>;

/// Records every call (ref count, atomic flag) and answers from `script`.
struct Writer {
    calls: RefCell<Vec<(usize, bool)>>,
    script: Script,
}

impl Writer {
    fn new(
        script: impl Fn(&[RefOperation], bool) -> Result<PushReport, PushError> + 'static,
    ) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            script: Box::new(script),
        }
    }
}

impl RefWriter for Writer {
    fn push(&self, operations: &[RefOperation], atomic: bool) -> Result<PushReport, PushError> {
        self.calls.borrow_mut().push((operations.len(), atomic));
        (self.script)(operations, atomic)
    }
}

fn all(operations: &[RefOperation], status: RefPushStatus) -> PushReport {
    PushReport {
        refs: operations
            .iter()
            .map(|o| (o.name.clone(), status))
            .collect(),
    }
}

#[test]
fn atomic_success_is_applied_and_always_requested_first() -> TestResult {
    let ops = vec![
        op("refs/heads/a", OperationKind::FastForward)?,
        op("refs/heads/b", OperationKind::Create)?,
    ];
    let writer = Writer::new(|ops, _| Ok(all(ops, RefPushStatus::Updated)));
    assert!(matches!(
        apply_authorized(&authority(ops), &writer),
        ApplyOutcome::Applied(_)
    ));
    assert_eq!(*writer.calls.borrow(), vec![(2, true)]);
    Ok(())
}

#[test]
fn a_rejected_atomic_push_is_rejected_not_success() -> TestResult {
    // TEST-ATOMIC-PUSH: one ref refused (protected branch / stale lease).
    let ops = vec![
        op("refs/heads/a", OperationKind::Rewrite)?,
        op("refs/heads/b", OperationKind::Create)?,
    ];
    let writer = Writer::new(|ops, _| {
        Ok(PushReport {
            refs: vec![
                (
                    ops[0].name.clone(),
                    RefPushStatus::Rejected(RejectReason::RemoteRejected),
                ),
                (
                    ops[1].name.clone(),
                    RefPushStatus::Rejected(RejectReason::AtomicAborted),
                ),
            ],
        })
    });
    assert!(matches!(
        apply_authorized(&authority(ops), &writer),
        ApplyOutcome::Rejected(_)
    ));
    assert_eq!(writer.calls.borrow().len(), 1, "no retry");
    Ok(())
}

#[test]
fn without_atomic_support_destructive_multi_ref_plans_are_blocked() -> TestResult {
    let ops = vec![
        op("refs/heads/a", OperationKind::DeleteRef)?,
        op("refs/heads/b", OperationKind::Create)?,
    ];
    let writer = Writer::new(|_, atomic| {
        if atomic {
            Err(PushError::AtomicUnsupported)
        } else {
            Err(PushError::Unknown)
        }
    });
    assert_eq!(
        apply_authorized(&authority(ops), &writer),
        ApplyOutcome::Blocked(ApplyBlock::DestructiveNeedsAtomic)
    );
    assert_eq!(
        *writer.calls.borrow(),
        vec![(2, true)],
        "nothing non-atomic sent"
    );

    // A single destructive ref is trivially atomic.
    let one = vec![op("refs/heads/a", OperationKind::Rewrite)?];
    let writer = Writer::new(|ops, atomic| {
        if atomic {
            Err(PushError::AtomicUnsupported)
        } else {
            Ok(all(ops, RefPushStatus::Updated))
        }
    });
    assert!(matches!(
        apply_authorized(&authority(one), &writer),
        ApplyOutcome::Applied(_)
    ));
    Ok(())
}

#[test]
fn non_atomic_fallback_reports_partial_per_ref() -> TestResult {
    let ops = vec![
        op("refs/heads/a", OperationKind::FastForward)?,
        op("refs/heads/b", OperationKind::Create)?,
    ];
    let writer = Writer::new(|ops, atomic| {
        if atomic {
            return Err(PushError::AtomicUnsupported);
        }
        Ok(PushReport {
            refs: vec![
                (ops[0].name.clone(), RefPushStatus::Updated),
                (
                    ops[1].name.clone(),
                    RefPushStatus::Rejected(RejectReason::StaleLease),
                ),
            ],
        })
    });
    let ApplyOutcome::Partial(report) = apply_authorized(&authority(ops), &writer) else {
        return Err("expected partial".into());
    };
    assert_eq!(
        report.refs[1].1,
        RefPushStatus::Rejected(RejectReason::StaleLease)
    );
    assert_eq!(*writer.calls.borrow(), vec![(2, true), (2, false)]);
    Ok(())
}

#[test]
fn uncertain_or_mismatched_reports_are_unknown_and_not_retried() -> TestResult {
    let ops = vec![op("refs/heads/a", OperationKind::Create)?];
    let writer = Writer::new(|_, _| Err(PushError::Unknown));
    assert!(matches!(
        apply_authorized(&authority(ops.clone()), &writer),
        ApplyOutcome::Unknown(_)
    ));
    assert_eq!(writer.calls.borrow().len(), 1);
    // A report about other refs than those pushed is not trusted.
    let writer = Writer::new(|_, _| {
        Ok(PushReport {
            refs: vec![(
                RefName::parse("refs/heads/other").ok_or(PushError::Unknown)?,
                RefPushStatus::Updated,
            )],
        })
    });
    assert!(matches!(
        apply_authorized(&authority(ops.clone()), &writer),
        ApplyOutcome::Unknown(_)
    ));
    let writer = Writer::new(|_, _| Err(PushError::NotStarted));
    assert_eq!(
        apply_authorized(&authority(ops), &writer),
        ApplyOutcome::NotStarted
    );
    Ok(())
}

#[test]
fn large_plans_batch_only_when_non_destructive() -> TestResult {
    let mut ops = Vec::new();
    for i in 0..(MAX_OPERATIONS_PER_PUSH + 5) {
        ops.push(op(&format!("refs/heads/b{i}"), OperationKind::Create)?);
    }
    let writer = Writer::new(|ops, _| Ok(all(ops, RefPushStatus::Updated)));
    assert!(matches!(
        apply_authorized(&authority(ops.clone()), &writer),
        ApplyOutcome::Applied(ref r) if r.refs.len() == MAX_OPERATIONS_PER_PUSH + 5
    ));
    assert_eq!(
        *writer.calls.borrow(),
        vec![(MAX_OPERATIONS_PER_PUSH, true), (5, true)]
    );
    // Second batch uncertain after the first applied: unknown, with the first's report.
    let writer = Writer::new(|ops, _| {
        if ops.len() == 5 {
            Err(PushError::NotStarted)
        } else {
            Ok(all(ops, RefPushStatus::Updated))
        }
    });
    assert!(matches!(
        apply_authorized(&authority(ops.clone()), &writer),
        ApplyOutcome::Unknown(ref r) if r.refs.len() == MAX_OPERATIONS_PER_PUSH
    ));
    ops.push(op("refs/heads/gone", OperationKind::DeleteRef)?);
    let writer = Writer::new(|ops, _| Ok(all(ops, RefPushStatus::Updated)));
    assert_eq!(
        apply_authorized(&authority(ops), &writer),
        ApplyOutcome::Blocked(ApplyBlock::TooManyOperations)
    );
    assert!(writer.calls.borrow().is_empty());
    Ok(())
}
