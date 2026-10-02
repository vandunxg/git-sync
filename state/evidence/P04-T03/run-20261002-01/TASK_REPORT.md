# P04-T03 — Full selected-ref observations and status inputs

Requirements: REQ-06, REQ-07, REQ-08, REQ-42.
Scenarios: TEST-REFS, TEST-READONLY-SOURCE, TEST-FRESHNESS, TEST-EMPTY-REPO.

## Changed files
- crates/domain/src/refs.rs (+tests): ObjectId (full lower-case SHA-1/SHA-256, zero
  rejected), RefName (refs/heads|tags + check-ref-format, no leading `-`), RefMap,
  fail-closed `parse_ls_remote` (symref HEAD and `^{}` kept separate, other namespaces
  ignored and counted, duplicates/mixed format/orphan peeled rejected), `diff`.
- crates/domain/src/observation.rs (+tests): SourceObservation ordered by start
  sequence; SourceView.apply ignores older/replayed results; partial/unavailable never
  erase the last complete map; source_state = unknown/unavailable/stale/changed/current
  (current only when fresh, complete and equal to the snapshot map).
- crates/application/src/observation.rs (+tests): read-only `SourceRefReader` port (no
  write method exists) and `observe_source` mapping errors to honest outcomes.
- crates/infrastructure/src/git/{mod,tests}.rs: `ls_remote_args` (`--` before URL);
  integration tests run real `git ls-remote` against local fixture repos only.
- lib.rs module registrations (infrastructure/src/lib.rs outside listed paths — same
  one-line scope note as P04-T02).

## Scenario coverage
- TEST-REFS: domain synthetic + real Git (non-default branch commit + annotated tag
  move with main/HEAD unchanged → exactly those two refs; tag object ID stored).
- TEST-FRESHNESS: older/replayed observation ignored; partial → unknown, failure →
  unavailable, old → stale, all distinct; last complete map retained.
- TEST-EMPTY-REPO: observation half (explicit empty map, unborn HEAD target not turned
  into a ref, first commit detected). Empty manifest capture / no zero-length bundle is
  P05-T02; no push is involved yet.
- TEST-READONLY-SOURCE: observation half — no source write API; a write-proof source is
  fully observed and its tree is byte/mtime-identical afterwards. Full scan/plan/sync
  lifecycle with network trace belongs to P05/P06 integration.

## Review round 1 fixes
- M1 peeled entry of an ignored ref / HEAD (`refs/custom/x^{}`, `HEAD^{}`) no longer
  rejects the advertisement; orphan check tracks any previous non-peeled name.
- M2 v2 `ref: <target>\t<name>` lines for non-HEAD symrefs accepted (only HEAD's target
  recorded; symbolic branches still compared by their own OID line).
- L1 non-empty output without final newline → `Truncated` → Partial; reader contract
  documents `Ok` = exit 0, not truncated, valid UTF-8.
- L2 real-Git test across protocol v0 and v2, with symbolic branch, remote-tracking
  symbolic HEAD, custom annotated ref and detached HEAD at an annotated tag.

## Commands
checks.log: fmt 0, clippy -D warnings 0, workspace tests 0. negative-controls.log: N1–N11
each caught (N9/N10 reintroduce the pre-review M2/M1 behaviour).

## Limitations
- No production execution of ls-remote yet (secure runner P05-T01, per P04 transport
  dependency note). No persistence of observations (sqlite outside scope).
- Observation sequence allocation is the caller's (job/queue, P07).
