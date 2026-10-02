# Restore drill runbook

Purpose: demonstrate recoverability without modifying GitHub, an existing worktree or a
production GitLab project. This is not a destructive production-restore automation.

## Preconditions

Select an explicitly authorized snapshot and a new empty restore folder outside managed
live data. Record archive/manifest hash, object format, source/destination stable IDs, scope,
expected refs, LFS objects and gitlink limitations. Confirm free space. Disallow GitHub
network fallback in the drill. Network to GitLab is allowed only when that drill specifies it.

Do not execute an unreviewed command pasted from a manifest. Treat paths/ref names as
untrusted input and run the same native validation as the application. Stop if any selected
path resolves into the active mirror, source workspace or another user's data.

## Git archive drill

1. Check every checksum before consuming the archive. Reject incomplete/staging snapshots.
2. Verify the full self-contained Git bundle. For an explicitly empty repo verify its empty
   manifest instead; do not claim there was a populated bundle.
3. Restore into a new isolated repository with the manifest's logical refs, preserving tag
   object OIDs and non-default branches. Do not blindly push provider-internal namespaces.
4. Run Git object integrity verification and compare every claimed ref/full OID against the
   manifest. Verify representative historical commits and annotated tags are traversable.
5. Confirm the operation did not consult GitHub to retrieve missing objects.

## Destination and LFS drill

For a remote-backup drill, clone/fetch from GitLab into a different empty sandbox and compare
with the backed-up source snapshot, not the latest moving GitHub state. Materialize required
LFS payloads and verify declared SHA-256/size. Pointer text alone is insufficient. Prove that
payloads come from the permitted backup/local object store rather than a cached GitHub copy.
Use an isolated LFS cache/config to prevent a false positive. Check gitlink content only for
explicitly included child repositories and required commits; report the rest as not covered.

## Exit evidence

Record commands, exit codes, snapshot hash, expected/actual ref digests, LFS hash results,
network restrictions and recovered scope. On mismatch preserve evidence and block destructive
sync/release for that scope. Do not edit the manifest to match a defective restore.
On success remove only disposable folders whose identity was recorded by this drill and only
when authorized. Retain production snapshots and logs according to the documented policy.
