# P07-T03 scope exception (coordinator record)

Allowed paths: `apps/desktop/src-tauri/src/events/`, `apps/desktop/src/lib/native/`,
`crates/application/`.

Touched outside (wiring only; outcome "committed revisions drive the Query cache" is
not reachable without it):
- `apps/desktop/src-tauri/src/lib.rs`: `mod events;` and wrapping the live handler with
  `events::emitting` (startup failure handler if the run id is invalid).
- `apps/desktop/src/lib/api/stateEvents.ts` (new): event type → Query key prefixes and the
  `useStateEvents` hook; `lib/api/hooks.ts`: `structuralSharing: keepNewestRevision` so an
  older read model never replaces a newer one; `lib/api/index.ts`: exports.
- `apps/desktop/src/app/App.tsx`, `apps/desktop/src/main.tsx`: mount the bridge with the
  Tauri event source (fixture entries pass none).
- `apps/desktop/src/test/native/state-events.test.ts` (new tests).
No capability change: `core:event:default` (listen) was already the only permission.
