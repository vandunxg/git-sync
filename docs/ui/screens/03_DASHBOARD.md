# UI-03 - Dashboard

Reference: Refined V2 `6:140`. Status: **candidate / baseline missing**.
Requirements: REQ-08, REQ-23, REQ-24, REQ-25, REQ-28, REQ-42, REQ-43.
Test scenario: `TEST-UI-03` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_dashboard_get`, `gbm_activity_list`, `gbm_source_scan_enqueue`, `gbm_sync_plan_create`, `gbm_sync_apply_enqueue`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Header: Dashboard, freshness/subtitle, secondary Scan, primary Sync all. Exactly three
summary metrics: tracked repositories, up to date, needs attention. Below: attention
list, recent activity and a compact GitHub -> local -> GitLab pipeline. Do not add
charts, score dials, nested cards per row or unrelated storage analytics.

## Data and actions

All counts come from the same revisioned Dashboard read model. When discovery is
partial, label the tracked count as partial/unknown; do not synthesize a denominator.
Summary cards can apply a repository filter. Attention rows show repo, concise problem,
last observation and a specific action (Reconnect / Review changes / Retry / Create
local copy). The action maps to a real command and appropriate confirmation.

Sync all queues eligible selected repos under safe policy. It does not approve any
rewrite/delete plan or skip required destination review. No percentage bar is required
for health; status plus freshness and actual counts are clearer. Service-running label
is separate from backup health and cannot override an auth failure.

## States

Loading retains metric/list geometry. Empty directs to connect or scan. Filter-free
healthy state says no action required at the last verification time, not always safe.
Offline shows cached state with timestamp. Failed discovery does not wipe previous
backups. Recent activity uses semantic event icons and opens an event detail.

## Acceptance

Changing a repository's actual state updates card, attention row and detail consistently.
Keyboard reaches the primary action and each row action. The pipeline remains legible
at compact width. Long names wrap/truncate only in their intended cells. No internal
collision between summary values, labels and buttons; no fake fixture data in release.
