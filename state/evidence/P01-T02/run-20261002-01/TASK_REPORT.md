# Task report - P01-T02

- Task / phase / requirement IDs: P01-T02 / P01 / REQ-13, REQ-36
- Validation kind: implementation. Base commit 88ee9dd. Host: EndeavourOS, XFCE/X11.
- Permitted: contracts/, crates/domain/, crates/application/, apps/desktop/src-tauri/src/commands/,
  apps/desktop/src/lib/native/ (+ state/).

## Coordinator-approved out-of-allowlist edits (scope-check.json lists exactly these)

| Path | Change | Why it cannot live in the allowlist |
|---|---|---|
| apps/desktop/src-tauri/src/lib.rs | `mod commands;`, `.manage(Facade)`, `.invoke_handler(commands::invoke_handler())`; doc line | Tauri registers handlers on the Builder in the bootstrap file; commands/ cannot register itself |
| apps/desktop/src-tauri/Cargo.toml | deps gbm-application, gbm-domain, serde_json 1.0.151; dev-dep tauri `test` feature | commands/ must link the facade; mock-runtime IPC router test |
| Cargo.lock | new edges only (serde/serde_json already locked via tauri; no new package versions) | follows from the manifests above |

Approval: coordinator (this session), recorded here per R-CORE "Minimal complete unit". Reviewer to confirm.

## Delivered

- `contracts/codegen.py`: one generator, two outputs from the same schema:
  `crates/domain/src/contract/generated.rs` (60 DTOs as serde structs with `deny_unknown_fields`,
  33 enums, 31-command table, forbidden list, `typed_roundtrip`) and
  `apps/desktop/src/lib/native/generated/contract.ts` (types, `CommandMap`, `COMMANDS`, embedded schema).
  Also `contracts/fixtures/cases.json` + `generated/cases.ts`: 1273 shared cases (119 valid, 1154 invalid
  mutations: missing/null/wrong-type/extra field, pattern, date-time, enum, bounds, const, maxItems,
  maxLength). Verdicts come from `scripts/contract_checks.py` (oracle). `--check` fails on drift.
- Native: `domain::contract::schema` (subset validator, hand-written matchers for exactly the 4 schema
  patterns, unknown pattern fails closed, RFC 3339), `envelope` (exactly `schemaVersion/requestId/args`,
  version 1 as integer, `Id` requestId, args validated; result has exactly one of data/error),
  `secret::SecretString` (no Serialize/Clone, redacted Debug; secret DTOs derive only Deserialize).
- `application::ipc::dispatch`: unknown/forbidden names -> UNSUPPORTED; invalid envelope ->
  CONTRACT_MISMATCH (no arg details for the sensitive credential command); handler output validated
  against the response DTO before leaving native (else INTERNAL); invalid handler errors replaced.
  `NotYetImplemented` answers UNSUPPORTED for all 31 commands (honest, no placeholder data).
- Tauri `commands/`: 31 async commands from one macro list taking `request` and returning the envelope.
- Renderer `lib/native/`: `validate.ts` (same semantics: code-point lengths, ECMA patterns), `client.ts`
  (`callNative` sends envelope with UUID requestId; transport errors -> INTERNAL failure without details;
  `interpretResponse` rejects both/neither payload, wrong requestId, invalid data/error).
- `ipc.v1.json` envelopeRule: documents `requestId: null` only on failures without a valid requestId.

Not delivered: any real command behaviour (later tasks), Zod (package.json is outside this allowlist; the
schema-driven validator covers runtime validation), Vitest wiring of the self-test (P01-T03).

## Known oracle divergence (documented, oracle not edited)

`contract_checks.py` uses Python `re.search`, where `$` also matches before a trailing newline. JSON Schema
patterns are ECMA-262, where it does not. codegen corrects only this case (`ecma_dollar_violation`), so
e.g. `"id\n"` is invalid in cases.json, Rust and TS. Proposed follow-up: change request for scripts/.

