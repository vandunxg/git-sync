# UI-05 - Repository detail and tabs

Reference: Refined V2 `6:571`. Status: **candidate / baseline missing**.
Requirements: REQ-17, REQ-20, REQ-23, REQ-24, REQ-30, REQ-42.
Test scenario: `TEST-UI-05` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_repository_get`, `gbm_repository_refs`, `gbm_activity_list`, `gbm_logs_read`, `gbm_sync_plan_create`, `gbm_sync_plan_get`, `gbm_sync_plan_approve`, `gbm_sync_apply_enqueue`, `gbm_repository_open_workspace`, `gbm_repository_open_provider`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Header: repository name/full path, Open folder (secondary) and Sync/Review (primary,
state-dependent). A compact metadata row shows branch, known size and coverage.
A single actionable banner explains the most important blocker, not a wall of alerts.
Tabs: Overview, Branches, History, Logs. Do not show every tab's content simultaneously.

Overview groups source/destination/path configuration and recent attempts. Branches
shows all selected logical refs, source OID, destination OID and operation class; it
is not a git branch-creation/editor UI. Annotated tag object IDs are retained, full
hash available by accessible copy action. No `New Branch` feature in V1.

History lists attempts with real duration/result and links to bounded log details.
Logs are on-demand, virtualized/paged as necessary, redacted before display, selectable
and copyable without enabling HTML/ANSI execution. They are not a terminal emulator.

## Review and coverage

Review changes opens the actual native plan with exact ref operations, destination,
policy revision, expiry and verified recovery snapshot. Approval returns a native
approval ID. Any drift resets the review state. Destructive operations have explicit
confirmation; ordinary ref reads do not.

Show Git refs, local objects, LFS and submodule coverage separately. Working-copy dirty
status has a safe explanation and open-folder action; do not show reset/stash/continue
anyway controls. A current mirror does not imply the working tree is current.

## Acceptance

Tabs preserve selected repo and focus, links are provider-allowlisted, copy buttons
never copy tokens. Error banner matches the native error code. New snapshots/events
cannot overwrite a newer detail revision. The source ref changed during review case
must invalidate approval. Non-overview tabs require separate candidate/baseline captures;
the overview Figma frame is not evidence they were visually designed already.
