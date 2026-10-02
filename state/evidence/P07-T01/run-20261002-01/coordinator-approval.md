# P07-T01 scope exception (coordinator record)

Allowed paths: `crates/application/`, `crates/infrastructure/src/sqlite/`.

Touched outside: `crates/infrastructure/migrations/0002_scheduler.sql` (new, additive).
The SQLite store (`src/sqlite/`) embeds migrations from that directory
(`sqlx::migrate!("./migrations")`); the scheduler needs durable `not_before` (retry
attempts) and `mapping_id` (shared destination lock, spec "also lock a shared
destination mapping"), plus a unique partial index for one active job per mapping.
No existing column or row is changed; `SCHEMA_VERSION` 1 → 2 with the existing
downgrade refusal and pre-migration backup.
