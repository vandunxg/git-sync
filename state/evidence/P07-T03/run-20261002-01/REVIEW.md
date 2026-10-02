# P07-T03 review record (lifecycle: Light)

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — APPROVED with 2 Medium (fixed as suggested, with tests; Light rule: no round 2)
1. Medium: a committed change with no readable revision emitted nothing and consumed no
   sequence (no gap) → `EventPublisher::skip()` consumes a sequence (control E9).
2. Medium: plan/log read models not invalidated by events → `["plan"]` on
   catalog/job changes, `["logs"]` on job changes (control E10).

## Low (batched for the end-of-phase P07 owner decision)
- M1: `keys.queuePaused` is local state (skipToken), not a native read model; a pause from
  elsewhere or a restart is not resynced (no paused-state query in the contract).
- M2: concurrent publishes can emit seq n+1 before n → treated as a gap (full re-query).
- M3: a handler that commits then returns Err emits nothing (mutation hooks invalidate).
- M4: `keepNewestRevision` guards `useNativeQuery` only, not paged/infinite queries.
- M5: if the database were replaced by one with a lower revision within one renderer
  session, the guard would keep old data until reload.
- M6: background job progress/terminal events await the scheduler loop.
