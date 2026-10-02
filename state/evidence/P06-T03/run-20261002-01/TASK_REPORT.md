# P06-T03 — Apply scoped ref updates with leases (lifecycle: Full)

Requirements: REQ-06, REQ-20, REQ-22. Scenarios: TEST-ATOMIC-PUSH, TEST-PLAN-LEASE (Git
lease enforcement), TEST-READONLY-SOURCE.

## Changed files (all in scope)
- `crates/infrastructure/src/git/runner.rs` (+ tests): `Subcommand::Push` with exact flags
  `--porcelain --atomic --no-follow-tags --recurse-submodules=no --no-verify` (no
  `--force`, `--mirror`, `--all`, `--tags`, bare `--force-with-lease`, receive-pack);
  `Arg::Lease(RefName, Option<ObjectId>)` → `--force-with-lease=<ref>:<oid|>` (push
  only, before `--`); `Arg::PushSpec` `<oid>:<ref>` / `:<ref>` (push only, after `--`;
  never `+`); `GitOutput::complete_stdout` (any exit code, complete + UTF-8).
- `crates/infrastructure/src/git/push.rs` (+ tests): `GitPusher` (`RefWriter`) runs in
  the mirror (`GIT_DIR`) and pushes only to the given destination location; parses the
  porcelain report (must end in `Done`, one line per sent ref, no other refs, exit code
  must agree with rejections); exit 128 with Git's exact "does not support --atomic
  push" line → AtomicUnsupported; killed/timeout/incomplete → Unknown; failure before
  spawn → NotStarted. `MirrorAncestry` (`AncestryOracle`) via `merge-base --is-ancestor`:
  0 → true, 1 with empty stderr → false, anything else → unknown.
- `crates/application/src/apply.rs` (+ tests): `apply_authorized(authority, writer)`:
  atomic first; without atomic support a destructive multi-ref plan is blocked and
  others go non-atomic with per-ref results (Partial); a report not exactly about the
  sent refs, or an uncertain push, is Unknown and never retried; non-destructive plans
  over 100 refs are batched (later-batch uncertainty → Unknown with earlier results);
  destructive plans over one batch are blocked.

## Evidence (real Git, local bare destinations)
- TEST-READONLY-SOURCE: source tree chmod read-only through capture → plan → authorize →
  apply; destination equals the snapshot (annotated tag objects intact); source bytes
  unchanged; only heads/tags on the destination.
- TEST-PLAN-LEASE: destination moved after planning → atomic push rejected with
  `stale info`, nothing overwritten; the application gate also refuses it.
- TEST-ATOMIC-PUSH: `receive.denyDeletes` → remote rejected + atomic abort, destination
  unchanged; `receive.advertiseAtomic=false` → destructive multi-ref plan blocked
  (nothing sent), non-destructive plan reports Partial with the other party's ref kept.
- checks.log: fmt 0, clippy 0, workspace tests 0; infrastructure 142 tests ×3 green.
- negative-controls.log: N1–N12 each caught.

## Limitations
- Real GitLab (protected branches, HTTPS auth through the broker) not exercised; GitLab
  push uses the same runner path with `AuthArgs`.
- Post-push verification and persistence of results are P06-T04; jobs/locks are P07.

## Round 2
Review findings 1–6 fixed (strict push argv, `[remote failure]` → Unknown, GitLab-only
writer, replace refs disabled, exact reject reasons, exit/report tests). checks.log
round 2: fmt 0, clippy 0, workspace tests 0; infrastructure 146 tests ×3 green.
negative-controls.log: N1–N18; N8/N9 (old placement checks) are a redundant layer
behind `valid_push` and are not caught alone; N13 (removing `valid_push`) is caught.
