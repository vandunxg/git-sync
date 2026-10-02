# P07-T03 — Revisioned read models and frontend invalidation (lifecycle: Light)

Requirements: REQ-08, REQ-36, REQ-43. Scenarios: TEST-EVENTS, TEST-FRESHNESS,
TEST-CONTRACT-FIXTURES. Scope exception: coordinator-approval.md.

## Changes
- `crates/application/src/events.rs` (+ tests): `invalidation(command, args, data)` maps
  every state-changing contract command (by `effect`) to an event type and affected IDs;
  `EventPublisher` emits contract-validated `Event`s with id `e-<run>.<seq>` (strictly
  increasing per run; a rejected event still consumes a sequence → renderer sees a gap);
  `Emitting` decorator publishes only after the inner handler returned success, with the
  response's committed `revision` or the store's committed revision (none → no event).
- `apps/desktop/src-tauri/src/events/` (+ test): `TauriSink` on `gbm://state`,
  `StoreRevisions`, OS-seeded run id; wired around `LiveHandler` in `lib.rs`.
- `apps/desktop/src/lib/native/events.ts`: `parseStateEvent` (contract + id format),
  `StateEventTracker` (duplicates/reordered ignored, gap/restart/invalid → re-query
  all), `connectStateEvents` (subscribe, then snapshot; teardown unsubscribes, also when
  the subscription resolves late; snapshot still loaded if delivery fails),
  `isOlderRevision`.
- `apps/desktop/src/lib/api/`: event → Query key invalidation, `useStateEvents`, and
  `keepNewestRevision` as `structuralSharing` for every native query.

## Evidence
- TEST-EVENTS: Rust (emit only after committed success, revision, contract validity,
  sequences, every mutating command mapped; Tauri mock app listener receives the event)
  and vitest (ordering, gap, restart, invalid, subscribe-then-snapshot, teardown).
- TEST-FRESHNESS: an older snapshot (synced/verified) after a newer one (stale) does not
  regress the Query cache (real `QueryClient`); unknown/stale stay as delivered.
- TEST-CONTRACT-FIXTURES: native events validate in Rust and the same shape validates in
  the TS validator; existing shared contract cases still pass.
- checks.log: fmt/clippy/cargo tests 0; typecheck/lint 0; vitest 124 passed.
- negative-controls.log: E1–E8 all caught.

## Limitations
- Background job progress/terminal events need the scheduler loop (not yet wired);
  `EventPublisher` is the sink it will use. Throttling applies to progress only, which is
  not emitted yet.
- "Summary totals match the same revision" for Dashboard vs list is not exercised here
  (status aggregation P10-T04).
