use super::*;
use crate::refs::parse_ls_remote;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MINUTE: i64 = 60_000;
const STALE_AFTER: i64 = 60 * MINUTE;

fn complete(
    sequence: u64,
    at: i64,
    text: &str,
) -> Result<SourceObservation, Box<dyn std::error::Error>> {
    Ok(SourceObservation {
        sequence,
        observed_at_ms: at,
        outcome: ObservationOutcome::Complete(parse_ls_remote(text)?),
    })
}

fn line(fill: char, name: &str) -> String {
    format!("{}\t{name}\n", fill.to_string().repeat(40))
}

#[test]
fn no_observation_is_unknown_not_healthy() {
    let view = SourceView::default();
    assert_eq!(
        view.source_state(None, 0, STALE_AFTER),
        SourceState::Unknown
    );
}

#[test]
fn an_older_result_arriving_late_cannot_regress_the_view() -> TestResult {
    // TEST-FRESHNESS: newer observation first, then an older one finishes.
    let mut view = SourceView::default();
    let newer = complete(2, 10 * MINUTE, &line('b', "refs/heads/main"))?;
    let older = complete(1, 5 * MINUTE, &line('a', "refs/heads/main"))?;
    assert_eq!(view.apply(newer.clone()), Applied::Accepted);
    assert_eq!(view.apply(older), Applied::IgnoredOlder);
    // Replaying the same sequence is also ignored.
    assert_eq!(view.apply(newer.clone()), Applied::IgnoredOlder);
    let ObservationOutcome::Complete(expected) = newer.outcome else {
        return Err("fixture".into());
    };
    assert_eq!(view.last_complete(), Some(&expected));
    assert_eq!(view.observed_at_ms(), Some(10 * MINUTE));
    Ok(())
}

#[test]
fn partial_and_failed_attempts_keep_the_last_map_but_not_the_claim() -> TestResult {
    // TEST-FRESHNESS: partial -> unknown, failure -> unavailable; both distinct from
    // stale, and neither erases the last complete map nor reports current.
    let mut view = SourceView::default();
    let first = complete(1, 0, &line('a', "refs/heads/main"))?;
    let ObservationOutcome::Complete(ad) = first.outcome.clone() else {
        return Err("fixture".into());
    };
    view.apply(first);
    let snapshot = ad.refs.clone();
    assert_eq!(
        view.source_state(Some(&snapshot), MINUTE, STALE_AFTER),
        SourceState::Current
    );

    view.apply(SourceObservation {
        sequence: 2,
        observed_at_ms: 2 * MINUTE,
        outcome: ObservationOutcome::Partial,
    });
    assert_eq!(
        view.source_state(Some(&snapshot), 2 * MINUTE, STALE_AFTER),
        SourceState::Unknown
    );
    assert_eq!(view.last_complete(), Some(&ad));

    view.apply(SourceObservation {
        sequence: 3,
        observed_at_ms: 3 * MINUTE,
        outcome: ObservationOutcome::Unavailable(UnavailableReason::NotFound),
    });
    assert_eq!(
        view.source_state(Some(&snapshot), 3 * MINUTE, STALE_AFTER),
        SourceState::Unavailable
    );
    assert_eq!(view.unavailable_reason(), Some(UnavailableReason::NotFound));
    assert_eq!(view.last_complete(), Some(&ad), "404 never deletes the map");
    Ok(())
}

#[test]
fn old_complete_observations_become_stale_not_current() -> TestResult {
    let mut view = SourceView::default();
    let obs = complete(1, 0, &line('a', "refs/heads/main"))?;
    let ObservationOutcome::Complete(ad) = obs.outcome.clone() else {
        return Err("fixture".into());
    };
    view.apply(obs);
    assert_eq!(
        view.source_state(Some(&ad.refs), STALE_AFTER + 1, STALE_AFTER),
        SourceState::Stale
    );
    Ok(())
}

#[test]
fn changed_until_a_matching_snapshot_exists() -> TestResult {
    let mut view = SourceView::default();
    view.apply(complete(1, 0, &line('a', "refs/heads/main"))?);
    // No snapshot yet: the source needs capture.
    assert_eq!(
        view.source_state(None, 0, STALE_AFTER),
        SourceState::Changed
    );
    // Snapshot of an older map: changed.
    let old = parse_ls_remote(&line('b', "refs/heads/main"))?;
    assert_eq!(
        view.source_state(Some(&old.refs), 0, STALE_AFTER),
        SourceState::Changed
    );
    Ok(())
}

#[test]
fn empty_repository_lifecycle() -> TestResult {
    // TEST-EMPTY-REPO (domain): empty is a complete observation; current only once an
    // explicit empty snapshot exists; the first commit is then detected.
    let mut view = SourceView::default();
    view.apply(complete(1, 0, "ref: refs/heads/main\tHEAD\n")?);
    assert_eq!(
        view.source_state(None, 0, STALE_AFTER),
        SourceState::Changed
    );
    let empty = RefMap::default();
    assert_eq!(
        view.source_state(Some(&empty), 0, STALE_AFTER),
        SourceState::Current
    );
    view.apply(complete(2, MINUTE, &line('c', "refs/heads/main"))?);
    assert_eq!(
        view.source_state(Some(&empty), MINUTE, STALE_AFTER),
        SourceState::Changed
    );
    Ok(())
}
