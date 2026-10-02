//! Source ref observation use case (REQ-06, REQ-07, REQ-08).
//!
//! The only source port is `SourceRefReader`, which can *read* an advertisement and
//! nothing else: there is deliberately no push/write method anywhere on the source
//! side (one-way authority). The production reader runs `git ls-remote` through the
//! secure process runner (P05-T01); this module only maps its result into a domain
//! observation, failing closed on anything it cannot fully trust.

use gbm_domain::observation::{ObservationOutcome, SourceObservation, UnavailableReason};
use gbm_domain::refs::{RefError, parse_ls_remote};

/// Sanitized reader failures (no stderr, URLs or credentials).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    AccessDenied,
    NotFound,
    Transport,
    /// The bounded output limit was hit; the advertisement is incomplete.
    Truncated,
}

/// Read-only access to a source repository's ref advertisement.
pub trait SourceRefReader: Send + Sync {
    /// Returns the raw, complete `git ls-remote --symref` output for `repository_id`.
    /// `Ok` means Git exited 0, the output was not cut by the size limit and is valid
    /// UTF-8; `Ok("")` is only a successful empty listing. Anything else is an `Err`.
    fn read_advertisement(&self, repository_id: &str) -> Result<String, ReadError>;
}

/// Observes one source. `sequence` must be allocated (monotonically, per repository)
/// before calling, and `started_at_ms` taken at the same moment, so a slow call that
/// finishes late is ordered by when it started.
pub fn observe_source(
    reader: &dyn SourceRefReader,
    repository_id: &str,
    sequence: u64,
    started_at_ms: i64,
) -> SourceObservation {
    let outcome = match reader.read_advertisement(repository_id) {
        Ok(text) => match parse_ls_remote(&text) {
            Ok(advertisement) => ObservationOutcome::Complete(advertisement),
            Err(RefError::Truncated) => ObservationOutcome::Partial,
            Err(_) => ObservationOutcome::Unavailable(UnavailableReason::Unsupported),
        },
        Err(ReadError::Truncated) => ObservationOutcome::Partial,
        Err(ReadError::AccessDenied) => {
            ObservationOutcome::Unavailable(UnavailableReason::AccessDenied)
        }
        Err(ReadError::NotFound) => ObservationOutcome::Unavailable(UnavailableReason::NotFound),
        Err(ReadError::Transport) => ObservationOutcome::Unavailable(UnavailableReason::Transport),
    };
    SourceObservation {
        sequence,
        observed_at_ms: started_at_ms,
        outcome,
    }
}

#[cfg(test)]
mod tests;
