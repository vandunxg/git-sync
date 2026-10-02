# P05-T01 security review record

Reviewer: independent security review subagent (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED
- H1 long-option abbreviation bypass (`--upload-pa`, `--exe`, `--temp`) reproduced
  with git 2.54 → arbitrary command execution with local transport. Fixed (typed args,
  exact per-subcommand flag allowlist); N6.
- M1 raw https URLs with userinfo/any host via `new`. Fixed (Location only).
- M2 unvalidated `with_auth` / `with_local_transport`. Fixed (AuthArgs newtype; file
  transport only with Location::Local); N12.
- M3 setsid/reparented helpers survived normal exit. Improved (periodic descendant
  scan + kill after exit); N10; remaining window → residual risk 1.
- L1–L8: fixed or documented (see TASK_REPORT.md).

## Round 2 — ACCEPTED, subject to OWNER acceptance of residual risks
1. setsid + reparent between two ~200 ms scans escapes signalling (run never reported
   as success; process and 1–2 drain threads may survive until pipes close). Future
   hardening: subreaper / cgroup / pidfd.
2. Repo-local config not overridable by `-c` (url.*.insteadOf, remote.*, include*) is
   trusted for app-managed mirrors only; URLs always explicit Locations.
3. Trust in git's exec-path helpers (/usr/lib/git-core) and a small check-to-exec
   window on a root-owned install dir.
4. /proc scans capped at 65,536 entries.
5. Negligible PID-reuse check-then-kill window (start-time checks, pinned pgid).
6. Timing-based tests (generous ceilings) may flake on a loaded host.
7. Detached waiter/drain threads bounded per run, not capped across runs.
Caller obligations: WorkDir::GitDir and Location::Local only for app-owned paths.

Owner decision: ACCEPTED residual risks 1–7 (owner answer in session, 2026-10-02:
"Chấp nhận, làm tiếp"). Risk 1 hardening (subreaper/cgroup/pidfd) tracked as a later item.
