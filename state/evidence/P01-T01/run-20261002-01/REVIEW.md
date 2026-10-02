# Review - P01-T01 Scaffold audited Tauri/Vite workspace

- Reviewer: independent review subagent designated by the owner on 2026-10-02 (OD-002). Not the implementer.
- Base: 31b778f; subject: untracked worktree (all new files). Date: 2026-10-02.
- Outcome: **ACCEPTED** (no blocking defects; 3 medium/low follow-ups listed below, none require source change before DONE).

## Commands re-run by reviewer

| cwd | command | exit |
|---|---|---|
| apps/desktop | pnpm run typecheck | 0 |
| apps/desktop | pnpm run lint | 0 |
| apps/desktop | pnpm run build | 0 |
| apps/desktop | pnpm install --frozen-lockfile --offline | 0 (lockfile up to date) |
| . | cargo fmt --all -- --check | 0 |
| . | cargo clippy --workspace --all-targets --locked -- -D warnings | 0 |
| . | cargo test --workspace --all-targets --locked | 0 (3 passed in git_backup_manager_lib; 0 in other targets) |
| . | python3 scripts/check_task_scope.py --task P01-T01 --repo-root . --base 31b778f | 0, PASS_PATH_SCOPE_ONLY, outOfScope empty |
| apps/desktop | pnpm view <pkg> time (5 excluded packages; read-only registry metadata) | 0 |

No GUI launched. Screenshot launch-release.png inspected: window renders heading with Tailwind padding and bundled Inter, so script/style/font under the CSP loaded.

## Scaffold comparison (original scaffold vs apps/desktop)

Every claimed removal verified absent: debug-panel.tsx, debug-events.ts, lib/tauri.ts, theme-provider.tsx, external-link-guard.tsx, assets/react.svg, public/vite.svg (and the index.html link), README.md, bun.lock, .git, tauri-plugin-log, tauri-plugin-opener, serde/serde_json/log direct deps, `greet`, `core:default`/`opener:default`/`log:default`, `.dark` token block. No tauri-plugin-* crate in Cargo.lock; no telemetry libs; dist/index.html has no inline script/remote refs. Nothing unrequested found remaining in source. Differences from scaffold are exactly those listed in TASK_REPORT (plus `noUncheckedIndexedAccess`, exact pins, workspace lints).

## Focus areas

