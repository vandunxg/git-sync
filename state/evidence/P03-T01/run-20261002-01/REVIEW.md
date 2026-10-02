# Independent review - P03-T01 (SQLite repositories and migrations)

- Reviewer: independent review subagent designated by the owner on 2026-10-02 (not the implementer).
- Scope reviewed: `git diff e121053` (Cargo.lock only; the rest is untracked), `crates/infrastructure/**`, and the evidence in `state/evidence/P03-T01/run-20261002-01/`.
- Edits: only this file. A throwaway copy of the crates was built under the session scratchpad to test one fix; the repository was not touched.

## Outcome: CHANGES_REQUIRED

One blocking defect: transactions are opened as deferred (`BEGIN`) and error paths drop the transaction without an explicit rollback. This makes the implementation flaky (reproduced here) and makes `busy_timeout` ineffective for the SELECT-then-write methods. The evidence claim "12 tests, all PASS" is not reliably reproducible. Everything else is acceptable or minor.

## Commands re-run (cwd = repo root)

| Command | Exit | Result |
|---|---|---|
| `cargo test -p gbm-infrastructure --locked` (first run) | 101 | `one_active_job_per_repository` FAILED, `Error: Database("sqlite code 5")` (SQLITE_BUSY), 11 passed |
| same, 15 repeats | mixed | 3 of 15 runs FAILED, same test and error; 12 of 15 passed (so 4 of 16 runs failed overall) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | clean |
| `cargo fmt --all -- --check` | 0 | clean |
| `python3 scripts/check_task_scope.py --task P03-T01 --repo-root . --base e121053` | 1 | `outOfScope` = `Cargo.lock`, `crates/infrastructure/Cargo.toml`, `crates/infrastructure/src/lib.rs` (matches TASK_REPORT) |
| `cargo tree --workspace -e normal,build --locked -i sqlx-postgres` (and `-i sqlx-mysql`) | 0 | "nothing to print": not in the build graph (claim verified) |
| `cargo tree -p gbm-infrastructure -e features --locked` | 0 | sqlx-sqlite features: `bundled`, `migrate`, `offline`, `serde`, `default`. libsqlite3-sys: `bundled`, `bundled_bindings`, `cc`, `pkg-config`, `vcpkg`. No load-extension feature (grep count 0). |
| Experiment in scratchpad copy: `begin_with("BEGIN IMMEDIATE")` in `Database::begin`, 25 runs | 0 | 25 of 25 passed |

## Findings

### F1 - HIGH (blocking): deferred transactions + drop-without-rollback give spurious SQLITE_BUSY and a flaky test
- `Database::begin` uses `pool.begin()`, which issues a deferred `BEGIN`. `transition_job`, `put_settings` and `recover_interrupted_jobs` read first and write second.
- Under WAL, a deferred transaction that read and then tries to write while another connection holds the write lock gets `SQLITE_BUSY` immediately, or `BUSY_SNAPSHOT` if the other writer committed after the read. The busy handler is not invoked in that case, so the 5 s `BUSY_TIMEOUT` does not help.
- Error paths such as `ActiveJobExists`, `InvalidTransition` after `rows_affected != 1`, `Duplicate` and `StaleRevision` return with `?` and drop the transaction. sqlx queues that rollback asynchronously. The connection can still hold the write lock when the next call takes another pool connection (pool max is 4). That is the most likely cause of the failure seen here (`blocked` then `transition_job(CancelRequested)`). I did not trace the exact statement, but the fix below removed every failure.
- Impact:
  - Correctness is not violated, because the CAS still refuses a double pass (see F2).
  - Liveness is: two writers, or a writer right after a rejected mutation, produce `StoreError::Database("sqlite code 5")`. A scheduler would see that as a persistent failure.
  - The reported test result is not reproducible.
