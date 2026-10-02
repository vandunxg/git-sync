# V1 release checklist

A checked box needs an evidence path and the final build/source identity. Empty evidence
means NOT_RUN/BLOCKED. No release upload or personal remote write is authorized by this list.

## Implementation and contracts

- Every V1 requirement maps to reviewed tasks and actually implemented/executed scenarios.
- All 31 app-owned commands have native validation, type parity, structured errors and real
  adapters where enabled. No arbitrary shell/fs commands or fixture fallback in release.
- SQLite migration/recovery, job revision events, refresh/reconnect and status totals agree.
- No schema/API claim relies on invented functions; exact toolchain and upstream licenses
  are recorded and lockfiles committed through the owner's workflow.

## Data safety

- All selected heads/tags, annotated tag objects and allowed format histories verified.
- Rewrites, moved tags, deletions and destination drift require valid plan/approval/snapshot.
- Partial discovery, source disappearance and 404 never erase backups automatically.
- Credential broker/keyring/path containment and redaction pass adversarial tests.
- Workspace dirty/ahead/detached/untracked data preserved; no reset/stash/clean on autopilot.
- Unknown write outcomes reconciled; cancellation reaps processes; restart does not duplicate
  destination project creation or claim success from stale in-memory state.
- Bundle restore and LFS restore evidence exists; submodule limitations are visible.

## UI and Linux

- Owner-approved source references and separately reviewed implementation goldens registered.
- All screen states/subviews implemented without unsupported/inert controls, arbitrary charts,
  placeholder glyphs, overlap or clipping. Semantic icons and keyboard/a11y checks pass.
- Real Arch/Hyprland Tauri/WebKitGTK test environment recorded, including portal, keyring,
  tray absent/present, DPI, close/minimize/exit/suspend, offline and folder-picker paths.
- Background behavior truthfully stops on exit; no hidden daemon or telemetry.
- Packaging installed/launched from a clean profile; dependencies, missing-capability errors
  and uninstall/retained-data policy verified. A dev-server screenshot is not this evidence.

## Final gate

Run applicable quality profiles on the final tree, review all required scenarios in TESTS.json
and store a consolidated evidence index. Security exceptions need explicit scoped approval;
no silent audit suppression. Human owner acceptance and the final release artifact hash are
required. Windows/macOS and OAuth extensions remain DEFERRED unless separately authorized,
implemented and verified. Publish only on a new explicit release instruction.
