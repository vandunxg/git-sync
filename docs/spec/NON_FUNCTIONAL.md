# Non-functional requirements and budgets

These are target budgets for testing, not benchmark claims. P00 records actual test
hardware, OS, compositor, WebKitGTK, Git, Git LFS, Rust and Node versions.

## Responsiveness

At 1440x900 and 1,000 deterministic repository rows, cached list filtering and selection
should respond within 150 ms at p95 on recorded hardware. Initial window usable target
is 2 seconds warm / 4 seconds cold, excluding initial network scan. Virtualize large
lists only after measuring; preserve keyboard and accessible table semantics.

Git work, keyring calls, checksums, filesystem scans and SQLite operations must not
block the renderer/UI thread. Progress delivery is throttled (target <=4 updates/sec/job)
and terminal states are delivered without relying on a lossy progress stream.
Bound retained log output to avoid resource exhaustion; default log retention budget is
50 MB, with rotation, not truncation of the authoritative job result.

## Consistency and reliability

No simultaneous mutating jobs for the same repository or destination mapping. Survive
app interruption between any two sync stages without claiming a false verified result.
Persist job intent before execution, terminal state before emitting UI success, and
reconcile uncertain remote outcomes after restart. An event gap triggers re-query.

Offline mode reads local state and marks it stale. No network polling storm on resume;
coalesce missed intervals into one scan and apply bounded backoff/jitter. API page
failures retain previous catalog entries. Scheduler shutdown does not destroy data.

## Security and privacy

No credentials in disk database, config, remote URL, argv, logs, telemetry or snapshots.
V1 has no telemetry. All writes are within a validated owned root or explicitly approved
remote destination. Provider input is untrusted. No general native shell/file API.

## Platform behavior

Linux-first does not mean assuming GNOME services exist under Hyprland. Detect usable
Secret Service, dialog portal, notification service and tray host. Missing tray must
not hide the only way to reopen an active app. Do not assume systemd user services
exist for every platform. Closing/minimizing is different from quitting.

Support main desktop viewports 1280x800, 1440x900 and 1920x1080; test a compact
1024x768 fallback. At 200% text scaling, content must reflow or use explicitly designed
local scrolling without unreachable actions. Native scaling tests include 100%, 125%
and 150% where available. No generic mobile redesign is required.

## Measurement honesty

Do not synthesize percentage from elapsed time. Transfer totals and stage percentages
can be null. "Not available" is valid. Hash/reference verification is separate from
performance statistics. Stated backup freshness always carries the last successful
observation/verification time.
