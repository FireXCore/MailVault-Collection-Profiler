CREATE TABLE workspace_meta (
    singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
    workspace_id TEXT NOT NULL UNIQUE,
    schema_version INTEGER NOT NULL CHECK(schema_version > 0),
    created_at TEXT NOT NULL,
    created_by_version TEXT NOT NULL,
    last_migrated_at TEXT,
    last_migrated_by_version TEXT,
    migration_state TEXT NOT NULL DEFAULT 'ready'
        CHECK(migration_state IN ('ready', 'migrating', 'failed'))
) STRICT;

-- The composite key prevents a review event from pairing a run with a finding
-- that belongs to another run, even if raw SQL bypasses the application layer.
CREATE UNIQUE INDEX idx_findings_id_run
    ON findings(id, run_id);

CREATE TABLE finding_review_events (
    event_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE RESTRICT,
    finding_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    action TEXT NOT NULL CHECK(action IN ('status_set', 'status_cleared', 'note_added')),
    previous_status TEXT CHECK(
        previous_status IS NULL OR previous_status IN (
            'acknowledged', 'expected', 'needs_investigation', 'resolved_externally'
        )
    ),
    new_status TEXT CHECK(
        new_status IS NULL OR new_status IN (
            'acknowledged', 'expected', 'needs_investigation', 'resolved_externally'
        )
    ),
    note TEXT,
    actor_kind TEXT NOT NULL CHECK(actor_kind IN ('local_interactive_user', 'local_cli_user')),
    actor_label TEXT,
    occurred_at TEXT NOT NULL,
    previous_event_hash TEXT,
    event_hash TEXT NOT NULL UNIQUE,
    UNIQUE(run_id, finding_id, sequence),
    FOREIGN KEY(finding_id, run_id)
        REFERENCES findings(id, run_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE finding_review_state (
    run_id TEXT NOT NULL REFERENCES profiler_runs(id) ON DELETE RESTRICT,
    finding_id TEXT NOT NULL,
    current_status TEXT CHECK(
        current_status IS NULL OR current_status IN (
            'acknowledged', 'expected', 'needs_investigation', 'resolved_externally'
        )
    ),
    latest_note TEXT,
    last_event_id TEXT NOT NULL REFERENCES finding_review_events(event_id) ON DELETE RESTRICT,
    last_sequence INTEGER NOT NULL CHECK(last_sequence > 0),
    reviewed_at TEXT NOT NULL,
    PRIMARY KEY(run_id, finding_id),
    FOREIGN KEY(finding_id, run_id)
        REFERENCES findings(id, run_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX idx_review_events_run_finding
    ON finding_review_events(run_id, finding_id, sequence);

CREATE INDEX idx_review_state_run_status
    ON finding_review_state(run_id, current_status, finding_id);

CREATE INDEX idx_runs_started_at
    ON profiler_runs(started_at DESC, id);

CREATE TRIGGER finding_review_events_no_update
BEFORE UPDATE ON finding_review_events
BEGIN
    SELECT RAISE(ABORT, 'finding review events are append-only');
END;

CREATE TRIGGER finding_review_events_no_delete
BEFORE DELETE ON finding_review_events
BEGIN
    SELECT RAISE(ABORT, 'finding review events are append-only');
END;
