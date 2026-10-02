use gbm_domain::contract::RepositorySummarySourceState as SourceState;
use gbm_domain::observation::{Applied, SourceView};

use super::*;

/// Test-only reader returning a canned result; never compiled into production.
struct Canned(Result<String, ReadError>);

impl SourceRefReader for Canned {
    fn read_advertisement(&self, _: &str) -> Result<String, ReadError> {
        self.0.clone()
    }
}

fn oid(fill: char) -> String {
    fill.to_string().repeat(40)
}

#[test]
fn reader_results_map_to_honest_outcomes() {
    let cases = [
        (Err(ReadError::Truncated), ObservationOutcome::Partial),
        (
            Err(ReadError::AccessDenied),
            ObservationOutcome::Unavailable(UnavailableReason::AccessDenied),
        ),
        (
            Err(ReadError::NotFound),
            ObservationOutcome::Unavailable(UnavailableReason::NotFound),
        ),
        (
            Err(ReadError::Transport),
            ObservationOutcome::Unavailable(UnavailableReason::Transport),
        ),
        (
            Ok(format!("{}\trefs/heads/a b\n", oid('1'))),
            ObservationOutcome::Unavailable(UnavailableReason::Unsupported),
        ),
        (
            Ok(format!("{}\trefs/heads/ma", oid('1'))),
            ObservationOutcome::Partial,
        ),
    ];
    for (result, expected) in cases {
        let observation = observe_source(&Canned(result), "repo-1", 4, 1_000);
        assert_eq!(observation.outcome, expected);
        assert_eq!(
            (observation.sequence, observation.observed_at_ms),
            (4, 1_000)
        );
    }
}

#[test]
fn a_late_slow_observation_is_ordered_by_its_start() {
    // TEST-FRESHNESS through the use case: observation 1 started first but finished
    // after observation 2; applying it afterwards must not regress the view.
    let started_first = observe_source(
        &Canned(Ok(format!("{}\trefs/heads/main\n", oid('a')))),
        "repo-1",
        1,
        1_000,
    );
    let started_second = observe_source(
        &Canned(Ok(format!("{}\trefs/heads/main\n", oid('b')))),
        "repo-1",
        2,
        2_000,
    );
    let mut view = SourceView::default();
    assert_eq!(view.apply(started_second), Applied::Accepted);
    assert_eq!(view.apply(started_first), Applied::IgnoredOlder);
    assert_eq!(view.observed_at_ms(), Some(2_000));
    assert_eq!(view.source_state(None, 2_000, 60_000), SourceState::Changed);
}
