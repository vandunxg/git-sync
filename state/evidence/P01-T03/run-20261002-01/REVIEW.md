# Independent review - P01-T03 (reproducible quality harness and fixtures)

Reviewer: independent review subagent designated by owner 2026-10-02. Base 80441f5, dirty tree plus untracked files. Date 2026-10-02.

## Outcome: CHANGES_REQUIRED (no blocking defect in code; evidence/traceability gaps below must be fixed or explicitly accepted by the owner)

## Commands re-run (cwd, exit)
| Command | cwd | Exit |
|---|---|---|
| pnpm run test:unit (2 files, 80 tests) | apps/desktop | 0 |
| pnpm run typecheck | apps/desktop | 0 |
| pnpm run lint | apps/desktop | 0 |
| cargo test -p gbm-test-fixtures --locked (7 passed) | root | 0 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | root | 0 |
| pnpm --dir apps/desktop build | root | 0 |
| python3 quality/checks/no_fakes_dist.py (3 files, 0 hits) | root | 0 |
| python3 scripts/run_gates.py --profile core (scratch evidence dir) | root | 0 |
| python3 scripts/run_gates.py --profile frontend (scratch evidence dir) | root | 0 |
| python3 scripts/check_task_scope.py --task P01-T03 --base 80441f5 | root | 1 (status FAIL, expected: the 3 out-of-allowlist paths) |

No leftover gbm-fixture-* dirs in /tmp after tests (cleanup works). Nothing outside state/evidence/P01-T03/ changed by me.

## Findings

### F1 (Medium) - Out-of-allowlist edits: approval is claimed, not recorded
The 3 paths (Cargo.lock, apps/desktop/pnpm-lock.yaml, deleted contract-cases.selftest.ts) are technically needed and low risk:
- Cargo.lock: +4 lines, only the new workspace member, no external crates. Verified in diff.
- pnpm-lock.yaml: vitest 5.0.2 and its tree only.
- selftest deletion: replaced by an equivalent, stricter Vitest suite (all defs x valid/invalid, plus 11 envelope cases vs 7 before; 1619 cases). P01-T02 report explicitly carried this forward to P01-T03.
But R-CORE requires "coordinator approval recorded in its evidence". TASK_REPORT only asserts "Coordinator-approved"; no approval record (who/when/text) exists in evidence. Also the deletion is staged in the index ("D ") while everything else is unstaged - harmless but inconsistent. Fix: add the owner/coordinator approval reference to the evidence (or a change record), and unstage or note it.
Note: GATES.json is NOT out of allowlist (quality/ is allowed).

### F2 (Low) - GATES.json change is a legitimate correction, not a weakening
Diff is exactly 4 `pnpm-lock.yaml` -> `apps/desktop/pnpm-lock.yaml` replacements plus a top-level `lockfileNote` key. The lockfile really is at apps/desktop (no root pnpm-lock.yaml exists; DECISIONS RD-001, STACK_AND_VERSION_POLICY, P01-T01). The old path would make every profile permanently BLOCKED for a wrong reason. No command, profile, threshold or blocker removed. run_gates tolerates the extra key (runs PASS). Acceptable. Suggest the owner confirm since GATES.json is a quality-contract file.

### F3 (Medium) - no_fakes_dist.py is not wired into any gate or script
grep shows no reference in GATES.json, package.json or run_gates. The "frontend PASS" gate therefore does not execute the bundle check, and TASK_REPORT's "TEST-NO-FAKES PASS" for the build level rests on a manual run. Also the script scans whatever is in dist without checking it was built from the current tree (stale dist passes). Fix: add to the frontend profile (after build) or a `test:nofakes` package script, and fail/rebuild on stale dist. Without that, reproducibility claim (task outcome) is only partly met.

