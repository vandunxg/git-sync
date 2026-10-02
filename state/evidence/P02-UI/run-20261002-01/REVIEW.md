# Independent review — P02-UI candidate (2026-10-02)

Reviewer: independent review subagent (owner-designated 2026-10-02). Not a visual approval.

Verdict: ACCEPTED (conditional) — no high-severity finding. Checks: boundaries (no production import of
src/test, Tauri only in lib/native, no timers/randomness), no fabricated success, PAT handling, plan approval
gating (explicit acknowledgement, expectedPlanDigest/expectedPolicyRevision, no bulk auto-approval), escaped
log rendering, BASELINES.json hashes match files and status is `reference_provided`.

Findings and resolution:
- M1 PAT kept in TanStack MutationCache variables for 5 min -> FIXED: mutations gcTime 0 (src/app/App.tsx).
- M2 "Review changes" could queue a duplicate plan when the waiting job is beyond the first jobs page ->
  FIXED: usePendingPlan exposes `complete`; button disabled with reason when lookup incomplete.
- L3 "Reconnect GitLab" hardcoded for AUTH_REQUIRED -> FIXED: provider-specific only when destination inaccessible.
- L4 repository Logs tab did not neutralize control characters -> FIXED: shared visibleLogText (lib/format.ts).
- L5 invented ETA wording -> FIXED: "estimate reported by Git".
- L7 sidebar wrote a browser cookie -> FIXED: cookie write removed.
- L1 (sync-selected counts may use snapshot), L2 (two eligibility sets, intentional: bulk excludes synced),
  L6 (derived "% of tracked"), L8 (plan expiry re-evaluated on render only; native rejects) -> OPEN, low.

Post-fix verification: see checks.log (typecheck, lint, 115 unit, 37 e2e, build, no-fakes PASS).
