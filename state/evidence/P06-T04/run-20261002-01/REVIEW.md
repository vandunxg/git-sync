# P06-T04 review record (lifecycle: Light — one review round)

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED (2 Medium, fixed per reviewer suggestions with tests)
1. Medium: a verified Git backup was recorded as complete although LFS/submodule scope
   was not proven → `SyncRecord.git_result` (Git-ref scoped) + `lfs`/`submodules`
   coverage carried; `job_result` is Partial (LfsIncomplete / SubmoduleUncovered) until
   both are Complete (N9).
2. Medium: an unobserved source re-check looked like "unchanged" → `SourceRecheck
   { Same, Newer, NotObserved }` (N10).
Light rule: no round 2 because the fixes follow the reviewer's suggested fixes exactly
and each has a test and a caught negative control.

## Residual risks (all Low; batched for the end-of-phase P06 owner decision)
- R1 Low: on a store `RevisionConflict` the caller must re-read and rebuild the record
  (documented on the port; implemented at wiring, P07-T03).
- R2 Low: `CoverageSubmodules::PointersOnly` / `None` allow Synced; the submodule
  warning must be surfaced as a secondary warning and "not checked" never mapped to
  `None` (P07/P09 mappers).
- R3 Low: "complete observation" is a doc contract; wrap in a newtype at wiring.
- R4 Low: TEST-VERIFY is unit-level on synthetic ref maps here; the integration part
  (real push exiting 0 then re-read) is owed at wiring.
- R5 Low: after an uncertain push with an unobservable destination the next run blocks
  on drift and needs adoption (fails safe).
- R6 Low: managed state adopts third-party values equal to the snapshot (harmless).
- R7 Low: a mixed-format reconciliation yields `None` → treated as never written
  (fails safe as all-drift).

Owner decision: PENDING (end-of-phase P06 batch of Low risks).
