# P06-T03 review record (lifecycle: Full)

Reviewer: independent review subagent with security focus (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
1. Medium: runner did not enforce "per-ref lease, never forced" for push → `valid_push`
   (flags/leases, `--`, one location, push specs; leases ≡ push-spec targets) (N13, N14).
2. Medium: `[remote failure]` classified as rejected though possibly applied → Unknown (N15).
3. Medium: writer accepted any location incl. the source host → GitLab-only
   `RemoteTarget`; local only in test builds (N17).
4. Low: `refs/replace` could fake ancestry → `core.useReplaceRefs=false` (N18, real git).
5. Low: server text could pose as client reasons → prefix-first exact matching.
6. Low: exit/report agreement, cancel/timeout/incomplete untested → unit tests (N16).

## Round 2 — APPROVED
Residual risks:
- R1 Medium: per-ref push report is server-controlled; only P06-T04 post-push
  verification can confirm Applied/Rejected/Partial.
- R2 Medium: real GitLab protected branches/push rules/pre-receive hooks not exercised
  (local bare repo with receive.* config only; destination hooks disabled in tests).
- R3 Low: `ApplyAuthority` does not carry the destination identity; wiring must pass the
  plan's destination to `GitPusher` (host check only excludes GitHub).
- R4 Low: server may stop advertising atomic → non-destructive multi-ref plans Partial.
- R5 Low: >100-ref non-destructive plans are batched (not atomic across batches).
- R6 Low: mirror repository config trusted (pushInsteadOf/include/push.* not checked).
- R7 Low: lease mismatch with target already present reports up to date (success).
- R8 Low: unknown `!` reasons labelled Other (not applied).
- R9 Low: runner does not require `--porcelain`/`--no-verify`/`--no-follow-tags`
  (fails safe to Unknown without porcelain).

Owner decision on Medium risks: ACCEPTED R1–R2 (owner answer in session, 2026-10-02:
"Chấp nhận"). Low risks: batched for the end-of-phase P06
owner decision (owner rule 2026-10-02).
