# Task report - P03-T01

- Task / phase / requirement IDs: P03-T01 / P03 / REQ-10, REQ-36
- Validation kind: implementation. Base commit e121053. Host: EndeavourOS, XFCE/X11.
- Permitted: crates/infrastructure/src/sqlite/, crates/infrastructure/migrations/ (+ state/).

## Coordinator-approved out-of-allowlist edits (scope-check.json lists exactly these)

| Path | Change | Reason |
|---|---|---|
| crates/infrastructure/Cargo.toml | new crate manifest: gbm-domain, serde_json, sqlx 0.9.0; dev: gbm-test-fixtures, tokio | the crate named by the allowlist did not exist |
| crates/infrastructure/src/lib.rs | `pub mod sqlite;` only | crate root |
| Cargo.lock | +48 packages (new-crates.txt), all from sqlx | follows from the manifest |

Approval: coordinator (this session) under OD-001/OD-002; same form as run-02 of P01-T03.

## Dependency proof (R-DEPS)

sqlx 0.9.0 (MIT/Apache-2.0, published 2026-05-21, rust-version 1.94.0 <= MSRV 1.96) is the stack choice in
STACK_AND_VERSION_POLICY.md. `cargo info sqlx@0.9.0` and its Cargo.toml were read: features used are
`runtime-tokio`, `sqlite-bundled`, `migrate`, `macros` with default features off. The `sqlite` umbrella was
avoided because it enables `sqlite-load-extension`. Resolved: sqlx-sqlite 0.9.0 [bundled, migrate, offline,
serde], libsqlite3-sys 0.37.0 [bundled]. sqlx-postgres/sqlx-mysql appear in Cargo.lock as optional entries but
are not in the build graph (`cargo tree -i` prints nothing). sqlx 0.9 rejects non-static SQL at compile time
(`SqlSafeStr`); production code compiles, i.e. it contains only static, parameterized SQL.
Alternative considered: rusqlite (sync, needs spawn_blocking wrappers; not the recorded stack choice).

## Delivered

- migrations/0001_initial.sql: DDL identical to contracts/db.reference.sql (verified by a normalizing compare),
  plus the `revisions` seed row the counter needs.
- `Database::open`: refuses databases with tables but no migration history, a partially applied migration,
  applied versions unknown to this build (newer app), and modified applied migrations; takes a `VACUUM INTO`
  backup before applying pending migrations to an existing database; runs `PRAGMA integrity_check`.
  Connections: foreign_keys ON, WAL, synchronous FULL, busy_timeout 5 s, pool max 4.
- Stores (each method = one short transaction, no callbacks, revision bumped in the same transaction):
  accounts (upsert, disconnect keeps the row and clears credential_ref, list; credential_ref must be `cred:<id>`
  and must not look like a GitHub/GitLab token), jobs (idempotent enqueue with Coverage DTO validation,
  compare-and-set transitions per SCHEDULER_AND_STATE.md, unique active job per repository mapped to
  `ActiveJobExists`, startup recovery running/cancel_requested -> interrupted), settings (Settings DTO
  validation, optimistic concurrency, no invented defaults), activity (bounded, rejects credential-looking text).
- Typed `StoreError` exposes no SQL parameters or row contents.

Transition edges beyond the spec's literal list, marked (*) in jobs.rs: running -> cancel_requested /
waiting_for_approval / waiting_for_retry; cancel_requested -> canceled / succeeded / partial / failed / interrupted;
waiting_* -> queued / canceled. Recovery sets error_code CANCEL_UNCERTAIN only for jobs whose cancel was pending.

Not delivered: repositories/mappings/snapshots/plans/approvals stores (their phase tasks), DB placement and
network-filesystem check (platform task), wiring into the Tauri app (no command uses persistence yet).

## Tests (initial run; see correction below - this run was flaky)

