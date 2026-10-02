# CR-001 — Add P07-T04 "Wire native commands and the job runner"

Date: 2026-10-02. Decision: owner, via AskUserQuestion at the end of phase P07
("Thêm task P07-T04 (Recommended)").

## Gap
After P07-T03 the live native handler (`apps/desktop/src-tauri/src/backend/`) implements
11 of the 31 contract commands. Repository list/get/select, scan, refs, dashboard, plan
create/approve/get, apply, jobs list/cancel/retry, queue pause, destination
preview/commit, logs, activity export and the safe openers answer `UNSUPPORTED`, and no
job runner/scheduler loop connects the P04–P07 use cases. No task's permitted paths
include the backend module, while every P08 task needs these commands on real native
contracts.

## Change
- New task P07-T04 in phase P07 (P08 depends on phase P07, so it runs before P08).
- Lifecycle: Full (remote writes, credentials, process/race code).
- Permitted paths: `apps/desktop/src-tauri/src/`, `crates/application/`,
  `crates/infrastructure/`, `crates/domain/` (no contract change without a further CR).
- Implementation slices, each reviewed: (a) read models + selection + queue pause;
  (b) scan job runner + scheduler loop + job events; (c) destination preview/commit,
  plan create/get/approve, apply/verify pipeline with the credential bridge;
  (d) jobs cancel/retry, logs, activity export, safe openers.
- Tests: TEST-IPC, TEST-CONTRACT-FIXTURES, TEST-SCHEDULER, TEST-EVENTS,
  TEST-READONLY-SOURCE, TEST-PLAN-LEASE, TEST-VERIFY, TEST-REDACTION, TEST-NO-FAKES —
  against local sandbox remotes and controlled HTTP doubles only (no personal
  repositories, no GitLab project creation without authorized test targets).
