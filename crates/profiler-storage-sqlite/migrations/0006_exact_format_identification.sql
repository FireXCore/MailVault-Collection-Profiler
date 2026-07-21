-- Exact format identification is a versioned downstream projection over a completed
-- physical inventory. Existing format_assertions remain for backwards compatibility;
-- all alpha.4 writes use the richer observation/match model below.

ALTER TABLE content_objects ADD COLUMN format_state TEXT NOT NULL DEFAULT 'uninspected'
    CHECK(format_state IN (
        'uninspected', 'identified', 'unknown', 'ambiguous', 'empty',
        'skipped_unavailable', 'tool_error'
    ));
ALTER TABLE content_objects ADD COLUMN primary_puid TEXT;
ALTER TABLE content_objects ADD COLUMN primary_format_name TEXT;
ALTER TABLE content_objects ADD COLUMN primary_format_version TEXT;
ALTER TABLE content_objects ADD COLUMN primary_format_mime_type TEXT;
ALTER TABLE content_objects ADD COLUMN format_match_count INTEGER NOT NULL DEFAULT 0
    CHECK(format_match_count >= 0);
ALTER TABLE content_objects ADD COLUMN extension_checked INTEGER NOT NULL DEFAULT 0
    CHECK(extension_checked IN (0, 1));
ALTER TABLE content_objects ADD COLUMN extension_mismatch INTEGER NOT NULL DEFAULT 0
    CHECK(extension_mismatch IN (0, 1));
ALTER TABLE content_objects ADD COLUMN last_format_run_id TEXT;
ALTER TABLE content_objects ADD COLUMN last_format_at TEXT;

CREATE TABLE format_tools (
    id TEXT PRIMARY KEY,
    tool_name TEXT NOT NULL,
    tool_version TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    executable_sha256 TEXT NOT NULL,
    signature_path TEXT NOT NULL,
    signature_sha256 TEXT NOT NULL DEFAULT '',
    signature_version TEXT NOT NULL,
    signature_created TEXT,
    identifiers_json TEXT NOT NULL CHECK(json_valid(identifiers_json)),
    probed_at TEXT NOT NULL,
    UNIQUE(tool_name, executable_sha256, signature_version, signature_sha256)
) STRICT;

CREATE TABLE format_identification_runs (
    id TEXT PRIMARY KEY,
    baseline_run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE RESTRICT,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE RESTRICT,
    tool_id TEXT NOT NULL REFERENCES format_tools(id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK(state IN ('running', 'succeeded', 'failed', 'cancelled')),
    configuration_fingerprint TEXT NOT NULL,
    batch_size INTEGER NOT NULL CHECK(batch_size > 0),
    worker_count INTEGER NOT NULL CHECK(worker_count > 0),
    timeout_seconds INTEGER NOT NULL CHECK(timeout_seconds > 0),
    total_objects INTEGER NOT NULL CHECK(total_objects >= 0),
    eligible_objects INTEGER NOT NULL CHECK(eligible_objects >= 0),
    completed_objects INTEGER NOT NULL DEFAULT 0 CHECK(completed_objects >= 0),
    total_bytes INTEGER NOT NULL CHECK(total_bytes >= 0),
    completed_bytes INTEGER NOT NULL DEFAULT 0 CHECK(completed_bytes >= 0),
    identified INTEGER NOT NULL DEFAULT 0 CHECK(identified >= 0),
    unknown INTEGER NOT NULL DEFAULT 0 CHECK(unknown >= 0),
    ambiguous INTEGER NOT NULL DEFAULT 0 CHECK(ambiguous >= 0),
    empty_objects INTEGER NOT NULL DEFAULT 0 CHECK(empty_objects >= 0),
    skipped_unavailable INTEGER NOT NULL DEFAULT 0 CHECK(skipped_unavailable >= 0),
    tool_errors INTEGER NOT NULL DEFAULT 0 CHECK(tool_errors >= 0),
    extension_mismatches INTEGER NOT NULL DEFAULT 0 CHECK(extension_mismatches >= 0),
    checkpoint_sha256 TEXT,
    checkpoint_sequence INTEGER NOT NULL DEFAULT 0 CHECK(checkpoint_sequence >= 0),
    started_at TEXT NOT NULL,
    finished_at TEXT,
    failure_code TEXT,
    failure_message TEXT,
    UNIQUE(baseline_run_id, configuration_fingerprint, started_at)
) STRICT;

CREATE TABLE format_observations (
    id TEXT PRIMARY KEY,
    format_run_id TEXT NOT NULL REFERENCES format_identification_runs(id) ON DELETE CASCADE,
    baseline_run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE RESTRICT,
    content_object_id TEXT NOT NULL REFERENCES content_objects(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN (
        'identified', 'unknown', 'ambiguous', 'empty',
        'skipped_unavailable', 'tool_error'
    )),
    source_mime_type TEXT NOT NULL,
    preferred_extension TEXT,
    staging_mode TEXT NOT NULL,
    primary_identifier TEXT,
    primary_format_name TEXT,
    primary_format_version TEXT,
    primary_mime_type TEXT,
    match_count INTEGER NOT NULL CHECK(match_count >= 0),
    extension_checked INTEGER NOT NULL CHECK(extension_checked IN (0, 1)),
    extension_mismatch INTEGER NOT NULL CHECK(extension_mismatch IN (0, 1)),
    error_code TEXT,
    error_message TEXT,
    observed_at TEXT NOT NULL,
    UNIQUE(format_run_id, content_object_id)
) STRICT;

CREATE TABLE format_matches (
    id TEXT PRIMARY KEY,
    observation_id TEXT NOT NULL REFERENCES format_observations(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    namespace TEXT NOT NULL,
    identifier TEXT NOT NULL,
    format_name TEXT NOT NULL,
    format_version TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    format_class TEXT,
    basis TEXT NOT NULL,
    warning TEXT NOT NULL,
    is_primary INTEGER NOT NULL CHECK(is_primary IN (0, 1)),
    UNIQUE(observation_id, ordinal)
) STRICT;

CREATE INDEX idx_format_runs_baseline_started
    ON format_identification_runs(baseline_run_id, started_at DESC);
CREATE UNIQUE INDEX idx_format_runs_one_active
    ON format_identification_runs(baseline_run_id)
    WHERE state='running';
CREATE INDEX idx_format_observations_run_state_sha
    ON format_observations(format_run_id, state, sha256);
CREATE INDEX idx_format_observations_content_run
    ON format_observations(content_object_id, format_run_id);
CREATE INDEX idx_format_matches_identifier
    ON format_matches(identifier, observation_id);
CREATE INDEX idx_content_objects_collection_format_state_sha
    ON content_objects(collection_id, format_state, sha256);
CREATE INDEX idx_content_objects_collection_puid_sha
    ON content_objects(collection_id, primary_puid, sha256)
    WHERE primary_puid IS NOT NULL;
CREATE INDEX idx_content_objects_collection_format_mismatch_sha
    ON content_objects(collection_id, extension_mismatch, sha256);