- Fix: `self.pool.begin_with("BEGIN IMMEDIATE")` for every mutation, so the write lock is taken up front and `busy_timeout` applies. Roll back explicitly on error returns (`tx.rollback().await`) or keep them as a drop-only path with IMMEDIATE. Map BUSY and BUSY_SNAPSHOT to a dedicated retryable error, not `Database("sqlite code 5")`. Add a concurrency test: N concurrent `transition_job(Running)` calls for the same repository must give exactly one Ok and the rest `ActiveJobExists` or `InvalidTransition`, never a BUSY error. Run the whole suite in a loop.

### F2 - INFO: the compare-and-set is sufficient
- Two writers cannot both pass the state check. The UPDATE carries `AND state = ?` and the transaction aborts if `rows_affected != 1`. SQLite also refuses a stale-snapshot upgrade.
- The revision bump happens before the UPDATE inside the same transaction, so a rejected mutation does not leak a bump (tested).
- `enqueue_job`, `upsert_account`, `disconnect_account`, `append_activity` and `put_settings` each bump exactly once, in the same transaction as the write. No partial commit path was found.
- Gap: `recover_interrupted_jobs` returns early on an empty set and drops the transaction without a bump. That is correct, but it is subject to F1.

### F3 - MEDIUM: test and evidence gaps against TEST-DB
- The negative controls cover only `foreign_keys`, the backup and the recovery no-op. They do not cover the unique active-job index, the CAS (`AND state = ?`), revision bumps, downgrade or modified-migration refusal, or credential-ref rejection. Those tests have not been shown to fail when the guarded behaviour is removed.
- "Interrupted migration" is simulated by a failing SQL statement (the per-migration transaction rolls back). That is a fair proxy for sqlx 0.9, where `apply` wraps the script and its bookkeeping in one transaction. There is no test for a process kill, for the `success = false` ("dirty") branch, or for a `no_tx` migration. The code refuses a dirty database before backup, which is the right behaviour, but it is untested.
- `no_secret_values_are_stored`: the marker scan of the DB, WAL and SHM files is close to vacuous. The marker is rejected before it can be written, so absence proves only the validator, not storage. The column-name check is a reasonable add-on.
- "No network inside transaction" holds by API shape only. That is acceptable at this phase and is stated honestly in the report.
- No test checks that the backup passes `integrity_check`, or that a backup file is not overwritten.

### F4 - LOW: migration safety details (no blocker)
- Verified correct:
  - Downgrade or unknown applied versions are refused before any change.
  - A modified migration is refused via `VersionMismatch`.
  - A database that has tables but no history is refused.
  - sqlx 0.9 SQLite runs each migration in its own transaction, so one failed migration rolls back and leaves the earlier ones applied.
  - `VACUUM INTO` through the pool captures WAL content.
  - `VACUUM INTO` fails if the target exists, so the `exists()` loop in `backup_path` cannot overwrite a backup, and its TOCTOU is harmless.
- Minor:
  - The backup is taken before the modified-migration check, so a refused open with pending migrations still leaves a backup.
  - A failed `VACUUM INTO` may leave a partial `.bak`, which is never cleaned up. Later opens skip to a new name, so it is not misused.
  - Backups use the default umask. They hold private repo names, so 0600 would be preferable.
  - `connect_options` sets WAL before refusing a foreign database, which changes that file's journal mode. The test checks only the table count. Consider refusing before switching the journal mode, or document it.
  - A fresh open creates the file before any validation.
  - With several migrations pending, the first can commit and a later one fail, leaving the database at an intermediate version. The pre-migration backup covers this, but the report's "stays at previous version" wording in `StoreError::MigrationFailed` is only true per migration.
  - `MigrationFailed(other.to_string())` for the uncommon `MigrateError` variants may include driver text. It is not secret-bearing, but it is not as sanitized as the rest.

