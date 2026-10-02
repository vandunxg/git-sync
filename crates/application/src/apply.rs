//! Applying an authorized plan with exact-ref leases (REQ-06, REQ-20, REQ-22;
//! SYNC_ENGINE.md steps 9-10, "Do not push all namespaces").
//!
//! Only the operations of an `ApplyAuthority` are pushed, each with its own
//! `--force-with-lease` (expected OID, or expected absence). An atomic push is always
//! requested first. If the destination does not support atomic pushes, a plan with a
//! destructive operation and more than one ref is blocked; otherwise the refs are
//! pushed non-atomically and every ref's own result is reported (partial is never
//! reported as success). An unknown outcome (killed, timed out, unparsable output) is
//! reported as such and never retried here: the destination must be observed again.

use gbm_domain::plan::RefOperation;
use gbm_domain::refs::RefName;

use crate::plan::ApplyAuthority;

/// Operations per push invocation (two runner arguments each).
pub const MAX_OPERATIONS_PER_PUSH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// The destination did not hold the expected value (lease).
    StaleLease,
    NonFastForward,
    /// Refused by the server (protected branch, deny-deletes, hook, permissions).
    RemoteRejected,
    /// Not applied because another ref of the same atomic push failed.
    AtomicAborted,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefPushStatus {
    Updated,
    UpToDate,
    Rejected(RejectReason),
}

impl RefPushStatus {
    fn succeeded(self) -> bool {
        matches!(self, Self::Updated | Self::UpToDate)
    }
}

/// Git's own per-ref report for one push invocation, in operation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushReport {
    pub refs: Vec<(RefName, RefPushStatus)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushError {
    /// The destination does not offer atomic pushes; nothing was sent.
    AtomicUnsupported,
    /// Nothing was started (cancelled before spawn, invalid arguments, spawn failure).
    NotStarted,
    /// The push may or may not have changed the destination.
    Unknown,
}

impl std::fmt::Display for PushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AtomicUnsupported => "the destination does not support atomic pushes",
            Self::NotStarted => "the push was not started",
            Self::Unknown => "the push outcome is unknown; the destination must be checked",
        })
    }
}

impl std::error::Error for PushError {}

/// Pushes exact refs from the app's mirror to the mapped destination.
pub trait RefWriter {
    fn push(&self, operations: &[RefOperation], atomic: bool) -> Result<PushReport, PushError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyBlock {
    /// Destructive multi-ref plan on a destination without atomic push support.
    DestructiveNeedsAtomic,
    /// A destructive plan larger than one atomic push.
    TooManyOperations,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Every ref reported updated or up to date (still to be verified, P06-T04).
    Applied(PushReport),
    /// No ref was applied.
    Rejected(PushReport),
    /// Some refs applied, some not (non-atomic, or across push batches).
    Partial(PushReport),
    Blocked(ApplyBlock),
    /// Nothing was started.
    NotStarted,
    /// The destination may have changed: observe and reconcile; do not retry blindly.
    /// `report` holds the batches that completed before the uncertain one.
    Unknown(PushReport),
}

fn covers_exactly(report: &PushReport, operations: &[RefOperation]) -> bool {
    report.refs.len() == operations.len()
        && report
            .refs
            .iter()
            .zip(operations)
            .all(|((name, _), op)| *name == op.name)
}

enum Batch {
    Done(PushReport),
    NotStarted,
    Unknown,
    Blocked(ApplyBlock),
}

fn push_batch(writer: &dyn RefWriter, operations: &[RefOperation], destructive: bool) -> Batch {
    let attempt = match writer.push(operations, true) {
        Err(PushError::AtomicUnsupported) => {
            if destructive && operations.len() > 1 {
                return Batch::Blocked(ApplyBlock::DestructiveNeedsAtomic);
            }
            writer.push(operations, false)
        }
        other => other,
    };
    match attempt {
        Ok(report) if covers_exactly(&report, operations) => Batch::Done(report),
        // A report that does not match what was sent cannot be trusted.
        Ok(_) | Err(PushError::Unknown) => Batch::Unknown,
        Err(PushError::NotStarted) => Batch::NotStarted,
        // The non-atomic retry cannot report this; treat as uncertain.
        Err(PushError::AtomicUnsupported) => Batch::Unknown,
    }
}

/// Pushes exactly the authorized operations. See the module documentation.
pub fn apply_authorized(authority: &ApplyAuthority, writer: &dyn RefWriter) -> ApplyOutcome {
    let operations = &authority.operations;
    let destructive = operations.iter().any(|op| op.kind.is_destructive());
    if destructive && operations.len() > MAX_OPERATIONS_PER_PUSH {
        return ApplyOutcome::Blocked(ApplyBlock::TooManyOperations);
    }
    let mut combined = PushReport { refs: Vec::new() };
    for (index, chunk) in operations.chunks(MAX_OPERATIONS_PER_PUSH).enumerate() {
        match push_batch(writer, chunk, destructive) {
            Batch::Done(report) => combined.refs.extend(report.refs),
            Batch::Blocked(block) => return ApplyOutcome::Blocked(block),
            Batch::NotStarted if index == 0 => return ApplyOutcome::NotStarted,
            // Earlier batches may have applied: the result is at best partial.
            Batch::NotStarted | Batch::Unknown => return ApplyOutcome::Unknown(combined),
        }
    }
    let applied = combined.refs.iter().filter(|(_, s)| s.succeeded()).count();
    if applied == combined.refs.len() {
        ApplyOutcome::Applied(combined)
    } else if applied == 0 {
        ApplyOutcome::Rejected(combined)
    } else {
        ApplyOutcome::Partial(combined)
    }
}

#[cfg(test)]
mod tests;
