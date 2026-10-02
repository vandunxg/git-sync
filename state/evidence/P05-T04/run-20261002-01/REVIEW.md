# P05-T04 review record

Reviewer: independent review subagent with security focus (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
- F1 HIGH (reproduced with git 2.54): `merge --ff-only` silently overwrote an ignored
  local file at a path the target adds; status never lists ignored files →
  `target_overwrites_local_files` (diff-tree A/D paths, existing path or non-directory
  parent = collision) and SkipReason::WouldOverwriteLocalFiles.
- F2 MEDIUM: no ignored-file test → ignored path, ignored parent, ignored dir, and
  tracked file→dir swap real-git tests (N10–N13).
- F3: status could refresh `.git/index` → `GIT_OPTIONAL_LOCKS=0` for WorkTree runs.
- F4: branch switch between re-inspect and merge → residual R1.
- F5: added REBASE_HEAD, BISECT_START to in-progress markers.
- F6: drop_target swallowed any rev-parse failure → only exit 1 means "no ref".
Runner additions (WorkTree, status/merge/merge-base/ls-tree/cat-file, diff-tree):
no weakening found.

## Round 2 — APPROVED, subject to OWNER acceptance of residual risks
1. (R1) Branch switch in the window between the last inspect and merge is detected only
   after the fact (post-check → Failed); ff-only may advance the newly checked-out branch.
2. (R1b) An ignored file created between the collision check and merge can still be
   overwritten (millisecond window; status cannot see ignored files).
3. (R2) Status may run clean filters from the workspace's own `.git/config` for paths
   already carrying filter attributes in HEAD.
4. (R3) `url.*.insteadOf` in the workspace's `.git/config` could redirect the local
   fetch; accepted content is bounded by the exact OID check.
5. (R4) Skipped ahead/diverged workspaces receive the target's objects (objects only).
6. (R5) Huge untracked trees exceed the stdout limit → Failed instead of a skip reason.
7. (R6) Filter detection is a conservative substring match; attributes removed by the
   target are not examined.
8. (R7) File↔directory replacements where the path exists are skipped (conservative).
9. (R8) An interrupted run can leave `refs/gbm/incoming/<job>`; harmless, the next run
   overwrites then deletes it.

Owner decision: PENDING.
