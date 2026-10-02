# R-RUST - native implementation rules

Applies to all Rust work. Architecture and app contracts are specified separately.

## Types and boundaries

Use newtypes for AccountId, RepositoryId, JobId, PlanId, SnapshotId and validated paths.
Model finite states with enums and validate transitions centrally. Avoid boolean
parameter soup and stringly typed status dispatch. Provider payload types stay in
adapters; domain types do not depend on Tauri, reqwest or SQLite row structs.

Use `Result<T, E>` for recoverable errors and explicit conversion at boundaries.
Use typed domain errors; attach context internally while returning sanitized stable
error codes to the renderer. Do not derive Debug or Serialize for secret wrappers.
Never log raw reqwest errors/URLs without redaction review.

## Ownership and maintainability

Prefer owned data at task boundaries and borrowed data within synchronous algorithms.
Do not add Arc<Mutex<...>> as a universal data model. No MutexGuard or database
transaction held across HTTP/Git awaits. Prefer direct functions and small traits at
external boundaries; avoid a trait for every struct or premature plugin frameworks.

No `unsafe` in application code without a reviewed ADR, proof of necessity, documented
invariants and targeted tests. No unsafe Send/Sync implementations to silence errors.
Avoid `.unwrap()`, `.expect()`, `panic!`, `todo!`, `unimplemented!` in production paths.
In tests and statically proven initialization invariants, a narrow documented exception
is acceptable. Never use a global clippy allow to conceal a real error path.

## Async and resource lifecycle

Use Tokio async networking/process APIs. Run blocking keyring/filesystem/hash work on
bounded blocking workers when it would otherwise stall the runtime. Bound job, network,
process-output and event queues. Acquire per-repo locks before mutation; release with
RAII. Define deadlines, retry classification, cancellation and shutdown paths.

Every spawned task has an owner and observed result. No detached forever loops.
Cancellation must terminate and reap the Git process tree and drain pipes; merely
dropping an async future or child handle is not a termination guarantee [S08]. Never
load unlimited stdout/stderr into memory. Parse machine-readable Git output and bound
individual lines; drain stdout and stderr concurrently to avoid pipe deadlocks.

## Filesystem and persistence

Use Path/PathBuf and a validated root-relative handle; do not concatenate shell paths.
Check symlink traversal and app ownership before mutations. No deletion of user-owned
folders. Network requests do not occur inside SQLite transactions. Use parameterized
queries, bounded busy timeout, versioned migrations and a migration rollback/recovery
plan. A local DB should not be placed on a network filesystem [S13].

## Verification

Run rustfmt, clippy with warnings denied for the selected workspace targets, unit
and integration tests. Test failure and cancellation paths, not only the happy path.
Property tests are useful for ref diffs, status aggregation and path normalization;
introduce their dependency only after P00/version review. Test public domain invariants
without requiring live credentials. See [test strategy](../docs/quality/TEST_STRATEGY.md).

References: [S07 Rust API Guidelines; S08 Tokio; S13 SQLite](../sources/REFERENCES.md).
These rules are project choices; cited documentation explains the relevant APIs.
