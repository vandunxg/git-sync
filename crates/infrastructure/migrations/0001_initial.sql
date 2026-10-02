-- Migration 0001: initial schema (from contracts/db.reference.sql, blueprint v1.0.0).
-- Never edit after release: sqlx records a checksum and refuses a modified migration.
-- Connection-level PRAGMAs (foreign_keys, journal_mode, busy_timeout) are set by the
-- application when it opens the database, not here.

CREATE TABLE accounts (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL CHECK(provider IN ('github','gitlab')),
  provider_user_id TEXT NOT NULL,
  username TEXT NOT NULL,
  credential_ref TEXT,
  auth_state TEXT NOT NULL,
  storage_mode TEXT NOT NULL CHECK(storage_mode IN ('keyring','session_only','none')),
  verified_at TEXT,
  UNIQUE(provider, provider_user_id)
);
CREATE TABLE roots (
  id TEXT PRIMARY KEY,
  canonical_path TEXT NOT NULL UNIQUE,
  ownership_marker TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE repositories (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id),
  provider_repository_id TEXT NOT NULL,
  full_name TEXT NOT NULL,
  default_branch TEXT,
  is_private INTEGER NOT NULL CHECK(is_private IN (0,1)),
  is_fork INTEGER NOT NULL CHECK(is_fork IN (0,1)),
  is_archived INTEGER NOT NULL CHECK(is_archived IN (0,1)),
  selected INTEGER NOT NULL CHECK(selected IN (0,1)),
  observed_at TEXT,
  revision INTEGER NOT NULL DEFAULT 0,
  UNIQUE(account_id, provider_repository_id)
);
CREATE TABLE mappings (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL UNIQUE REFERENCES repositories(id),
  root_id TEXT NOT NULL REFERENCES roots(id),
  gitlab_account_id TEXT NOT NULL REFERENCES accounts(id),
  namespace_id TEXT NOT NULL,
  project_id TEXT,
  intended_project_path TEXT NOT NULL,
  mirror_relative_path TEXT NOT NULL,
  workspace_relative_path TEXT,
  allow_create_missing INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 0,
  UNIQUE(gitlab_account_id, intended_project_path),
  UNIQUE(root_id, mirror_relative_path)
);
CREATE TABLE scan_sessions (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id),
  started_at TEXT NOT NULL,
  completed_at TEXT,
  complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0,1)),
  error_code TEXT
);
CREATE TABLE snapshots (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  kind TEXT NOT NULL CHECK(kind IN ('source','destination_recovery')),
  status TEXT NOT NULL CHECK(status IN ('staging','ready','invalid')),
  digest TEXT NOT NULL,
  object_format TEXT NOT NULL,
  created_at TEXT NOT NULL,
  manifest_relative_path TEXT,
  bundle_relative_path TEXT,
  coverage_json TEXT NOT NULL
);
CREATE TABLE snapshot_refs (
  snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
  ref_name TEXT NOT NULL,
  oid TEXT NOT NULL,
  PRIMARY KEY(snapshot_id, ref_name)
);
CREATE TABLE plans (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  source_snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
  mapping_id TEXT NOT NULL REFERENCES mappings(id),
  source_digest TEXT NOT NULL,
  destination_digest TEXT NOT NULL,
  plan_digest TEXT NOT NULL,
  policy_revision INTEGER NOT NULL,
  operations_json TEXT NOT NULL,
  requires_approval INTEGER NOT NULL,
  expires_at TEXT NOT NULL
);
CREATE TABLE approvals (
  id TEXT PRIMARY KEY,
  plan_id TEXT NOT NULL REFERENCES plans(id),
  plan_digest TEXT NOT NULL,
  policy_revision INTEGER NOT NULL,
  approved_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
);
CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  repository_id TEXT REFERENCES repositories(id),
  account_id TEXT REFERENCES accounts(id),
  plan_id TEXT REFERENCES plans(id),
  kind TEXT NOT NULL CHECK(kind IN ('scan','plan','sync','restore_test')),
  state TEXT NOT NULL CHECK(state IN ('queued','running','waiting_for_approval','waiting_for_retry','cancel_requested','succeeded','partial','failed','canceled','interrupted')),
  stage TEXT NOT NULL,
  attempt INTEGER NOT NULL CHECK(attempt >= 1),
  idempotency_key TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  error_code TEXT,
  coverage_json TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 0,
  UNIQUE(idempotency_key, attempt)
);
CREATE UNIQUE INDEX one_active_repo_job ON jobs(repository_id)
WHERE repository_id IS NOT NULL AND state IN ('running','cancel_requested');
CREATE TABLE settings (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  revision INTEGER NOT NULL,
  document_json TEXT NOT NULL
);
CREATE TABLE activity (
  id TEXT PRIMARY KEY,
  repository_id TEXT REFERENCES repositories(id),
  job_id TEXT REFERENCES jobs(id),
  occurred_at TEXT NOT NULL,
  severity TEXT NOT NULL CHECK(severity IN ('info','warning','error')),
  event_type TEXT NOT NULL,
  safe_message TEXT NOT NULL,
  error_code TEXT
);
CREATE INDEX activity_by_time ON activity(occurred_at, id);
CREATE TABLE revisions (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  current_revision INTEGER NOT NULL
);
INSERT INTO revisions(singleton, current_revision) VALUES (1, 0);
