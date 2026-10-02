# Review - P03-T02 (keyring and explicit session-only mode)

- Reviewer: independent review subagent designated by owner 2026-10-02
- Date: 2026-10-02. Base 58ee0cf, working tree (uncommitted) incl. untracked `crates/infrastructure/src/secrets/`, `crates/application/src/credentials.rs`.
- Read: AGENTS.md, rules 00/01/02/04/07/08, AUTH_AND_LIFECYCLE.md, DATA_MODEL.md, TASKS.json P03-T02, TESTS.json TEST-KEYRING/TEST-REDACTION, full diff, all new sources, TASK_REPORT.md and evidence files, and the sources of keyring-core 1.0.0 and zbus-secret-service-keyring-store 1.0.1.

## Outcome: CHANGES_REQUIRED (minor; no security defect found)

The implementation is sound and the no-fallback / no-leak design holds. The required changes are about honesty of the reported status, one vacuous assertion, and a missing approval record. None needs a redesign.

## Commands re-run (exit codes)

| Command | Exit |
|---|---|
| `cargo test -p gbm-infrastructure --locked` x3 | 0, 0, 0 (20 passed each; 5 secrets tests incl. 3 isolated keyring scenarios) |
| `cargo test -p gbm-application --locked` x2 | 0, 0 (12 passed each) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `python3 scripts/check_task_scope.py --task P03-T02 --repo-root . --base 58ee0cf` | 1 (status FAIL, outOfScope = exactly the 4 listed files; expected, see F1) |
| Test binary run with `env -i PATH=/nonexistent` (keyring scenario only, returns before spawning anything) | 2: fails loudly with "BLOCKED: dbus-run-session not installed" |

