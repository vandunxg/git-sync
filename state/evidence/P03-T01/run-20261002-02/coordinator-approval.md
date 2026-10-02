# Coordinator approval - P03-T01 out-of-allowlist edits and transition edges

Approver: coordinator (Claude Code session acting under OD-001/OD-002, 2026-10-02). Not owner approval.

| Path | Approved change | Condition |
|---|---|---|
| crates/infrastructure/Cargo.toml | new crate manifest (gbm-domain, serde_json, sqlx 0.9.0 sqlite-bundled; dev: test fixtures, tokio) | crate did not exist; no other crate changed |
| crates/infrastructure/src/lib.rs | `pub mod sqlite;` | crate root only |
| Cargo.lock | +48 packages from sqlx, no existing version changed | `cargo test --locked` passes |

Job transition edges in `allowed_transition`: spec edges plus `running -> cancel_requested` and
`cancel_requested -> canceled|succeeded|partial|failed|interrupted` (spec: cancel_requested is a request
state). Edges for waiting_for_approval / waiting_for_retry are NOT enabled; the scheduler task (P07) must
define them and obtain owner ratification. Owner ratification of the cancel_requested edges is still open.
