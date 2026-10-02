# R-GIT - immutable invariants for backup jobs

1. GitHub is read-only and authoritative for selected heads/tags at a captured snapshot.
2. Push only explicit refs/heads/* and refs/tags/* refspecs. Provider-internal refs,
   refs/pull/*, refs/merge-requests/*, local stash, private staging refs and symbolic
   HEAD are not pushed. Annotated tag object OIDs, not just peeled commits, are compared.
3. A blind `git push --mirror`, `--prune`, `--force`, `reset --hard`, `clean -fd`, or
   automatic stash is forbidden in production. The verified plan executor may perform
   exact-ref updates/deletes with exact-OID leases after applicable approval [S03].
4. New refs/fast-forwards can run automatically only under enabled per-account policy
   and an app-owned destination mapping. Rewrites/deletions require a reviewed plan,
   a pre-change snapshot, and a fresh destination comparison. Never disable branch
   protection automatically to make a push work.
5. The plan binds source snapshot, destination identity/observed OIDs, policy revision,
   operation list and expiry. Any drift invalidates approval; recompute, do not reuse it.
6. Do not confuse source branch deletion with a vanished/inaccessible repository.
   401/403/404, rate limits and partial pagination never trigger deletion of backups.
7. Full-refs comparison and local object validation define Git verification. One HEAD
   comparison or timestamps do not. Git LFS and submodules have separate coverage.
8. A successful backup is current relative to a snapshot and observation timestamp,
   never a claim of perpetual equality with a remote that can change concurrently.
9. Managed mirrors and workspaces are independent. Dirty/detached/ahead/diverged working
   copies are not automatically altered. A skipped workspace update is shown separately.
10. Mirroring is not historical retention. Verify pre-change bundles/manifests and keep
    needed LFS objects before allowing destructive changes. A bundle alone contains
    no external LFS payload [S04, S14]. Restore tests are part of release acceptance.

See [sync algorithm](../docs/spec/SYNC_ENGINE.md), [retention](../docs/spec/RETENTION_RESTORE.md)
and [references](../sources/REFERENCES.md) for details. These guards must have executable
tests and native checks, not only comments or confirmation text in React.
