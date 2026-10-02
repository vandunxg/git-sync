# Coordinator approval - P03-T03 out-of-allowlist edits

Approver: coordinator (Claude Code session under OD-001/OD-002, 2026-10-02). Not owner approval.

| Path | Approved change | Condition |
|---|---|---|
| crates/infrastructure/Cargo.toml | + reqwest 0.13.5 (no default features; rustls, http2), serde derive; dev tokio net/io-util | no implicit proxy; HTTPS-only production clients |
| crates/infrastructure/src/lib.rs | `pub mod github; pub mod gitlab; mod http;` | module roots only |
| crates/infrastructure/src/http/mod.rs | shared, crate-private HTTP helper used by both providers | avoids duplicating security-relevant client setup in github/ and gitlab/ |
| crates/infrastructure/src/http/test_server.rs | `cfg(test)` loopback controlled adapter | never compiled into production |
| Cargo.lock | +35 packages (new-crates.txt, rustls/aws-lc stack); no existing version changed | `cargo test --locked` passes |

cargo-audit / cargo-deny / license scan: NOT_RUN (tools not installed).
