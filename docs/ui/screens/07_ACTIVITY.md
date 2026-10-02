# UI-07 - Activity and logs

Reference: Refined V2 `6:968`. Status: **candidate / baseline missing**.
Requirements: REQ-14, REQ-24, REQ-26, REQ-32.
Test scenario: `TEST-UI-07` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_activity_list`, `gbm_logs_read`, `gbm_activity_export`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Search, severity/type filters and time range above a readable timeline/table. Events
have timestamp, semantic icon, repository/context and concise outcome. Open a selected
event's detail drawer to see bounded logs and job metadata; no always-open console.

## Interaction

Filters are local view choices with stable paging. Selected event persists while
incremental events arrive; do not steal scroll/focus. Export opens a native save dialog
for the filtered scope, returns saved/canceled honestly and includes redaction warnings
for private repo/path metadata. Never auto-export an entire log folder.

Log text is escaped plain text with wrap/copy controls and bounded load-more paging.
No terminal escape execution, HTML rendering or secret-bearing raw exception dump.
Direct links to a repo/job resolve known IDs and gracefully handle unavailable records.

## States and acceptance

Distinguish no events from no matching events. A failed activity query keeps the last
readable data marked stale. Unknown time/duration is not fabricated. Test provider error
payloads containing HTML, ANSI escape sequences and token-looking strings. Verify actual
redaction before UI/clipboard/export, keyboard drawer focus, cancellation of export,
long wrapped lines and no scroll lock on return from the drawer.
