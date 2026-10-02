# Task report - P03-T03

- Task / phase / requirement IDs: P03-T03 / P03 / REQ-11, REQ-41
- Validation kind: implementation. Base commit 110bdb4.
- Permitted: crates/infrastructure/src/github/, crates/infrastructure/src/gitlab/, crates/application/ (+ state/).
- Out-of-allowlist (approved, coordinator-approval.md; scope guard exit 1 lists exactly these): infrastructure
  Cargo.toml, src/lib.rs, src/http/mod.rs, src/http/test_server.rs, Cargo.lock.

## Delivered

- application::providers: `TokenIdentity` (provider, immutable numeric user ID as string, username, reported
  token kind/scopes, expiry only when reported), sanitized `ProviderError` with contract `ErrorCode` mapping and
  retryability, `precheck_token` (empty / non-printable rejected before any request), and assessments:
  GitHub source - classic `repo` ok (+ warning that `repo` also grants write), `public_repo` ok with
  public-only warning, otherwise InsufficientScope{repo}; scopes not reported (fine-grained) -> "checked during
  discovery", nothing claimed. GitLab destination - `api` required (also covers Git push for PATs per docs),
  else InsufficientScope{api}; warnings for `sudo` and for no reported expiry; namespace create permission is
  explicitly deferred to destination selection. Identity alone never claims create/push permission.
- infrastructure::github `GitHubClient::validate_token` (one GET /user) and infrastructure::gitlab
  `GitLabClient::validate_token` (GET personal_access_tokens/self then GET user, user_id cross-check, revoked or
  inactive -> Revoked, `expires_at` date -> start-of-day UTC instant, i.e. the conservative earlier time).
- http helper: fixed origin per client (https://api.github.com/, https://gitlab.com/), HTTPS-only in production,
  no redirects, `no_proxy`, 20 s total / 10 s connect timeouts, 1 MiB body cap, auth header marked sensitive,
  reqwest errors never formatted. User-Agent `git-backup-manager/<version>`.
- Sources: provider-doc-sources.md (official docs fetched 2026-10-02; unverified facts not used).

Not delivered: wiring into `gbm_credentials_validate_store` (credential command task), persistence of the
validated account, GitHub organisation/collaboration access checks (discovery P04), real-provider integration
run (needs owner-authorized sandbox tokens; NEED-SANDBOX-TARGETS).

## Tests (controlled adapter = cfg(test) loopback HTTP server)

application 4 (precheck, GitHub/GitLab assessments, code mapping). GitHub 7: classic identity + scopes + request
headers + token only in header (not URL) + 64-bit ID exactness; unscoped token not guessed; insufficient scope;
401/403-ratelimit/403-retry-after/429/403/502/302-not-followed/bad JSON/wrong types/empty login (exactly one
request each, no sentinel in error text); oversized valid JSON rejected; hang -> Timeout; empty and CRLF-injected
tokens send nothing; production client builds. GitLab 5: valid api token (both endpoints, header auth, expiry
conversion); read_api/write_repository -> InsufficientScope{api}; revoked, inactive, 401, 429, 403, 503, 301,
bad date, mismatched user, second-call 401; empty token; production client builds.
negative-controls.log (final tree): following redirects, removing GitHub 403 rate-limit detection, removing
revoked/inactive check, removing the `api` requirement, removing the body cap, removing the user-id
cross-check -> each makes tests fail.
Process note: while strengthening the oversized-body test, a faulty scripted edit deleted three GitHub tests; the
full test listing exposed it, the tests were restored and all controls were re-run on the final tree.

## Commands

run_gates core exit 0, frontend exit 0; reqwest resolved features: rustls, http2 only.

## Scenario status

- TEST-PAT: PASS with controlled adapters (empty, malformed, invalid/expired, revoked, insufficient scope,
  rate limits, unexpected responses); identity alone does not claim create/push permission.
- TEST-PRIVACY: NOT_RUN (visibility mapping belongs to destination tasks).
- Real GitHub/GitLab validation: NOT_RUN (no authorized sandbox credentials).

## State: REVIEW_READY
