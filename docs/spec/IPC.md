# Native command and event contract

`contracts/ipc.v1.json` is the authoritative allowlist for **new app-owned commands**.
It does not describe pre-existing Tauri APIs. P01 must implement the command facade,
verify the installed Tauri registration/permission API, and generate/test compatible
Rust/TypeScript types. A command can return UNSUPPORTED during development only if the
UI is honest and the feature's task remains incomplete.

Requests use `{schemaVersion, requestId, args}`. Successful result envelope contains
`{ok:true, requestId, data}`; failure contains `{ok:false, requestId, error}`. No both/neither
case. Native errors are mapped centrally; a JS exception caused by IPC transport failure
is not silently converted into business success. The schema currently defines DTOs;
envelope validation is an explicit test requirement, not an assumed Tauri behavior.

All exposed IDs are validated and resolved natively. Source URLs, shell args, storage
paths and permission grants are not trusted merely because they originated in React.
Only the root-picker flow can introduce a local root; the native side must validate
its candidate again at commit. Existing root relocation is unsupported in V1.

Repository lists use bounded cursor paging and stable sorting. Provider IDs are strings
because a JavaScript number is not a safe universal provider-ID representation. OIDs
are full hex strings with format compatibility checked natively. Timestamp fields are
RFC 3339 UTC or null. UI formats them for display. Unknown totals are null, not zero.

The secret-bearing credential command is never covered by generic argument logging.
It returns only AccountSummary. Do not generate fixtures with real secrets. Directory
pickers and exports use StoragePickResult / ExportResult. Canceled means candidate/path
is null and no action was performed; a successful selection/save requires the non-null
matching field. Test this cross-field invariant in addition to the DTO shape.

## Job submission and approval

`destination_commit` persists mapping/create policy but does not immediately push Git
refs. Project creation occurs during an authorized job and is reconciled on retry.
`sync_plan_create` queues a plan job and returns QueuedResult. It may fetch/validate
local data and observe remote refs but must not write to GitLab. Its terminal Job
contains planId; gbm_sync_plan_get retrieves the completed plan. Plan creation failure
remains an ordinary typed Job error, not a made-up plan. Do not block the renderer.

`sync_plan_approve` creates an opaque native approval record bound to plan digest,
policy revision and expiry. `sync_apply_enqueue` validates it and returns a job ID.
Automatic non-destructive jobs still have plans, but may use the enabled safe-policy
authority instead of a manual approval record. Plan/approval changes need contract tests.

## Event semantics

`gbm://state` carries safe invalidation events with committed revision and affected IDs.
The UI subscribes and requests a snapshot, disregards stale events, and re-queries after
a gap. Progress-specific queries return Job fields; no token/path/raw process output
is serialized in general events. Log viewing is separate and bounded.

## Error classification

Use the ErrorCode enum in the DTO schema; details are sanitized and optional. A native
error can be retryable while requiring a new plan after reconciliation. `WORKSPACE_SKIPPED`
is not a failed bare-mirror backup, and `PUSH_PARTIAL` is not full success. The frontend
maps codes to user actions; it does not guess logic from English error strings.
