ALTER TABLE source_messages ADD COLUMN account_id INTEGER;
ALTER TABLE source_messages ADD COLUMN subject_normalized TEXT NOT NULL DEFAULT '';
ALTER TABLE source_messages ADD COLUMN parse_defects_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(parse_defects_json));

ALTER TABLE source_parts ADD COLUMN charset TEXT;
ALTER TABLE source_parts ADD COLUMN transfer_encoding TEXT;
ALTER TABLE content_occurrences ADD COLUMN sender_domain TEXT;


CREATE TABLE source_blobs (
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    detected_mime_type TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_verified_at TEXT,
    PRIMARY KEY(snapshot_id, sha256)
) STRICT;

CREATE TABLE source_message_occurrences (
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    source_occurrence_id INTEGER NOT NULL,
    source_message_id INTEGER NOT NULL,
    generation_id INTEGER NOT NULL,
    uid INTEGER NOT NULL,
    labels_json TEXT NOT NULL CHECK(json_valid(labels_json)),
    internal_date TEXT,
    fetch_status TEXT NOT NULL,
    PRIMARY KEY(snapshot_id, source_occurrence_id),
    FOREIGN KEY(snapshot_id, source_message_id)
      REFERENCES source_messages(snapshot_id, source_message_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE source_participants (
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    source_participant_id INTEGER NOT NULL,
    source_message_id INTEGER NOT NULL,
    role TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    name TEXT NOT NULL,
    address TEXT NOT NULL,
    domain TEXT,
    PRIMARY KEY(snapshot_id, source_participant_id),
    UNIQUE(snapshot_id, source_message_id, role, ordinal),
    FOREIGN KEY(snapshot_id, source_message_id)
      REFERENCES source_messages(snapshot_id, source_message_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE source_message_relations (
    snapshot_id TEXT NOT NULL REFERENCES source_snapshots(id) ON DELETE CASCADE,
    source_relation_id INTEGER NOT NULL,
    source_message_id INTEGER NOT NULL,
    target_message_id INTEGER NOT NULL,
    relation_type TEXT NOT NULL,
    evidence_type TEXT NOT NULL,
    confidence REAL NOT NULL,
    source_created_at TEXT NOT NULL,
    PRIMARY KEY(snapshot_id, source_relation_id),
    UNIQUE(
        snapshot_id,
        source_message_id,
        target_message_id,
        relation_type,
        evidence_type
    ),
    FOREIGN KEY(snapshot_id, source_message_id)
      REFERENCES source_messages(snapshot_id, source_message_id) ON DELETE CASCADE,
    FOREIGN KEY(snapshot_id, target_message_id)
      REFERENCES source_messages(snapshot_id, source_message_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_source_blobs_sha
    ON source_blobs(sha256);
CREATE INDEX idx_source_occurrences_message
    ON source_message_occurrences(snapshot_id, source_message_id);
CREATE INDEX idx_source_participants_message
    ON source_participants(snapshot_id, source_message_id);
CREATE INDEX idx_source_participants_domain
    ON source_participants(snapshot_id, domain);
CREATE INDEX idx_source_relations_source
    ON source_message_relations(snapshot_id, source_message_id);
CREATE INDEX idx_source_relations_target
    ON source_message_relations(snapshot_id, target_message_id);
CREATE INDEX idx_filename_variants_normalized
    ON filename_variants(normalized_filename);
