//! TEST-SCHEDULER (admission) and TEST-RETRY (classification and backoff) policy tests;
//! the durable queue is tested with real SQLite in infrastructure/src/sqlite.

use super::*;

struct Fixed(f64);

impl Jitter for Fixed {
    fn unit(&self) -> f64 {
        self.0
    }
}

fn queued(id: &str, repo: &str, mapping: Option<&str>, sequence: i64) -> QueuedJob {
    QueuedJob {
        id: id.to_owned(),
        repository_id: Some(repo.to_owned()),
        mapping_id: mapping.map(str::to_owned),
        not_before_ms: None,
        sequence,
    }
}

#[test]
fn concurrency_repo_and_mapping_locks_and_fifo() {
    let jobs = vec![
        queued("j1", "r1", Some("m1"), 1),
        queued("j2", "r1", Some("m1"), 2), // same repo as j1
        queued("j3", "r2", Some("m1"), 3), // same mapping as j1
        queued("j4", "r3", Some("m3"), 4),
        queued("j5", "r4", Some("m4"), 5),
        queued("j6", "r5", Some("m5"), 6),
    ];
    assert_eq!(
        select_starts(&jobs, &[], false, 3, 0),
        vec!["j1", "j4", "j5"]
    );
    let active = vec![ActiveJob {
        id: "j1".to_owned(),
        repository_id: Some("r1".to_owned()),
        mapping_id: Some("m1".to_owned()),
    }];
    let rest: Vec<QueuedJob> = jobs.into_iter().skip(1).collect();
    assert_eq!(select_starts(&rest, &active, false, 3, 0), vec!["j4", "j5"]);
    assert!(
        select_starts(&rest, &active, false, 1, 0).is_empty(),
        "full"
    );
}

#[test]
fn pause_starts_nothing_and_retries_wait_for_their_time() {
    let mut retry = queued("j1", "r1", None, 1);
    retry.not_before_ms = Some(1_000);
    let jobs = vec![retry, queued("j2", "r2", None, 2)];
    assert!(select_starts(&jobs, &[], true, 3, 0).is_empty());
    assert_eq!(select_starts(&jobs, &[], false, 3, 999), vec!["j2"]);
    assert_eq!(select_starts(&jobs, &[], false, 3, 1_000), vec!["j1", "j2"]);
}

#[test]
fn a_hundred_repositories_never_exceed_the_limit() {
    // TEST-SCHEDULER admission loop (pure): 100 repos + duplicates of 10, limit 3.
    let mut queue: Vec<QueuedJob> = (0..100)
        .map(|i| queued(&format!("j{i}"), &format!("r{i}"), None, i))
        .collect();
    for i in 0..10 {
        queue.push(queued(&format!("dup{i}"), &format!("r{i}"), None, 100 + i));
    }
    let mut active: Vec<ActiveJob> = Vec::new();
    let mut started_repos: Vec<String> = Vec::new();
    let mut rounds = 0;
    while !queue.is_empty() && rounds < 1_000 {
        rounds += 1;
        for id in select_starts(&queue, &active, false, 3, 0) {
            let Some(pos) = queue.iter().position(|j| j.id == id) else {
                continue;
            };
            let job = queue.remove(pos);
            assert!(
                !active.iter().any(|a| a.repository_id == job.repository_id),
                "two jobs on one repository"
            );
            started_repos.extend(job.repository_id.clone());
            active.push(ActiveJob {
                id: job.id,
                repository_id: job.repository_id,
                mapping_id: job.mapping_id,
            });
        }
        assert!(active.len() <= 3);
        // The oldest running job finishes.
        if !active.is_empty() {
            active.remove(0);
        }
    }
    assert!(queue.is_empty());
    assert_eq!(started_repos.len(), 110);
}

#[test]
fn only_transient_errors_retry_and_auth_never_storms() {
    let policy = DEFAULT_RETRY;
    for code in [
        ErrorCode::AuthRequired,
        ErrorCode::AccessDenied,
        ErrorCode::PushPartial,
        ErrorCode::VerifyFailed,
        ErrorCode::CancelUncertain,
        ErrorCode::PlanStale,
        ErrorCode::Unsupported,
    ] {
        assert_eq!(
            retry_decision(&policy, code, "fetch", 1, 0, 0, None, &Fixed(0.5)),
            RetryDecision::NeedsOwner,
            "{code:?}"
        );
    }
    assert!(matches!(
        retry_decision(
            &policy,
            ErrorCode::Timeout,
            "fetch",
            1,
            0,
            0,
            None,
            &Fixed(0.5)
        ),
        RetryDecision::RetryAt(_)
    ));
}

