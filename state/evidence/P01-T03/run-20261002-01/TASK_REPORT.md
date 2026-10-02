# Task report - P01-T03

- Task / phase / requirement IDs: P01-T03 / P01 / REQ-37, REQ-43
- Validation kind: implementation. Base commit 80441f5. Host: EndeavourOS, XFCE/X11.
- Permitted: apps/desktop/package.json, apps/desktop/src/test/, crates/, quality/, .github/ (+ state/).

## Coordinator-approved out-of-allowlist edits (scope-check.json lists exactly these)

| Path | Change | Reason |
|---|---|---|
| apps/desktop/pnpm-lock.yaml | vitest 5.0.2 (exact) and its transitive deps | lockfile must follow package.json; lockfile owner = integrator (this session) |
| Cargo.lock | new workspace member gbm-test-fixtures (no new external crates) | follows from crates/ |
| apps/desktop/src/lib/native/contract-cases.selftest.ts | deleted | replaced by Vitest tests in src/test/native (P01-T02 carry-forward "move into Vitest") |

## Delivered

- Non-watch script contracts in apps/desktop/package.json (GATES.json `packageScripts`):
  `test:unit` = `vitest run`; `test:visual` and `test:native` exit 2 with an explicit BLOCKED reason
  (no approved baselines / no native harness, Hyprland unavailable); `security:check` = `pnpm audit
  --audit-level low`. Existing `lint`, `typecheck`, `build` unchanged.
- Vitest (5.0.2, published 2026-09-25; no release-age exclusion needed):
  - src/test/native/contract-cases.test.ts: per-definition oracle verdicts for all 1619 shared cases,
    coverage check (each def has valid and invalid cases), 11 malformed-envelope cases -> CONTRACT_MISMATCH.
  - src/test/policy/no-fakes.test.ts (TEST-NO-FAKES, source level): production sources (via Vite
    `import.meta.glob`, sanity-checked to be non-empty) must not import test/fixture/cases modules, use
    setTimeout/setInterval/Math.random, touch @tauri-apps outside lib/native, or contain debug/telemetry hooks.
- quality/checks/no_fakes_dist.py (TEST-NO-FAKES, build level): scans the built bundle for test-case labels,
  probe output, template debug/demo strings and remote code in HTML; BLOCKED (2) without a bundle.
- crates/test-fixtures (gbm-test-fixtures, test-only): isolated Git sandbox (env_clear, private HOME,
  GIT_CONFIG_NOSYSTEM, GIT_CONFIG_GLOBAL=/dev/null, reset credential.helper, hooks disabled, fixed identities
  and timestamps => deterministic OIDs). Builders: source repo (multiple heads incl. `release/1.x` and a
  non-ASCII branch, annotated + lightweight tags, non-fast-forward rewritten `feature`, moved tag), empty bare
  destination, dirty/ahead/detached workspaces, hostile ref-name list, LFS repo (returns ToolUnavailable).
  7 self-tests verify these properties, determinism and isolation from the owner's global Git config.
- quality/GATES.json: `pnpm-lock.yaml` prerequisite path -> `apps/desktop/pnpm-lock.yaml` (actual location,
  RD-001). No command, profile or threshold removed or weakened.

## Commands and results

`scripts/run_gates.py --repo-root .` (reports under gates-<profile>/, stdout in gate-<profile>.stdout.txt):
core PASS (fmt, clippy -D warnings, tests, doctests); frontend PASS (lint, typecheck, test:unit, build);
visual BLOCKED (baselines unapproved); security BLOCKED (missing deny.toml; cargo-audit/cargo-deny not
installed); native BLOCKED (no --allow-native / sandbox config).
controls.log: [1] injected setTimeout -> no-fakes test FAILS (1 failed), restored; [2] injected case label
into dist -> dist check FAIL exit 1, rebuild -> PASS; [3] test:visual / test:native exit 2; [4]
security:check exit 0 "No known vulnerabilities found" (read-only registry query); [5] cargo tree: 0
occurrences of gbm-test-fixtures in git-backup-manager and gbm-application normal deps; [6] fixture tests
7 passed; [7b] blueprint lint_pack + 33 tool tests PASS on a blueprint-only view (repo-root lint is the known
GAP-LINT-PACK-APP-TREE). A first view run failed because this run's own runner stdout captures were named
.json; renamed to .stdout.txt and re-run (kept in controls.log).

## Scenario status

- TEST-NO-FAKES: heuristic source + bundle guards PASS on current tree, with negative controls (see post-review section). Adapter-wiring part
  re-applies once production adapters exist.
- TEST-CONTRACT-FIXTURES: PARTIAL (unchanged: revision-consistent totals NOT_RUN; no data producers yet).
- LFS fixture: BLOCKED (git-lfs not installed). The self-test only proves the fixture reports unavailability.
- .github/: no CI workflow added - no GitHub remote exists to execute or verify it; not claimed.

## State: REVIEW_READY

## Post-review fixes -> run-20261002-02 (review: CHANGES_REQUIRED)

1. Approval record: run-02/coordinator-approval.md. Scope guard result stated plainly: check_task_scope
   exits 1 (FAIL) listing exactly the 3 paths; they are waived only through that approval.
2. no_fakes_dist.py wired into the frontend gate as `no-fakes-dist`, after `build` in the same profile, so it
   always scans a fresh bundle (run-02 frontend gate: lint, typecheck, test:unit, build, no-fakes-dist PASS).
   Scanner also covers .mjs/.json/.svg/.txt/.map.
3. TEST-NO-FAKES is a heuristic guard (not a proof). Source regexes now catch side-effect and dynamic imports
   and any mention of setTimeout/setInterval/Math.random (aliasing, string property access). run-02 controls:
   4 injections (`import "./test/x.ts"`, `import("../fixtures/y.ts")`, timer aliasing,
   `globalThis["setTimeout"]`) each fail the suite; App.tsx restored. Minifier-mangled identifiers are not
   reliable dist markers; string literals are.
4. LFS: the real LFS test is `#[ignore = "BLOCKED: requires git-lfs (NEED-GIT-LFS)"]` (cargo shows
   "1 ignored"); a separate test only checks that the fixture's unavailability report matches `git lfs version`.
   Sandbox root is created with `create_dir` (fails if the path exists). OIDs are reproducible per builder
   sequence and Git version (clock advances per Git call); cross-version determinism is not claimed.
5. R-DEPS alternatives for the unit runner: Jest (needs separate TS/ESM transform config and would not share
   Vite aliases/`import.meta.glob`), node:test (no Vite transforms, cannot load `?raw` globs or `@/` aliases).
   Vitest reuses vite.config.ts; cost: dev-only, ~40 transitive packages, MIT.
