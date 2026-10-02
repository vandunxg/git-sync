# Independent review - P03-T03 (PAT identity and capability validation)

- Reviewer: independent review subagent designated by owner, 2026-10-02 (not the implementer).
- Base: 110bdb4 plus untracked files. Outcome: **ACCEPTED** (non-blocking follow-ups below).

## Commands (all run by reviewer, no real provider call, no token used)

| Command | Exit |
|---|---|
| `cargo test -p gbm-infrastructure -p gbm-application --locked` x3 | 0, 0, 0 (application 16 passed, infrastructure 32 passed each run) |
| `cargo test ... -- --list` | 16 provider tests present: application 4, github 7, gitlab 5; the three deleted-then-restored GitHub tests are present |
| `cargo clippy -p gbm-infrastructure -p gbm-application --all-targets --locked -- -D warnings` | 0 |
| `cargo fmt --check` | 0 |
| `python3 scripts/check_task_scope.py --task P03-T03 --repo-root . --base 110bdb4` | 1; outOfScope is exactly Cargo.lock, infrastructure Cargo.toml, src/lib.rs, src/http/mod.rs, src/http/test_server.rs, all covered by coordinator-approval.md |
| `cargo tree -e features -i reqwest` | 0; features are rustls (aws-lc-rs) and http2 only |
| WebFetch of GitHub GET /user, GitHub troubleshooting, GitLab PAT API, GitLab token scopes | docs read, see below |

## API facts vs official docs

Confirmed: GET /user returns 200/304/401/403, `id` and `login` required, `X-GitHub-Api-Version: 2026-03-10` listed. Primary and secondary limits are 403 or 429; `retry-after` and `x-ratelimit-remaining: 0` match the code. GitLab `/personal_access_tokens/self` has fields active, revoked, scopes, user_id, expires_at (date `YYYY-MM-DD`), 401 for a bad token, PATs only. `api` on a PAT grants Git-over-HTTP read/write; `write_repository` cannot use the API, so `api` is required for project creation. No GitHub expiry header is documented, and expiry stays `None`, which is correct.

## Findings

No blocking findings.

1. LOW - GitLab expiry instant. The docs give only a date, not a time of day. The `T00:00:00Z` conversion is a derived value (the conservative earlier bound), not a documented fact. It is acceptable under "unknown unless provided", but the UI task should label it "date" or "by". Recommend a comment saying the time of day is undocumented.
2. LOW - GitLab `granular_scopes` (docs: introduced in 19.2). A token that reports granular scopes with an empty `scopes` array would get `InsufficientScope{api}`. That is a possible false negative, not an over-claim, and it is unverified. Record it as a follow-up for the real-sandbox run.
3. LOW - GitHub fine-grained detection rests on the `X-OAuth-Scopes` header being absent. The GET /user docs do not document that header either way. The outcome claims nothing (`GitHubUnscoped`, "checked during discovery"), so REQ-41 holds. The label "Fine-grained or unscoped" is honest.
4. LOW - GitHub secondary rate limits sometimes arrive as 403 with only a message body and no `retry-after`. These map to `AccessDenied` rather than `RateLimited`. The body is deliberately not inspected (a safe choice). Consider handling this when discovery retry logic is built.
5. LOW - `retry-after` is parsed only as seconds, with no upper clamp. A hostile or buggy value of u64::MAX is passed to the caller, so the retry scheduler must clamp it.
6. LOW - Test strength.
   - The assertion `!format!("{expected} ...").contains(SENTINEL)` is tautological, because `ProviderError` cannot hold text. It adds little value. A real leak test would need a path that carries backend text.
   - No test asserts the sensitive header flag.
   - CRLF injection is tested for GitHub only, although GitLab shares `precheck_token`.
   - GitLab has no timeout or oversize test. The shared code is covered through GitHub, and the body-cap control fails a test.
   - The controls are real. Reviewer read the final log: the baseline passes and each of the six mutations fails a test. Controls were re-run after the restoration, per the log timestamp and the "restore-check-done" line.
7. LOW (process) - cargo-audit, cargo-deny and the license scan are NOT_RUN (tools not installed). R-DEPS makes this BLOCKED for release, not for this task. The 35 new lock entries (aws-lc-sys C build, rustls platform verifier using system roots) need that scan before release. reqwest has no `gzip`/`brotli` feature, so there is no decompression-bomb path.
8. INFO - `no_proxy()` means users behind a mandatory corporate proxy cannot validate tokens. This is a deliberate trade-off under R-SEC and should be documented as a V1 limitation.
9. INFO - The Bearer string is a plain `String` that is not zeroized. This is acceptable for V1 and consistent with `SecretString`.

## Assessment against the scrutiny list

- Security: the token appears only in the auth header (tests check the request line and the header). The header is marked sensitive. Redirects are disabled and a 302 is not followed (exactly one request seen). The origin is fixed and checked after `join`. `https_only` is on in production and relaxed only under `cfg(test)`. Timeouts are 20 s total and 10 s connect (a Hang test gives `Timeout`). The body is capped at 1 MiB while streaming. reqwest errors are never formatted. `ProviderError` Display holds no backend text. CRLF and non-graphic tokens are rejected before any request.
- Classification: 401 gives InvalidOrExpired; 403 with rate-limit evidence or `retry-after` gives RateLimited; other 403 gives AccessDenied; 429 gives RateLimited; 5xx gives ProviderUnavailable; 3xx, 304 and 404 give UnexpectedResponse; bad JSON and wrong types give UnexpectedResponse. This matches the spec error policy. The 404 "not automatic deletion" rule is irrelevant here.
- Capability claims: no over-claiming. Identity never implies create or push. GitLab `api` is required and the summary lists only the reported scopes. The comment "api covers push" matches the docs. Namespace create permission is deferred and stated. GitHub `repo` carries a write-capable warning.
- R-RUST: no unwrap, expect or panic in production paths. Provider payload types stay in the adapters. Domain and application do not depend on reqwest. No lock is held across an await. Clippy is clean.
- Dependency and scope: reqwest 0.13.5 with default features off is justified, with rustls/aws-lc as expected. The http module is a sensible shared security helper, and coordinator approval is recorded (coordinator, not owner, which R-CORE permits for path allowance).
- Not delivered, correctly reported: real-provider run (NOT_RUN), TEST-PRIVACY (NOT_RUN), wiring to the credentials command.

## Limits of this review

No real GitHub or GitLab call was made. TEST-PAT passes only against the loopback controlled adapter. Real-endpoint behavior (findings 2-4) is unverified.
