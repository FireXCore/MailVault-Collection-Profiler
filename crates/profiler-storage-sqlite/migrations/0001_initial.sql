CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    adapter_kind TEXT NOT NULL,
    archive_identity TEXT NOT NULL,
    archive_root_display TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(adapter_kind, archive_identity)
) STRICT;

CREATE TABLE source_snapshots (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    source_schema_version INTEGER NOT NULL,
    source_database_display TEXT NOT NULL,
    snapshot_database_path TEXT NOT NULL,
    snapshot_sha256 TEXT NOT NULL,
    snapshot_bytes INTEGER NOT NULL CHECK(snapshot_bytes >= 0),
    source_metrics_json TEXT NOT NULL CHECK(json_valid(source_metrics_json)),
    snapshot_metrics_json TEXT NOT NULL CHECK(json_valid(snapshot_metrics_json)),
    created_at TEXT NOT NULL,
    UNIQUE(collection_id, snapshot_sha256)
) STRICT;

CREATE TABLE profiler_runs (
    id TEXT PRIMARY KEY,
    collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
    source_snapshot_id TEXT REFERENCES source_snapshots(id) ON DELETE SET NULL,
    state TEXT NOT NULL,
    pipeline_version TEXT NOT NULL,
    configuration_fingerprint TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    failure_code TEXT,
    failure_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE run_stages (
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    state TEXT NOT NULL,
    completed_items INTEGER NOT NULL DEFAULT 0 CHECK(completed_items >= 0),
    total_items INTEGER CHECK(total_items IS NULL OR total_items >= 0),
    completed_bytes INTEGER NOT NULL DEFAULT 0 CHECK(completed_bytes >= 0),
    total_bytes INTEGER CHECK(total_bytes IS NULL OR total_bytes >= 0),
    warnings INTEGER NOT NULL DEFAULT 0 CHECK(warnings >= 0),
    errors INTEGER NOT NULL DEFAULT 0 CHECK(errors >= 0),
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(run_id, stage)
) STRICT;

CREATE TABLE run_checkpoints (
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence >= 0),
    stable_cursor TEXT NOT NULL,
    tool_versions_json TEXT NOT NULL CHECK(json_valid(tool_versions_json)),
    configuration_fingerprint TEXT NOT NULL,
    committed_at TEXT NOT NULL,
    PRIMARY KEY(run_id, stage)
) STRICT;

CREATE TABLE source_messages (
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    source_message_id INTEGER NOT NULL,
    archive_id TEXT NOT NULL,
    provider_thread_namespace TEXT,
    provider_thread_value TEXT,
    rfc_message_id TEXT,
    subject_raw TEXT NOT NULL,
    header_date TEXT,
    raw_path TEXT,
    raw_sha256 TEXT,
    raw_size_bytes INTEGER,
    PRIMARY KEY(snapshot_id, source_message_id)
) STRICT;

CREATE TABLE source_parts (
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    source_part_id INTEGER NOT NULL,
    source_message_id INTEGER NOT NULL,
    part_path TEXT NOT NULL,
    parent_part_path TEXT,
    role TEXT NOT NULL,
    declared_mime_type TEXT NOT NULL,
    detected_mime_type TEXT,
    content_disposition TEXT,
    content_id TEXT,
    filename_original TEXT,
    filename_safe TEXT,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    sha256 TEXT,
    blob_path TEXT,
    defects_json TEXT NOT NULL CHECK(json_valid(defects_json)),
    PRIMARY KEY(snapshot_id, source_part_id),
    UNIQUE(snapshot_id, source_message_id, part_path),
    FOREIGN KEY(snapshot_id, source_message_id)
      REFERENCES source_messages(snapshot_id, source_message_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE content_objects (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL,
    expected_size_bytes INTEGER NOT NULL CHECK(expected_size_bytes >= 0),
    source_detected_mime_type TEXT NOT NULL,
    canonical_path_display TEXT NOT NULL,
    availability_state TEXT NOT NULL,
    integrity_state TEXT NOT NULL,
    security_state TEXT NOT NULL,
    first_seen_at TEXT,
    last_seen_at TEXT,
    occurrence_count INTEGER NOT NULL DEFAULT 0 CHECK(occurrence_count >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(collection_id, sha256)
) STRICT;

CREATE TABLE content_occurrences (
    id TEXT PRIMARY KEY,
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    content_object_id TEXT NOT NULL REFERENCES content_objects(id) ON DELETE CASCADE,
    source_message_id INTEGER NOT NULL,
    source_part_id INTEGER NOT NULL,
    part_path TEXT NOT NULL,
    filename_original TEXT,
    filename_normalized TEXT,
    role TEXT NOT NULL,
    message_date TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(snapshot_id, source_message_id, part_path)
) STRICT;

CREATE TABLE filename_variants (
    content_object_id TEXT NOT NULL REFERENCES content_objects(id) ON DELETE CASCADE,
    normalized_filename TEXT NOT NULL,
    display_filename TEXT NOT NULL,
    occurrence_count INTEGER NOT NULL CHECK(occurrence_count > 0),
    first_seen_at TEXT,
    last_seen_at TEXT,
    PRIMARY KEY(content_object_id, normalized_filename)
) STRICT;

CREATE TABLE format_assertions (
    id TEXT PRIMARY KEY,
    content_object_id TEXT NOT NULL REFERENCES content_objects(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    tool_version TEXT NOT NULL,
    signature_version TEXT NOT NULL,
    puid TEXT,
    format_name TEXT,
    format_version TEXT,
    mime_type TEXT,
    identification_basis TEXT,
    warning TEXT,
    asserted_at TEXT NOT NULL,
    UNIQUE(content_object_id, tool_name, tool_version, signature_version)
) STRICT;

CREATE TABLE processing_events (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE CASCADE,
    content_object_id TEXT REFERENCES content_objects(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    agent_version TEXT NOT NULL,
    outcome TEXT NOT NULL,
    detail_json TEXT NOT NULL CHECK(json_valid(detail_json)),
    occurred_at TEXT NOT NULL
) STRICT;

CREATE TABLE findings (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE CASCADE,
    content_object_id TEXT REFERENCES content_objects(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    severity TEXT NOT NULL,
    message TEXT NOT NULL,
    evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
    created_at TEXT NOT NULL,
    resolved_at TEXT
) STRICT;

CREATE INDEX idx_snapshots_collection_created
    ON source_snapshots(collection_id, created_at DESC);
CREATE INDEX idx_runs_collection_created
    ON profiler_runs(collection_id, created_at DESC);
CREATE INDEX idx_source_parts_sha
    ON source_parts(snapshot_id, sha256);
CREATE INDEX idx_source_parts_role
    ON source_parts(snapshot_id, role);
CREATE INDEX idx_content_objects_collection_size
    ON content_objects(collection_id, expected_size_bytes);
CREATE INDEX idx_occurrences_content
    ON content_occurrences(content_object_id);
CREATE INDEX idx_occurrences_message
    ON content_occurrences(snapshot_id, source_message_id);
CREATE INDEX idx_findings_code_severity
    ON findings(code, severity);
CREATE INDEX idx_events_content_type
    ON processing_events(content_object_id, event_type);
