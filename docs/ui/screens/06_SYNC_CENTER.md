# UI-06 - Sync center

Reference: Refined V2 `6:755`. Status: **candidate / baseline missing**.
Requirements: REQ-09, REQ-10, REQ-19, REQ-24, REQ-31.
Test scenario: `TEST-UI-06` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_jobs_list`, `gbm_job_cancel`, `gbm_job_retry`, `gbm_queue_set_paused`, `gbm_logs_read`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

One queue list and a selected-job inspector. Small summary counts are enough; do not
add another metric-card dashboard. Queue rows show repo, actual stage, progress when
known, state and contextual action. Inspector shows real stage sequence, scope and
observed transfer data. No default log-console wall.

At compact width, inspector is a drawer/detail route that returns focus to the selected
row. Queue remains usable while jobs update. Selected identity is JobId, not array index.

## Controls

Pause queue means stop starting new jobs; existing jobs continue unless individually
canceled. Label that behavior. Cancel request shows Canceling until native confirms
termination/reconciliation. Retry creates a linked new attempt; it cannot reuse an
expired destructive approval. Confirm only when canceling a write may leave an uncertain
result; after cancellation the app observes destination, not assumes rollback.

Stage percentage is nullable. Show indeterminate progress when Git cannot provide a
valid total. Transfer speed/ETA are real estimates with clear scope, not time-based
fake bars. Reduced motion turns off spinning without hiding status. Terminal states
remain visible after switching pages because jobs are native/persistent.

## Acceptance

Test paused queue with active jobs, cancel during fetch, cancel during push, failed LFS,
retry after expired token, restart recovery and out-of-order progress events. Inspector
must show the same job as the selected row. Large names and long messages do not push
controls out of view. Toasts are grouped, not one per streamed progress event.
