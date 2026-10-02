# P07-T01 — Persistent queue and policy scheduler (lifecycle: Light)

Requirements: REQ-09, REQ-19. Scenarios: TEST-SCHEDULER, TEST-RETRY (classification and
backoff part; process timeouts/reaping are covered by the runner tests of P05-T01).

## Changed files
- `crates/application/src/scheduler.rs` (+ tests): `classify(ErrorCode)` into
  transient / actionable / safety / unsupported; `retry_decision` (only transient
  errors retry; exponential backoff with equal jitter, Retry-After honoured, max
  attempts and deadline; auth/review errors → NeedsOwner, never a retry storm);
  `SystemJitter` (std RandomState, OS-keyed); `select_starts` (pause starts nothing,
  global concurrency 1–8, one job per repository and per destination mapping, FIFO,
  retry not-before); `idempotency_key` over kind, repository, basis, mapping, policy
  revision.
- `crates/application/src/time.rs`: UTC RFC 3339 ↔ Unix ms (no dependency).
- `crates/infrastructure/src/sqlite/jobs.rs` (+ tests): `NewJob.mapping_id`,
  `not_before`; mapping lock enforced in `transition_job`; `enqueue_or_existing`
  (dedupe on non-terminal same key, race → existing); `queued_jobs`, `active_jobs`;
  `enqueue_retry` (only from failed; new attempt row, same key and scope; failed
  attempt kept).
- `crates/infrastructure/migrations/0002_scheduler.sql` (scope exception, see
  coordinator-approval.md); migration tests moved their synthetic upgrades to 0003.

## Evidence
- TEST-SCHEDULER with real SQLite: 100 repositories, 10 duplicate requests (→ existing
  job), concurrency 3 never exceeded, pause starts nothing and leaves running jobs as
  they are, shared mapping lock (scheduler and store), retry attempt waits for its time.
- checks.log: fmt 0, clippy 0, workspace tests 0. negative-controls.log: N1–N10 caught
  (N3, N9 after adding the tests named there).

## Limitations
- The loop that calls `select_starts` and runs jobs, and the pause flag's persistence,
  are wired in P07-T02/T03 (job runner, lifecycle, IPC).

## Review fixes (rounds 1–2)
Transactional dedupe with linked attempts, no automatic re-run of a failed logical job
(`NeedsOwner`), stage-aware retries with a pre-write allow-list (scheduler and store),
digit-only fractions, canonical `not_before` (fail closed), store-level `start_job`
(pause + capacity in the lock transaction), interrupted jobs keep their locks.
negative-controls.log: N1–N18 (N10 is a redundant layer behind a unique index).
