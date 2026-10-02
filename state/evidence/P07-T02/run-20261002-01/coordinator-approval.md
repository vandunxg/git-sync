# P07-T02 scope exception (coordinator record)

Allowed paths: `crates/application/`, `crates/infrastructure/src/platform/`.

Touched outside, required by review round 1 (findings 1, 2, 4, 5):
- `crates/infrastructure/migrations/0003_checkpoints.sql` (new, additive table
  `job_checkpoints`): the spec requires "Persist job intent, stage and safe checkpoints";
  without it the create intent (marker) and push checkpoint cannot survive a restart.
- `crates/infrastructure/src/sqlite/{jobs,mod,migrate,tests}.rs`:
  - the job state machine had no legal exit from `interrupted` except re-queuing the same
    row with its stale `plan_id`; `resolve_interrupted` closes the attempt, expires its
    plan/approvals and queues a plan-less linked attempt in one transaction;
  - `save_checkpoint` / `checkpoint`; `SCHEMA_VERSION` 2 → 3; synthetic migration tests
    shifted to 0004; store-integrated reconcile tests.
No existing column or row is changed. The edge `interrupted -> queued` is realised as a
new linked attempt (spec: "retry creates a new attempt preserving earlier evidence"),
never by re-queuing the interrupted row.
