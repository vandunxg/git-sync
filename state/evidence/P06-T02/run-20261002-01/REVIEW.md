# P06-T02 review record (lifecycle: Full)

Reviewer: independent review subagent with security focus (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
1. Medium: plan not bound to the destination identity → `DestinationIdentity`
   (account + project id) in plan and digest; apply rejects a re-pointed mapping (N18).
2. Low: approval replay after apply → `last_verified_digest` bound and re-checked (N19).
3. Low: unbounded TTL → `MAX_TTL_MS` 24h for plans and approvals (N21).
4. Low: `valid_id` looser than the contract pattern → leading alphanumeric enforced.
5. Low: recovery snapshot repository not checked → `RecoveryForeign` (N20).
6. Low: test gaps → re-digested destructive plan without approval flag, clock before
   creation/approval (N22).

## Round 2 — APPROVED
Residual risks:
- R1 Medium: inputs (`source_state`, fresh observations, last verified state, destination
  identity) are trusted from the caller; wiring must derive them from native state.
- R2 Medium: a legitimately empty source (or selection change) yields delete_ref for every
  managed ref; no mass-delete threshold beyond approval + verified recovery snapshot.
- R3 Low: approvals are not single-use until persistence marks them consumed.
- R4 Low: wall-clock expiry (forward jump then correction), bounded by 24h.
- R5 Low: object-format mismatch with an empty destination surfaces only at push time.
- R6 Low: unkeyed plan digest (integrity relies on native storage).
- R7 Low: production ancestry oracle and lease enforcement are P06-T03.

Owner decision on Medium risks: ACCEPTED R1–R2 (owner answer in session, 2026-10-02:
"Chấp nhận, làm tiếp"). Low risks: batched for the end-of-phase P06
owner decision (owner rule 2026-10-02).
