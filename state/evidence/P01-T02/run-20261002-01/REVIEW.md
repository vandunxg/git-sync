# Review - P01-T02 (DTOs, narrow IPC facade, FE/native parity)

- Reviewer: independent review subagent designated by the owner, 2026-10-02 (not the implementer).
- Base: 88ee9dd plus untracked files (`git status -uall`). Nothing was edited except this file.
- Read: AGENTS.md, EXECUTION_PROTOCOL, rules 00/01/02/03/04, IPC.md, ipc.v1.json, contract_checks.py,
  the TASKS.json entry, TEST-IPC and TEST-CONTRACT-FIXTURES, contracts/codegen.py, and all hand-written
  Rust and TS. Generated files were reviewed through the generator plus selected excerpts of generated.rs.

## Outcome: ACCEPTED (with non-blocking findings F1-F4 to be fixed or recorded before DONE)

No blocker. The facade, the envelope handling and the secret handling hold up. The one real
semantic gap is an untested validator divergence (F1). The scope-check guard still reports FAIL
because of the three approved out-of-allowlist files. That is expected, and the coordinator must
accept it explicitly.

## Commands re-run by the reviewer

| Command | cwd | Exit |
|---|---|---|
| `python3 contracts/codegen.py --check` | repo root | 0 (`PASS_PARITY_GENERATED`) |
| `node apps/desktop/src/lib/native/contract-cases.selftest.ts` | repo root | 0 (PASS 1273 cases + 11 envelope checks) |
| `pnpm run typecheck` | apps/desktop | 0 |
| `pnpm run lint` | apps/desktop | 0 |
| `cargo fmt --all -- --check` | repo root | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | repo root | 0 |
| `cargo test --workspace --all-targets --locked` | repo root | 0 (application 6, domain unit 11, contract_cases 3, src-tauri 5, all ok) |
| `python3 scripts/check_task_scope.py --task P01-T02 --repo-root . --base 88ee9dd` | repo root | 1 (`outOfScope`: Cargo.lock, src-tauri/Cargo.toml, src-tauri/src/lib.rs; matches scope-check.json exactly) |
| `python3 scripts/lint_pack.py` (extra) | repo root | 1 (tsconfig JSONC and similar; confirms the report's tooling-gap claim) |

Ad hoc probes (read-only, no files written): Python oracle `validate` and Node `RegExp` on edge strings (see F1).

## 1. Out-of-allowlist edits (lib.rs, src-tauri/Cargo.toml, Cargo.lock)

Justified and minimal.
- lib.rs: +`mod commands;`, `.manage(Facade)`, `.invoke_handler(...)`. Tauri handlers are registered on
  the Builder in the bootstrap file, so this cannot live in `commands/`. The existing navigation guard
  is untouched.
- src-tauri/Cargo.toml: two path deps, `serde_json` (already locked), and a dev-only `tauri` `test`
  feature. The workspace uses `resolver = "3"`, so the dev feature does not leak into release builds.
- Cargo.lock: only new dependency edges for gbm-* and serde/serde_json. No new package or version.
  `domain` now depends directly on `serde`; it was already in the lock via tauri.
- The approval is recorded only in TASK_REPORT.md, written by the implementer. R-CORE asks for the
  coordinator approval to be recorded in the evidence, so the form is satisfied. I cannot verify it
  independently beyond the owner's instruction to me. The coordinator should confirm it in
  state/IMPLEMENTATION.json when it records acceptance.
- Direct dependency additions (rules/08) are limited to already-locked crates. OK.

## 2. Envelope and dispatch: correctness and leakage

Verified by reading and by the tests.
- Request keys are exactly `schemaVersion/requestId/args`. `schemaVersion` must be an integer 1
  (1.0 and "1" are rejected). `requestId` must match the Id pattern. `args` is validated against the
  command's request DTO before the handler runs (`handler_is_not_called_for_invalid_args`).
- The handler's response is validated against the response DTO, and the handler's AppError is validated
  too. Failures become INTERNAL with no handler content. Good defence in depth.
- Sensitive command (`gbm_credentials_validate_store`, the only `sensitive: true`, asserted in tests):
  no violation paths or envelope-key names are placed in `details`. Violation reasons are static
  strings, never values. The TS client drops transport-error text.
- `requestId` echo: a valid id is echoed on every failure after the key check. When the id is invalid
  the failure carries `null` and a native `native-N` correlation id.
- `NotYetImplemented` honestly returns UNSUPPORTED for all 31 commands. No placeholder data.
- Tauri router test, on the mock runtime and through the real invoke path: forbidden names, plugin
  names and a missing `request` argument are all transport errors. The registered list equals the contract.
- `capabilities/default.json` grants only `core:event:default`. App commands are not ACL-gated without
  an AppManifest, so containment depends on the navigation guard and CSP plus per-command native
  validation. This is consistent with R-SEC and the capability description. The report does not state
  that the installed Tauri permission behaviour for app commands was verified, though IPC.md asks P01 to
  verify it. See F4.

Minor issues (Low):
- Unknown command: dispatch returns `requestId: null` even when the request carried a valid id. This
  contradicts the new `envelopeRule` text ("null only when the request had no valid Id"). It is
  unreachable through Tauri, because only allowlisted names are registered. Either adjust the
  rule/test or parse the id first. (F3)
- `ValidatedRequest` derives `Debug` and `Clone`, and `args: serde_json::Value` carries the raw token in
  cleartext for the sensitive command. `SecretString` protects only the typed DTO. Nothing logs it today.
  Before the credentials task lands, the handler for the sensitive command should deserialize
  immediately, drop `Value`, and avoid `{:?}`. Consider a non-Debug wrapper for sensitive args. (F2)
- `dispatch` is sync CPU work inside `async` commands. This is fine now, but real handlers must use
  `spawn_blocking` or async I/O (R-RUST).
- `IPC.md` says native IDs are "validated and resolved natively". Here IDs are `Id = String` aliases
  validated by schema. The R-RUST newtypes (AccountId etc.) are not introduced. Acceptable for this
  task but should be tracked for the tasks that resolve IDs.

## 3. Validators: Rust vs Python oracle vs TS

Verdict parity is established only for the cases the generator emits. That set is 1273 cases, built
from missing/null/wrong-type/extra field, bad pattern, date-time, enum, min/max, const, maxItems and
maxLength mutations. Rust and TS both pass all of them, and Rust round-trips the 119 valid samples
through typed serde unchanged.

Divergences I confirmed that no shared case would catch:
- F1 (Medium) Rust vs TS: pattern `^refs/(heads|tags)/.+$`. TS (`RegExp` with the `u` flag, and `.`
  without `s`) rejects U+2028, U+2029, `\n` and `\r` in the ref remainder. Rust
  (`schema.rs::pattern_matches`) excludes only `\n` and `\r`, so `refs/heads/a\u{2028}b` is VALID in
  Rust and INVALID in TS. The Python oracle accepts all of these (`.` excludes only `\n`). The
  generated cases contain no such string, so the parity suite stays green. This is a genuine FE/native
  parity gap in the exact area the task is meant to guard. Fix: exclude `\u{2028}` and `\u{2029}` in the
  Rust matcher and add generator cases (`\r`, U+2028, U+2029, non-BMP character, multi-byte maxLength
  boundary).
- Date-time (Low, oracle looser): the oracle uses `datetime.fromisoformat` (Python 3.11+), which
  accepts `2026-01-01T00:00:00+0700`, `2026-01-01 00:00:00Z`, `20260101T000000Z`, `2026-01-01T00:00Z`,
  `...+07` and `2026-01-01t00:00:00Z` (all verified True in the oracle). Rust and TS correctly
  implement strict RFC 3339 and reject these (the `t` separator is accepted by all three). The shared
  cases use only clean bad inputs (`2026-01-01`, no tz, `not-a-date`, month 13) and one valid
  `+07:00` offset, so the oracle/native difference is undocumented and untested. Rust and TS agree
  with each other. Document it next to the `$` correction and add negative cases.
  Note that lowercase `z` and `t` are accepted by Rust and TS but the oracle rejects `z` (RFC 3339
  allows them), which is a further undocumented difference.
- Integers (Low): Rust rejects `1.0` and `1e21` (f64), TS accepts `1.0` and `1e21` (`Number.isInteger`).
  The oracle rejects `1.0`. From the real renderer `JSON.stringify(1.0)` is `1`, so this cannot occur
  over IPC, and `fraction:*` cases use 1.5. The case set cannot represent `1.0` through JSON-to-JS
  anyway. Acceptable; a comment would help.
- String length: Rust `chars().count()`, TS `[...value].length` and Python `len` all count code points.
  Consistent, but untested with non-BMP input (the maxLength case uses ASCII `y`).
- `const`/`enum`: the TS `deepEqual` and the Rust `Value` equality agree for the schema's usage
  (integer `1` and booleans). `anyOf`: all three use the same any-branch-valid logic. The generator
  restricts nullable `anyOf` to the 2-branch shape and fails hard otherwise.
- Unsupported patterns: Rust fails closed (`unsupported pattern`, with a test that every schema
  pattern has a matcher). TS compiles whatever pattern it finds. Safe because the generated schema is
  the same file.

## 4. ECMA `$` oracle correction

Sound and honestly documented. Python `re.search("^...$", "id\n")` matches and ECMA does not. The
generator corrects only that case (`ecma_dollar_violation`: Python match true and `\Z` match false) and
leaves `scripts/contract_checks.py` unmodified, with a proposed follow-up change request. Rust (`is_id`,
etc.) and TS (JS `$` semantics) both reject trailing `\n`, and the envelope tests cover `"x\n"`. Caveat:
the correction is scoped to `$` only, so `.`/`\r`/U+2028 (F1) and the date-time looseness remain. Also,
`cases.json` verdicts are therefore "oracle plus one override", which the report states. The report
should add the other divergences once F1 is fixed.

## 5. R-RUST

- Workspace lints deny `unwrap_used`/`expect_used`/`panic`/`todo`; `unsafe_code = "forbid"`. Clippy with
  `-D warnings --all-targets` is clean. I found no unwrap/expect/panic in production paths. The
  `OnceLock<Option<Value>>` plus `ok_or(SchemaError::Unavailable)` avoids a panic on a bad embedded
  schema.
- `SecretString`: no `Serialize` or `Clone`, `Debug` redacted, with a test. Secret-bearing DTOs derive
  only `Deserialize` (generator `has_secret`). `CredentialRequest` is the only one. Zeroize-on-drop
  is not implemented, which is a reasonable later hardening, not required by the rules.
- Domain does not depend on Tauri, reqwest or SQLite. OK. Tauri is confined to src-tauri.

## 6. R-FE

- Only `lib/native/client.ts` imports `@tauri-apps/api` (grep confirmed). Generated files are outside
  any feature code, and `cases.ts` is imported only by the self-test.
- No `any`, `@ts-ignore` or eslint disables in `lib/native` outside generated files. Casts: after
  validation (`raw.data as ...`, `raw.error as AppError`), two casts on the schema (`as unknown as
  Record`, a tuple cast on `.map(Number)`). These follow validation, so they are acceptable, but
  `validateDefinition` is a runtime check and not a type guard.
- `client.ts` header says "sends one validated request envelope", but args are not validated client-side
  before `invoke`. Only the native side validates, which is acceptable per IPC.md, but the comment is
  inaccurate. (F3)
- `failure` interpretation accepts `requestId: null` even when the client sent a valid id. Native does not
  produce that today. It is lenient and loses correlation, so consider tightening.
- Zod is not used. The task does not touch package.json, and R-FE requires Zod for forms, not the IPC
  boundary. OK.

## 7. Report vs evidence; scenario statuses

Accurate, with these corrections:
- Counts: generated.rs has 60 `$defs`, of which 56 are objects (the report says "60 DTOs"). 33 enums
  is correct.
- checks.log shows `python3 scripts/lint_pack.py | head -3` printing `"status": "FAIL"` with
  `exit=0`, which is the pipeline's exit status and misleading on its own. TASK_REPORT.md explains
  the failure (tsconfig JSONC/node_modules), I reproduced it (exit 1), and blueprint-lint.txt ran lint
  on a reduced "blueprint-only view". That proves pack structure only and is correctly labelled
  `PASS_BLUEPRINT_STRUCTURE_ONLY`. It is not a repo-wide lint pass. Owner should schedule the
  `lint_pack.py` tooling fix. Do not treat this as a pass.
- scope-check.json is `FAIL` (3 approved out-of-scope paths). The report does not hide it.
- The checks.log records commands and exit codes but not per-command timestamps, commit/diff identifier
  or environment (EXECUTION_PROTOCOL section 5). Only the header has a time and base. Low.
- Drift control: codegen-drift-control.txt shows exit 1 on drift and 0 after regeneration, as claimed.
- CSP/IPC probe: csp-ipc-probe.png shows
  `PROBE ok=false code=UNSUPPORTED requestId=echoed details=gbm_app_bootstrap is not implemented yet`.
  It is consistent with the claim that IPC works under the strict CSP in WebKitGTK. The probe source was
  reverted (`main.tsx` has no diff). The probe was run by the implementer, and I did not relaunch a GUI.
  The v1 and v2 failed attempts are recorded honestly.
- Scenario statuses are honest. TEST-IPC "PASS for implemented scope" is supported by the router and
  dispatch tests. TEST-CONTRACT-FIXTURES "PARTIAL" is correct, and the "totals match the same revision"
  part is correctly NOT_RUN. I would not mark TEST-CONTRACT-FIXTURES passed until F1 is closed.

## Findings summary

| ID | Severity | Finding | Action |
|---|---|---|---|
| F1 | Medium | Rust accepts U+2028/U+2029 in `refs/...` remainder, TS rejects. Shared cases miss it (also missing: `\r`, non-BMP length, date-time forms the oracle accepts but RFC 3339 rejects) | Fix Rust matcher, extend generator cases, document oracle date-time looseness beside the `$` note |
| F2 | Low-Med | Sensitive args live as `Value` in `ValidatedRequest` (Debug+Clone) and `handle(args: Value)` | Credentials task: deserialize at once into `CredentialRequest`, never format `args`; consider a non-Debug wrapper |
| F3 | Low | Unknown-command failure returns `requestId: null` despite the new rule; `client.ts` comment says args are "validated" (they are not) | Align rule/test or id handling; fix comment |
| F4 | Low | Report does not state the verified Tauri ACL behaviour for app commands (IPC.md requires verification); state/evidence time/identifier gaps; lint_pack tooling gap | Add a short note and a tooling follow-up |

Remaining before DONE: the coordinator accepts the 3 out-of-scope edits and updates
state/IMPLEMENTATION.json; F1 is fixed or explicitly deferred with an owner decision.

---

## Re-review after post-review fixes (run-20261002-02)

Reviewer: same independent review subagent, 2026-10-02.

Re-run by the reviewer (all exit 0 unless noted): `codegen.py --check` (PASS_PARITY_GENERATED); node
selftest (PASS 1619 cases + 11 envelope checks); `pnpm run typecheck`; `pnpm run lint`;
`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`;
`cargo test --workspace --all-targets --locked` (application 6, domain unit 12, contract_cases 3,
src-tauri 5, all ok). `check_task_scope.py` still exits 1 with exactly the 3 approved out-of-allowlist
paths (expected; coordinator accepts them in IMPLEMENTATION.json on DONE). Diff vs 88ee9dd stat is unchanged
for tracked files.

- F1 CLOSED. `schema.rs` now excludes U+2028/U+2029 (plus `\n`, `\r`) in the refs matcher with a unit test.
  Cases grew to 1619 (140 valid) and include line-terminator/astral insertion per patterned field, extra
  date-time forms (`+0700`, space separator, `+07`, `T00:00Z`, compact, Feb 29 in a non-leap year) and
  non-BMP maxLength. The two negative controls (old Rust matcher gives 4 mismatches, exit 101; TS UTF-16
  length gives 19 failures, exit 1) show the suite now detects both bug classes. Oracle/contract
  differences are covered by the generalized `strict_violation` override and documented in the report.
- F2 CLOSED for this task. `ValidatedRequest` no longer derives Debug/Clone (confirmed in envelope.rs).
  The handler obligation to deserialize at once carries to P03-T02.
- F3 CLOSED. Unknown commands now echo a valid requestId (ipc.rs); the client comment was corrected.
- F4 CLOSED. tauri-acl-app-commands.txt cites tauri 2.12.1 source: without an AppManifest, app commands
  are accepted only from local origins and remote origins are rejected. run-02 checks.log uses pipefail and
  per-command timestamps. Repo-root `lint_pack.py` remains a logged tooling gap (exit 1), not a pass.
  The report's "60 DTOs" was corrected to 56 object DTOs + 4 scalar/enum defs.
- Remaining low note: the Rust-vs-TS integer `1.0`/`1e21` difference is unreachable over JSON IPC; accepted.

### Final outcome: ACCEPTED

No open findings that block REVIEW_READY to DONE. Pre-DONE actions for the coordinator: record acceptance of
the 3 out-of-allowlist edits and update state/IMPLEMENTATION.json; keep TEST-CONTRACT-FIXTURES as PARTIAL
because the "totals match the same revision" part is NOT_RUN.