### F5 - LOW: `ActiveJobExists` mapping
- The match on `db.message().contains("jobs.repository_id")` with code 2067 works. SQLite reports a partial unique index violation as `UNIQUE constraint failed: jobs.repository_id`.
- The other unique constraints on `jobs` are the primary key (`jobs.id`, code 1555) and `(idempotency_key, attempt)`. Their messages do not contain `repository_id`, so they map to `Duplicate`. This is verified by the tests.
- It depends on SQLite message text, which is stable but not an API. A more robust option is to precheck inside the IMMEDIATE transaction: `SELECT 1 FROM jobs WHERE repository_id = ? AND state IN ('running','cancel_requested')`. Keep the index as the backstop.
- Also check `transition_job`: `error_code = ?` is overwritten with `NULL` on every transition that passes `None`. For example `cancel_requested -> canceled` drops a recorded code. Decide whether that is intended.

### F6 - LOW/INFO: transition edges marked (*)
Spec text: `queued -> running -> succeeded|partial|failed|canceled|interrupted`, `queued -> canceled`, `interrupted -> queued` after reconciliation. `waiting_for_approval`, `waiting_for_retry` and `cancel_requested` are described as durable or request states without explicit edges.
- `running -> cancel_requested`: justified (the spec says "UI may say Canceling" and `cancel_requested` is in the active-job index).
- `cancel_requested -> canceled|succeeded|partial|failed|interrupted`: justified (a cancel can race with completion; recovery uses `interrupted`).
- `running -> waiting_for_approval|waiting_for_retry` and `waiting_* -> queued|canceled`: a reasonable reading, but it is an interpretation and is not in the spec.
  - The source of `waiting_for_approval` could equally be `queued` (approval before any run), and that edge is absent.
  - `waiting_for_retry -> queued` is plausible, but the spec says a retry creates a new attempt.
  - `interrupted -> canceled` is absent, so an interrupted job cannot be abandoned.
- Recommendation: record these edges in a change request or ledger entry for the owner to ratify before the scheduler task builds on them. They are persistence-level only and cause no invented UI or contract surface. The caller's obligation to reconcile before `interrupted -> queued` is documented but not enforced.

### F7 - LOW: secret handling and `credential_ref`
- `credential_ref` is limited to `cred:<id>` with `is_id`, at most 64 characters, and known token prefixes are rejected. No secret-named columns exist (DDL is identical to the reference plus the `revisions` seed row). `disconnect_account` clears the ref and keeps the row, as DATA_MODEL.md requires.
- Limits:
  - An unprefixed token such as 40 hex characters would pass as `cred:<40hex>`. The filter is best-effort.
  - A `credential_ref` is accepted when `storage_mode` is `none` or `session_only`. Consider requiring `keyring` for a non-null ref.
  - `append_activity` validates `error_code` for nothing (it is bound as given), unlike `jobs`. This is a free-text sink. Apply the same `^[A-Z_]{1,64}$` check.
  - `looks_secret_bearing` is a documented defence in depth only.
  - `AUTH_STATES` and `provider_user_id` digits-only are validations not present in the reference DDL. They are plausible and low risk, but confirm the auth_state enumeration against `contracts/` before later tasks rely on it.

### F8 - OK: R-RUST
- No `unwrap`, `expect` or `panic!` in production code (grep, outside `tests.rs`); workspace lints deny them and clippy is clean. `unsafe_code = "forbid"` applies.
- Public API takes borrowed input structs and returns owned records. Newtype IDs are not used yet (string IDs validated by `is_id`). That is acceptable until the domain ID types are wired in.
- `Database` derives only `Clone` (no `Debug`). `StoreError` has `Debug`, but its messages are sanitized (SQLite code only, never SQL parameters or row contents). `NewAccount` has `Debug` and holds only a credential reference. No secret wrappers exist.
- Queries are static and parameterized (sqlx 0.9 `SqlSafeStr` enforces that at compile time). The only dynamic SQL is inside sqlx's migrator.

