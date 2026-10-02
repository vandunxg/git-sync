# Data model and persistence

The executable reference DDL is `contracts/db.reference.sql`. It is a model aid, not
an applied production migration. P03 must create versioned migrations, test upgrades
from every supported schema version and back up DB metadata before risky migration.
Use parameterized SQL and validate all JSON documents against their native DTO models.

Identity tables separate accounts, repositories, roots and destination mappings. A
source repository's immutable provider ID survives display rename/transfer. Account
records are retained after disconnect when needed by historical mappings; remove the
credential reference and mark the account disconnected rather than cascading data loss.

Snapshots store immutable logical ref maps and coverage; plans reference their exact
source snapshot and expected destination state. Approvals bind a plan digest and policy
revision. A job owns each attempt and idempotency identity; updates increment the
read-model revision. Activity stores only sanitized messages and reference IDs.

Do not store `primaryStatus` as an independently editable truth; derive it from observed
state and coverage. Add read-model tables only after documenting derivation/rebuild
behavior. The reference DDL omits process-local locks, transient secret buffers and
cache implementation details deliberately; these are not fake persistent security.

## Transaction rules

Persist intent before launching external effects. Avoid any SQLite transaction around
network/Git awaits. Update terminal job state, observation and revision atomically
where required; then emit an event. On a write-after-network failure, mark uncertainty
and reconcile remote state instead of blindly repeating the external operation.

WAL is suitable only for the selected local DB placement; configure foreign keys,
busy timeout and bounded connections consistently. File backup of an active WAL DB
requires a SQLite-aware backup/checkpoint procedure, not copying only state.sqlite
[S13]. Database integrity checks do not replace Git object/remote verification.

## Retention and deletion

Deleting an app history row does not delete remote projects or snapshots implicitly.
V1 has no whole-project remote-delete action. An unavailable source remains represented
with its previous verified state and a warning. Clean up only exact app-owned temporary
artifacts tracked to a failed job, under the filesystem safety policy.
