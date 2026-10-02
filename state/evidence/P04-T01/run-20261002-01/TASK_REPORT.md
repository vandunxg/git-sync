# Task report — P04-T01 Implement paginated repository discovery

Requirements: REQ-03, REQ-21, REQ-41. Scenarios: TEST-PAGINATION, TEST-SCOPE, TEST-NO-DELETE-ON-404 — PASS
against the loopback controlled adapter (no live GitHub call; no personal account touched).

## Changed files
crates/application/src/discovery.rs (+ discovery/tests.rs), crates/application/src/lib.rs — identity by
provider ID, DiscoveryScope (owned default, collaborations/orgs opt-in), DiscoveryBuilder (dedupe, scope,
rejected entries => incomplete), Incomplete reasons, reconcile() with no deletion variant (not_observed /
out_of_scope / source_unavailable candidates only after a complete scan, renames as metadata).
crates/infrastructure/src/github/mod.rs (+ tests.rs) — `GET /user/repos?affiliation=..&visibility=all&
sort=full_name&per_page=50`, RFC 8288 Link pagination restricted to the API origin, fail-closed parsing,
payload validation, page cap 200.

## Commands — checks.log
cargo fmt --check 0; clippy -D warnings 0; application 22 + infrastructure 72 tests pass; workspace tests 0.
Negative controls N1–N4 (stop after page one, ignore scope, flag unseen on partial scans, comma-truncated Link)
each fail the expected tests (negative-controls.log). scope-check: FAIL only for the owner's untracked docs/design/.

## Limitations / next
Catalog is not persisted yet and no IPC command uses discovery (scan job wiring needs P07 queue or an
early-wiring change). Conditional requests/ETag caching not implemented. Next ready task: P04-T02.
