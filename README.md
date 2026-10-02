# Git Backup Manager - implementation blueprint

Version: **1.0.0**. Prepared: **2026-10-01**. Deliverable: specifications, rules,
phase/task plans, contract schemas, test scenarios, and executable quality-gate
utilities. **This package is not an implemented application.**

Start with [START_HERE.vi.md](START_HERE.vi.md), then [AGENTS.md](AGENTS.md).
The technical specifications are written in English; the onboarding guide is Vietnamese.

## Product boundary

A Linux-first desktop application for Arch Linux + Hyprland. GitHub is the source;
GitLab is a separate backup destination; a selected local folder contains managed
bare mirrors and optionally independent working copies. Scan for newly accessible
repositories and ref changes, queue one-way backups, and make backup freshness and
errors understandable. No two-way merge engine, cloud service, or new SaaS dashboard.

The selected implementation direction is Tauri 2 + Rust + React/TypeScript/Vite,
shadcn/ui, SQLite, and the system Git CLI. Use the requested agmmnn/tauri-ui scaffolder
as an audited starting point, not its demo dashboard as a product design.

## What makes this an execution system

- [Requirements](docs/spec/REQUIREMENTS.json) connect to task IDs and test IDs.
- [Phase plan](docs/plan/ROADMAP.md) and [task board](docs/plan/TASKS.json) establish dependencies and permitted scope.
- [UI contract](docs/ui/UI_CONTRACT.md) and [screen specs](docs/ui/SCREEN_INDEX.md) define structure, behavior and exclusions.
- [Visual baseline gate](docs/quality/VISUAL_ACCEPTANCE.md) prevents a self-generated screenshot from being treated as owner approval.
- [IPC contract](contracts/ipc.v1.json) names app-owned commands, DTOs and errors. These are **new application contracts**, not existing Tauri APIs.
- [Agent execution protocol](agents/EXECUTION_PROTOCOL.md) requires implementation evidence, not a verbal claim of completion.
- `scripts/lint_pack.py`, `scripts/check_visual_baselines.py` and `scripts/run_gates.py` are runnable using Python 3.11+.

## Important corrections to earlier sketches

1. A continuously updated mirror is not an immutable or historical backup. Deletion
   and rewritten history require a reviewed plan and a verified pre-change snapshot.
2. `pushed_at` and the default branch SHA are hints, not proof all branches/tags match.
3. Do not push provider-internal refs or run a blind `git push --mirror`.
4. Do not reset, stash, clean, checkout or overwrite a user's dirty workspace automatically.
5. PAT + OS keyring is the V1 authentication path. Browser authorization is a separately
   gated extension, not a fake working button in the first release.
6. Figma Refined V2 is a reference, **not a fully approved visual baseline**. The
   pending semantic-icon correction overrides its remaining status dots.
7. A previous outer-frame bounds check is not proof of no internal overlap or clipping.

## Evidence state

The Figma metadata read attempted for page `6:2` during this packaging run returned
`MCP tool call limit on Starter plan`. No fresh Figma screenshot or edit was produced.
Known node IDs come from successful earlier tool results in this conversation.
See [SOURCE_OF_TRUTH.md](docs/spec/SOURCE_OF_TRUTH.md).

All implementation tasks start `NOT_STARTED`. The visual-baseline gate intentionally
returns `BLOCKED` until owner-reviewed exports and their hashes are registered.
Package validation does not claim that Rust code, the UI, GitHub/GitLab integration,
or Arch/Hyprland behavior has already been implemented or tested.

## Safe installation

For a new repository, merge this directory into its root. For an existing repository,
keep it under `blueprint/` first and explicitly reference `blueprint/AGENTS.md` in the
agent's startup prompt. Inspect existing instructions before merging; never overwrite
an existing `AGENTS.md`, `CLAUDE.md`, or application configuration blindly.

The quality runner accepts `--repo-root` because the blueprint and application may
be in different directories. The bundled helpers do not implement provider writes. Future application test scripts may perform writes and require explicit authorized sandbox targets; review them before running integration/native profiles.

## Package verification

See [PACKAGE_VALIDATION.md](PACKAGE_VALIDATION.md) for actual checks. Product acceptance scenarios are planned, not executed. The archive includes a SHA-256 file manifest.
