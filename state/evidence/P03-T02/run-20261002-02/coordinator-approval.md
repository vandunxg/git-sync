# Coordinator approval - P03-T02 out-of-allowlist edits

Approver: coordinator (Claude Code session under OD-001/OD-002, 2026-10-02). Not owner approval.

| Path | Approved change | Condition |
|---|---|---|
| crates/domain/src/contract/secret.rs | `SecretString::new(String)` only | type still has no Serialize/Clone; Debug redacted |
| crates/infrastructure/Cargo.toml | + gbm-application, keyring-core 1.0.0, zbus-secret-service-keyring-store 1.0.1 [rt-async-io-crypto-rust] | no other dependency changes |
| crates/infrastructure/src/lib.rs | `pub mod secrets;` | module root only |
| Cargo.lock | additions only (new-crates.txt); no existing version changed | `cargo test --locked` passes |

cargo-audit / cargo-deny / license scan for the new transitive crates: NOT_RUN (tools not installed).
