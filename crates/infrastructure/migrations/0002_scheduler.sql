-- P07-T01: scheduler support. Additive only.
-- not_before: earliest start of a queued retry attempt (RFC 3339 UTC); NULL = now.
-- mapping_id: destination mapping a mutating job writes to (shared destination lock).
ALTER TABLE jobs ADD COLUMN not_before TEXT;
ALTER TABLE jobs ADD COLUMN mapping_id TEXT REFERENCES mappings(id);
CREATE UNIQUE INDEX one_active_mapping_job ON jobs(mapping_id)
WHERE mapping_id IS NOT NULL AND state IN ('running','cancel_requested');
CREATE INDEX jobs_by_idempotency_key ON jobs(idempotency_key, state);
