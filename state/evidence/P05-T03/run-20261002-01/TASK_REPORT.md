# P05-T03 — Pre-change snapshots and basic restore drill

Requirements: REQ-18, REQ-20. Scenarios: TEST-SNAPSHOT, TEST-BUNDLE-RESTORE.

## Changed files
- crates/application/src/recovery.rs (+ tests): SnapshotStore port (create/verify),
  RecoveryRequest/RecoverySnapshot, StoreError (NoSpace, Corrupt, Exists, ...),
  ChangeKind/PlannedChange and `require_recovery` gate: destructive changes (rewrite,
  tag move, delete) need a recovery snapshot that verifies *now* and holds every old
  ref at the exact old OID; mislabelled changes are rejected; sha256_hex helper.
- crates/infrastructure/src/snapshots/{mod,tests}.rs: BundleStore under
  `.gbm/snapshots/gh-<id>/<snapshot-id>/` — staging dir (owned, 0700) → `git bundle
  create --glob=<namespace>*` from the mirror → list-heads equals manifest refs →
  full restore into a fresh bare repo (init + fetch from the bundle file only + fsck +
  exact ref comparison) → SHA-256 + size → manifest.json (0600, fsync; refs, format,
  reason, namespace, checksums, coverage lfs/submodules not_covered, limitations, no
  credentials) → dir fsync → rename to final. Failures remove exactly the staging
  dir. verify(): checksum + size + heads + restore. restore(): runbook restore into
  an existing empty canonical dir outside the backup root; never contacts a source.
  Empty repos: explicit manifest, no bundle.
- crates/infrastructure/src/git/runner.rs: bundle support (coordinator-approval.md).
- crates/application/src/lib.rs, crates/infrastructure/src/lib.rs: registrations.

## Scenario coverage
- TEST-BUNDLE-RESTORE (real Git): source moved away, restore into a fresh dir: all 7
  refs with exact OIDs, annotated tag is a tag object, non-default (rewritten) branch
  matches, no remote configured, fsck --full --strict clean.
- TEST-SNAPSHOT: gate with good (allowed), corrupt / no-space / not-found / failed
  (blocked), stale snapshot not covering an old OID (blocked), missing snapshot
  (blocked); real store: byte flip → Corrupt → gate blocks; valid-but-different
  bundle (v3) → checksum Corrupt; tampered manifest → Corrupt; append → Corrupt.
  Disk-full: classification of ENOSPC / "No space left on device" unit-tested and the
  gate blocks on NoSpace; a real full filesystem was NOT exercised (needs a
  size-limited mount, not available without privileges) — stated honestly.
- Failed creation leaves no staging/verify dirs; ids/namespaces validated; append-only
  (Exists); restore refuses non-empty or in-root destinations.

## Review round 1 fixes
- 1 restore layer pinned independently (manifest sha/bytes rewritten to match): thin
  bundle with prerequisites (heads identical) → Corrupt + gate blocks; extra ref →
  Corrupt; truncated pack → Corrupt; manifest `bundle: null` with a bundle present →
  Corrupt. Stale test comment removed. N8 (skip restore) caught by the new tests.
- 2 verify temp dir unique per call (`<id>.verify-<pid>-<n>`); single-writer-per-repo
  invariant for create documented (job queue serializes per repository).
- 3 PlannedChange contract documented: kind must come from the planner's observed
  state + ancestry (P06 acceptance item); the gate cannot see ancestry.
- 4 manifest parsing rejects kind != recovery, manifestVersion != 1, a different
  sourceRepositoryId and an invalid bundleRefNamespace (test; N9).
- 5 staging removed if the final rename fails; partial restore destination and
  unsigned manifests documented.

## Commands
checks.log: fmt 0, clippy -D warnings 0, workspace tests 0, infra suite 3× green.
negative-controls.log: N1–N9 each caught. scope-check.json: FAIL only for runner.rs/
runner tests and lib.rs (documented) + owner's untracked docs/design.

## Residual risks for OWNER acceptance (security decision)
1. Manifests/checksums are unauthenticated: a local writer to the backup root can
   forge a snapshot that verifies (protects against corruption, not local attackers).
2. Each gate-time verify performs a full restore (≈ one repository of transient disk
   and time); NoSpace there blocks the change.
3. Only source-mirror refs are snapshotted; destination-only GitLab state is a P06
   item. HEAD/default-branch metadata is not captured.
4. No retention/GC and no orphaned staging-namespace GC yet.
5. Fetch from a bundle uses the local file transport; a local writer swapping the file
   for a repository mid-verify could only produce data that still must match the
   manifest refs exactly and pass fsck.

## Limitations
- Destination-only data (observed GitLab refs) is not snapshotted yet: P06 must add
  destination recovery capture before destructive pushes.
- Real disk-full not executed (see above). No retention cleanup (manual, reviewed op).
- RESTORE runbook document/UI wiring not included (restore() API + tests only).
- Orphaned staging namespace GC (P05-T02 follow-up) still open.