## Commands (checks.log after the final source edit; exit codes)

probe reverted 0; codegen --check 0; contract self-test (node) 0 = 1273 cases + 11 envelope checks;
pnpm install --frozen-lockfile 0; typecheck 0; lint 0; build 0; cargo fmt --check 0;
cargo clippy --workspace --all-targets --locked -D warnings 0; cargo test --workspace --all-targets --locked 0
(native contract cases, typed round trip, command table, envelope, schema, secret, dispatch, Tauri mock router,
navigation guard); pnpm tauri build --no-bundle 0; no PROBE text in src/dist.
codegen-drift-control.txt: appended drift -> `--check` exit 1, regenerate -> exit 0.
blueprint-lint.txt: `lint_pack.py --root <blueprint-only view>` exit 0. Running it on the repo root now FAILS
because it rglob()s app tsconfig (JSONC) and node_modules READMEs - tooling gap since P01-T01, recorded.

## Native IPC under production CSP (P01-T01 carry-forward)

Throwaway probe appended to main.tsx, release-built, run, then reverted (git diff of main.tsx empty;
final release rebuilt without it). csp-ipc-probe.png shows
`PROBE ok=false code=UNSUPPORTED requestId=echoed details=gbm_app_bootstrap is not implemented yet`:
WebKitGTK invoke -> Tauri router -> dispatch -> envelope -> interpretResponse works under the strict CSP.
Probe v1/v2 attempts recorded in csp-ipc-probe.txt (v1: document.title not mirrored to the window title;
v2: output rendered outside the overflow-hidden root).

## Scenario status

- TEST-IPC: PASS for implemented scope (malformed envelopes, extra fields, wrong version, arbitrary IDs/paths,
  forbidden names at dispatch and at Tauri router level). Renderer cannot reach shell/Git/fs: no such commands
  or plugins exist. Will be re-run as commands gain behaviour.
- TEST-CONTRACT-FIXTURES: PARTIAL. Schema/FE/native agreement on casing, nullability and enums PASS via shared
  cases; "summary totals match the same revision" NOT_RUN (no native data producers yet).

## State: REVIEW_READY

## Post-review fixes -> run-20261002-02

- F1 (Medium): Rust refs matcher now excludes U+2028/U+2029 (ECMA `.`). Generator correction generalized
  (`strict_violation`): ECMA pattern semantics (`.` excludes all line terminators, `$` end of input) and an
  RFC 3339 profile for date-time, stricter than the oracle's `fromisoformat` (oracle accepts `+0700`, space
  separator, `+07`, `T00:00Z`, compact form; contract rejects them). New cases: line-terminator/astral
  insertion into every patterned field, 6 extra date-time forms, non-BMP maxLength (exact and +1).
  Cases now 1619 (140 valid). Negative controls: pre-fix Rust matcher -> 4 mismatches, exit 101
  (refs-matcher-negative-control.txt); TS UTF-16 length bug -> 19 failures, exit 1 (ts-length-negative-control.txt).
- F2: `ValidatedRequest` no longer derives Debug/Clone (args may hold a secret); handlers must deserialize
  immediately. Carried to P03-T02 (credential command).
- F3: unknown commands echo a valid requestId; client header corrected; null requestId on failure is the
  documented envelopeRule.
- F4: Tauri ACL verified from tauri-2.12.1/src/webview/mod.rs:2078-2083 (tauri-acl-app-commands.txt): app
  commands without an AppManifest are allowed only from local origins; remote origins are rejected.
  Repo-root lint_pack remains a tooling gap (exit 1, logged); integer `1.0`/`1e21` Rust-vs-TS difference is
  unreachable over JSON IPC (serde_json parses them as floats; TS cannot distinguish) - noted, not changed.
- "60 DTOs" corrected: 60 schema defs = 56 object DTOs + 4 scalar/enum defs.
- run-02/checks.log uses pipefail and per-command timestamps; all exit 0.
