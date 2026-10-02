# Task report - P00-T02

- Task / phase / requirement IDs: P00-T02 / P00 / REQ-01, REQ-02, REQ-45
- Validation kind: inspection
- Repository path / base commit: /home/dungnv1/Documents/vandunxg/git-sync, base 84c71a0
- Permitted files: state/TOOLCHAIN.json, docs/architecture/STACK_AND_VERSION_POLICY.md (+ common state/)
- Actual changed files: state/TOOLCHAIN.json, docs/architecture/STACK_AND_VERSION_POLICY.md (appended
  section only), state/IMPLEMENTATION.json, state/evidence/P00-T02/
- Delivered: observed toolchain/library versions, template revision + checksum, capability probe
  results and plan. Not delivered: lockfiles/resolved dependency proof (P01-T01), any app code.

## Commands (cwd = repo root unless stated)

- versions.log: 19 version/capability commands + crates.io queries, each with exit code.
- Upstream audit (scratchpad, not in repo): `git clone https://github.com/agmmnn/tauri-ui` -> HEAD
  8eb86d8; `npm pack create-tauri-ui@1.2.0` -> sha1 5ddcc3fd... equals npm `dist.shasum`;
  sha256 5d6d3f6d...; read `packages/create-tauri-ui/{README.md,src/scaffold.ts,src/merge.ts,src/adapters/vite.ts}`.
- credential-helper-probe.txt: throwaway git credential helper probe, exit 0 for fill and approve.
- `python3 scripts/check_task_scope.py --task P00-T02 --repo-root . --base 84c71a0` -> exit 0 (scope-check.json).
- `python3 scripts/lint_pack.py` -> exit 0.

## Key findings

1. git-lfs is not installed -> LFS acceptance (P09) BLOCKED until owner installs it.
2. Running session is XFCE/X11; hyprland 0.55.4 is installed but not running -> Hyprland checks BLOCKED.
3. Template generates bun-managed projects (`bun run` in tauri.conf hooks; `bun install`); blueprint
   allowlist names pnpm-lock.yaml and owner chose pnpm for the Tauri CLI. P01-T01 must convert
   hooks to pnpm and delete bun.lock. Recorded for P00-T03 decision ledger.
4. Template always adds a debug panel (+ tauri-plugin-log) -> must be removed (REQ-39, TEST-TEMPLATE).
5. `create-tauri-app` is called unpinned by the scaffolder -> P01-T01 records resolved version.
6. Owner's global git config has `credential.helper=store` (plaintext). The probe shows
   `-c credential.helper=` prevents both use and write of that store. The parent environment of
   child processes contains a secret-like variable name (agent harness) -> Git children need env_clear + allowlist.
7. Keyring (gnome-keyring Secret Service), tray watcher and desktop portal are present on the bus.

## Scenario status

TEST-ENV, TEST-TEMPLATE, TEST-CREDENTIAL-BROKER: NOT_RUN (probes only; no implementation).

## Safety

No remote writes. Only public read-only HTTP queries (GitHub API, npm, crates.io) and a public clone
into scratchpad. Probe used a dummy credential string; the real ~/.git-credentials was only stat'ed and
grep-counted for the probe host (0 matches), contents not read into evidence.

## State: REVIEW_READY

## Post-review fixes
F1: IMPLEMENTATION.json now updated (DONE + evidence). F2: bus-capabilities.log added; TOOLCHAIN wording
distinguishes running vs activatable and notes the second Secret Service provider (ksecretd).
F3: template-audit.log added with the greps behind the template claims. F4 deferred to P00-T03.
