# UI-04 - Repository list

Reference: Refined V2 `6:359`. Status: **candidate / baseline missing**.
Requirements: REQ-03, REQ-07, REQ-24, REQ-26, REQ-29, REQ-40, REQ-43.
Test scenario: `TEST-UI-04` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_repositories_list`, `gbm_repositories_select`, `gbm_repository_get`, `gbm_source_scan_enqueue`, `gbm_sync_plan_create`, `gbm_sync_apply_enqueue`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Header and a single filter/search toolbar. Main content is a table, not a card per repo.
Visible columns: selection, repository, branch, local mirror, GitLab, observed/updated,
primary status, row actions. Details such as SHA, LFS and multi-branch diff belong in
the detail view. Use the table's local horizontal scroll only when needed.

The narrow Actions cell uses an actual Ellipsis icon button, never a text "..." or
Unicode glyph. Status cells use semantic SVG + text. Repository name may show one
small FolderGit2 icon; do not wrap every table cell in another icon tile.

## Interaction

Search is debounced and cancellable; filter/sort/page values live in route state.
Empty-filter state has Clear filters. Row click opens detail; checkbox click only
selects. Display selection count and whether selection is current page or all matching.
V1 selection is explicit IDs (bounded batches), not a misleading implicit select-all
across unseen partial pages. Maintain selection by immutable ID, never row index.

Bulk action states the exact number of selected eligible repos and blocked/review items.
A create/rewrite/delete plan is not bypassed by bulk sync. Disable duplicate submits;
queued is not shown as synced. Row menu offers only implemented actions: open detail,
plan/sync/retry as applicable and safe open-provider/open-folder.

## Acceptance

Test 0, 1, 8 and 1,000 mock rows, names 120+ characters, Unicode owner names, missing
branch for an empty repo, stale observations and page changes. Keyboards can select
and open row menus. A rename keeps identity/selection/history. Sorting/search does not
mutate source/destination. Actual data updates preserve focus and do not reorder a row
under the cursor unexpectedly without a visible user-driven sort policy.
