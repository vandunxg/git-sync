# P07-T02 review record (lifecycle: Full)

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED (2 High, 3 Medium, 3 Low)
1. High: reconcile outcomes could not be persisted; the only exit from `interrupted`
   re-queued the same row with its stale plan, whose approval would still authorize →
   `resolve_interrupted` (close + expire plan/approvals + plan-less linked attempt, one
   transaction); `interrupted -> queued/running` removed; generic transitions refused.
2. High/Medium: `KeepInterrupted` could hold locks forever → write stage without
   checkpoint is `Uncertain` at once; unobservable push escalates after 24 h; owner can
   choose `Uncertain` any time (locks released).
3. Medium: cancel before the next child spawned reported "Canceled" after a write →
   `Canceled` only for pre-write stage without checkpoint.
4. Medium: project create intent/marker not durable; create cancel reported certain →
   migration 0003 `job_checkpoints`, `CheckpointRecord`, intent carried to next attempt.
5. Medium: report overstated evidence → store-integrated tests, corrected wording.
6–8. Low: VerifiedComplete via `record_attempt` (documented); backwards clock → scan due;
   suspend threshold `SUSPEND_THRESHOLD_MS`.

## Round 2 — CHANGES_REQUIRED (1 Medium, 2 Low) — all fixed with tests (N19–N22)
1. Medium: approvals revoked only via `jobs.plan_id` → also the push checkpoint's plan.
2. Low: CANCEL_UNCERTAIN overwritten on close → kept unless verified success.
3. Low: next attempt could duplicate a pending one → skipped if one is pending.
- Residual Medium (partial bypassed review) → `enqueue_or_existing` returns NeedsOwner
  after a partial with PUSH_PARTIAL / CANCEL_UNCERTAIN.

## Round 3 — APPROVED (focused check of round-2 fixes)
New Low items, batched: L6 an unparsable push checkpoint (tampered/corrupt DB only)
makes every close fail `Corrupt` and keeps the job's locks; L7 a skipped duplicate
attempt is not distinguishable from a created one in the return value (no duplicate
project can result: the pending attempt runs its own create lookup).

## Residual risks
- Medium A: shutdown/startup wiring (cancel + bounded wait; recover → reconcile → start
  scheduler) is not implemented here (Tauri shell); real kill-the-app test is P10-T01.
- Medium B: an orphaned `git push` child can outlive an app crash (own process group,
  no PDEATHSIG); per-ref leases and verification bound the effect.
- Low (batched for P07 end): L1 a carried-over create checkpoint makes later preflight
  cancels "uncertain" until a push checkpoint replaces it; L2 `expires_at` compared as
  text (canonical RFC 3339 enforced by writers); L3 `scan_due` caller must store the
  scan time; L4 `job_checkpoints` rows are not pruned (≤ 4 KiB per job); L5 the spec edge
  "interrupted -> queued" is realised as a new linked attempt.

Owner decision on Medium A/B: PENDING (asked at commit). Lows L1–L7: end-of-phase P07 batch.
