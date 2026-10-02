# Scope exception — P05-T02 (coordinator, 2026-10-02)

- Cargo.lock: one added dependency edge (gbm-application -> sha2 0.10.9). sha2 0.10.9
  is already locked (via sqlx); no new package or version. crates/application/Cargo.toml
  itself is inside the allowed path crates/application/.
- crates/infrastructure/src/git/runner.rs gained allowlisted words (update-ref -d/
  --no-deref, rev-parse --show-object-format, init --object-format=sha1|sha256) and the
  typed `FullRef` arg; git/ is inside the allowed paths, noted here because runner.rs
  was security-reviewed in P05-T01.
- docs/design/*: owner's untracked folder, not committed.
