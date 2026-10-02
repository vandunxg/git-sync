//! Source observations, freshness and the derived source state (REQ-08, REQ-42;
//! SCHEDULER_AND_STATE.md "Status dimensions").
//!
//! Observations are ordered by a per-repository sequence assigned when the observation
//! *started*, not by arrival time or wall clock, so a slow older result arriving after
//! a newer one is ignored and can never regress the view. A partial or failed attempt
//! never replaces the last complete ref map; it only changes what we can claim about
//! the source now (unknown / unavailable), and only a complete, fresh observation equal
//! to the captured snapshot is `current`.

use crate::contract::RepositorySummarySourceState as SourceState;
use crate::refs::{Advertisement, RefMap};

/// Why the source could not be observed. Never interpreted as deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// 401/403-style refusal or missing repository grant.
    AccessDenied,
    /// 404-style: inaccessible or missing, not "deleted".
    NotFound,
    /// Network/transport failure or timeout.
    Transport,
    /// The advertisement was malformed or used an unsupported format/name.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationOutcome {
    /// Every selected ref was read. An empty map is a valid empty repository.
    Complete(Advertisement),
    /// Output was cut short (e.g. bounded output exceeded); nothing can be certified.
    Partial,
    Unavailable(UnavailableReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceObservation {
    /// Monotonic per repository, assigned at start.
    pub sequence: u64,
    /// Unix milliseconds when the observation started.
    pub observed_at_ms: i64,
    pub outcome: ObservationOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Accepted,
    /// Not newer than what the view already holds; dropped without effect.
    IgnoredOlder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptKind {
    Complete,
    Partial,
    Unavailable(UnavailableReason),
}

/// What is known about one source, built only through `apply`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceView {
    last_complete: Option<(u64, i64, Advertisement)>,
    last_attempt: Option<(u64, i64, AttemptKind)>,
}

impl SourceView {
    pub fn apply(&mut self, observation: SourceObservation) -> Applied {
        if self
            .last_attempt
            .is_some_and(|(sequence, _, _)| observation.sequence <= sequence)
        {
            return Applied::IgnoredOlder;
        }
        let at = observation.observed_at_ms;
        let kind = match observation.outcome {
            ObservationOutcome::Complete(advertisement) => {
                self.last_complete = Some((observation.sequence, at, advertisement));
                AttemptKind::Complete
            }
            ObservationOutcome::Partial => AttemptKind::Partial,
            ObservationOutcome::Unavailable(reason) => AttemptKind::Unavailable(reason),
        };
        self.last_attempt = Some((observation.sequence, at, kind));
        Applied::Accepted
    }

    /// The last complete ref map, regardless of later failures (kept, never deleted).
    pub fn last_complete(&self) -> Option<&Advertisement> {
        self.last_complete.as_ref().map(|(_, _, ad)| ad)
    }

    /// Start time of the last complete observation (`observedAt` in read models).
    pub fn observed_at_ms(&self) -> Option<i64> {
        self.last_complete.as_ref().map(|(_, at, _)| *at)
    }

    pub fn unavailable_reason(&self) -> Option<UnavailableReason> {
        match self.last_attempt {
            Some((_, _, AttemptKind::Unavailable(reason))) => Some(reason),
            _ => None,
        }
    }

    /// Source dimension against the last captured snapshot ref map (`None` when no
    /// snapshot exists yet, including before an empty repo's first empty manifest).
    pub fn source_state(
        &self,
        snapshot: Option<&RefMap>,
        now_ms: i64,
        stale_after_ms: i64,
    ) -> SourceState {
        let Some((_, _, attempt)) = self.last_attempt else {
            return SourceState::Unknown;
        };
        match attempt {
            AttemptKind::Unavailable(_) => SourceState::Unavailable,
            AttemptKind::Partial => SourceState::Unknown,
            AttemptKind::Complete => {
                let Some((_, at, advertisement)) = &self.last_complete else {
                    return SourceState::Unknown;
                };
                if now_ms.saturating_sub(*at) > stale_after_ms {
                    SourceState::Stale
                } else if snapshot == Some(&advertisement.refs) {
                    SourceState::Current
                } else {
                    SourceState::Changed
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
