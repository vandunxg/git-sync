# P06-T01 — Destination preview / adoption / create policy

Requirements: REQ-04, REQ-41. Scenarios: TEST-PROJECT-CREATE, TEST-PRIVACY (destination part).

## Changed files
- `crates/application/src/destination.rs` (+ `destination/tests.rs`), `lib.rs`:
  - `Visibility`, `ProjectView`, `CreateIntent` (namespace, path, visibility,
    source_private, random marker), `DestinationApi` port (find_by_path, get_by_id, create).
  - `ensure_project`: exact-path lookup first; create only when definitely absent;
    after any create failure except auth, look up again; accept only a project in
    the intended namespace whose description carries `gbm-intent:<marker>` as a whole
    word, no more visible than intended, and empty. Otherwise Blocked(Foreign/NotEmpty/
    VisibilityUnsafe). Absent after an uncertain create → original error (later retry
    starts with the lookup). One create per call; never a duplicate per intent.
  - `allowed_visibility`: a private source only to private (V1 blocks; the contract has
    no consent field). Re-checked inside `ensure_project`.
  - `assess_adoption`: explicit-only; requires same namespace, not already mapped,
    visibility safe, access level ≥ Developer (30) reported.
  - `observe_intended_paths` (read-only exact lookups), `build_preview` (items matching
    DestinationPreviewItem: needsCreation only when the exact path was observed absent;
    stable warning codes), `preview_digest` (SHA-256, length-prefixed fields),
    `commit_preview` (digest + expiry; returns proposals + allow_create_missing; never
    adopts).
- `crates/infrastructure/src/gitlab/destination.rs` (+ tests), `gitlab/mod.rs`:
  `GitLabDestination` implements the port over `GET /api/v4/projects/:id|:path` and
  `POST /api/v4/projects` (JSON: name, path, namespace_id, visibility, description,
  initialize_with_readme=false). Validates numeric IDs, plain paths, clone URL exactly
  `<origin><path_with_namespace>.git` (no userinfo/other host), known visibility, and
  that a path lookup answered for that same path. 404 = absent; redirects refused.
- Scope exception: `http/mod.rs` (POST + origin accessor), `http/test_server.rs`
  (test-only) — see coordinator-approval.md.

## Evidence
- checks.log: fmt 0, clippy -D warnings 0, workspace tests 0 (application 70,
  infrastructure 135).
- negative-controls.log: N1–N16 each caught (N14–N16 added in round 2).
- TEST-PROJECT-CREATE: in-memory stateful double (timeout after remote success →
  Reconciled, retry → Reconciled, exactly one project) and the real HTTP adapter against
  the loopback server (POST hangs past timeout → lookup finds marker → Reconciled, one
  POST; same-name unrelated project → Blocked with GET only).
- TEST-PRIVACY (destination part): private source → public/internal blocked at preview
  and at create; private source cannot adopt an internal/public project.

## Limitations
- No real GitLab account was used (no authorized test target); remote behaviour is per
  the documented API, exercised against a loopback server.
- IPC wiring and SQLite persistence of mappings/intents are not in this task; the
  onboarding Review screen still returns UNSUPPORTED.
- Push capability is only inferred from reported access level for adoption; actual push
  permission/protected branches are checked by P06-T03.
- TEST-PRIVACY's telemetry/debug half is covered elsewhere (REQ-39 tasks).

## Round 2
Review findings 1, 2, 4 fixed (non-empty intendedPath, per-item source privacy carried
into the digest and commit, per-item unknown lookups). checks.log round 2: fmt 0,
clippy 0, workspace tests 0 (application 72, infrastructure 135).