### F9 - OK / process: out-of-allowlist scaffolding
- `crates/infrastructure/Cargo.toml`, `src/lib.rs` and `Cargo.lock` are outside the allowlist. The allowlisted directory cannot exist without a crate, so the need is real.
- The report cites coordinator approval under OD-001/OD-002, but I could not find the approval record within the files I was asked to read. The coordinator must attach or confirm it.
- The Cargo.lock diff adds packages and disambiguates `sha2`, `digest` and similar entries. There are no version changes to existing packages.
- The dependency record (R-DEPS) is adequate: sqlx 0.9.0 from the stack policy, minimal features, no `sqlite` umbrella (no extension loading), no postgres or mysql in the build graph, `bundled` libsqlite3-sys. No cargo-audit or license scan is recorded. R-DEPS lists those as separate checks, so say so explicitly in the report (BLOCKED for release, not PASS).

## Required before acceptance
1. Fix F1 (BEGIN IMMEDIATE or equivalent, rollback handling, BUSY mapping) and add a concurrent-writer test.
2. Re-run the suite repeatedly (for example 30 or more iterations) and record the results. Update TASK_REPORT.md; "12 PASS" is currently not reproducible.
3. Extend the negative controls to the active-job index, the CAS and the revision bump. Add a downgrade or dirty-state check.
4. Cover or consciously accept F4 to F7. A tiny `error_code` validation in `append_activity` is cheap.
5. Have the coordinator ratify the (*) edges and attach the scaffolding approval record.

---

# Re-review after fixes (run-20261002-02)

## Outcome: ACCEPTED (review verdict; DONE remains the coordinator's call, with the open items below)

## Commands re-run
| Command | Exit | Result |
|---|---|---|
| `cargo test -p gbm-infrastructure --locked` x25 | 0 each | 25 of 25 passed, 15 tests each (previously 4 of 16 failed) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | clean |
| `cargo fmt --all -- --check` | 0 | clean |
| `python3 scripts/check_task_scope.py --task P03-T01 --repo-root . --base e121053` | 1 | `outOfScope` is exactly the 3 approved paths |
| grep for unwrap, expect, panic! in production code | - | none |

## Verification of fixes
- F1: closed. `Database::begin` uses `BEGIN IMMEDIATE`. `finish()` commits or explicitly rolls back. SQLITE_BUSY, LOCKED and BUSY_SNAPSHOT map to a retryable `StoreError::Busy`. The new multi-thread test is covered by the 25 clean runs. The coordinator's repeat-40.log shows 40 of 40, and its deferred-BEGIN control fails 3 of 3. I did not write my own concurrency probe beyond my earlier scratchpad experiment.
- F3: mostly closed. Controls C, D and B2 fail as they should. Controls B and E pass when only one layer is removed, because a second layer (unique index, sqlx `VersionMissing`) covers it. That is acceptable, and the log says so. A dirty-history test was added.
- F4: closed per the coordinator's report. The refusal checks (including the checksum) run before the backup, a failed backup is cleaned up, files are 0600, and a foreign database is inspected read-only without switching it to WAL. I did not individually re-audit those tests beyond the clean suite runs. The intermediate-version case is accepted with the backup as mitigation.
- F5: closed. The precheck runs inside the IMMEDIATE transaction, with the index and message mapping kept as a backstop.
- F6: the waiting_* edges are removed, which is the conservative option. The `cancel_requested` edges remain and are still unratified by the owner.
- F7: closed (storage_mode/ref coupling, hex-token refs, activity `error_code` validation).
- F9: the approval record exists (run-02/coordinator-approval.md). It is coordinator-level, not owner-level.

## Remaining items (non-blocking)
1. Owner ratification of the `running -> cancel_requested` and `cancel_requested -> terminal/interrupted` edges is still open. The scheduler task (P07) must define the waiting_* edges.
2. cargo-audit, cargo-deny and the license scan were not run (tools not installed). The security and dependency gate is BLOCKED for release, not PASS.
3. "No network inside transaction" is proven by API shape only, because there is no network code yet.
