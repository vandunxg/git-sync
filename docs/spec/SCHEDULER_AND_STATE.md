# Scheduler, events and repository status

## Persistent job state machine

`queued -> running -> succeeded | partial | failed | canceled | interrupted`
`queued -> canceled`; `interrupted -> queued` only after reconciliation; retry creates
a new attempt preserving earlier evidence. `waiting_for_approval` and
`waiting_for_retry` are durable non-running states. `cancel_requested` is a request
state, not a claim the process already stopped.

Job stage is separate: `preflight`, `observe_source`, `fetch`, `plan`, `snapshot`,
`transfer_lfs`, `push`, `verify`, `workspace`, `finalize`. Some stages are skipped with
an explicit reason. Stage text must reflect actual native transitions.

One active mutating attempt per repository; also lock a shared destination mapping.
Queue pause stops starting new jobs and does not send SIGSTOP to existing Git processes.
Cancel requests bounded termination/reaping and records whether a remote write outcome
is uncertain. UI may say "Canceling" until confirmed, not immediately "Canceled".

## Retry and restart

Classify errors into retryable/transient, actionable/auth, safety/approval or unsupported.
Exponential backoff with jitter has maximum attempts/deadline and honors provider retry
metadata. A maximum retry budget is explicit in settings/native policy, not infinite.

Persist job intent, stage and safe checkpoints. Do not put raw command argv containing
secrets into the checkpoint. On restart, process IDs alone are not trusted (PID reuse).
Reconcile destination refs/created project identity before retrying a possibly completed
write. Never resume a stale approval or stale policy revision.

App suspend/resume and offline intervals coalesce into one due scan. Provider failures
must not continually recreate the same logical job. Idempotency is based on repo,
source snapshot, destination mapping and policy revision; a retry attempt remains linked.

## Status dimensions

Keep these dimensions independent:

- source: `unknown | current | changed | unavailable | stale`;
- local mirror: `missing | staging | current | invalid | unknown`;
- destination: `missing | current | behind | diverged | inaccessible | unknown`;
- workspace: `disabled | current | behind | dirty | ahead | detached | diverged | missing`;
- coverage: Git refs / local integrity / LFS / submodule contents;
- job: durable state + real stage.

The primary badge follows precedence: active job -> safety/actionable blocker -> failed
attempt with unverified destination -> missing -> update -> stale/unknown -> synced.
Auth/access blockers are not masked by a successful old snapshot. Workspace-only dirty
state is a secondary warning if Git backup is current. A healthy service process does
not imply all backups are healthy.

Dashboard totals and table/detail rows derive from one revisioned repository snapshot.
Do not aggregate different pages of a partial catalog as a complete account count.
Read models include `observedAt`, `verifiedAt`, `revision`, and completeness flags.

## Events

Events are invalidations/progress notifications, not the durable source of truth.
Envelope includes schemaVersion, eventId, revision, occurredAt, type, affected IDs and
safe payload. Native state is committed before emission. On subscription race, subscribe
then request a snapshot; discard older revisions and re-query on a gap. Throttle progress,
not terminal updates. Bound buffers and show stale data if event delivery is unavailable.