fresh migration + PRAGMAs; upgrade 1->2 with sample data, verified backup contents; interrupted migration rolls
back (no partial table, data intact) and the fixed migration then applies; newer, modified, foreign and garbage
databases refused (foreign one left untouched); FK enforcement + idempotency; one active job per repository
(running and cancel_requested both hold the slot; repository-less jobs unaffected); state machine table and
CAS transitions; restart recovery after a simulated crash (single revision bump, queued intent kept, requeue
allowed); revision monotonicity (rejected mutations do not bump); no secret-like columns, token-shaped
credential refs and credential-bearing messages rejected, synthetic marker absent from all DB/WAL files;
SQL-injection-shaped input stored literally; settings concurrency and `telemetry: true` rejection.
negative-controls.log: foreign_keys(false) -> 2 tests fail; backup removed -> upgrade test fails; recovery
made a no-op -> recovery test fails; sources restored (cmp).

## Commands

run_gates core exit 0 (fmt, clippy -D warnings, tests, doctests); frontend exit 0 (unchanged FE);
`cargo test --workspace --all-targets --locked` exit 0 (cargo-test-locked.txt); scope check exit 1 = the 3
approved paths.

## Scenario status

TEST-DB: PASS for the implemented stores (schema upgrade with sample data, interrupted migration, active-job
uniqueness, FKs, revisions, recovery, no secret values). "No network inside transaction" holds by construction
(store API shape); there is no network code yet to test against.

## State: REVIEW_READY

## Correction after review (CHANGES_REQUIRED) -> run-20261002-02

The original "12 PASS" claim above was NOT reliably reproducible: the reviewer saw 4/16 runs fail
(`one_active_job_per_repository`, SQLITE_BUSY). Cause (production defect): deferred `BEGIN` transactions that
read then write fail immediately with BUSY under WAL instead of waiting on busy_timeout; error paths dropped
transactions without explicit rollback.

Fixes:
- F1: all write transactions use `BEGIN IMMEDIATE`; every mutating method commits or explicitly rolls back via
  `finish()`; SQLITE_BUSY/LOCKED map to retryable `StoreError::Busy`. New multi-thread test: 8 jobs of one
  repository race to start (exactly 1 wins, others ActiveJobExists), 8 writers race one edge (1 win, 7
  InvalidTransition) alongside 16 concurrent inserts (all commit), revision count exact.
  repeat-40.log: 40/40 runs PASS. Negative control [A]: deferred BEGIN -> concurrency test fails 3/3.
- F3: more controls (negative-controls.log): [C] state-machine bypass -> 3 failures; [D] revision bump skipped
  -> 2 failures; [B2] active-job precheck + error mapping removed -> 2 failures. [B]/[E] single-layer removals
  pass because a second layer remains (unique index; sqlx VersionMissing) - intentional layering.
  New test for dirty (`success = 0`) history. The secret-marker file scan is described as defence-in-depth.
- F4: all refusal checks (incl. checksum comparison) run before the backup, so a refused open leaves no backup
  (tested); a failed `VACUUM INTO` removes its partial output; DB, WAL/SHM and backup files are 0600 (tested);
  an existing file is inspected through a read-only connection first, so a foreign DB is refused without
  switching it to WAL (tested: no -wal file, journal_mode stays `delete`). Several pending migrations can still
  stop at an intermediate version - accepted: each is atomic and the pre-upgrade backup exists.
- F5: explicit active-job precheck inside the IMMEDIATE transaction; index + message mapping remain as backstop.
  `transition_job` documents that `error_code` replaces the stored code.
- F6: edges for waiting_for_approval / waiting_for_retry removed (await P07 + owner ratification);
  cancel_requested edges kept and listed for owner ratification (run-02/coordinator-approval.md).
- F7: `storage_mode = none` requires no credential_ref; keyring/session_only require one; `cred:` followed by
  >= 32 hex chars is rejected as token-like; activity error_code validated like jobs. AUTH_STATES come from the
  AccountSummary.authState contract enum; numeric provider_user_id reflects GitHub/GitLab numeric user IDs.
- F9: approval record attached (run-02/coordinator-approval.md). cargo-audit / cargo-deny / license scan are
  NOT run: tools not installed (security gate BLOCKED, see P01-T03).

run-02: 15 tests; run_gates core exit 0, frontend exit 0; scope check exit 1 = the 3 approved paths.
