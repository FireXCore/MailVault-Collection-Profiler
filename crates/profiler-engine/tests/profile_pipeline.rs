use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use profiler_core::{
    AvailabilityState, FindingCategory, FindingsPageRequest, InventoryFilters,
    InventoryPageRequest, ProfilerResult, ProgressEvent, ProgressSink, ReviewActorKind,
    ReviewStatus, RunStage, StageState, WorkspaceAccessMode, WorkspaceOpenMode,
};
use profiler_engine::{
    ProfileEngine, ProfileOptions, ProfileRequest, ProfileResult as EngineProfileResult,
    workspace::{
        WorkspaceSession, add_review_note, export_sanitized_run, findings_page, list_runs,
        open_run, set_review_status,
    },
};
use profiler_storage_sqlite::ProfilerStore;
use rusqlite::{Connection, params};
use tempfile::TempDir;

const HASH_AVAILABLE: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HASH_MISMATCH: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const HASH_MISSING: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const HASH_EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Debug, Default)]
struct RecordingProgress {
    events: Mutex<Vec<ProgressEvent>>,
}

impl ProgressSink for RecordingProgress {
    fn send(&self, event: ProgressEvent) -> ProfilerResult<()> {
        self.events
            .lock()
            .expect("progress mutex poisoned")
            .push(event);
        Ok(())
    }
}

#[test]
fn profiles_physical_inventory_end_to_end_without_mutating_mailvault() {
    let fixture = Fixture::create();
    let source_database = fixture.archive.join("database/mailvault.sqlite3");
    let source_database_before = fs::read(&source_database).expect("read source database");
    let progress = RecordingProgress::default();

    let result = run_profile(&fixture, &progress);
    assert_profile_summary(&result);
    assert_persisted_results(&result);
    assert_explorer_queries(&result);
    assert_source_unchanged(&source_database, &source_database_before);
    assert_progress_contract(&progress);
}

