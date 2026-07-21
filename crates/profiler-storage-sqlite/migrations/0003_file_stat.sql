ALTER TABLE content_objects ADD COLUMN actual_size_bytes INTEGER CHECK(actual_size_bytes IS NULL OR actual_size_bytes >= 0);
ALTER TABLE content_objects ADD COLUMN size_state TEXT NOT NULL DEFAULT 'uninspected';
ALTER TABLE content_objects ADD COLUMN modified_unix_ns INTEGER;
ALTER TABLE content_objects ADD COLUMN last_stat_run_id TEXT REFERENCES profiler_runs(id) ON DELETE SET NULL;
ALTER TABLE content_objects ADD COLUMN last_stat_at TEXT;

CREATE TABLE file_stat_observations (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE CASCADE,
    content_object_id TEXT NOT NULL REFERENCES content_objects(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL,
    source_locator TEXT NOT NULL,
    expected_locator TEXT NOT NULL,
    availability_state TEXT NOT NULL,
    size_state TEXT NOT NULL,
    expected_size_bytes INTEGER NOT NULL CHECK(expected_size_bytes >= 0),
    actual_size_bytes INTEGER CHECK(actual_size_bytes IS NULL OR actual_size_bytes >= 0),
    modified_unix_ns INTEGER,
    error_kind TEXT,
    error_message TEXT,
    observed_at TEXT NOT NULL,
    UNIQUE(run_id, content_object_id)
) STRICT;

CREATE INDEX idx_content_objects_collection_availability
    ON content_objects(collection_id, availability_state);
CREATE INDEX idx_content_objects_collection_size_state
    ON content_objects(collection_id, size_state);
CREATE INDEX idx_file_stat_run_availability
    ON file_stat_observations(run_id, availability_state);
CREATE INDEX idx_file_stat_content_run
    ON file_stat_observations(content_object_id, run_id);
