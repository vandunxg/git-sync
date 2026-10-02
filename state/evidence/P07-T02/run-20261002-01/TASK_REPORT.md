# P07-T02 — Cancel, shutdown and restart reconciliation (lifecycle: Full)

Requirements: REQ-10, REQ-19, REQ-44. Scenarios: TEST-CRASH, TEST-PROCESS-CANCEL,
TEST-LIFECYCLE (native-shell parts are P10-T02). Scope exception: coordinator-approval.md.

## Changes
- `crates/application/src/reconcile.rs` (+ tests):
  - `CheckpointRecord` (create intent incl. marker, or plan/snapshot ids; strict decode;
    never argv or secrets) stored in `job_checkpoints` before each effect starts.
  - `reconcile_interrupted(stage, checkpoint, destination_now, interrupted_for_ms)`:
    pre-write stage without checkpoint → new attempt; create checkpoint → new attempt
    re-running the same persisted intent (lookup first, no duplicate); push checkpoint →
    destination = snapshot → verified complete (to be recorded via `record_attempt`),
    = plan's start → new attempt, else partial/review; unobservable → keep interrupted
    until `RECONCILE_DEADLINE_MS` (24 h), then uncertain; write stage without checkpoint
    → uncertain at once (fail `VERIFY_FAILED`, locks released, owner decides).
  - `Reconciliation::closing()` maps every decision to how the attempt is closed.
  - `cancel_result(stage, checkpoint, observation)`: "Canceled" only when confirmed
    stopped *and* no write may have started (pre-write stage, no checkpoint), also for
    a cancel before the next child starts; otherwise `CANCEL_UNCERTAIN`.
  - `scan_due`: one coalesced scan; a backwards clock makes a scan due.
- `crates/infrastructure/src/sqlite/jobs.rs` + migration 0003: `save_checkpoint` (running
  attempt only, ≤ 4 KiB, known kinds), `checkpoint`, `resolve_interrupted` (one IMMEDIATE
  transaction: close with reconciled outcome, expire the attempt's plan and approvals,
  optionally queue a plan-less linked attempt carrying a create intent). Generic
  `transition_job` can no longer move an interrupted row; `interrupted -> queued/running`
  edges removed.
- `crates/infrastructure/src/platform/mod.rs`: strict `parse_uptime_ms`,
  `SuspendDetector` (BOOTTIME − MONOTONIC) with `SUSPEND_THRESHOLD_MS` (5 s) and
  `resumed()`.

## Evidence
- TEST-CRASH: decision level (killed before/after/during push, third-party changes,
  deadline, create without duplicate) and through the real SQLite store
  (`sqlite::tests::reconcile_store`: running → checkpoint → restart recovery → decode →
  close; the stale plan and approval expire, the interrupted row cannot be re-queued or
  run, the new attempt has no plan, create intent carried over with the same marker,
  uncertain job releases its locks).
- TEST-PROCESS-CANCEL: real child tree cancel/drain/reap is the P05-T01 runner test
  `cancel_stops_a_flooding_tree_with_helper_and_drains_boundedly`; this task adds the
  honest terminal mapping.
- TEST-LIFECYCLE: coalesced scan on resume / clock change; no resume detected while awake.
- checks.log: fmt 0, clippy 0, workspace tests 0; infrastructure 153 tests ×3 green.
- negative-controls.log: N1–N18 caught except N17 (redundant layer; N17b with both
  guards removed is caught).

## Limitations / residual
- Shutdown and startup wiring (cancel + bounded wait + persist; recover → reconcile →
  start scheduler) live in the Tauri shell (P07-T03 / native wiring); a real
  kill-the-app test is P10-T01.
- An orphaned `git push` child can outlive an app crash (own process group, no
  PDEATHSIG); the per-ref lease and verification bound the effect.
