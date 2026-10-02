# Scope exception — P03-T04 (coordinator, 2026-10-02)

Out-of-allowlist paths and reason:
- crates/infrastructure/src/lib.rs: `pub mod git_auth; pub mod platform;` — module registration only.
- crates/infrastructure/Cargo.toml: tokio 1.53.1 moved into [dependencies] (features net, rt, time, io-util,
  sync, macros) for UnixListener + SO_PEERCRED. Already locked via sqlx; Cargo.lock unchanged; no new crate.
- docs/design/*: owner's own untracked folder, not part of this task, not committed.
Same pattern as P01-T02 / P03-T01 scope exceptions. Reviewed by the security reviewer (REVIEW.md).
