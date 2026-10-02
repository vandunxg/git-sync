# UI-10 - Empty, error, warning and confirmation patterns

Reference: Refined V2 `6:1406`. Status: **candidate / baseline missing**.
Requirements: REQ-20, REQ-24, REQ-26, REQ-35.
Test scenario: `TEST-UI-10` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_source_scan_enqueue`, `gbm_credentials_validate_store`, `gbm_sync_plan_approve`, `gbm_sync_apply_enqueue`, `gbm_job_retry`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Role

The Figma States & Dialogs frame is a component/pattern showcase, NOT a production page
or sidebar item. Implement these patterns in the relevant real screen and review them
as separate states. No decorative card-grid screen needs to be shipped.

## Required patterns

1. Empty account: brief explanation and Connect with token; filtered empty: Clear filters.
2. Auth expired: inline diagnosis and Reconnect; do not hide other cached data.
3. Missing project: show verified destination mapping, visibility and create policy;
   no automatic adoption of an unrelated project.
4. Dirty workspace: "Your local edits will not be changed" + Skip workspace update /
   Open folder. No automatic Stash, Reset or misleading Continue anyway.
5. Failed job: show typed reason, prior verified state, Retry where safe, View details.
6. Destructive plan: exact refs, old/new OIDs, destination, recovery snapshot and impact;
   approve only the current digest. A generic "Are you sure" is insufficient.
7. Keyring unavailable: explicit session-only choice or setup guidance; no plaintext fallback.

## Dialog behavior

One dominant primary action, secondary Cancel/Close. Primary cannot run with invalid
inputs or stale plan. Focus trap and restoration, Escape when cancel is safe, no
click-through to the underlying table. Modal content scrolls locally at short heights;
footer actions remain reachable. Warnings/errors use a semantic icon plus text, not
only color or Unicode dot. Avoid two primary buttons competing in one form.

## Acceptance

Keyboard-only open/submit/cancel works; focus returns to trigger. Long error text and
many changed refs scroll without overlapping buttons. Approval invalidated mid-dialog
must disable apply and explain why. Empty/canceled operations never render completed
success. All destructive authority remains native, not a renderer confirmation boolean.
