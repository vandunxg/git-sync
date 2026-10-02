# Safe sync engine specification

## Terminology

`Observation`: complete selected remote ref map at a known time.
`Snapshot`: locally retained OIDs/objects and immutable manifest for one observation.
`Plan`: proposed exact-ref operations bound to source snapshot, destination observation,
policy revision, destination ID and expiry.
`Verification`: post-write comparison against that plan's source snapshot, not an
unbounded claim that the remote can never change again.

## Managed mirror layout

Each repository owns a bare Git repository with app-only staging namespaces. Fetch
only allowed heads/tags into a fresh `refs/gbm/staging/<job-id>/...` namespace using
explicit refspecs. Validate job ID/ref names; forbid wildcard-derived destinations
outside this namespace. Disable tag auto-follow and submodule recursion; use full
history, not shallow/partial clones for a completed backup [S18].

Do not turn provider-private refs into writable GitLab refs. Keep the manifest's
logical names `refs/heads/*` and `refs/tags/*`; map them to local staging OIDs explicitly.
The app-owned working snapshot is distinct from staged partial fetches. Only promote a
validated full observation. A new empty source repo is valid and does not generate a
zero-length `bundle` command masquerading as a populated snapshot.

## One job, in order

1. Persist job intent; acquire repo lock; validate account, mapping, paths and policy.
2. Complete source ref observation. If inaccessible/partial, stop without modifying
   destination or deleting previous snapshots.
3. Fetch into a fresh staging namespace, validate object availability and Git integrity.
   Re-observe source refs if needed; if the source changed mid-capture, retry capture
   within a bounded budget. Never treat a mixed partial ref map as a coherent snapshot.
4. Record immutable source manifest with logical ref names, full OIDs, object format,
   default branch metadata, coverage, timestamps and a digest. Retain previous good refs.
5. Read destination selected refs and reconcile against last verified destination state.
6. Build plan; determine safety/required approvals. New/fast-forward refs on a managed
   unmodified destination can be auto-applied only when policy permits. All branch
   rewrites, tag moves, ref deletions and destination drift require review.
7. Before any destructive update, snapshot the old source/destination objects needed
   for recovery and verify the archive. Failure/disk full blocks the update.
8. Transfer required LFS payloads and verify local payload integrity before advertising
   their final refs. If LFS support is not yet implemented, block affected sync coverage
   rather than publishing a misleading complete result. See coverage rules below.
9. Revalidate plan immediately before writing. Apply exact explicit refspecs with
   exact expected-OID leases, including expected absence for newly created refs [S03].
10. Re-read destination refs and verify against the captured source map. Persist result
    before emitting success. Re-observe source cheaply; a newer source means "new changes
    detected" even if the captured snapshot was correctly backed up.
11. Optionally update the workspace under its independent safety policy. A skipped
    workspace does not erase the successful mirror result or imply the workspace is current.

## Ref comparison and operation classes

Compare full logical ref name + full object ID. Exclude peeled `^{} ` pseudo-entries
and symbolic HEAD from the ref-set digest. Store annotated tag object IDs intact;
peeling can be used only for separate informational displays. Validate object-format
compatibility; reject unsupported formats explicitly instead of truncating hashes.

| Source vs destination | Plan action | Automatic? |
|---|---|---|
| Equal OIDs | none | yes |
| New source ref, destination absent | create with absence lease | under configured policy |
| Branch ancestor -> descendant | fast_forward with old OID lease | managed destination only |
| Branch unrelated/rewritten | rewrite | approval + pre-change snapshot |
| Tag OID changed | move_tag | approval + pre-change snapshot |
| Known managed ref removed in source | delete_ref | approval + pre-change snapshot |
| Destination-only unknown ref/change | destination_drift | blocked pending adoption/review |
| Source unavailable/scan partial | no plan | never delete |

Do not push all namespaces or use blind `--mirror` [S03]. Request atomic multi-ref push
when supported. If atomic update is unavailable, V1 blocks destructive multi-ref plans;
non-destructive partial attempts must be individually observed/reconciled and reported
as partial, never falsely atomic. Do not retry blindly after an unknown push outcome.

## Approval contract

A review shows destination path, created/updated/deleted refs, old/new OIDs, snapshot
recovery path, coverage and impact. Approval is stored in native state and binds the
plan digest, policy revision and expiry. A renderer boolean `confirmed=true` is not
sufficient. Provider or policy drift invalidates the plan and requires a new review.
Bulk sync does not auto-approve destructive plans.

## LFS and submodule boundary

Git objects preserve LFS pointer files and submodule gitlinks, not necessarily the
external payloads [S04]. Missing Git LFS fails the LFS portion, not silently falls back
to pointers. Local LFS payload integrity uses hashes; server upload acknowledgment is
`uploaded`, while an isolated restore/download check is `restore_verified`.

Do not blindly recurse into .gitmodules URLs. V1 lists dependencies and their coverage;
children can be covered only as separately selected authorized repositories with the
required gitlink OIDs available. Otherwise mark `submodule_content_not_backed_up`.

## Progress

Use actual subprocess progress when parseable. `percent`, byte total and ETA may be
null. Stages are real transitions, not timed animations. `Verifying` is not `Succeeded`.
Terminal result contains separate Git/LFS/workspace coverage and structured errors.

References: [S03, S04, S14, S18](../../sources/REFERENCES.md).
