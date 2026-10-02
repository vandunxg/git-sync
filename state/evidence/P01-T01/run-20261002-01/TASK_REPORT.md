# Task report - P01-T01

- Task / phase / requirement IDs: P01-T01 / P01 / REQ-01, REQ-02, REQ-39
- Validation kind: implementation. Base commit 31b778f. Host: EndeavourOS, XFCE/X11, DISPLAY=:0.0.
- Permitted: apps/desktop/, crates/, Cargo.toml, Cargo.lock, rust-toolchain.toml, package.json,
  pnpm-lock.yaml (+ state/). Root package.json/pnpm-lock.yaml were NOT created: the lockfile lives at
  apps/desktop/pnpm-lock.yaml (inside apps/desktop/) so no root pnpm-workspace.yaml (unlisted path) is needed.
- Changed: Cargo.toml (workspace), Cargo.lock, rust-toolchain.toml, crates/{domain,application}
  (doc-only lib crates so later tasks can fill them without editing the workspace manifest),
  apps/desktop/** (cleaned scaffold), state/evidence/P01-T01/.

## Scaffold provenance

`bunx create-tauri-ui@1.2.0 git-backup-manager --template vite --identifier com.gitbackupmanager.desktop
--no-starter --no-invoke-example --no-workflow --no-size-optimize --yes` in scratchpad, exit 0
(scaffold-run.log). Internally resolved create-tauri-app 4.7.4 (bun cache) and shadcn 4.16.1 (pinned).
Output copied into apps/desktop without .git, node_modules, bun.lock.

## Template audit and removals (RD-003)

| Generated item | Action | Reason |
|---|---|---|
| `greet` command (emitted despite `--no-invoke-example`) | removed | sample invoke, TEST-TEMPLATE |
| debug-panel.tsx, debug-events.ts, lib/tauri.ts (tracked invoke) | removed | debug panel, REQ-39 |
| tauri-plugin-log (+ Webview log target) | removed | streams native logs to renderer |
| tauri-plugin-opener + `opener:default`, external-link-guard | removed | renderer-wide opener; safe open actions become app commands later |
| theme-provider (dark/system, `d` hotkey, localStorage) | removed; `.dark` tokens removed | dark theme unapproved (PRODUCT.md) |
| demo App content, react.svg, vite.svg, README | removed / replaced by empty shell heading | demo |
| `withGlobalTauri: true` | false | only lib/native may touch Tauri APIs |
| `csp: null` | strict self-only CSP + freezePrototype | R-SEC production CSP, no remote scripts/fonts |
| `core:default`, `log:default` | `core:event:default` only | narrow capability |
| `visible:false` + show on page load | `visible:true` | window must stay reachable if load fails (TEST-ENV) |
| bun hooks | pnpm (RD-001) | one package manager |
| window 1400x918 | 1440x900 | UI_CONTRACT review viewport |
| `shadcn`, tailwind in dependencies | moved to devDependencies | build-time only |
| all `^` ranges | exact versions from scaffold's resolved bun.lock | pinning |
| `typecheck` = `tsc --noEmit` on solution tsconfig (`files: []`, checked nothing) | per-project `tsc -p` | no-op check; negative probe returned exit 2 |
| — | added `noUncheckedIndexedAccess` | R-FE |
| `.expect()` in `run()` | `run() -> tauri::Result<()>`, main returns ExitCode | R-RUST |
| navigation guard allowed 127.0.0.1/::1 and opened external URLs | only tauri://, tauri.localhost, and localhost in debug builds; refuse others, open nothing | R-SEC; unit-tested |

Kept: base-ui shadcn primitives (badge, button, dropdown-menu, tabs, tooltip) as the single primitive
family; Inter font bundled locally; desktop selection/overscroll CSS; release profile.

## Locked versions (proof)

Cargo.lock: tauri 2.12.1, tauri-build 2.7.1, wry 0.57.0, webkit2gtk (crate) 2.0.2.
apps/desktop/pnpm-lock.yaml: @tauri-apps/api 2.12.1, @tauri-apps/cli 2.12.1, react 19.3.0, vite 8.3.2,
typescript 6.0.3, tailwindcss 4.3.3, @base-ui/react 1.8.0 (`pnpm list --depth 0` in checks.log).
pnpm 11 wrote apps/desktop/pnpm-workspace.yaml with `minimumReleaseAgeExclude` for 5 packages younger
than its default release-age policy; reviewer should confirm this is acceptable (policy itself kept on).
state/TOOLCHAIN.json `resolvedDependencyProof` not updated: file is outside this task's allowlist.

## Commands (checks.log, all after the final source edit)

pnpm install --frozen-lockfile 0; pnpm run typecheck 0; pnpm run lint 0; pnpm run build 0;
cargo fmt --check 0; cargo clippy --workspace --all-targets --locked -D warnings 0;
cargo test --workspace --all-targets --locked 0 (3 navigation-guard tests); pnpm tauri build --no-bundle 0.
Launch: release binary started from an env without the harness token; X11 window "Git Backup Manager"
1440x900 visible after 8 s; screenshot launch-release.png; process terminated by SIGTERM (stopped).
test-template-assertions.txt: 12 assertions PASS. scope-check.json: PASS_PATH_SCOPE_ONLY.

## Scenario status

- TEST-TEMPLATE: executed for this task's scope (template assertions + build) -> PASS. Bundle-level
  inspection (deb/rpm/AppImage) NOT_RUN: `--no-bundle` used; packaging is P10.
- TEST-ENV: PARTIAL. Window launches and is reachable on XFCE/X11. Hyprland BLOCKED (OD-004);
  tray/keyring/portal-absent variants NOT_RUN (no such features yet).

## Limitations / open points

- Identifier `com.gitbackupmanager.desktop` is a SPEC_DEFAULT chosen here; owner may change before release.
- Binary-strings check has no positive control (template binary not built).
- Clean shutdown on window close not exercised (killed by SIGTERM).

## State: REVIEW_READY

## Post-review fixes -> run-20261002-02

- M1: re-pinned to releases older than pnpm's release-age policy (vite 8.3.1, shadcn 4.21.0,
  globals 17.12.0, @types/node 24.19.0); pnpm-workspace.yaml with exclusions removed; fresh install
  needed no exclusions. (TOOLCHAIN.json's shadcn 4.16.1 is the scaffolder's internal init pin; the app
  devDependency is 4.21.0.)
- L1: run-02 checks.log logs launch, window detection, screenshot and SIGTERM exit codes.
- L2: debug navigation to localhost restricted to port 3000 (+2 test assertions).
- L3: scenario status corrected: TEST-TEMPLATE = PARTIAL (source/dist/binary assertions PASS; bundle
  inspection NOT_RUN until P10). TEST-ENV = PARTIAL. `ipc:` CSP allowance to be exercised by P01-T02.
  `style-src 'unsafe-inline'` flagged for the P10 security gate.