Owner keyring safety: before and after the runs, `~/.local/share/keyrings/*` mtimes are unchanged (2026-09-25 / 2026-02-24) and no extra gnome-keyring-daemon, dbus-daemon or gcr-prompter processes remain (only the owner's pre-existing daemon, started 2026-09-23). I did not touch D-Bus or the keyring directly. `cargo audit` and `cargo deny` are not installed, so the advisory/licence scans are NOT_RUN by me (see F8).

## Findings

### Medium

**F1. Out-of-allowlist edits: necessary and minimal, but approval is not evidenced.** (rules/00_CORE.md: approval must be "recorded in its evidence".)
- `crates/domain/src/contract/secret.rs`: `SecretString::new(String)`. Necessary (private field, Deserialize-only). Keeps no Clone/Serialize and redacted Debug. Acceptable.
- `crates/infrastructure/Cargo.toml`: + `gbm-application` (path), `keyring-core 1.0.0`, `zbus-secret-service-keyring-store 1.0.1`. Necessary. Infrastructure -> application matches "adapters implement application ports".
- `crates/infrastructure/src/lib.rs`: `pub mod secrets;`. Necessary (the allowlisted directory cannot be reached otherwise).
- `Cargo.lock`: 585 insertions, 0 deletions, so no existing version changed. Consistent with `new-crates.txt`.
- Problem: TASK_REPORT.md labels the table "Coordinator-approved" but there is no approval record (id, message reference or owner statement) in the evidence directory. Only `scope-check.json` (status FAIL) exists. Add a reference to the actual coordinator/owner approval, or have the coordinator record it. I cannot verify it from the artifacts.

**F2. TEST-KEYRING "PASS" is overstated for "disconnect preserves backups".** TESTS.json `then` requires "disconnect preserves backups". No test touches any backup, mapping or history row. `disconnect_account` has no capability to delete backups (true by construction), but the only `AccountDirectory` implementations are test fakes, so the property "mark disconnected keeps the account row, mappings and backups" is unproven, and the SQLite implementation is not delivered. Reword the status to PARTIAL (store/reconnect/disconnect of secrets: PASS at adapter and use-case level; backup preservation: by construction only, NOT_RUN end to end until the `AccountDirectory` implementation exists). The report already lists the missing `AccountDirectory` impl as a gap, but the scenario status line does not reflect it.

**F3. Disconnect lifecycle gap not disclosed.** AUTH_AND_LIFECYCLE.md: disconnect "cancels or pauses affected jobs, removes local keyring entries, marks the account unavailable". `disconnect_account` does only the last two. Job cancel/pause is not in the report's "Not delivered" list (only zeroization, AccountDirectory impl, wiring and the rotation lock are). Add it to the gaps and to the owning later task.

### Low

**F4. Vacuous no-fallback assertion** (`secrets/tests.rs`, `locked_keyring_is_reported_without_fallback`). The `SessionStore` is created but never passed to `store_credential`, so `session.get == NotFound` proves nothing, yet the report says "nothing stored anywhere". Real no-fallback is guaranteed structurally (`store_credential` takes exactly one store and returns its error), and the application test `keyring_failures_never_fall_back` covers that with fakes, which is adequate. Fix the claim, or make the assertion meaningful by scanning the sandbox HOME (parent side, after the child exits) for the sentinel in the locked scenario, and in the available scenario to confirm no plaintext file. The sandbox is deleted on drop, so the scan must be before drop.

**F5. Error mapping is coarser than the variant names imply** (verified in the store source).
- `NoStorageAccess` is produced for `Locked`, `Prompt` (dismissed or failed prompt) and `NoResult` (e.g. no default collection). "KeyringLocked" is right for the tested case but also covers "prompt dismissed" and "no default collection". Safe: both paths refuse to store.
- `PlatformFailure` also covers any D-Bus method error. In `create_item` the store maps every error with `platform_failure`, so a locked collection or dismissed prompt at item-creation time (when the earlier `unlock` succeeded and the collection re-locks) surfaces as `KeyringUnavailable`, not Locked. UI guidance built on these variants could be wrong in such cases. Document that the two variants are best-effort hints for guidance, never a basis for a security decision. Not worth a direct `secret-service` dependency for downcasting now.
- `Ambiguous` (several items with same service+username, e.g. created by another app or a duplicate) maps to `Backend`. On `put`/`get` this fails closed (good). On `delete` it means disconnect can never succeed for that account and the user has no guidance. Unlikely (service `git-backup-manager` is app-specific), but untested. Either add a distinct variant or test and document the behaviour. Items from other apps with additional attributes are matched by search (documented upstream); acceptable given the namespaced service. The `label` modifier is only applied at creation; rotation keeps the old label. Harmless.

**F6. Isolation guard is weaker than the isolation itself.** The isolation is correct: `env_clear()`, new `dbus-run-session` bus, sandbox HOME/XDG_DATA_HOME/XDG_RUNTIME_DIR (the owner's daemon uses `/run/user/1000/keyring`, never reached), no DISPLAY (so no prompt can appear in the owner's session; the log shows gcr-prompter failing on "cannot open display"), the "unavailable" scenario points DBUS_SESSION_BUS_ADDRESS at a non-existent socket with no fallback, and missing tools fail loudly (verified). But `assert_isolated()` only checks that HOME is under `temp_dir()`. A hand-set `GBM_KEYRING_SCENARIO=available` with `HOME=/tmp/x` and the owner's `DBUS_SESSION_BUS_ADDRESS` would pass the guard and hit the owner's keyring (the entry user is `cred:acc-1`, and the test deletes it). Suggest the guard also require `XDG_RUNTIME_DIR` under the temp dir and `DBUS_SESSION_BUS_ADDRESS` not equal to the parent-inherited value (or a nonce env var set only by `run_isolated`). Low because it needs deliberate misuse. The `discover_other_daemon: 1` line in the control log is unexplained, but env/control dir are isolated and owner keyring mtimes are unchanged.

**F7. Locked and failure paths tested only partially at the real-keyring level.** Locked is tested for `put` only. `get` and `delete` while locked (the disconnect-while-locked case) are tested only with fakes. Disconnect fails and leaves the account connected when the keyring is locked, which matches "handle failure cleanly" but means a user with a permanently unavailable keyring cannot disconnect locally. That may be a legitimate owner decision (force local disconnect); flag it for the command task. Also no test for `mark_disconnected` failing after a successful delete (retry converges because delete is idempotent; worth a one-line test).

**F8. Dependencies (R-DEPS).** Choice is right: keyring-core plus a store crate (as keyring 4 docs advise), `rt-async-io-crypto-rust` (zbus with its own executor; `cargo tree` shows no tokio under the store crate; pure-Rust crypto, no OpenSSL, no libdbus). `EncryptionType::Dh` is used, so secrets are not sent in the clear over the bus (the 1024-bit DH Secret Service transport is the standard session-bus scheme; consistent with R-SEC's "keyring is not protection against a compromised session"). 126 unique transitive crates are added; this is sizeable but justified in the report with alternatives. Licence and advisory scans were not run by the implementer or by me (tools absent): record as NOT_RUN/BLOCKED-for-release per R-DEPS, do not imply they passed.

**F9. Minor.** (a) `MAX_SECRET_CHARS = 8192` is an unexplained constant (R-FACT); add a one-line rationale. (b) `CredentialRef::parse` is looser than the SQLite `validate_credential_ref` (no token-prefix or token-like-hex rejection). The DB validator still blocks bad refs, so no leak, but the rule exists in two places and can drift. (c) The upstream store `expect()`s on a poisoned mutex, so a panic inside a keyring call poisons later calls into a panic; callers on blocking workers must treat a panic as `Backend` and not retry in a loop. The panic message contains no secret.

## Checks that passed (no action)

- No-fallback: `store_credential` writes only to the store passed in and returns its error; there is no file, SQLite or implicit session path anywhere in the diff (grep of new code: no `fs::write`, no sqlite use). `KeyringStore::connect` failure is returned as `KeyringUnavailable`.
- Secret exposure: `map_error` never formats the keyring error (`BadEncoding` and `BadDataFormat` hold raw secret bytes; `Ambiguous` holds entries; `Display` of `PlatformFailure` may carry backend text). `CredentialError` is `Copy` with static strings. `KeyringStore` and `SessionStore` Debug are redacted; `SecretString` has no Serialize/Clone. The parent test checks that the child's stdout and stderr never contain the sentinel; `keyring_errors_are_mapped_without_secret_bytes` injects the sentinel into every secret-carrying variant. Test-only `assert_eq!` on secret values can print the sentinel on failure; acceptable (test constants).
- Disconnect order: delete secret first, then mark disconnected. A failure at step 1 leaves the account intact and retryable; a failure at step 2 is retryable because delete is idempotent. This matches "removes local keyring entries, marks the account unavailable" and DATA_MODEL ("remove the credential reference and mark disconnected rather than cascading"). It does not claim provider token revocation.
- Isolation controls (`isolation-controls.log`) show the child scenarios genuinely run and fail when expectations are wrong. 15/15 repeats recorded; my 3 runs also passed.
- `unsafe`, `unwrap`, `expect` and `panic!` are absent from production paths. Clippy `-D warnings` is clean.

## Honesty of scenario statuses and documented gaps

- TEST-REDACTION "PARTIAL" is honest. TEST-KEYRING "PASS" must be reworded per F2. "UI-level session-only choice NOT_RUN" and "real desktop unlock prompt not exercised" are honest.
- Zeroization gap: honestly documented (`String` copies in `SessionStore::put/get`, keyring-core `set_password`, `SecretString`). Not a blocker under R-SEC (which does not mandate zeroization), but should stay in the handoff for P03-T03.
- `AccountDirectory` not implemented: honestly documented, but see F2 for its effect on the TEST-KEYRING claim.
- TESTS.json and state/IMPLEMENTATION.json were not updated (`implementationStatus` still NOT_IMPLEMENTED, task IN_PROGRESS, evidence []). The DoD item "Updated traceability and handoff" is pending; that is the coordinator's step after acceptance.

## Required before ACCEPTED

1. F1: add a reference to the recorded coordinator/owner approval of the 4 out-of-allowlist edits.
2. F2 and F3: correct the TEST-KEYRING status text to PARTIAL (backup preservation by construction only) and add job cancel/pause to the "Not delivered" list.
3. F4: fix or retract the "nothing stored anywhere" claim (preferably a sandbox sentinel scan).

Recommended (not blocking): F5 (document or add `Ambiguous` handling), F6 (stronger isolation guard), F7 (`mark_disconnected` failure test), F8 (record scans as NOT_RUN), F9.

---

# Re-review (after fixes, evidence run-20261002-02)

Re-verified by the same independent reviewer on 2026-10-02. I read the changed sources, `coordinator-approval.md`, `plaintext-scan-control.log` and the "Post-review fixes" section of TASK_REPORT.md.

## Commands re-run (exit codes)

| Command | Exit |
|---|---|
| `cargo test -p gbm-infrastructure --locked` x3 | 0, 0, 0 (20 passed each) |
| `cargo test -p gbm-application --locked` x2 | 0, 0 (12 passed each) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `python3 scripts/check_task_scope.py ... --base 58ee0cf` | 1 (FAIL as expected; outOfScope is exactly the 4 approved paths) |

Owner keyring: file mtimes unchanged, and only the owner's pre-existing daemon is running. I did not touch D-Bus or the keyring.

## Required items

- F1: closed. `coordinator-approval.md` lists the 4 paths with conditions. It states that it is coordinator approval, not owner approval, and records the audit/licence scans as NOT_RUN. That is honest.
- F2: closed. TEST-KEYRING is PARTIAL, with backup preservation "by construction, not tested end to end".
- F3: closed. Job cancel/pause is in the not-delivered list, and forced local disconnect while locked is noted as an owner decision.
- F4: closed. The vacuous assertion is gone. After every isolated scenario the parent scans the whole sandbox HOME for the plaintext sentinel, before the sandbox is dropped (`run_isolated` -> `plaintext_hits`). The control log shows a planted `fallback.txt` makes the test fail. The scan covers HOME, which includes XDG_DATA_HOME where the isolated keyring files live.

## Recommended items

- F5: documented in the `map_error` doc comment (best-effort guidance; `Ambiguous` fails closed as Backend). Still untested, which the report states.
- F6: `assert_isolated` now also checks XDG_RUNTIME_DIR under the temp dir and rejects a bus address containing `/run/user/`. This is adequate. Residual: an unset XDG_RUNTIME_DIR or bus variable passes the guard (`map_or(true, ..)`, `unwrap_or_default`). An unset bus would make zbus fail rather than reach the owner's session, so this is a negligible risk.
- F9: `CredentialRef::parse` now mirrors the SQLite token-prefix and long-hex rules (prefix lists are identical, 8 entries each) and has a test (`cred:glpat-x`). The rules still exist in two places and can drift. This is acceptable for now, with a shared helper as a later cleanup.
- F7 (mark_disconnected failure test) and F8 (scans NOT_RUN) remain open. Both are non-blocking and honestly recorded.

## Final outcome: ACCEPTED

All three required changes are resolved and verified by re-execution. Remaining items are non-blocking and disclosed: no end-to-end backup-preservation test, no job cancel/pause on disconnect, the zeroization gap, license/advisory scans NOT_RUN, and the untested `Ambiguous` path. TEST-KEYRING stays PARTIAL and TEST-REDACTION stays PARTIAL. Neither should be marked fully passed until the later tasks cover them. The coordinator still owns the DoD traceability and state updates.
