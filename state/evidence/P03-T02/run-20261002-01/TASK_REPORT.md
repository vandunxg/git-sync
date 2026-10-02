# Task report - P03-T02

- Task / phase / requirement IDs: P03-T02 / P03 / REQ-12, REQ-39
- Validation kind: implementation. Base commit 58ee0cf. Host: EndeavourOS, XFCE/X11, gnome-keyring 50.0.
- Permitted: crates/infrastructure/src/secrets/, crates/application/ (+ state/).

## Coordinator-approved out-of-allowlist edits (scope-check.json lists exactly these)

| Path | Change | Reason |
|---|---|---|
| crates/domain/src/contract/secret.rs | `SecretString::new(String)` | adapters must wrap secrets read from the keyring; the type had no constructor |
| crates/infrastructure/Cargo.toml | + gbm-application, keyring-core 1.0.0, zbus-secret-service-keyring-store 1.0.1 [rt-async-io-crypto-rust] | adapter dependencies |
| crates/infrastructure/src/lib.rs | `pub mod secrets;` | module root |
| Cargo.lock | +50 packages (new-crates.txt), no existing versions changed | follows from manifest |

## Dependency proof (R-DEPS)

keyring 4.2.0's own docs say applications should link `keyring-core` plus the specific store instead of
`keyring`; done so. keyring-core 1.0.0 (2026-04-21), zbus-secret-service-keyring-store 1.0.1 (2026-08-15),
secret-service 5.2.0, zbus 5.19.0: MIT/Apache-2.0 or MIT, rust-version <= 1.88. Feature
`rt-async-io-crypto-rust`: zbus uses its own async-io executor (no dependency on a caller's tokio runtime)
and pure-Rust crypto (no OpenSSL). Alternatives: `dbus-secret-service` store (libdbus C dependency), the
`keyring` v1 facade (pulls Windows/macOS stores). A throwaway probe (scratchpad, not committed) verified
set/get/delete/NoEntry in an isolated session, `PlatformFailure` without a bus, and `NoStorageAccess`
(fast, no hang) when the collection is locked and no prompt can be shown.
Important API fact: `keyring_core::Error::BadEncoding/BadDataFormat` contain the raw secret bytes; the
adapter never formats keyring errors and maps them to sanitized `CredentialError` variants.

## Delivered

- application::credentials: `StorageMode`, `CredentialRef` (`cred:<id>`), sanitized `CredentialError`,
  `CredentialStore` and `AccountDirectory` ports, `store_credential` (validates the secret, uses only the
  chosen store, keyring failure is returned, never retried elsewhere) and `disconnect_account` (delete the
  secret first, then mark disconnected; failure leaves the account intact and retryable; backups/mappings
  untouched; no provider revocation).
- infrastructure::secrets: `KeyringStore` (Secret Service, service `git-backup-manager`, user = credential
  ref, item label "Git Backup Manager credential", idempotent delete, redacted Debug) and `SessionStore`
  (process memory only, Debug shows only the count).
- No plaintext or file store exists anywhere. SQLite keeps only `cred:` refs (P03-T01 enforces the format).

Not delivered: wiring into the Tauri credential command (P03-T03 validates PATs first), `AccountDirectory`
implementation for the SQLite `Database` (sqlite/ is outside this task's allowlist; P03-T03 or the command
task), memory zeroization of secrets (no zeroize dependency; secrets live in `String`s; recorded as a gap),
per-account rotation lock (needs the command layer).

## Tests

- application (6 new, fakes are `#[cfg(test)]` only): no fallback on Unavailable/Locked, explicit session store,
  invalid secrets/refs, disconnect order, failed deletion keeps account connected, no secret in Debug/Display.
- infrastructure secrets (5): real Secret Service in an isolated `dbus-run-session` + fresh gnome-keyring:
  store, reconnect (new connection reads it), rotate, disconnect, idempotent delete; locked collection ->
  `KeyringLocked` and nothing stored anywhere; no session bus -> `KeyringUnavailable`; session store
  semantics; error mapping drops secret bytes. The child asserts HOME is under the temp dir before touching any
  keyring; the parent asserts the sentinel never appears in child stdout/stderr.
- isolation-controls.log: wrong expected value in the child and wrong expected error in the locked child both
  make the parent test fail (dbus-daemon/gcr-prompter output proves the child really ran).
- owner-keyring-untouched.txt: owner's keyring files' mtimes predate this task.
- run_gates core exit 0, frontend exit 0; infrastructure suite 15/15 repeated runs pass.

## Scenario status

- TEST-KEYRING: PARTIAL (see post-review F2) - store/reconnect/disconnect with keyring available, locked and unavailable, at adapter +
  use-case level. UI-level explicit session-only choice is NOT_RUN (no UI yet). Real desktop unlock prompt
  (gcr on X11) not exercised - only the headless prompt-failure path.
- TEST-REDACTION: PARTIAL - sentinels checked in credential errors, Debug output and child process output; DB,
  logs, events, exports, argv and remote URLs are covered by later tasks.

## State: REVIEW_READY

## Post-review fixes (CHANGES_REQUIRED, minor) -> run-20261002-02

- F1: approval record run-02/coordinator-approval.md. Scope guard exits 1 listing exactly the 4 approved paths.
- F2: TEST-KEYRING downgraded to PARTIAL: "disconnect preserves backups" holds by construction (use case touches
  only the secret store and AccountDirectory; P03-T01 keeps the account row) but is not tested end to end
  because no production AccountDirectory implementation exists yet.
- F3: NOT delivered (added): AUTH_AND_LIFECYCLE.md "disconnect cancels or pauses affected jobs" - needs the
  scheduler/command layer (P07 / credential command task). Also open: whether a locked keyring should allow a
  forced local disconnect (owner decision for the command task).
- F4: the vacuous session assertion was removed; instead the parent scans the whole isolated HOME for a
  plaintext copy of the sentinel after every isolated scenario. Control: a child writing the sentinel to
  HOME/fallback.txt makes the test fail (plaintext-scan-control.log).
- F5: mapping documented as best-effort UI guidance; `Ambiguous` fails closed as Backend (documented, untested).
- F6: child guard also requires XDG_RUNTIME_DIR under the temp dir and refuses a `/run/user/` bus address.
- F9: `CredentialRef::parse` now applies the same token-prefix and long-hex rules as the SQLite adapter (tested);
  8192 limit documented as `CredentialRequest.token.maxLength`. Upstream store's poisoned-mutex panic: noted.
- License/advisory scans for the 126 transitive crates: NOT_RUN (no cargo-audit/cargo-deny).
run-02: gates core/frontend exit 0; infrastructure 20 tests, application 12 tests pass; owner keyring mtimes unchanged.
