# P05-T02 — Capture complete local source snapshots

Requirements: REQ-05, REQ-07, REQ-42. Scenarios: TEST-REFS, TEST-EMPTY-REPO, TEST-VERIFY.

## Changed files
- crates/application/src/snapshot.rs (+ snapshot/tests.rs): MirrorGit port,
  ObservationClock, capture_snapshot: complete observation → (empty: explicit empty
  manifest, no fetch/prepare) → prepare mirror (object format checked) → clear stale
  staging → explicit forced refspecs into refs/gbm/staging/<job>/ → read back; accept
  only if equal to an observation (re-observe once per attempt; ≤3 attempts) →
  integrity (fsck) → refuse existing snapshot id → copy to refs/gbm/snapshots/<id>/ →
  read back (mismatch: delete exact promoted refs, PromotionMismatch) → remove exact
  staging refs on every path → manifest (format, refs, head target, observedAt,
  SHA-256 ref-set digest, namespace, coverage git=Complete / lfs, submodules=
  NotChecked) → post-capture re-observation (Unchanged/Changed/Unknown).
- crates/application/src/observation.rs: ReadError::Unsupported.
- crates/application/Cargo.toml: sha2 0.10.9 (already locked).
- crates/infrastructure/src/git/mirror.rs (+ mirror/tests.rs): GitMirror (MirrorGit via
  GitRunner; GIT_DIR = app-owned mirror; fetch batches of 200 refspecs; for-each-ref
  parsing fail-closed; update-ref -d with expected old OID) and RunnerRefReader
  (ls-remote via runner; Truncated/Unsupported/NotFound/AccessDenied/Transport).
- crates/infrastructure/src/git/runner.rs: allowlist additions + FullRef (see
  coordinator-approval.md). git/mod.rs: module registration.

## Scenario coverage (real Git on local fixtures + in-memory fakes)
- TEST-REFS: real repository (branches incl. unicode, annotated/lightweight/moved tags)
  captured: manifest == ls-remote map, annotated tag object IDs kept, snapshot
  namespace has 7 refs, no staging left, mirror passes fsck; second capture keeps the
  first snapshot intact (incl. a tag deleted at source); snapshot id reuse refused.
- TEST-EMPTY-REPO: explicit empty manifest, mirror directory left empty (no init, no
  fetch, no bundle, no fabricated HEAD); first commit captured afterwards.
- TEST-VERIFY: Git "success" with a wrong staged ref is never promoted; real source
  moving mid-capture recorded coherently via re-observation; a source that keeps
  moving exhausts the budget with nothing promoted; newer source after capture →
  recheck Changed (update, not synced); integrity failure blocks promotion.
- Also: SHA-256 source into SHA-1 mirror → ObjectFormatMismatch; non-repository source
  → NotFound (never "empty").

## Review round 1 fixes
- #1 partial promotion: any failure after the SnapshotExists check removes the whole
  target namespace (remove_namespace); copy fetch is `--atomic` per batch. Fake test +
  N8; real >200-ref capture covers multi-batch copy.
- #2 cancellation: ReadError::Cancelled / CaptureError::Cancelled (never "source
  unavailable"); cleanup runs via remove_namespace with an uncancelled token;
  `staging_removed` reported in the outcome. Real-git test found that a pre-cancelled
  token still ran git (runner only polled after 25 ms): runner now returns
  RunError::Cancelled before spawning (test; N11). N10.
- #3 a failed source fetch is retried after re-observation; persistent failure
  surfaces as the mirror error. N9.
- #4 delete/cleanup restricted to refs/gbm/staging/ and refs/gbm/snapshots/ (real
  test with a correct-OID ref outside the namespace; N12).
- #5 classify uses Git's own phrases (repository not found, does not appear to be a
  git repository, returned error: 401/403/404, authentication failed).
- #6 real-git tests: 237 refs (2 fetch/copy batches), corrupted loose object →
  IntegrityFailed with old snapshot kept, cancelled job, wrong-OID deletion refused.

## Commands
checks.log: fmt 0, clippy -D warnings 0, workspace tests 0, infra suite 3× green.
negative-controls.log: N1–N12 each caught. scope-check.json: FAIL only for Cargo.lock
(documented) and the owner's untracked docs/design.

## Limitations
- Manifest is returned in memory; persistence (snapshots table) and manifest files /
  bundles are P05-T03 / persistence wiring. Not wired to IPC/jobs yet (P07).
- Integrity check is a full-repository fsck per capture (correct, may be slow on very
  large mirrors).
- Credentials for private sources: adapters accept AuthArgs; per-job broker start and
  account binding are wired with the job runner (P07).