#[test]
fn backoff_is_bounded_jittered_and_honours_retry_after() {
    let policy = DEFAULT_RETRY;
    let at = |attempt, jitter| {
        retry_decision(
            &policy,
            ErrorCode::NetworkUnavailable,
            "fetch",
            attempt,
            0,
            0,
            None,
            &Fixed(jitter),
        )
    };
    // attempt 1: base 30s → [15s, 30s)
    assert_eq!(at(1, 0.0), RetryDecision::RetryAt(15_000));
    assert_eq!(at(1, 0.999), RetryDecision::RetryAt(29_985));
    // attempt 3: 120s → [60s, 120s)
    assert_eq!(at(3, 0.0), RetryDecision::RetryAt(60_000));
    // Budget exhausted.
    assert_eq!(at(4, 0.0), RetryDecision::GiveUp);
    // 429 with Retry-After longer than the backoff.
    assert_eq!(
        retry_decision(
            &policy,
            ErrorCode::RateLimited,
            "observe_source",
            1,
            0,
            0,
            Some(600),
            &Fixed(0.0)
        ),
        RetryDecision::RetryAt(600_000)
    );
    // Past the deadline.
    assert_eq!(
        retry_decision(
            &policy,
            ErrorCode::Timeout,
            "fetch",
            1,
            0,
            i64::try_from(policy.deadline_ms).unwrap_or(0),
            None,
            &Fixed(0.0)
        ),
        RetryDecision::GiveUp
    );
    // A huge Retry-After cannot extend beyond the deadline.
    assert_eq!(
        retry_decision(
            &policy,
            ErrorCode::RateLimited,
            "observe_source",
            1,
            0,
            0,
            Some(u64::MAX),
            &Fixed(0.0)
        ),
        RetryDecision::GiveUp
    );
}

#[test]
fn identity_keys_are_stable_and_distinguish_logical_jobs() {
    let base = JobIdentity {
        kind: "sync",
        repository_id: Some("repo-1"),
        basis: Some("snap-1"),
        mapping_id: Some("map-1"),
        policy_revision: 3,
    };
    let key = idempotency_key(&base);
    assert_eq!(key, idempotency_key(&base.clone()));
    assert!(key.starts_with("k-") && key.len() == 42);
    for other in [
        JobIdentity {
            kind: "plan",
            ..base.clone()
        },
        JobIdentity {
            repository_id: Some("repo-2"),
            ..base.clone()
        },
        JobIdentity {
            basis: Some("snap-2"),
            ..base.clone()
        },
        JobIdentity {
            mapping_id: None,
            ..base.clone()
        },
        JobIdentity {
            policy_revision: 4,
            ..base.clone()
        },
    ] {
        assert_ne!(idempotency_key(&other), key);
    }
}

#[test]
fn system_jitter_stays_in_the_unit_interval_and_varies() {
    let jitter = SystemJitter::new();
    let values: Vec<f64> = (0..200).map(|_| jitter.unit()).collect();
    assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
    let distinct: std::collections::HashSet<u64> = values.iter().map(|v| v.to_bits()).collect();
    assert!(distinct.len() > 190);
}

#[test]
fn one_repository_never_runs_twice_even_without_a_mapping() {
    let jobs = vec![
        queued("scan", "r1", None, 1),
        queued("sync", "r1", None, 2),
        queued("other", "r2", None, 3),
    ];
    assert_eq!(
        select_starts(&jobs, &[], false, 3, 0),
        vec!["scan", "other"]
    );
}

#[test]
fn a_timeout_after_a_write_may_have_started_is_never_retried_blindly() {
    for stage in MUTATING_STAGES.into_iter().chain(["unknown-stage"]) {
        assert_eq!(
            retry_decision(
                &DEFAULT_RETRY,
                ErrorCode::Timeout,
                stage,
                1,
                0,
                0,
                None,
                &Fixed(0.5)
            ),
            RetryDecision::NeedsOwner,
            "{stage}"
        );
    }
}
