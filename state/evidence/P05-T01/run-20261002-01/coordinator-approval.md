# Scope exception — P05-T01 (coordinator, 2026-10-02)

Out-of-allowlist paths and reason:
- crates/infrastructure/Cargo.toml: direct dependency `rustix = "1.1.5"`
  (default-features off; features std, process) to signal a child's process group and
  descendants without `unsafe` (workspace forbids unsafe). rustix 1.1.5 is already in
  Cargo.lock via async-io/tempfile; no new crate is downloaded.
- Cargo.lock: one added dependency edge (gbm-infrastructure -> rustix); no new package
  or version.
- docs/design/*: owner's untracked folder, not part of this task, not committed.
Same pattern as the P03-T04 tokio exception. Subject to the security review (REVIEW.md).
