# Retention and restore verification

## Why retention is separate

A mirror follows source updates and can reproduce unwanted deletion or rewritten
history. It is not immutable storage. This system uses safe mirror plans plus local
recovery snapshots; it does not advertise GitLab as immutable or independent of the
user's credentials/machine. For stronger disaster recovery, an independent offline or
immutable destination is a future deployment decision, not a hidden V1 guarantee.

## Snapshot transaction

Before destructive ref updates, retain the previous selected source refs and any
observed destination-only data required for recovery. Write a new snapshot directory:
manifest + complete Git bundle for the retained logical refs + hashes + LFS object
coverage manifest. Use full self-contained bundles in V1; incremental bundle chains
are deferred. Empty repos use an explicit empty manifest, not a fake bundle success.

Verify the bundle, its object/ref coverage, file hashes and required LFS payloads before
marking snapshot `ready`. Keep a staging suffix until verification completes. If free
space, object availability or validation fails, block the destructive plan. Never
auto-delete the last verified snapshot to free space for an unsafe update.

Manifests contain source/destination IDs, full ref names/OIDs, object format, scope,
creation time, reason, checksum list, bundle version and LFS/submodule limitations.
They contain no credentials. Keep previous snapshots append-only in application policy;
do not claim underlying storage is physically immutable. Manual cleanup is a separate
reviewed operation, not an automatic V1 retention timer.

## Restore drill - not a production overwrite

Restore into a fresh temporary folder that is not the source/workspace/destination.
Verify bundle checksums, recreate refs, run object integrity checks and compare all
selected ref/OID entries. For LFS fixtures, fetch/materialize actual payloads from the
backup and verify hashes. Verify annotated tags and at least one non-default branch.
Check that no GitHub network access is required for the restored data claimed to be
covered. A restore that silently downloads missing objects from GitHub is not proof.

The drill for basic Git backups is required before enabling destructive sync. The full
LFS/retention/native restore drill is required for V1 release. Failed coverage must be
reported exactly; do not downgrade it to an informational log and show "fully backed up".

## Runbook inputs

The owner supplies an existing snapshot path, a new empty restore destination and
whether network access to GitLab is allowed. The runbook previews all writes; it never
pushes to GitHub or replaces an existing working tree. Automated tests use isolated
fixtures. See [restore acceptance](../quality/RESTORE_RUNBOOK.md).

References: [S03 mirror behavior, S04 LFS separation, S14 bundles](../../sources/REFERENCES.md).
