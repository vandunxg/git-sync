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
