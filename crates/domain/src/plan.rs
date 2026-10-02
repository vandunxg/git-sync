//! Exact-ref operation classification (SYNC_ENGINE.md "Ref comparison and operation
//! classes"; REQ-20, REQ-21).
//!
//! Inputs are the captured source snapshot refs, a complete observation of the
//! destination's selected refs, and the destination state last verified after our own
//! write ("managed" state). Every destination ref that differs from what we last
//! verified is drift and blocks the whole plan: the app never overwrites, re-creates or
//! deletes something it did not put there. Deletes are only proposed for refs we
//! verified earlier and that the (complete, current) source no longer has.
//! Branch ancestry comes from the caller; unknown ancestry is treated as a rewrite.

use crate::refs::{ObjectId, RefError, RefKind, RefMap, RefName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationKind {
    Create,
    FastForward,
    Rewrite,
    MoveTag,
    DeleteRef,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::FastForward => "fast_forward",
            Self::Rewrite => "rewrite",
            Self::MoveTag => "move_tag",
            Self::DeleteRef => "delete_ref",
        }
    }

    /// Needs owner approval and a verified pre-change recovery snapshot.
    pub fn is_destructive(self) -> bool {
        matches!(self, Self::Rewrite | Self::MoveTag | Self::DeleteRef)
    }
}

/// One exact ref update with its lease: the destination must still hold
/// `expected_destination` (`None` = must be absent) when it is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefOperation {
    pub kind: OperationKind,
    pub name: RefName,
    pub expected_destination: Option<ObjectId>,
    /// `None` only for `DeleteRef`.
    pub source: Option<ObjectId>,
}

/// A destination ref whose current value is not what the app last verified there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub name: RefName,
    pub last_verified: Option<ObjectId>,
    pub observed: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// Operations in ref-name order (may be empty: nothing to do).
    Operations(Vec<RefOperation>),
    /// Destination changed outside the app: review/adoption required, nothing planned.
    DestinationDrift(Vec<Drift>),
}

/// `Some(true)` when `ancestor` is an ancestor of (or equal to) `descendant`;
/// `None` when it cannot be determined (e.g. objects unavailable).
pub trait AncestryOracle {
    fn is_ancestor(&self, ancestor: &ObjectId, descendant: &ObjectId) -> Option<bool>;
}

/// Classifies the update of `destination` to `source`. `last_verified` is `None` for
/// a destination the app has never written; it is then treated as empty, so any ref
/// already present there is drift.
pub fn classify(
    source: &RefMap,
    destination: &RefMap,
    last_verified: Option<&RefMap>,
    ancestry: &dyn AncestryOracle,
) -> Result<Classification, RefError> {
    let empty = RefMap::default();
    let managed = last_verified.unwrap_or(&empty);
    let formats = [source.format(), destination.format(), managed.format()];
    let mut known = formats.iter().flatten();
    if let Some(first) = known.next()
        && known.any(|f| f != first)
    {
        return Err(RefError::MixedObjectFormat);
    }

    let mut drift = Vec::new();
    let mut names: Vec<&RefName> = source
        .iter()
        .chain(destination.iter())
        .chain(managed.iter())
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names.dedup();
    for name in &names {
        let observed = destination.get(name);
        let verified = managed.get(name);
        if observed != verified {
            drift.push(Drift {
                name: (*name).clone(),
                last_verified: verified.cloned(),
                observed: observed.cloned(),
            });
        }
    }
    if !drift.is_empty() {
        return Ok(Classification::DestinationDrift(drift));
    }

    // From here the destination equals the last verified state for every name.
    let mut operations = Vec::new();
    for name in names {
        let old = destination.get(name);
        let new = source.get(name);
        let kind = match (old, new) {
            (Some(o), Some(n)) if o == n => continue,
            (None, None) => continue,
            (None, Some(_)) => OperationKind::Create,
            (Some(_), None) => OperationKind::DeleteRef,
            (Some(o), Some(n)) => match name.kind() {
                RefKind::Tag => OperationKind::MoveTag,
                RefKind::Branch => match ancestry.is_ancestor(o, n) {
                    Some(true) => OperationKind::FastForward,
                    Some(false) | None => OperationKind::Rewrite,
                },
            },
        };
        operations.push(RefOperation {
            kind,
            name: name.clone(),
            expected_destination: old.cloned(),
            source: new.cloned(),
        });
    }
    Ok(Classification::Operations(operations))
}

#[cfg(test)]
mod tests;
