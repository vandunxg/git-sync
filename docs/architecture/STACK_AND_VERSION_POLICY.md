# Stack and version policy

| Layer | Selected direction | Binding constraint |
|---|---|---|
| Desktop | Tauri 2 | Do not mix v1 plugin/API examples |
| Native | Rust stable, pinned in P00 | Record MSRV/toolchain and exact compatible dependencies |
| UI | React + TypeScript + Vite | Static bundled UI, no Next/SSR server |
| Components | shadcn/ui from audited tauri-ui | One primitive family; no demo dashboard cloning |
| Styling | Tailwind + CSS variables | Version-compatible token setup from real scaffold |
| Routing/query | TanStack Router + Query | One cache owner; route state for filters/tabs |
| Forms | React Hook Form + Zod | Rust still validates all inputs |
| Global UI state | Zustand only when needed | Do not duplicate native/server/query data |
| Native async | Tokio | Owned cancellable tasks and bounded process output |
| HTTP | reqwest adapter | Timeouts, safe redirects, host-bound auth |
| Data | SQLite + sqlx | Versioned migrations; no transactions around network work |
| Git | system Git; git-lfs where needed | No blind shell string, no full-ref mirror push |
| Secrets | OS keyring abstraction | Runtime capability detection; no plaintext fallback |
| Diagnostics | tracing + bounded rolling logs | Redact before persistence and UI exposure |
| UI tests | Vitest, Testing Library, Playwright | Approved visual goldens + behavior, not self-snapshots |
| Native tests | Rust integration + Tauri native harness/manual | Arch/Hyprland evidence required |

No exact patch version is claimed here. P00 must verify the current template and
installed API surface, select compatible versions, record proof in
`state/TOOLCHAIN.json`, and lock them. The upstream scaffolder's README and feature set
may evolve [S01]. Do not copy commands/flags or icon imports from memory.

The final package must not include a runtime Node server. AppImage/AUR packaging is a
Linux delivery option, not a compatibility promise: verify the actual installed build
and runtime dependencies on the supported machine [S16]. AUR publishing and signing
are external actions requiring owner authorization.

References: [S01, S02, S16, S17](../../sources/REFERENCES.md).

## P00-T02 observations (2026-10-02)

Recorded from executed probes in `state/evidence/P00-T02/run-20261002-01/`; details in
`state/TOOLCHAIN.json`. These are observations and selections, not lockfile proof.

| Item | Observed / selected | Note |
|---|---|---|
| Rust | 1.96.0 (Arch package, no rustup) | MSRV = 1.96.0; `rust-toolchain.toml` is informational without rustup |
| Node / package manager | Node 24.17.0, pnpm 11.5.1 | pnpm owns the app lockfile (P01 allowlist names `pnpm-lock.yaml`) |
| Scaffolder | create-tauri-ui 1.2.0, sha1 5ddcc3fd..., git 2ee24658 | Requires bun to run; generated `bun run` hooks must be rewritten to pnpm |
| Tauri | 2.x line, registry latest 2.12.1 | Tauri 3 alpha exists; not selected |
| Git | 2.54.0 | git-lfs absent: LFS work BLOCKED |
| WebKitGTK | webkit2gtk-4.1 2.52.4 | Native checks run on XFCE/X11; Hyprland 0.55.4 installed but unverified |

Template audit findings for P01-T01: the scaffolder always installs a dev-only debug panel
(with `tauri-plugin-log`), an external-link guard and selection/scroll batteries; starter
dashboard, invoke example and release workflow are opt-out flags. `create-tauri-app` is
invoked unpinned inside the scaffolder, so the P01-T01 run must record its resolved version.

### Capability probe plan

| Capability | Probe done in P00-T02 | Acceptance test owner |
|---|---|---|
| Secret Service keyring | `org.freedesktop.secrets` running (gnome-keyring); ksecretd compat also running | P03 TEST-ENV / keyring tests |
| Tray | `StatusNotifierWatcher` present | P07/P10 TEST-ENV |
| Portal (open folder/browser) | `org.freedesktop.portal.Desktop` activatable (not running at probe) | P08/P10 native tests |
| Git credential isolation | `-c credential.helper=` blocks inherited global `store` helper (fill + approve) | P03-T04 TEST-CREDENTIAL-BROKER |
| Child environment | Parent env carries a secret-like variable; Git children need a cleared allowlisted env | P03-T04 / P05-T01 |
| Hyprland session | Not running; BLOCKED | P10 TEST-ENV / TEST-NATIVE-UI |