- **CSP / IPC (Tauri 2.12.1)**: `default-src 'self'; script-src 'self'; ... connect-src ipc: http://ipc.localhost` is the correct form for Tauri 2 IPC on Linux (ipc custom protocol, with http://ipc.localhost fallback). Built HTML has only an external module script and stylesheet, so Tauri adds no hash/nonce that would neutralise `'unsafe-inline'`. No command is registered and only `core:event:default` is granted, so IPC is not exercised by the shell; the connect-src value is unproven at runtime (see L1). Capability is narrow; `withGlobalTauri:false`; `freezePrototype:true`; devtools feature not enabled. Would not break the app (render confirmed in release build).
- **Navigation guard**: logic is sound and fails closed (unknown schemes, userinfo/lookalike hosts such as tauri.localhost.example.com, 127.0.0.1, file:, mailto: refused; localhost only under debug_assertions). Opens nothing externally. Tests cover those cases and pass. Residual (L2): localhost allowed on any port in debug; `tauri://` host not checked; `window.open`/new-window path is not handled by `on_navigation` (no links exist now).
- **Exact pins / lockfile**: package.json has no ranges (verified); Cargo.toml tauri 2.12.1 / tauri-build 2.7.1 are minimum-version requirements resolved to those exact versions by Cargo.lock (normal Cargo semantics; `--locked` used). pnpm-lock.yaml lockfileVersion 9, all resolutions registry+integrity (no git/tarball sources), frozen install passes. packageManager/engines pinned.
- **pnpm-workspace.yaml minimumReleaseAgeExclude** (see M1).
- **Scope**: all paths within allowlist (lockfile inside apps/desktop/ is covered by the `apps/desktop/` prefix; reasonable and explained). Cargo.lock/rust-toolchain at root as permitted. state/ evidence is the common permitted scope.
- **Scenario honesty**: TASK_REPORT states TEST-ENV PARTIAL (Hyprland BLOCKED, variants NOT_RUN) and bundle inspection NOT_RUN; this is honest. TESTS.json statuses must remain coordinator-owned.

## Findings

**M1 (medium, supply chain, owner awareness) - release-age cooldown bypassed for brand-new pins.** `minimumReleaseAgeExclude` lists @shadcn/registry@0.1.0, @types/node@24.19.1, globals@17.13.0, shadcn@4.21.1, vite@8.3.2. Registry timestamps (reviewer-verified): published 2026-10-01 03:57Z to 22:38Z, i.e. under ~28 h before the 2026-10-02 install. The policy was not disabled and the entries are version-specific (good), but these are exactly the packages the cooldown was meant to delay, and vite is the build toolchain. Not a blocker for a scaffold (all dev-time; lockfile integrity-pinned), but record an explicit decision (RD entry) that the owner accepts them, or re-pin to older releases and drop the excludes. Also note the exclusion-list-churn: remove entries once they age out.

**L1 (low, evidence gap) - IPC/CSP runtime coverage and launch evidence.** checks.log does not record the launch/window-visibility/SIGTERM commands with exit codes (launch-stdout/stderr are empty, window geometry claim rests on the report and screenshot). checks.log's cargo test excerpt shows only the 0-test main.rs target; the 3 guard tests are real (reviewer re-ran). The CSP's `ipc:` allowance is untested until P01-T02 adds a command; that task should include an invoke-under-CSP check. Dev-mode (devUrl localhost:3000, Vite HMR websocket) was not run; CSP behaviour in `tauri dev` is unverified.

**L2 (low) - guard hardening for later**: restrict debug localhost to port 3000, and add a deny-by-default new-window handler when any link/`window.open` appears.

**L3 (low, housekeeping)**
- TASK_REPORT is in English; AGENTS.md/CLAUDE.md require reporting to the owner in Vietnamese. Provide the Vietnamese handoff at coordinator hand-off.
- TASK_REPORT names TEST-TEMPLATE "PASS (for scope)"; TESTS.json obligation also covers release assets, and bundles were not inspected. Record as PARTIAL until P10 packaging inspection. The assertion script has no positive control (acknowledged).
- TOOLCHAIN.json notes shadcn 4.16.1 pinned inside the template, but the scaffold and lock use shadcn 4.21.1; reconcile when updating `resolvedDependencyProof` (coordinator, outside this task's scope).
- Unused template pieces remain: badge/tabs/tooltip/dropdown-menu primitives, lucide-react, @tauri-apps/api, default Tauri icon set. These are the declared audited primitive family and acceptable; confirm icon branding before release.
- `style-src 'unsafe-inline'` is a conventional concession for CSSOM/style attributes from base-ui; revisit at the security profile gate.

## Verdict

Scaffold is a clean, buildable, empty shell; demo/debug/telemetry/permissive capability code is gone; CSP and capability are coherent for Tauri 2.12.1; lockfiles are reproducible. ACCEPTED, subject to coordinator recording M1 as an explicit decision and keeping TEST-TEMPLATE/TEST-ENV as PARTIAL (not DONE-level PASS) in state.

## Re-review after post-review fixes (run-20261002-02)

Reviewer: same independent review subagent. Date 2026-10-02.

Commands re-run (all exit 0): `pnpm install --frozen-lockfile --offline` (up to date); `pnpm run typecheck`, `lint`, `build`; `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --all-targets --locked` (3 guard tests pass); `check_task_scope.py --task P01-T01 --repo-root . --base 31b778f` (PASS_PATH_SCOPE_ONLY, outOfScope empty).

- **M1 resolved.** apps/desktop/pnpm-workspace.yaml is gone. package.json pins vite 8.3.1 (published 2026-09-24), shadcn 4.21.0 (2026-09-04), globals 17.12.0 (2026-09-01), @types/node 24.19.0 (2026-09-25). I queried registry publish times for all 498 unique name@version entries in pnpm-lock.yaml (0 missing): none was published within the last ~36 hours, so no cooldown exclusion is needed.
- **L1 resolved.** run-02 checks.log records launch, window found (1440x900 "Git Backup Manager"), screenshot and SIGTERM (143) with exit codes. Residual, unchanged and acceptable: `ipc:` CSP allowance and `tauri dev` CSP still unexercised until P01-T02 adds a command.
- **L2 partially resolved as claimed.** Debug localhost is now limited to port 3000 (`DEV_SERVER_PORT`, `cfg!(debug_assertions)` AND port match); verified in source. New-window handler is still deferred (no links exist).
- **L3.** Status wording is a coordinator state matter; no new defects found. The English report/Vietnamese handoff note stands as a hand-off item.

No new findings. Final outcome: **ACCEPTED**.
