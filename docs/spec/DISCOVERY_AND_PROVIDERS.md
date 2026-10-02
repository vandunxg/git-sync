# Catalog discovery and provider adapters

## GitHub discovery

Identity is provider + account grant context + immutable provider repository ID, never
a name string. Persist original owner/name and current display name as mutable metadata.
Use the documented repository API appropriate to the token. V1 PAT can use the
authenticated-user repository listing; do not reuse it blindly for future App tokens
whose visible repository set is installation-scoped [S05, S06].

Follow pagination links/cursors until complete. Scope filters are explicit: owned
(including owned forks) default on; collaborations/organizations opt-in. Grant failures
are surfaced. Fine-grained tokens may see only selected resources; "all" is not the
unobservable full account. Never claim exact total if pagination was interrupted.

Use conditional requests where documented. Cache validators by exact endpoint,
parameters/account and authorization generation. Invalidate when credentials or scope
change. `updated_at`, `pushed_at`, metadata ETag and default HEAD can prioritize probing,
but a scheduled full selected-ref reconciliation still happens; unchanged metadata
cannot certify every ref. Compare ref maps after a successful observation.

## Catalog commit

Build a scan session with staged observations. Commit additions/updates transactionally
and mark scan completeness. Missing entries after a partial scan are retained. Even a
complete scan only produces `source_unavailable` candidates; it never deletes local
or GitLab backups. A 404 can mean inaccessible, not deleted. Distinguish empty repo
(no refs) from access/transport failures.

Detect rename/transfer by provider ID and update display metadata/mapping suggestions,
not an automatic filesystem move. Case collisions and invalid path characters use a
stable internal ID suffix. Never claim ownership of an existing unrelated directory.

## GitLab projects

The owner must choose the destination namespace. V1 mapping is one project per source:
proposed slug `<source-name>-gh<source-repository-id>`. Show a preview before creation.
Existing destinations require ownership/mapping verification and explicit adoption;
matching a name alone is not enough. Store the numeric project ID and canonical HTTPS
clone URL. Do not accept userinfo or arbitrary host URLs from provider payloads.

Create empty projects with no README/initial commit only after authorization. Set
private visibility by default, and never make a private source public automatically.
Check API create and Git push capabilities separately. Permission/protected-branch
failures do not justify disabling protections or requesting sudo/admin rights [S12].

If a create request times out, search by the exact intended mapping/path and reconcile
before retry; do not create duplicate projects. If a mapped destination disappears,
show missing/review, do not overwrite a similarly named project. Deleting whole GitLab
projects is not a V1 action.

## Error policy

401 => reauthenticate; 403 => inspect rate-limit/access evidence; 404 => inaccessible
or missing, not automatic deletion; 429 => honor retry metadata; 5xx/timeouts => bounded
retry with jitter after reconciliation where a write may have happened. Do not hardcode
provider quotas as guarantees. Retry loops are observable and cancellable.

References: [S05, S06, S12](../../sources/REFERENCES.md).
