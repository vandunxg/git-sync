# P04-T03 review record

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
- M1 peeled entry of an ignored ref or HEAD rejected the whole advertisement
  (reproduced with git 2.54). Fixed; real-git test; negative control N10.
- M2 protocol v2 non-HEAD symref lines rejected the whole advertisement (reproduced).
  Fixed; real-git test across v0/v2; N9.
- L1 output cut without trailing newline could parse as a wrong map. Fixed (Truncated
  → Partial); reader contract documented; N11.
- L2 coverage overstated (no v0/v2, no M1/M2 cases). Fixed by the new tests.

## Round 2 — ACCEPTED, no new issues
Carry-forward notes (non-blocking, for P05-T01 / UI):
- `head_target` is unvalidated display data; sanitize before UI use.
- The production reader must fail on non-UTF-8 output instead of lossy conversion.
