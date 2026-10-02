# P06-T01 review record

Reviewer: independent review subagent with security focus (owner-designated 2026-10-02).

## Round 1 — APPROVED with findings
1. Blocked items had an empty intendedPath (schema minLength 1) → proposed path / "-" (N16).
2. Source privacy did not reach the commit → `PreviewItem.source_private` (unknown =
   private, in the digest, N15) and `CommittedProposal.source_private`.
3. Namespace ID vs path not cross-checked → residual 1 (wiring task).
4. One uncertain lookup aborted the whole preview → per-item unknown, non-creatable
   (`destination_state_unknown`); auth/rate-limit still abort (N14).
Verified: no duplicate per intent, no adoption by name/marker substring/other namespace,
lookups 404-only-absent, redirects refused, clone URL exact, token never in body/logs,
HTTP hardening unchanged for GET and applied to POST.

## Round 2 — APPROVED, subject to OWNER acceptance of residual risks
1. Namespace ID/path consistency must come from one verified record (wiring task).
2. Privacy guard relies on the future job passing fresh/committed `source_private`.
3. No-duplicate per intent relies on the future intent store + random marker reuse.
4. No-duplicate per source across renames relies on the repo-ID-keyed mapping store.
5. Namespace Maintainers are trusted (plain-text marker can be replayed).
6. Not exercised against real GitLab (docs + loopback server only).
7. Bounded/cancellable retry with jitter is the caller's job; slow previews possible.
8. Push access only inferred from access level; real push checks in P06-T03.
9. V1 blocks private source → non-private destination (no consent field).
10. Scope exception in `http/` (POST, origin accessor, test server).

Owner decision: ACCEPTED residual risks 1–10 (owner answer in session, 2026-10-02:
"Chấp nhận, làm tiếp"). Follow-ups tracked in IMPLEMENTATION.json.
