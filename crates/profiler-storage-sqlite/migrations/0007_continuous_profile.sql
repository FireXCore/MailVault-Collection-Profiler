-- MailVault Collection Profiler Alpha 6 continuous lifecycle.
ALTER TABLE collections ADD COLUMN archive_identity_aliases_json TEXT NOT NULL DEFAULT '[]'
    CHECK(json_valid(archive_identity_aliases_json));

ALTER TABLE source_snapshots ADD COLUMN retention_state TEXT NOT NULL DEFAULT 'retained'
    CHECK(retention_state IN ('retained', 'pinned'));
ALTER TABLE source_snapshots ADD COLUMN retention_reason TEXT;

ALTER TABLE profiler_runs ADD COLUMN run_kind TEXT NOT NULL DEFAULT 'initial'
    CHECK(run_kind IN ('initial', 'incremental', 'rebuild'));
ALTER TABLE profiler_runs ADD COLUMN parent_run_id TEXT REFERENCES profiler_runs(id) ON DELETE SET NULL;
ALTER TABLE profiler_runs ADD COLUMN generation INTEGER NOT NULL DEFAULT 1 CHECK(generation > 0);
ALTER TABLE profiler_runs ADD COLUMN resume_count INTEGER NOT NULL DEFAULT 0 CHECK(resume_count >= 0);
ALTER TABLE profiler_runs ADD COLUMN last_resumed_at TEXT;
ALTER TABLE profiler_runs ADD COLUMN last_heartbeat_at TEXT;
ALTER TABLE profiler_runs ADD COLUMN failure_retryable INTEGER NOT NULL DEFAULT 0 CHECK(failure_retryable IN (0,1));
ALTER TABLE profiler_runs ADD COLUMN source_revision_fingerprint TEXT;

ALTER TABLE content_objects ADD COLUMN active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1));
ALTER TABLE content_objects ADD COLUMN last_seen_snapshot_id TEXT REFERENCES source_snapshots(id) ON DELETE SET NULL;
ALTER TABLE content_objects ADD COLUMN stat_input_fingerprint TEXT;
ALTER TABLE content_objects ADD COLUMN last_stat_success_fingerprint TEXT;
ALTER TABLE content_objects ADD COLUMN stat_retry_count INTEGER NOT NULL DEFAULT 0 CHECK(stat_retry_count >= 0);
ALTER TABLE content_objects ADD COLUMN format_input_fingerprint TEXT;
ALTER TABLE content_objects ADD COLUMN last_format_success_fingerprint TEXT;
ALTER TABLE content_objects ADD COLUMN format_retry_count INTEGER NOT NULL DEFAULT 0 CHECK(format_retry_count >= 0);

CREATE TABLE snapshot_retention_events (
    id TEXT PRIMARY KEY,
    snapshot_id TEXT NOT NULL,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    action TEXT NOT NULL CHECK(action IN ('deleted','delete_failed')),
    reason TEXT NOT NULL,
    detail_json TEXT NOT NULL CHECK(json_valid(detail_json)),
    occurred_at TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX idx_runs_one_live_per_collection
    ON profiler_runs(collection_id)
    WHERE state IN ('pending','preflighting','snapshotting','ready','running','pausing','paused','cancelling');
CREATE INDEX idx_runs_collection_generation ON profiler_runs(collection_id, generation DESC);
CREATE INDEX idx_content_objects_collection_active_sha ON content_objects(collection_id, active, sha256);
CREATE INDEX idx_snapshots_collection_retention_created
    ON source_snapshots(collection_id, retention_state, created_at DESC);