### F4 (Medium) - no-fakes false-pass gaps
Source test (src/test/policy/no-fakes.test.ts):
- Import regex `from\s+["'][^"']*(/test/|fixtures|generated/cases)` misses side-effect imports (`import "./fixtures"`), dynamic `import("../test/x")` and `require`. Only static `from` forms are caught.
- Timer regex requires a call `setTimeout(`; `globalThis["setTimeout"](`, aliasing (`const t = setTimeout`), `requestAnimationFrame`-based fake progress and `new Promise(r => ...)` fake delays are not caught. Acceptable as a heuristic but it is documented as proof of "no fake timers-as-progress"; it is not.
- Glob excludes `/src/test/**` and `*.test.*` entirely, so a production file placed under src/test or named *.test.ts is invisible; no check that nothing production imports from src/test beyond the same regex.
- Only checks "adapter wiring" nominally; report correctly says that part re-applies later.
Dist test (no_fakes_dist.py):
- Markers include identifiers (`DebugPanel`, `trackedInvoke`, `greet`) that a minifier mangles; only string literals (`ctui:debug`, `Project ready!`, case labels like `maxLength-astral`) are reliable. The negative control used a string literal, so it does not prove the identifier markers work. Dead-code elimination also drops unreferenced cases module, so the "cases" markers only fire if it is actually reachable.
- Only .js/.css/.html scanned (no .map, .json, .wasm); `http://` only checked in HTML, and `https://` (non-cdn/fonts.googleapis) never.
- No timer/Math.random check at bundle level (libraries legitimately contain them, so source test is the only guard).
Severity Medium because the checks are real (negative controls in controls.log show both fail when a targeted fake is injected; I verified the logic by reading) but the report's PASS wording overstates coverage. Reword as "heuristic guard" and add dynamic-import/side-effect patterns.

### F5 (Low) - Git fixture isolation: sound, minor hardening
Verified in lib.rs: env_clear then only PATH/HOME(private)/LC_ALL/GIT_CONFIG_NOSYSTEM/GIT_CONFIG_GLOBAL=/dev/null/GIT_TERMINAL_PROMPT=0 plus fixed identities and dates; `-c credential.helper=` and `core.hooksPath=/dev/null` on every call; every call has an explicit cwd inside the sandbox; no git child can read ~/.gitconfig, credential helpers or XDG config (GIT_CONFIG_GLOBAL overrides both; HOME is private). The isolation self-test asserts `config --show-origin` shows only repo and command-line sources. Writes stay in the sandbox (clone/commit/lfs install --local only touch sandbox repos). Drop removes the tree (also on test panic/unwind); verified no leftovers.
Minor:
- Sandbox path is predictable (`/tmp/gbm-fixture-<label>-<pid>-<n>`) in a shared tmp and uses create_dir_all on a possibly pre-existing dir, then remove_dir_all on drop. Prefer an atomically created unique dir (create_dir, fail if exists, or tempfile). Low risk on a single-user workstation.
- `protocol.file.allow=always` is set globally for every call; fine for local clones, but note it is test-only.
- Determinism: timestamps advance 60 s per git call (including rev-parse), so OIDs depend on exact call sequence; the self-test proves two builds match, but any refactor of helper call counts silently changes OIDs. Acceptable; document it. Determinism across git versions is untested (only local git).
- init.templateDir default templates are still copied; harmless given hooksPath=/dev/null.

### F6 (Low-Medium) - BLOCKED honesty: correct, one untested path
- test:visual and test:native exit 2 with explicit BLOCKED messages (controls.log [3]); run_gates reports visual/security/native BLOCKED in the evidence reports; none counted as PASS. TASK_REPORT states this accurately.
- LFS: lfs_repo returns ToolUnavailable; the test prints BLOCKED to stderr but the cargo test itself reports "ok" (passed). In a pass/fail summary that is a PASS-looking line for a BLOCKED scenario. The report discloses it ("only proves the fixture reports unavailability") and does not claim LFS coverage, so no false claim, but the test should be `#[ignore = "BLOCKED: git-lfs missing"]`-style or the runner must surface it, otherwise the aggregate "7 passed" overstates. The git-lfs-present branch (lfs install/track/commit) was never executed here and is unverified code.
- security:check is `pnpm audit` only; Rust audit/deny still BLOCKED (deny.toml missing). Correctly left BLOCKED. Note pnpm audit needs network at gate time; a registry outage should surface as FAIL/BLOCKED, which pnpm's nonzero exit does.