#[test]
fn reopens_workspace_and_persists_append_only_finding_reviews() {
    let fixture = Fixture::create();
    let source_database = fixture.archive.join("database/mailvault.sqlite3");
    let source_before = fs::read(&source_database).expect("read source before reopen test");
    let result = run_profile(&fixture, &RecordingProgress::default());

    let session = WorkspaceSession::open(
        &fixture.workspace,
        WorkspaceOpenMode::ReadWritePreferred,
        true,
    )
    .expect("open completed workspace");
    let context = session.context();
    let runs = list_runs(&context).expect("list persisted runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, result.run_id);
    let reopened = open_run(&context, &result.run_id).expect("reopen completed run");
    assert_eq!(reopened.inventory.content_objects, 4);
    assert_eq!(reopened.findings.total, 5);

    let informational = findings_page(
        &context,
        &FindingsPageRequest {
            run_id: result.run_id.clone(),
            code: None,
            severity: None,
            review_status: None,
            category: Some(FindingCategory::InformationalEvidence),
            search: None,
            after_id: None,
            limit: 10,
        },
    )
    .expect("load informational findings");
    let finding_id = informational.items[0].id.clone();
    let history = set_review_status(
        &context,
        &result.run_id,
        &finding_id,
        ReviewStatus::Expected,
        Some("Known exact-content filename relationship."),
        ReviewActorKind::LocalInteractiveUser,
    )
    .expect("persist review decision");
    assert_eq!(history.current_status, Some(ReviewStatus::Expected));
    add_review_note(
        &context,
        &result.run_id,
        &finding_id,
        "Reviewed against the local archive baseline.",
        ReviewActorKind::LocalInteractiveUser,
    )
    .expect("append review note");
    drop(session);

    let second_session = WorkspaceSession::open(
        &fixture.workspace,
        WorkspaceOpenMode::ReadWritePreferred,
        false,
    )
    .expect("reopen workspace after full session drop");
    let second_context = second_session.context();
    let detail =
        profiler_engine::workspace::finding_detail(&second_context, &result.run_id, &finding_id)
            .expect("load persisted review history");
    assert_eq!(detail.review.current_status, Some(ReviewStatus::Expected));
    assert_eq!(detail.review.events.len(), 2);
    assert!(detail.review.integrity_valid);
    assert_eq!(
        fs::read(&source_database).expect("read source after reopen test"),
        source_before
    );
}

#[test]
fn second_writer_falls_back_to_read_only_and_cannot_change_reviews() {
    let fixture = Fixture::create();
    let result = run_profile(&fixture, &RecordingProgress::default());
    let writer = WorkspaceSession::open(
        &fixture.workspace,
        WorkspaceOpenMode::ReadWritePreferred,
        true,
    )
    .expect("open first review writer");
    assert_eq!(
        writer.descriptor().access_mode,
        WorkspaceAccessMode::ReadWrite
    );

    let reader = WorkspaceSession::open(
        &fixture.workspace,
        WorkspaceOpenMode::ReadWritePreferred,
        false,
    )
    .expect("open second session read-only");
    assert_eq!(
        reader.descriptor().access_mode,
        WorkspaceAccessMode::ReadOnlyLocked
    );

    let page = findings_page(
        &reader.context(),
        &FindingsPageRequest {
            run_id: result.run_id.clone(),
            code: None,
            severity: Some("warning".into()),
            review_status: None,
            category: Some(FindingCategory::RequiresAttention),
            search: None,
            after_id: None,
            limit: 10,
        },
    )
    .expect("browse locked workspace");
    let finding_id = page.items[0].id.clone();
    let error = set_review_status(
        &reader.context(),
        &result.run_id,
        &finding_id,
        ReviewStatus::Acknowledged,
        None,
        ReviewActorKind::LocalInteractiveUser,
    )
    .expect_err("locked workspace must reject review writes");
    assert_eq!(
        error.report().code,
        profiler_core::ErrorCode::ReviewWriteNotAllowed
    );
}

#[test]
fn sanitized_exports_contain_no_source_paths_filenames_or_notes() {
    let fixture = Fixture::create();
    let result = run_profile(&fixture, &RecordingProgress::default());
    let session = WorkspaceSession::open(
        &fixture.workspace,
        WorkspaceOpenMode::ReadWritePreferred,
        true,
    )
    .expect("open workspace for sanitized export");
    let context = session.context();
    let page = findings_page(
        &context,
        &FindingsPageRequest {
            run_id: result.run_id.clone(),
            code: None,
            severity: None,
            review_status: None,
            category: Some(FindingCategory::InformationalEvidence),
            search: None,
            after_id: None,
            limit: 10,
        },
    )
    .expect("load finding for export review");
    set_review_status(
        &context,
        &result.run_id,
        &page.items[0].id,
        ReviewStatus::Expected,
        Some("private review note that must not be exported"),
        ReviewActorKind::LocalInteractiveUser,
    )
    .expect("persist export fixture review");

    let json_path = fixture.temp.path().join("sanitized-summary.json");
    export_sanitized_run(&context, &result.run_id, &json_path)
        .expect("write sanitized JSON export");
    let json = fs::read_to_string(&json_path).expect("read sanitized JSON export");
    let source_path = fixture.archive.to_string_lossy().into_owned();
    for forbidden in [
        source_path.as_str(),
        "offer.pdf",
        "final-offer.pdf",
        "same.pdf",
        "private review note",
        "supplier1@supplier.example",
    ] {
        assert!(
            !json.contains(forbidden),
            "sanitized JSON leaked {forbidden}"
        );
    }

    let csv_path = fixture.temp.path().join("sanitized-findings.csv");
    export_sanitized_run(&context, &result.run_id, &csv_path).expect("write sanitized CSV export");
    let csv = fs::read_to_string(&csv_path).expect("read sanitized CSV export");
    assert!(csv.starts_with("finding_token,object_token,code,severity,review_status,reviewed_at"));
    assert!(!csv.contains("offer.pdf"));
    assert!(!csv.contains("private review note"));
}

fn run_profile(fixture: &Fixture, progress: &RecordingProgress) -> EngineProfileResult {
    let mut options = ProfileOptions::default();
    options.inventory.batch_size = 2;
    options.file_stat.batch_size = 2;
    options.file_stat.workers = 2;

    ProfileEngine
        .profile(
            &ProfileRequest {
                archive_root: fixture.archive.clone(),
                workspace_root: fixture.workspace.clone(),
                options,
            },
            progress,
        )
        .expect("profile succeeds with non-fatal physical findings")
}

fn assert_profile_summary(result: &EngineProfileResult) {
    assert_eq!(result.preflight.metrics.messages, 2);
    assert_eq!(result.preflight.metrics.message_occurrences, 2);
    assert_eq!(result.preflight.metrics.mime_parts, 6);
    assert_eq!(result.preflight.metrics.attachment_occurrences, 5);
    assert_eq!(result.preflight.metrics.blobs, 4);
    assert_eq!(result.preflight.metrics.message_relations, 1);
    assert_eq!(result.preflight.metrics.participants, 2);

    let inventory = &result.inventory.summary;
    assert_eq!(inventory.content_objects, 4);
    assert_eq!(inventory.content_occurrences, 5);
    assert_eq!(inventory.zero_byte_content_objects, 1);
    assert_eq!(inventory.same_hash_different_names, 1);
    assert_eq!(inventory.same_name_different_hashes, 1);

    let file_stat = &result.file_stat.summary;
    assert_eq!(file_stat.total_objects, 4);
    assert_eq!(file_stat.available_objects, 3);
    assert_eq!(file_stat.missing_objects, 1);
    assert_eq!(file_stat.unreadable_objects, 0);
    assert_eq!(file_stat.invalid_locator_objects, 0);
    assert_eq!(file_stat.non_regular_objects, 0);
    assert_eq!(file_stat.unsafe_reparse_objects, 0);
    assert_eq!(file_stat.io_error_objects, 0);
    assert_eq!(file_stat.size_matches, 2);
    assert_eq!(file_stat.size_mismatches, 1);
    assert_eq!(file_stat.expected_bytes, 12);
    assert_eq!(file_stat.available_bytes, 8);
}

fn assert_persisted_results(result: &EngineProfileResult) {
    let profiler_database = Connection::open(&result.profiler_database).expect("open profiler DB");
    let findings: i64 = profiler_database
        .query_row(
            "SELECT COUNT(*) FROM findings WHERE run_id=?1",
            [result.run_id.as_str()],
            |row| row.get(0),
        )
        .expect("count findings");
    assert_eq!(findings, 5);

    let observation_count: i64 = profiler_database
        .query_row(
            "SELECT COUNT(*) FROM file_stat_observations WHERE run_id=?1",
            [result.run_id.as_str()],
            |row| row.get(0),
        )
        .expect("count physical observations");
    assert_eq!(observation_count, 4);
}

fn assert_explorer_queries(result: &EngineProfileResult) {
    let explorer = ProfilerStore::open_read_only(Path::new(&result.profiler_database))
        .expect("open explorer store");
    assert_inventory_pagination(&explorer, result);
    assert_inventory_filters(&explorer, result);
    assert_content_detail(&explorer, result);
    assert_findings_query(&explorer, result);
}

fn inventory_request(
    result: &EngineProfileResult,
    filters: InventoryFilters,
    limit: u32,
) -> InventoryPageRequest {
    InventoryPageRequest {
        collection_id: result.collection_id.clone(),
        run_id: result.run_id.clone(),
        filters,
        after_sha256: None,
        limit,
    }
}

fn assert_inventory_pagination(explorer: &ProfilerStore, result: &EngineProfileResult) {
    let first_page = explorer
        .inventory_page(&inventory_request(result, InventoryFilters::default(), 2))
        .expect("load first inventory page");
    assert_eq!(first_page.total_filtered, 4);
    assert_eq!(first_page.items.len(), 2);
    assert!(first_page.has_more);
    let cursor = first_page
        .next_after_sha256
        .clone()
        .expect("first page cursor");

    let mut second_request = inventory_request(result, InventoryFilters::default(), 2);
    second_request.after_sha256 = Some(cursor.to_ascii_uppercase());
    let second_page = explorer
        .inventory_page(&second_request)
        .expect("load second inventory page from normalized cursor");
    assert_eq!(second_page.total_filtered, 4);
    assert_eq!(second_page.items.len(), 2);
    assert!(!second_page.has_more);

    let first_hashes = first_page
        .items
        .iter()
        .map(|item| item.sha256.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let second_hashes = second_page
        .items
        .iter()
        .map(|item| item.sha256.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(first_hashes.is_disjoint(&second_hashes));
}

fn assert_inventory_filters(explorer: &ProfilerStore, result: &EngineProfileResult) {
    let missing_page = explorer
        .inventory_page(&inventory_request(
            result,
            InventoryFilters {
                availability_state: Some(AvailabilityState::Missing),
                ..InventoryFilters::default()
            },
            10,
        ))
        .expect("filter missing physical objects");
    assert_eq!(missing_page.total_filtered, 1);
    assert_eq!(missing_page.items[0].sha256, HASH_MISSING);
}

fn assert_content_detail(explorer: &ProfilerStore, result: &EngineProfileResult) {
    let search_page = explorer
        .inventory_page(&inventory_request(
            result,
            InventoryFilters {
                search: Some("final-offer".into()),
                ..InventoryFilters::default()
            },
            10,
        ))
        .expect("search filename variants");
    assert_eq!(search_page.total_filtered, 1);
    assert_eq!(search_page.items[0].sha256, HASH_AVAILABLE);

    let subject_page = explorer
        .inventory_page(&inventory_request(
            result,
            InventoryFilters {
                search: Some("revised offer".into()),
                ..InventoryFilters::default()
            },
            10,
        ))
        .expect("search message subject");
    assert_eq!(subject_page.total_filtered, 3);

    let detail = explorer
        .content_object_detail(
            &result.run_id,
            &result.collection_id,
            &search_page.items[0].id,
        )
        .expect("load content object detail");
    assert!(
        explorer
            .content_object_detail(
                &result.run_id,
                "another-collection",
                &search_page.items[0].id,
            )
            .is_err(),
        "content detail must remain collection-scoped",
    );
    assert_eq!(detail.filename_variants.len(), 2);
    assert_eq!(detail.occurrence_total, 2);
    assert_eq!(detail.occurrences.len(), 2);
    assert_eq!(detail.findings.len(), 1);
    assert_eq!(detail.findings[0].code, "SAME_HASH_DIFFERENT_NAMES");
}

fn assert_findings_query(explorer: &ProfilerStore, result: &EngineProfileResult) {
    let findings_page = explorer
        .findings_page(&FindingsPageRequest {
            run_id: result.run_id.clone(),
            code: Some("blob_size_mismatch".into()),
            severity: Some("ERROR".into()),
            review_status: None,
            category: None,
            search: None,
            after_id: None,
            limit: 10,
        })
        .expect("load findings page");
    assert_eq!(findings_page.items.len(), 1);
    assert_eq!(findings_page.summary.total, 5);
    assert_eq!(findings_page.summary.missing, 1);
    assert_eq!(findings_page.summary.size_mismatch, 1);

    assert!(
        explorer
            .findings_page(&FindingsPageRequest {
                run_id: result.run_id.clone(),
                code: None,
                severity: Some("critical".into()),
                review_status: None,
                category: None,
                search: None,
                after_id: None,
                limit: 10,
            })
            .is_err(),
        "unsupported severities must fail closed",
    );
}

fn assert_source_unchanged(source_database: &Path, source_database_before: &[u8]) {
    let source_database_after = fs::read(source_database).expect("re-read source database");
    assert_eq!(source_database_after, source_database_before);
}

fn assert_progress_contract(progress: &RecordingProgress) {
    let events = progress.events.lock().expect("progress mutex poisoned");
    assert!(events.iter().any(|event| {
        event.stage == RunStage::SourceSnapshot && event.stage_state == StageState::Completed
    }));
    assert!(events.iter().any(|event| {
        event.stage == RunStage::MetadataInventory && event.stage_state == StageState::Completed
    }));
    let completed_file_stat = events
        .iter()
        .find(|event| {
            event.stage == RunStage::FileStat && event.stage_state == StageState::Completed
        })
        .expect("completed file-stat event");
    assert_eq!(completed_file_stat.completed_items, 4);
    assert_eq!(completed_file_stat.total_items, Some(4));
    assert_eq!(completed_file_stat.completed_bytes, 12);
    assert_eq!(completed_file_stat.total_bytes, Some(12));
}

#[derive(Debug)]
struct Fixture {
    temp: TempDir,
    archive: PathBuf,
    workspace: PathBuf,
}

impl Fixture {
    fn create() -> Self {
        let temp = tempfile::tempdir().expect("create fixture root");
        let archive = temp.path().join("archive");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(archive.join("database")).expect("create database directory");
        fs::create_dir_all(archive.join("objects/raw/sha256")).expect("create raw store");
        fs::create_dir_all(archive.join("objects/blobs/sha256")).expect("create blob store");
        fs::create_dir_all(archive.join("state")).expect("create state directory");
        fs::create_dir_all(&workspace).expect("create workspace");

        let connection = Connection::open(archive.join("database/mailvault.sqlite3"))
            .expect("create MailVault database");
        connection
            .execute_batch(SCHEMA)
            .expect("create source schema");
        seed_database(&connection);
        drop(connection);

        write_blob(&archive, HASH_AVAILABLE, b"ABCD");
        write_blob(&archive, HASH_MISMATCH, b"WXYZ");
        write_blob(&archive, HASH_EMPTY, b"");

        Self {
            temp,
            archive,
            workspace,
        }
    }
}

fn canonical_locator(hash: &str) -> String {
    format!(
        "objects/blobs/sha256/{}/{}/{}",
        &hash[..2],
        &hash[2..4],
        hash
    )
}

fn write_blob(archive: &Path, hash: &str, bytes: &[u8]) {
    let path = archive.join(canonical_locator(hash));
    fs::create_dir_all(path.parent().expect("blob parent")).expect("create blob fanout");
    fs::write(path, bytes).expect("write blob fixture");
}

#[allow(clippy::too_many_lines)]
fn seed_database(connection: &Connection) {
    let now = "2026-07-18T12:00:00Z";
    connection
        .execute(
            "INSERT INTO schema_meta(key, value) VALUES('schema_version', '3')",
            [],
        )
        .expect("insert schema version");
    connection
        .execute(
            "INSERT INTO accounts(id, archive_id, email, host, port, provider_kind, tls_mode, created_at, updated_at) \
             VALUES(1, 'fixture-account', 'archive@example.test', 'imap.example.test', 993, 'gmail', 'implicit', ?1, ?1)",
            [now],
        )
        .expect("insert account");
    connection
        .execute(
            "INSERT INTO mailboxes(id, account_id, name, flags_json, created_at, updated_at) \
             VALUES(1, 1, 'INBOX', '[]', ?1, ?1)",
            [now],
        )
        .expect("insert mailbox");
    connection
        .execute(
            "INSERT INTO mailbox_generations(id, mailbox_id, uidvalidity, highest_uid, created_at, updated_at) \
             VALUES(1, 1, 1, 2, ?1, ?1)",
            [now],
        )
        .expect("insert generation");

    for (id, archive_id, thread, subject) in [
        (1_i64, "message-one", "thread-a", "First offer"),
        (2_i64, "message-two", "thread-a", "Revised offer"),
    ] {
        connection
            .execute(
                "INSERT INTO messages(\
                    id, archive_id, account_id, provider_thread_namespace, provider_thread_value, \
                    rfc_message_id, subject_raw, subject_normalized, header_date, raw_path, raw_sha256, \
                    raw_size_bytes, parse_defects_json, created_at, updated_at\
                 ) VALUES(?1, ?2, 1, 'gmail_thread_id', ?3, ?4, ?5, ?5, ?6, ?7, ?8, 100, '[]', ?6, ?6)",
                params![
                    id,
                    archive_id,
                    thread,
                    format!("<{archive_id}@example.test>"),
                    subject,
                    now,
                    format!("objects/raw/sha256/{archive_id}"),
                    format!("raw-{archive_id}"),
                ],
            )
            .expect("insert message");
        connection
            .execute(
                "INSERT INTO message_occurrences(\
                    id, message_id, generation_id, uid, labels_json, internal_date, fetch_status, created_at, updated_at\
                 ) VALUES(?1, ?1, 1, ?1, '[\"INBOX\"]', ?2, 'archived', ?2, ?2)",
                params![id, now],
            )
            .expect("insert occurrence");
        connection
            .execute(
                "INSERT INTO message_participants(id, message_id, role, ordinal, name, address, domain) \
                 VALUES(?1, ?1, 'from', 0, 'Supplier', ?2, 'supplier.example')",
                params![id, format!("supplier{id}@supplier.example")],
            )
            .expect("insert participant");
    }

    for (hash, expected_size, mime) in [
        (HASH_AVAILABLE, 4_i64, "application/pdf"),
        (HASH_MISMATCH, 5_i64, "application/pdf"),
        (HASH_MISSING, 3_i64, "application/pdf"),
        (HASH_EMPTY, 0_i64, "application/octet-stream"),
    ] {
        connection
            .execute(
                "INSERT INTO blobs(sha256, size_bytes, detected_mime_type, storage_path, first_seen_at, last_verified_at) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?5)",
                params![hash, expected_size, mime, canonical_locator(hash), now],
            )
            .expect("insert blob");
    }

    let parts = [
        (
            1_i64,
            1_i64,
            "1",
            "attachment",
            "offer.pdf",
            HASH_AVAILABLE,
            4_i64,
        ),
        (
            2_i64,
            2_i64,
            "1",
            "attachment",
            "final-offer.pdf",
            HASH_AVAILABLE,
            4_i64,
        ),
        (
            3_i64,
            1_i64,
            "2",
            "attachment",
            "same.pdf",
            HASH_MISMATCH,
            5_i64,
        ),
        (
            4_i64,
            2_i64,
            "2",
            "attachment",
            "same.pdf",
            HASH_MISSING,
            3_i64,
        ),
        (
            5_i64,
            2_i64,
            "3",
            "attachment",
            "empty.dat",
            HASH_EMPTY,
            0_i64,
        ),
    ];
    for (id, message_id, part_path, role, filename, hash, size) in parts {
        connection
            .execute(
                "INSERT INTO message_parts(\
                    id, message_id, part_path, role, declared_mime_type, detected_mime_type, \
                    content_disposition, filename_original, filename_safe, transfer_encoding, size_bytes, \
                    sha256, blob_path, defects_json\
                 ) VALUES(?1, ?2, ?3, ?4, 'application/octet-stream', 'application/pdf', \
                          'attachment', ?5, ?5, 'base64', ?6, ?7, ?8, '[]')",
                params![id, message_id, part_path, role, filename, size, hash, canonical_locator(hash)],
            )
            .expect("insert attachment part");
    }
    connection
        .execute(
            "INSERT INTO message_parts(\
                id, message_id, part_path, role, declared_mime_type, detected_mime_type, charset, \
                transfer_encoding, size_bytes, defects_json\
             ) VALUES(6, 1, '0', 'body_plain', 'text/plain', 'text/plain', 'utf-8', '7bit', 12, '[]')",
            [],
        )
        .expect("insert body part");
    connection
        .execute(
            "INSERT INTO message_relations(\
                id, source_message_id, target_message_id, relation_type, evidence_type, confidence, created_at\
             ) VALUES(1, 2, 1, 'reply_to', 'rfc_headers', 1.0, ?1)",
            [now],
        )
        .expect("insert relation");
}

const SCHEMA: &str = r"
PRAGMA foreign_keys=ON;
CREATE TABLE schema_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE accounts(
    id INTEGER PRIMARY KEY, archive_id TEXT NOT NULL UNIQUE, email TEXT NOT NULL, host TEXT NOT NULL,
    port INTEGER NOT NULL, provider_kind TEXT NOT NULL, tls_mode TEXT NOT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE mailboxes(
    id INTEGER PRIMARY KEY, account_id INTEGER NOT NULL REFERENCES accounts(id), name TEXT NOT NULL,
    delimiter TEXT, flags_json TEXT NOT NULL DEFAULT '[]', mailbox_object_id TEXT,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE mailbox_generations(
    id INTEGER PRIMARY KEY, mailbox_id INTEGER NOT NULL REFERENCES mailboxes(id), uidvalidity INTEGER NOT NULL,
    highest_uid INTEGER NOT NULL DEFAULT 0, highest_modseq INTEGER, last_scan_at TEXT,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE messages(
    id INTEGER PRIMARY KEY, archive_id TEXT NOT NULL UNIQUE, account_id INTEGER NOT NULL REFERENCES accounts(id),
    provider_thread_namespace TEXT, provider_thread_value TEXT, rfc_message_id TEXT, in_reply_to TEXT,
    references_json TEXT NOT NULL DEFAULT '[]', subject_raw TEXT NOT NULL DEFAULT '',
    subject_normalized TEXT NOT NULL DEFAULT '', return_path TEXT, delivered_to_json TEXT NOT NULL DEFAULT '[]',
    x_original_to_json TEXT NOT NULL DEFAULT '[]', list_id TEXT, header_date TEXT, content_type_header TEXT,
    authentication_results_json TEXT NOT NULL DEFAULT '[]', received_headers_json TEXT NOT NULL DEFAULT '[]',
    headers_json TEXT NOT NULL DEFAULT '{}', raw_path TEXT, raw_sha256 TEXT, raw_size_bytes INTEGER,
    raw_archived_at TEXT, mime_parsed_at TEXT, parse_defects_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_messages_thread ON messages(account_id, provider_thread_namespace, provider_thread_value);
CREATE TABLE message_occurrences(
    id INTEGER PRIMARY KEY, message_id INTEGER NOT NULL REFERENCES messages(id),
    generation_id INTEGER NOT NULL REFERENCES mailbox_generations(id), uid INTEGER NOT NULL,
    flags_json TEXT NOT NULL DEFAULT '[]', labels_json TEXT NOT NULL DEFAULT '[]', internal_date TEXT,
    rfc822_size INTEGER NOT NULL DEFAULT 0, modseq INTEGER, selected_for_raw INTEGER NOT NULL DEFAULT 1,
    fetch_status TEXT NOT NULL DEFAULT 'metadata', last_error TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_occurrence_message ON message_occurrences(message_id);
CREATE TABLE message_participants(
    id INTEGER PRIMARY KEY, message_id INTEGER NOT NULL REFERENCES messages(id), role TEXT NOT NULL,
    ordinal INTEGER NOT NULL, name TEXT NOT NULL DEFAULT '', address TEXT NOT NULL, domain TEXT
);
CREATE INDEX idx_participants_domain ON message_participants(domain);
CREATE TABLE blobs(
    sha256 TEXT PRIMARY KEY, size_bytes INTEGER NOT NULL, detected_mime_type TEXT NOT NULL,
    storage_path TEXT NOT NULL, first_seen_at TEXT NOT NULL, last_verified_at TEXT
);
CREATE TABLE message_parts(
    id INTEGER PRIMARY KEY, message_id INTEGER NOT NULL REFERENCES messages(id), part_path TEXT NOT NULL,
    parent_part_path TEXT, role TEXT NOT NULL, declared_mime_type TEXT NOT NULL, detected_mime_type TEXT,
    content_disposition TEXT, content_id TEXT, filename_original TEXT, filename_safe TEXT, charset TEXT,
    transfer_encoding TEXT, size_bytes INTEGER NOT NULL DEFAULT 0, sha256 TEXT REFERENCES blobs(sha256),
    blob_path TEXT, headers_json TEXT NOT NULL DEFAULT '{}', defects_json TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX idx_parts_sha ON message_parts(sha256);
CREATE INDEX idx_parts_role ON message_parts(role);
CREATE TABLE message_relations(
    id INTEGER PRIMARY KEY, source_message_id INTEGER NOT NULL REFERENCES messages(id),
    target_message_id INTEGER NOT NULL REFERENCES messages(id), relation_type TEXT NOT NULL,
    evidence_type TEXT NOT NULL, confidence REAL NOT NULL, created_at TEXT NOT NULL
);
";
