# P06-T02 — Immutable plan and approval validation (lifecycle: Full)

Requirements: REQ-20, REQ-21. Scenarios: TEST-PLAN-LEASE, TEST-SNAPSHOT (gate), TEST-NO-DELETE-ON-404 (planning).

## Changed files (all in scope)
- `crates/domain/src/plan.rs` (+ tests): `OperationKind` (create, fast_forward, rewrite,
  move_tag, delete_ref), `RefOperation` with lease (`expected_destination`, `None` =
  must be absent), `classify(source, destination, last_verified, ancestry)`:
  any destination ref differing from the last verified state → `DestinationDrift`
  (whole plan blocked; a never-written destination must be empty); branch ancestry
  from an `AncestryOracle`, unknown → rewrite; tags never fast-forward; deletes only
  for refs the app verified and the source no longer has; object formats must agree.
- `crates/application/src/plan.rs` (+ tests):
  - `build_plan`: only when source state is `Current` (else `SourceNotCurrent`, so
    unavailable/404/partial/changed/stale never plans or deletes); re-checks the
    snapshot digest; binds source/destination digests, policy revision, mapping,
    created/expiry and operations into `plan_digest` (length-prefixed SHA-256).
    Destructive → requires approval + pre-change snapshot; non-destructive requires
    approval unless policy auto-apply.
  - `approve_plan`: recomputes the digest; rejects expired plan, renderer digest
    mismatch, policy revision mismatch; approval expiry capped by plan expiry.
  - `authorize_apply` (last gate before writing): recomputes digest; plan expiry;
    policy revision; fresh complete source and destination digests must equal the
    plan's; approval bound to plan id + digest + policy revision and unexpired;
    destructive or auto-off plans need an approval; destructive kinds go through
    `require_recovery` with kinds derived by `classify` (closes the P05-T03 follow-up
    "derive ChangeKind").

## Evidence
- checks.log: fmt 0, clippy -D warnings 0, workspace tests 0 (domain plan 8,
  application plan 8).
- negative-controls.log: N1–N17 each caught.

## Limitations
- The production `AncestryOracle` (git merge-base on the mirror) and the writer that
  enforces leases are P06-T03. Persistence of plans/approvals (SQLite) and the IPC
  commands are wiring tasks.
- Plan/approval IDs are supplied by the caller (native random generation at wiring).

## Round 2
Review findings 1–6 fixed (destination identity, managed-state replay guard, 24h TTL cap,
contract id pattern, foreign recovery rejection, clock-edge tests). checks.log round 2:
fmt 0, clippy 0, workspace tests 0 (application 84). negative-controls.log: N1–N22 caught.
