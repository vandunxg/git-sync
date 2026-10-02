# Test strategy and truthful evidence

## Layers

| Layer | Test boundary | Main obligations |
|---|---|---|
| Unit | pure Rust/domain and React component logic | ref classification, status aggregation, validation, cancellation policies, keyboard states |
| Contract | schema <-> Rust serialization <-> TypeScript bridge | valid/invalid DTOs, envelope errors, revisions and event resubscription |
| Local integration | temporary Git repos, isolated SQLite/keyring adapter | multiple heads, annotated tags, empty repos, crash/restart, worktree protection |
| Provider integration | explicitly authorized disposable accounts/projects | pagination, auth expiry, exact leases, rate limits, destination conflicts |
| Browser UI | deterministic fixtures + pinned renderer/fonts | screens, layouts, accessible names, navigation, visual comparison |
| Native Linux | actual Tauri/WebKitGTK on recorded Arch/Hyprland | portals, keyring, tray, focus, close/minimize, subprocess cancellation |
| Restore | isolated destination without GitHub fallback | complete refs/objects, bundles, LFS payloads, scope disclosure |

TESTS.json enumerates product scenarios. It is a plan, not runnable application tests.
The bundled Python tests validate the blueprint tooling only. Never report those as
proof of Git synchronization, Rust correctness, React behavior, or native support.

## Determinism and isolation

Use temporary local Git remotes for most network/state cases. Fixture builders must
create multiple branches, annotated/lightweight tags, a rewritten branch, a moved tag,
an empty repository, a dirty/ahead/detached workspace, hostile ref/path data and LFS
payloads. Fix clocks/IDs/seeds where assertions depend on them. Separate latency from
state assertions; do not use sleep-and-hope tests or fabricate progress timers.

Provider tests require a dedicated configured namespace plus explicit current approval.
Fail closed when no sandbox is configured. Never infer permission from saved personal
credentials or an accessible account. Read tests must not mutate projects, revoke tokens
or delete references. Cleanup targets only resources created by that test run and recorded
by stable provider ID. Simulate expired credentials rather than revoking the owner's token.

## Meaningful negative cases

A timeout after push is an unknown outcome, not automatically a failed write. Test recovery
by observation. Simulate incomplete discovery and confirm old backup entries are retained.
Race another destination writer between plan and apply; exact-OID leases must reject drift.
Crash between file staging, DB commit and UI event emission; rehydrate from durable state.
Cancel while both stdout/stderr are busy; no deadlock or orphan child should remain.

Exercise missing Git, missing/locked keyring, missing LFS, disk full, permission denied,
unsafe root path, malicious repository name, unsupported object format and denied provider
permissions. Logging tests inject synthetic recognizable secrets and assert their absence
from persisted evidence. Synthetic secret markers must never be real credentials.

## Evidence and gates

Each test implementation maps back to TEST-* and REQ-* IDs. Record exact command selectors,
environment and final tree identity. If an edit invalidates earlier results, rerun affected
checks. Unit, contract and integration tests are distinct from a successful build.
A release combines all required scenarios plus independent native/visual/restore review.

Use scripts/run_gates.py as an execution wrapper once actual app scripts exist. It reports
BLOCKED for missing prerequisites; no available subset is treated as a full profile pass.
Security and native profiles require extra setup as defined in quality/GATES.json.
Code coverage is diagnostic, not a substitute for failure-path and authorization assertions.
