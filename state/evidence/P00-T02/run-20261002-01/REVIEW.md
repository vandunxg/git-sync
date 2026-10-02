# Review - P00-T02

- Reviewer: independent review subagent designated by owner 2026-10-02
- Date: 2026-10-02; base 84c71a0; scope: git diff 84c71a0 + untracked state/evidence/P00-T02/
- Outcome: ACCEPTED (no blocking findings; 3 low, 1 informational)

## Commands re-run (all read-only)

| Command | Exit | Result |
|---|---|---|
| rustc --version / cargo --version | 0 / 0 | 1.96.0 (ac68faa20 2026-05-25), matches |
| node --version; pnpm --version; bun --version | 0 | v24.17.0; 11.5.1; 1.3.14, matches |
| git --version | 0 | 2.54.0, matches |
| git lfs version | 1 | not a git command; git-lfs absent (pacman -Q git-lfs exit 1), matches null in TOOLCHAIN.json |
| pkg-config --modversion webkit2gtk-4.1 | 0 | 2.52.4, matches |
| npm view create-tauri-ui@1.2.0 dist.shasum gitHead license | 0 | 5ddcc3fd...93e07720493e82 / 2ee24658...a165 / MIT, match |
| npm view create-tauri-app version; @tauri-apps/cli version | 0 | 4.7.4; 2.12.1, match |
| sha256sum scratchpad create-tauri-ui-1.2.0.tgz | 0 | 5d6d3f6d...19c2d, matches tarballSha256 |
| git -C upstream rev-parse HEAD; log -1 | 0 | 8eb86d89...; 2026-08-01, matches |
| crates.io API: tauri max_version / max_stable; sqlx rust_version | 0 | 3.0.0-alpha.4 / 2.12.1; sqlx 0.9.0 rust 1.94.0, match |
| busctl --user list (secrets/portal/SNI) | 0 | see F2 |
| python3 scripts/check_task_scope.py --task P00-T02 --repo-root . --base 84c71a0 | 0 | PASS_PATH_SCOPE_ONLY, outOfScope [] |

Upstream source checks (scratchpad clone): scaffold.ts pins shadcn 4.16.1 but runs `bunx create-tauri-app` with no version (line ~247);
index.ts requires bun; applyDebugPanel is called unconditionally (index.ts:405) while applyWorkflow is conditional; debug panel adds
`tauri-plugin-log = "2"` and is gated by `import.meta.env.DEV`; flags --no-starter/--no-invoke-example/--no-workflow/--yes/--template exist.
All template claims in TOOLCHAIN.json and the policy doc are confirmed.

## Checks requested

- Fictional versions: none found. Every version value traced to an executed output (versions.log or my re-run).
- Registry "latest" not presented as locks: OK. tauriVersion text says "registry latest ... exact resolved versions locked in P01-T01"; note says observations not locks; resolvedDependencyProof stays []. The policy doc says "observations and selections, not lockfile proof".
- Scope: OK. Only the two allowed files plus evidence directory changed (common state paths permitted).
- Scenarios: TEST-ENV / TEST-TEMPLATE / TEST-CREDENTIAL-BROKER stay NOT_RUN in TASK_REPORT; policy doc table is a probe plan, not a pass claim. No status flipped to PASS.
- Security findings: sound. Probe used a dummy credential on an .invalid host; store file only counted/mtime-compared, contents not recorded; env var name only (value not recorded). Conclusion correctly labelled a capability probe, not TEST-CREDENTIAL-BROKER. The env_clear + allowlist recommendation is justified.

## Findings

F1 (Low) TASK_REPORT.md lists state/IMPLEMENTATION.json as a changed file and claims "State: REVIEW_READY", but the diff does not touch it
(P00-T02 is still IN_PROGRESS with evidence [] and review null). Either update it (allowed common state) or correct the report. The
owner/orchestrator should record the review and evidence there after this verdict.

F2 (Low) TOOLCHAIN.json capabilities.secretService asserts "owned by gnome-keyring-daemon (gnome-keyring 1:50.0-1); also
org.kde.secretservicecompat on bus", but versions.log only lists the bus names (no owner column). My busctl re-run confirms gnome-keyring-d
owns org.freedesktop.secrets, but the owner is not in the evidence. Also the session runs ksecretd (KDE) with a secretservicecompat
name alongside gnome-keyring, so two Secret Service providers coexist; P03 keyring tests should pin which one is exercised. Also
org.freedesktop.portal.Desktop and org.x.StatusNotifierWatcher are "(activatable)" (not running) at re-run; "present" is accurate only
for activatable, so the P08/P10 portal test should do an actual call.

F3 (Low) Evidence gap: versions.log does not capture the scaffolder source greps (debug-panel always installed, bun requirement, unpinned
create-tauri-app, flag list); they exist only as prose in TASK_REPORT.md. Reproducible (I confirmed them), but a short grep log would make
them evidence rather than assertion.

F4 (Info) MSRV "1.96.0 selected = installed toolchain" is stricter than dependency minimums (tauri 1.90, sqlx 1.94.0). This is a legitimate
selection but a choice, not a derived value; P00-T03/P01-T01 should confirm it. The tool-less claim "Hyprland 0.55.4 installed" is
package-level only and correctly marked hyprlandVerified=false.

## Notes for next tasks
- git-lfs absent: LFS-dependent work BLOCKED until owner installs it.
- Hyprland session and TEST-NATIVE-UI remain BLOCKED/unverified (XFCE/X11 only).
- P01-T01 must record the resolved create-tauri-app version, rewrite bun hooks to pnpm, remove debug panel + tauri-plugin-log.
