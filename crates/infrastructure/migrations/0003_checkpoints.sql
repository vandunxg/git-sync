-- P07-T02: durable safe checkpoints for restart reconciliation. Additive only.
-- One checkpoint per job attempt: the persisted GitLab create intent (before it is
-- sent) or the plan/snapshot a push works from. Never command argv or secrets.
CREATE TABLE job_checkpoints (
  job_id TEXT PRIMARY KEY REFERENCES jobs(id),
  kind TEXT NOT NULL CHECK(kind IN ('project_create','push')),
  payload TEXT NOT NULL CHECK(length(payload) <= 4096),
  updated_at TEXT NOT NULL
);
