# Coordinator approval - out-of-allowlist edits for P01-T03

Approver: coordinator (Claude Code session acting under owner decisions OD-001/OD-002, 2026-10-02).
Basis: R-CORE "A task may touch a file outside its path allowance only after coordinator approval
recorded in its evidence"; COORDINATION.md (integrator owns lockfiles).

| Path | Approved change | Condition |
|---|---|---|
| apps/desktop/pnpm-lock.yaml | add vitest 5.0.2 and transitive deps only | `pnpm install --frozen-lockfile` passes; no release-age exclusions |
| Cargo.lock | add workspace member gbm-test-fixtures only | no new external crates (`cargo tree` shows none) |
| apps/desktop/src/lib/native/contract-cases.selftest.ts | delete | replaced by src/test/native/contract-cases.test.ts (P01-T02 carry-forward) |

The same approval form applies retroactively to P01-T02's three exceptions, recorded in
state/IMPLEMENTATION.json (scopeExceptions) and its TASK_REPORT. This is not owner approval of any
visual, security-sensitive or destructive policy.
