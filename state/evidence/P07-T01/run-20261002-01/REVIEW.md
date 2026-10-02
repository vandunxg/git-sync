# P07-T01 review record (lifecycle: Light)

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
1. High: `enqueue_or_existing` could return a terminal job and block re-runs; the check
   was outside the write transaction → one IMMEDIATE transaction, linked new attempt.
2. Medium: retries ignored the stage (push timeout retried) → stage-aware
   `retry_decision`; store `enqueue_retry` refuses non-pre-write stages and
   non-transient codes.
3. Medium: RFC 3339 fraction parsing could panic / accept garbage → digits only.
4. Medium: store accepted offsets the parser rejected (retry would start at once) →
   canonical UTC only; unreadable stored value → Corrupt.
5. Low: global concurrency/pause not enforced by the store → `start_job`.
6. Low: no store-level retry restrictions → added (rest carried as residual).
7. Low: interrupted jobs released locks → they hold repository/mapping locks.

## Round 2 — CHANGES_REQUIRED (1 Medium)
- A plain re-request after a failed attempt re-ran the job past the retry gate →
  `Enqueued::NeedsOwner(failed id)`; retries only via `enqueue_retry` or an owner
  action; retry stages are an allow-list (`PRE_WRITE_STAGES`, unknown stage never
  retried). Fixed exactly as suggested, with tests and caught controls N18, N13, N16
  (Light rule: no further round).

## Residual risks (all Low; batched for the end-of-phase P07 owner decision)
- R1: `select_starts` counts interrupted jobs against capacity (`start_job` does not):
  many unreconciled interrupted jobs stall the queue until P07-T02 reconciliation.
- R2: retries are `queued` + `not_before`, not `waiting_for_retry` (UI shows queued).
- R3: `enqueue_retry` copies `plan_id`; approval/policy are revalidated at apply (P06).
- R4: the explicit mapping check is backed only by the unique index (control N10).
- R5: pause flag persistence and the production scheduling loop are P07-T02/T03.
- R6: 403 secondary rate limits map to AccessDenied in the provider layer (no retry).
- R7: no real v1→v2 upgrade test with job rows (nullable ALTERs only).

Owner decision: ACCEPTED 2026-10-02 (end-of-phase P07 batch).
