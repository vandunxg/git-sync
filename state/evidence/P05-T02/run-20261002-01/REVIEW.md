# P05-T02 review record

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
- M #1 failed promotion could leave a partial snapshot namespace → guard removes the
  whole target on any failure; copy --atomic per batch (N8).
- M #2 cancellation misreported as source unavailable; staging leaked after cancel →
  Cancelled propagated; cleanup with uncancelled token; staging_removed reported.
  Real-git test also exposed runner running git with a pre-cancelled token → fixed in
  runner (N10, N11).
- L #3 fetch failure not retried → re-observe and retry (N9).
- L #4 delete accepted any refs/ name → restricted to app namespaces (N12).
- L #5 loose stderr classification → anchored Git phrases.
- L #6 real-git coverage gaps → 237-ref, corrupted object, cancel, wrong-OID tests.

## Round 2 — ACCEPTED
Residuals: (a) bare `refs/gbm/snapshots/` accepted by the namespace guard — fixed
after round 2 (id segment required; test asserts remove_namespace of all snapshots is
refused). (b) staging left behind if cleanup itself fails on an error path is not
surfaced → orphaned-staging GC to be added in P05-T03 or later.
