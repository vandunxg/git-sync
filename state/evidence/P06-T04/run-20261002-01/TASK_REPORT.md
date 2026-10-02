# P06-T04 — Verify ref maps and persist scoped results (lifecycle: Light)

Requirements: REQ-07, REQ-08, REQ-42, REQ-43. Scenarios: TEST-VERIFY, TEST-STATUS-AGGREGATION.

## Changed files (in scope)
- `crates/domain/src/status.rs` (+ tests): `verify_destination` (full ref map vs
  snapshot, incl. extra destination refs; no observation → Unverified); `sync_result`
  (success only when the destination equals the snapshot; Applied+mismatch → Failed
  VerifyFailed; Partial → PushPartial; stale lease → PlanStale; remote rejected →
  ProtectedRef; `newer_source` reported separately); `destination_state`;
  `primary_status` with precedence running → blocked → failed (unverified) → missing →
  update → stale/unknown → synced; workspace never affects it; LFS/submodule
  incomplete or mirror invalid block; synced requires every dimension verified.
- `crates/application/src/verify.rs` (+ tests): `record_attempt` builds a `SyncRecord`
  (result, error, git-refs coverage, verified_at only when verified, mismatched refs,
  newer source); `reconciled_managed_state` keeps refs written by this job or unchanged
  and keeps the old value where a third party changed it (so it remains drift);
  `ResultStore` port with revision-checked commit and `persist_attempt`.

## Evidence
- checks.log: fmt 0, clippy 0, workspace tests 0.
- TEST-STATUS-AGGREGATION: every combination (518,400) of source × mirror × destination
  × git refs × local objects × LFS × submodules × workspace/job/failed flags × blocker:
  deterministic, workspace-invariant, blockers never hidden, synced only when all
  verified, LFS incomplete never synced.
- negative-controls.log: N1–N8 each caught.

## Limitations
- TEST-VERIFY here is unit-level on synthetic ref maps; the integration part (real push
  exiting 0, then re-read) is owed at wiring (P07-T03).
- The SQLite `ResultStore`, events and read models are wiring tasks (P07-T03).
- LFS/submodule/workspace coverage values come from their own tasks (P05-T04, P09).

## Review fixes
`git_result` is Git-ref scoped; `job_result` stays Partial until LFS and submodule
scope are proven; `SourceRecheck::NotObserved` distinguishes an unobserved source.
Controls N9, N10 added and caught; gates re-run green.
