use std::{fs, path::Path};

use profiler_adapter_mailvault::MailVaultAdapter;
use profiler_core::CollectionAdapter;
use rusqlite::Connection;
use tempfile::tempdir;

const MAILVAULT_SCHEMA: &str = r"
CREATE TABLE schema_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO schema_meta VALUES('schema_version', '3');
CREATE TABLE accounts(id INTEGER PRIMARY KEY, archive_id TEXT, email TEXT, host TEXT, port INTEGER, provider_kind TEXT);
CREATE TABLE messages(id INTEGER PRIMARY KEY, archive_id TEXT, account_id INTEGER, provider_thread_namespace TEXT, provider_thread_value TEXT, rfc_message_id TEXT, subject_raw TEXT, subject_normalized TEXT, header_date TEXT, raw_path TEXT, raw_sha256 TEXT, raw_size_bytes INTEGER, parse_defects_json TEXT);
CREATE TABLE message_occurrences(id INTEGER PRIMARY KEY, message_id INTEGER, generation_id INTEGER, uid INTEGER, labels_json TEXT, internal_date TEXT, fetch_status TEXT);
CREATE TABLE message_participants(id INTEGER PRIMARY KEY, message_id INTEGER, role TEXT, ordinal INTEGER, name TEXT, address TEXT, domain TEXT);
CREATE TABLE blobs(sha256 TEXT PRIMARY KEY, size_bytes INTEGER, detected_mime_type TEXT, storage_path TEXT, first_seen_at TEXT, last_verified_at TEXT);
CREATE TABLE message_parts(id INTEGER PRIMARY KEY, message_id INTEGER, part_path TEXT, parent_part_path TEXT, role TEXT, declared_mime_type TEXT, detected_mime_type TEXT, content_disposition TEXT, content_id TEXT, filename_original TEXT, filename_safe TEXT, charset TEXT, transfer_encoding TEXT, size_bytes INTEGER, sha256 TEXT, blob_path TEXT, defects_json TEXT);
CREATE TABLE message_relations(id INTEGER PRIMARY KEY, source_message_id INTEGER, target_message_id INTEGER, relation_type TEXT, evidence_type TEXT, confidence REAL, created_at TEXT);
CREATE INDEX idx_messages_thread ON messages(account_id, provider_thread_namespace, provider_thread_value);
CREATE INDEX idx_occurrence_message ON message_occurrences(message_id);
CREATE INDEX idx_participants_domain ON message_participants(domain);
CREATE INDEX idx_parts_sha ON message_parts(sha256);
CREATE INDEX idx_parts_role ON message_parts(role);
INSERT INTO accounts(id, archive_id, email, host, port, provider_kind) VALUES(1, 'fixture-account', 'fixture@example.test', 'imap.example.test', 993, 'gmail');
";

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("database")).unwrap();
    fs::create_dir_all(root.join("objects/raw/sha256")).unwrap();
    fs::create_dir_all(root.join("objects/blobs/sha256")).unwrap();
    fs::create_dir_all(root.join("state")).unwrap();
    let connection = Connection::open(root.join("database/mailvault.sqlite3")).unwrap();
    connection.execute_batch(MAILVAULT_SCHEMA).unwrap();
}

#[test]
fn canonical_schema_three_passes_preflight() {
    let directory = tempdir().unwrap();
    fixture(directory.path());
    let report = MailVaultAdapter.preflight(directory.path()).unwrap();
    assert!(report.compatible, "{:#?}", report.checks);
    assert_eq!(report.schema_version, Some(3));
}

#[test]
fn newer_schema_fails_closed() {
    let directory = tempdir().unwrap();
    fixture(directory.path());
    let connection = Connection::open(directory.path().join("database/mailvault.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE schema_meta SET value='4' WHERE key='schema_version'",
            [],
        )
        .unwrap();
    let report = MailVaultAdapter.preflight(directory.path()).unwrap();
    assert!(!report.compatible);
}