### F7 (Low) - Dependency policy (R-DEPS) for vitest 5.0.2
Exact pin (no caret), devDependency only (no bundle/runtime cost), MIT (npm view), recent release (modified 2026-09-30), lockfile updated, all 80 tests run under it. R-DEPS also wants purpose, alternatives considered and a reviewer decision recorded: the report gives purpose and release date but no alternatives (e.g. node:test, jest) and no license/transitive review of the ~170 added lockfile lines. This review records the decision as acceptable for a dev-only dependency; add the alternatives line to the report.

### F8 (Low) - Report vs evidence
Claims match what I re-ran (core/frontend PASS, 7 fixture tests, 80 unit tests, dist PASS). Items to correct:
- scope-check.json status is "FAIL"; report says "scope-check.json lists exactly these", which is true, but the report should state the guard failed as expected and is waived only via F1 approval.
- controls.log [7] first lint run FAILED and was re-run after renaming files; disclosed in report - fine.
- "TEST-NO-FAKES: PASS" should read "PASS (heuristic, manual dist check; not gate-wired)" per F3/F4.
- No .github/ workflow added; honestly stated.
- TEST-CONTRACT-FIXTURES stays PARTIAL; the Vitest side is real but TESTS.json still says NOT_RUN/NOT_IMPLEMENTED (coordinator's job).
- Hostile-ref test asserts the exact set Git accepts; it passes on this git version only (version-sensitive, e.g. `-option` handling). Low.

## Required before ACCEPTED
1. Record coordinator/owner approval for the 3 out-of-allowlist edits in evidence (F1).
2. Wire no_fakes_dist.py into the frontend gate or a package script and make it build-fresh (F3), or have the owner explicitly accept manual-only.
3. Reword TEST-NO-FAKES status as heuristic and extend regexes for side-effect/dynamic imports (F4).
4. Make the LFS BLOCKED outcome visible in test output status (F6), e.g. ignored test or explicit non-pass marker.
Optional: F5 atomic temp dir, F7 alternatives line.

## Verdict
Implementation is sound, within intent, and the BLOCKED paths are not counted as PASS in gate reports. Gaps are in evidence/traceability and in the strength and wiring of the no-fakes checks.

---
# Re-review (after run-20261002-02 fixes)

Re-run: `pnpm run test:unit` (apps/desktop) exit 0, 80 tests; `cargo test -p gbm-test-fixtures --locked` exit 0, 7 passed, 1 ignored (LFS); `python3 scripts/run_gates.py --profile frontend --repo-root . --evidence-dir <scratch>` exit 0 with lint, typecheck, test:unit, build, no-fakes-dist all PASS. `git diff 80441f5` and the run-02 evidence read.

| Finding | Status |
|---|---|
| F1 approval record | Resolved. run-02/coordinator-approval.md names the 3 paths and conditions; scope guard still exits 1, waived only via that record. It is a coordinator (not owner) approval, which R-CORE allows. Staged-deletion inconsistency remains cosmetic. |
| F3 dist check not wired | Resolved. `no-fakes-dist` is a frontend-profile step after `build`, so it scans a fresh bundle. Gate only adds a step plus a note; nothing removed. |
| F4 regex gaps | Resolved for the named cases (side-effect/dynamic imports, aliasing, string property access); 4 injection controls fail the suite and App.tsx was restored. Still a heuristic (e.g. computed/concatenated specifiers, `.test`-named production files); now documented as heuristic. Dist identifier-marker weakness is acknowledged, not fixed; accepted as Low. |
| F6 LFS | Resolved. Real LFS test is `#[ignore = "BLOCKED..."]`, shown as "1 ignored". The git-lfs-present branch is still unexecuted, honestly reported as BLOCKED. |
| F5 / F7 | Resolved. `create_dir` fails on a pre-existing path; vitest alternatives recorded. |

No new issues found.

## Final outcome: ACCEPTED
Residual limitations (non-blocking): no-fakes guards are heuristics; LFS fixture path, Rust audit/deny, visual and native profiles remain BLOCKED and must not be counted as PASS; TESTS.json statuses are the coordinator's to update.
