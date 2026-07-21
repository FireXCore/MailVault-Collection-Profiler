mod explorer;
mod review;
mod workspace;

pub use workspace::{current_workspace_schema, expected_application_id};

pub fn migration_failure_marker_path(database_path: &Path) -> PathBuf {
    database_path.with_extension("sqlite3.migration-failed")
}

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use profiler_core::{
    AvailabilityState, FileStatCheckpoint, FileStatObservation, FileStatRequest, FileStatStore,
    FileStatSummary, FileStatWorkItem, InventoryBatch, InventoryCheckpoint, InventoryRequest,
    InventorySink, InventorySummary, InventoryTable, ProfilerError, ProfilerResult, RunState,
    SizeState, SourceSnapshotManifest, validate_transition,
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
    backup::{Backup, StepResult},
    params,
};
use time::OffsetDateTime;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

const APPLICATION_ID: i64 = 0x4D56_5046; // MVPF
pub const CURRENT_USER_VERSION: i64 = 5;

#[derive(Debug, Clone, Copy)]
struct Migration {
    id: &'static str,
    user_version: i64,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        id: "0001_initial",
        user_version: 1,
        sql: include_str!("../migrations/0001_initial.sql"),
    },
    Migration {
        id: "0002_inventory_contract",
        user_version: 2,
        sql: include_str!("../migrations/0002_inventory_contract.sql"),
    },
    Migration {
        id: "0003_file_stat",
        user_version: 3,
        sql: include_str!("../migrations/0003_file_stat.sql"),
    },
    Migration {
        id: "0004_explorer_indexes",
        user_version: 4,
        sql: include_str!("../migrations/0004_explorer_indexes.sql"),
    },
    Migration {
        id: "0005_workspace_reopen_reviews",
        user_version: 5,
        sql: include_str!("../migrations/0005_workspace_reopen_reviews.sql"),
    },
];

#[derive(Debug)]
pub struct ProfilerStore {
    connection: Connection,
}

impl ProfilerStore {
    pub fn open_existing(path: &Path) -> ProfilerResult<Self> {
        if !path.is_file() {
            return Err(ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceDatabaseMissing,
                "profiler database does not exist",
                false,
            ));
        }
        Self::open_internal(path, false)
    }

    pub fn open_read_only(path: &Path) -> ProfilerResult<Self> {
        if !path.is_file() {
            return Err(ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceDatabaseMissing,
                "profiler database does not exist",
                false,
            ));
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|source| sqlite_error("opening profiler database read-only", source))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|source| sqlite_error("configuring profiler read timeout", source))?;
        connection
            .execute_batch(
                "PRAGMA query_only=ON;\n\
                 PRAGMA foreign_keys=ON;\n\
                 PRAGMA trusted_schema=OFF;",
            )
            .map_err(|source| sqlite_error("hardening profiler read-only connection", source))?;
        verify_store_identity(&connection, true)?;
        verify_current_schema(&connection)?;
        verify_required_workspace_tables(&connection)?;
        verify_workspace_ready(&connection)?;
        verify_integrity(&connection)?;
        Ok(Self { connection })
    }

    pub fn open(path: &Path) -> ProfilerResult<Self> {
        Self::open_internal(path, true)
    }

    fn open_internal(path: &Path, allow_create: bool) -> ProfilerResult<Self> {
        if allow_create {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| ProfilerError::Io {
                    operation: "creating profiler database directory",
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        } else if !path.is_file() {
            return Err(ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceDatabaseMissing,
                "profiler database does not exist",
                false,
            ));
        }

        let existed = path.exists();
        let migration_failure_marker = migration_failure_marker_path(path);
        if existed && migration_failure_marker.is_file() {
            return Err(ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceMigrationFailed,
                "workspace contains a retained migration-failure marker",
                false,
            ));
        }
        let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if allow_create {
            flags |= OpenFlags::SQLITE_OPEN_CREATE;
        }
        let connection = Connection::open_with_flags(path, flags)
            .map_err(|source| sqlite_error("opening profiler database", source))?;

        verify_store_identity(&connection, existed)?;
        configure_connection(&connection)?;
        let schema_version = read_user_version(&connection)?;
        if schema_version > CURRENT_USER_VERSION {
            return Err(ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceSchemaNewerThanApplication,
                format!(
                    "workspace schema {schema_version} is newer than supported schema {CURRENT_USER_VERSION}"
                ),
                false,
            ));
        }
        if schema_version == CURRENT_USER_VERSION {
            verify_workspace_ready(&connection)?;
        }

        let migration_required =
            existed && schema_version > 0 && schema_version < CURRENT_USER_VERSION;
        if migration_required {
            backup_before_migration(&connection, path, schema_version)?;
            set_existing_migration_state(&connection, "migrating")?;
        }

        if let Err(error) = migrate(&connection) {
            if migration_required {
                let _ = set_existing_migration_state(&connection, "failed");
                let _ = write_migration_failure_marker(&migration_failure_marker, &error);
                return Err(ProfilerError::contract_with_context(
                    profiler_core::ErrorCode::WorkspaceMigrationFailed,
                    "workspace migration failed; the pre-migration backup was retained",
                    false,
                    [("cause".into(), error.to_string())].into_iter().collect(),
                ));
            }
            return Err(error);
        }
        ensure_workspace_meta(&connection)?;
        verify_required_workspace_tables(&connection)?;
        verify_workspace_ready(&connection)?;
        verify_integrity(&connection)?;
        if migration_failure_marker.exists() {
            fs::remove_file(&migration_failure_marker).map_err(|source| ProfilerError::Io {
                operation: "removing resolved migration failure marker",
                path: migration_failure_marker,
                source,
            })?;
        }
        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn register_collection(
        &self,
        adapter_kind: &str,
        archive_identity: &str,
        archive_root_display: &str,
    ) -> ProfilerResult<String> {
        let now = now_text();
        let existing = self
            .connection
            .query_row(
                "SELECT id FROM collections WHERE adapter_kind=?1 AND archive_identity=?2",
                params![adapter_kind, archive_identity],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| sqlite_error("looking up collection", source))?;

        if let Some(id) = existing {
            self.connection
                .execute(
                    "UPDATE collections SET archive_root_display=?1, updated_at=?2 WHERE id=?3",
                    params![archive_root_display, now, id],
                )
                .map_err(|source| sqlite_error("updating collection", source))?;
            return Ok(id);
        }

        let id = Uuid::now_v7().to_string();
        self.connection
            .execute(
                "INSERT INTO collections(id, adapter_kind, archive_identity, archive_root_display, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, adapter_kind, archive_identity, archive_root_display, now],
            )
            .map_err(|source| sqlite_error("registering collection", source))?;
        Ok(id)
    }

    pub fn register_source_snapshot(
        &self,
        collection_id: &str,
        manifest: &SourceSnapshotManifest,
    ) -> ProfilerResult<String> {
        let existing = self
            .connection
            .query_row(
                "SELECT id FROM source_snapshots WHERE collection_id=?1 AND snapshot_sha256=?2",
                params![collection_id, manifest.snapshot_sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| sqlite_error("looking up source snapshot", source))?;
        if let Some(id) = existing {
            return Ok(id);
        }

        let id = stable_id(&format!(
            "snapshot:{collection_id}:{}",
            manifest.snapshot_sha256
        ));
        self.connection
            .execute(
                "INSERT INTO source_snapshots(\
                    id, collection_id, run_id, source_schema_version, source_database_display, \
                    snapshot_database_path, snapshot_sha256, snapshot_bytes, source_metrics_json, \
                    snapshot_metrics_json, created_at\
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    collection_id,
                    manifest.run_id,
                    i64::from(manifest.schema_version),
                    manifest.source_database,
                    manifest.snapshot_database,
                    manifest.snapshot_sha256,
                    to_i64(manifest.snapshot_bytes, "snapshot_bytes")?,
                    to_json(&manifest.source_metrics, "serializing source metrics")?,
                    to_json(&manifest.snapshot_metrics, "serializing snapshot metrics")?,
                    manifest
                        .created_at
                        .format(&time::format_description::well_known::Rfc3339)
                        .map_err(|error| ProfilerError::Internal(format!(
                            "formatting snapshot time: {error}"
                        )))?,
                ],
            )
            .map_err(|source| sqlite_error("registering source snapshot", source))?;
        Ok(id)
    }

    pub fn create_run(
        &self,
        collection_id: Option<&str>,
        pipeline_version: &str,
        configuration_fingerprint: &str,
    ) -> ProfilerResult<String> {
        self.create_run_with_id(
            &Uuid::now_v7().to_string(),
            collection_id,
            pipeline_version,
            configuration_fingerprint,
        )
    }

    pub fn create_run_with_id(
        &self,
        run_id: &str,
        collection_id: Option<&str>,
        pipeline_version: &str,
        configuration_fingerprint: &str,
    ) -> ProfilerResult<String> {
        let now = now_text();
        self.connection
            .execute(
                "INSERT INTO profiler_runs(id, collection_id, state, pipeline_version, configuration_fingerprint, created_at, updated_at) \
                 VALUES(?1, ?2, 'pending', ?3, ?4, ?5, ?5) \
                 ON CONFLICT(id) DO NOTHING",
                params![run_id, collection_id, pipeline_version, configuration_fingerprint, now],
            )
            .map_err(|source| sqlite_error("creating profiler run", source))?;
        Ok(run_id.to_owned())
    }

    pub fn attach_source_snapshot(&self, run_id: &str, snapshot_id: &str) -> ProfilerResult<()> {
        let changed = self
            .connection
            .execute(
                "UPDATE profiler_runs SET source_snapshot_id=?1, updated_at=?2 WHERE id=?3",
                params![snapshot_id, now_text(), run_id],
            )
            .map_err(|source| sqlite_error("attaching source snapshot to run", source))?;
        if changed != 1 {
            return Err(ProfilerError::InvalidArgument(format!(
                "profiler run does not exist: {run_id}"
            )));
        }
        Ok(())
    }

    pub fn transition_run(&self, run_id: &str, to: RunState) -> ProfilerResult<()> {
        let from_text: String = self
            .connection
            .query_row(
                "SELECT state FROM profiler_runs WHERE id=?1",
                [run_id],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("loading profiler run state", source))?;
        let from = parse_run_state(&from_text)?;
        validate_transition(from, to)?;

        let now = now_text();
        let started_at = matches!(to, RunState::Preflighting).then_some(now.as_str());
        let finished_at = to.is_terminal().then_some(now.as_str());
        self.connection
            .execute(
                "UPDATE profiler_runs SET state=?1, started_at=COALESCE(started_at, ?2), \
                 finished_at=COALESCE(?3, finished_at), updated_at=?4 WHERE id=?5",
                params![to.to_string(), started_at, finished_at, now, run_id],
            )
            .map_err(|source| sqlite_error("transitioning profiler run", source))?;
        Ok(())
    }

    pub fn fail_run(&self, run_id: &str, error: &ProfilerError) -> ProfilerResult<()> {
        let report = error.report();
        self.connection
            .execute(
                "UPDATE profiler_runs SET state='failed', failure_code=?1, failure_message=?2, \
                 finished_at=?3, updated_at=?3 WHERE id=?4 AND state NOT IN ('cancelled','succeeded','failed')",
                params![format!("{:?}", report.code), report.message, now_text(), run_id],
            )
            .map_err(|source| sqlite_error("recording profiler run failure", source))?;
        Ok(())
    }

    pub fn list_inventory_summary(
        &self,
        request: &InventoryRequest,
    ) -> ProfilerResult<InventorySummary> {
        inventory_summary(&self.connection, request)
    }
}

impl InventorySink for ProfilerStore {
    fn load_checkpoint(
        &self,
        run_id: &str,
        table: InventoryTable,
    ) -> ProfilerResult<Option<InventoryCheckpoint>> {
        let stage = checkpoint_stage(table);
        self.connection
            .query_row(
                "SELECT stable_cursor FROM run_checkpoints WHERE run_id=?1 AND stage=?2",
                params![run_id, stage],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| sqlite_error("loading inventory checkpoint", source))?
            .map(|payload| {
                serde_json::from_str::<InventoryCheckpoint>(&payload).map_err(|error| {
                    ProfilerError::Internal(format!(
                        "invalid persisted checkpoint for {table}: {error}"
                    ))
                })
            })
            .transpose()
    }

    fn ingest_batch(
        &mut self,
        request: &InventoryRequest,
        batch: InventoryBatch,
        checkpoint: &InventoryCheckpoint,
    ) -> ProfilerResult<()> {
        if batch.table() != checkpoint.table {
            return Err(ProfilerError::Internal(format!(
                "inventory batch {} does not match checkpoint {}",
                batch.table(),
                checkpoint.table
            )));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sqlite_error("starting inventory batch", source))?;
        match batch {
            InventoryBatch::Messages(rows) => ingest_messages(&transaction, request, rows)?,
            InventoryBatch::MessageOccurrences(rows) => {
                ingest_occurrences(&transaction, request, rows)?;
            }
            InventoryBatch::Participants(rows) => {
                ingest_participants(&transaction, request, rows)?;
            }
            InventoryBatch::Blobs(rows) => ingest_blobs(&transaction, request, rows)?,
            InventoryBatch::Parts(rows) => ingest_parts(&transaction, request, rows)?,
            InventoryBatch::MessageRelations(rows) => {
                ingest_relations(&transaction, request, rows)?;
            }
        }
        persist_checkpoint(&transaction, request, checkpoint)?;
        transaction
            .commit()
            .map_err(|source| sqlite_error("committing inventory batch", source))?;
        Ok(())
    }

    fn finalize_inventory(
        &mut self,
        request: &InventoryRequest,
    ) -> ProfilerResult<InventorySummary> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sqlite_error("starting inventory aggregation", source))?;

        transaction
            .execute(
                "UPDATE content_objects AS content \
                 SET occurrence_count=(\
                    SELECT COUNT(*) FROM content_occurrences AS occurrence \
                    WHERE occurrence.content_object_id=content.id\
                 ), updated_at=?1 \
                 WHERE collection_id=?2",
                params![now_text(), request.collection_id],
            )
            .map_err(|source| sqlite_error("updating content occurrence counts", source))?;

        transaction
            .execute(
                "DELETE FROM filename_variants WHERE content_object_id IN (\
                    SELECT id FROM content_objects WHERE collection_id=?1\
                 )",
                [request.collection_id.as_str()],
            )
            .map_err(|source| sqlite_error("resetting filename variants", source))?;
        transaction
            .execute(
                "INSERT INTO filename_variants(\
                    content_object_id, normalized_filename, display_filename, occurrence_count, \
                    first_seen_at, last_seen_at\
                 ) \
                 SELECT content_object_id, filename_normalized, MIN(filename_original), COUNT(*), \
                        MIN(message_date), MAX(message_date) \
                 FROM content_occurrences \
                 WHERE filename_normalized IS NOT NULL \
                   AND content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1) \
                 GROUP BY content_object_id, filename_normalized",
                [request.collection_id.as_str()],
            )
            .map_err(|source| sqlite_error("building filename variants", source))?;

        replace_inventory_findings(&transaction, request)?;
        transaction
            .commit()
            .map_err(|source| sqlite_error("committing inventory aggregation", source))?;
        inventory_summary(&self.connection, request)
    }
}

impl FileStatStore for ProfilerStore {
    fn load_file_stat_checkpoint(
        &self,
        run_id: &str,
    ) -> ProfilerResult<Option<FileStatCheckpoint>> {
        self.connection
            .query_row(
                "SELECT stable_cursor FROM run_checkpoints WHERE run_id=?1 AND stage='file_stat'",
                [run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| sqlite_error("loading file-stat checkpoint", source))?
            .map(|payload| {
                serde_json::from_str(&payload).map_err(|error| {
                    ProfilerError::Internal(format!(
                        "persisted file-stat checkpoint is invalid JSON: {error}"
                    ))
                })
            })
            .transpose()
    }

    fn count_file_stat_objects(&self, collection_id: &str) -> ProfilerResult<(u64, u64)> {
        let (objects, bytes): (i64, i64) = self
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(expected_size_bytes), 0) \
                 FROM content_objects WHERE collection_id=?1",
                [collection_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|source| sqlite_error("counting file-stat objects", source))?;
        Ok((
            u64::try_from(objects).map_err(|_| {
                ProfilerError::Internal("negative file-stat object count returned".into())
            })?,
            u64::try_from(bytes).map_err(|_| {
                ProfilerError::Internal("negative file-stat byte count returned".into())
            })?,
        ))
    }

    fn load_file_stat_batch(
        &self,
        collection_id: &str,
        after_sha256: Option<&str>,
        limit: u32,
    ) -> ProfilerResult<Vec<FileStatWorkItem>> {
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT id, sha256, expected_size_bytes, canonical_path_display \
                 FROM content_objects \
                 WHERE collection_id=?1 AND sha256>?2 \
                 ORDER BY sha256 LIMIT ?3",
            )
            .map_err(|source| sqlite_error("preparing file-stat work batch", source))?;
        statement
            .query_map(
                params![collection_id, after_sha256.unwrap_or(""), i64::from(limit)],
                |row| {
                    let size: i64 = row.get(2)?;
                    Ok(FileStatWorkItem {
                        content_object_id: row.get(0)?,
                        sha256: row.get(1)?,
                        expected_size_bytes: u64::try_from(size)
                            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, size))?,
                        source_locator: row.get(3)?,
                    })
                },
            )
            .map_err(|source| sqlite_error("querying file-stat work batch", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting file-stat work batch", source))
    }

    fn commit_file_stat_batch(
        &mut self,
        request: &FileStatRequest,
        observations: &[FileStatObservation],
        checkpoint: &FileStatCheckpoint,
    ) -> ProfilerResult<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sqlite_error("starting file-stat batch", source))?;
        persist_file_stat_observations(&transaction, request, observations)?;
        persist_file_stat_checkpoint(&transaction, request, checkpoint)?;
        transaction
            .commit()
            .map_err(|source| sqlite_error("committing file-stat batch", source))
    }

    fn finalize_file_stat(&mut self, request: &FileStatRequest) -> ProfilerResult<FileStatSummary> {
        let summary = file_stat_summary(&self.connection, request)?;
        let (expected_objects, expected_bytes) =
            self.count_file_stat_objects(&request.collection_id)?;
        if summary.total_objects != expected_objects || summary.expected_bytes != expected_bytes {
            return Err(ProfilerError::Internal(format!(
                "file-stat reconciliation failed: observations={}/{}, bytes={}/{}",
                summary.total_objects, expected_objects, summary.expected_bytes, expected_bytes
            )));
        }
        Ok(summary)
    }
}

fn ingest_messages(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    rows: Vec<profiler_core::SourceMessageRecord>,
) -> ProfilerResult<()> {
    let mut statement = transaction
        .prepare_cached(
            "INSERT INTO source_messages(\
                snapshot_id, source_message_id, archive_id, account_id, provider_thread_namespace, \
                provider_thread_value, rfc_message_id, subject_raw, subject_normalized, header_date, \
                raw_path, raw_sha256, raw_size_bytes, parse_defects_json\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
             ON CONFLICT(snapshot_id, source_message_id) DO UPDATE SET \
                archive_id=excluded.archive_id, account_id=excluded.account_id, \
                provider_thread_namespace=excluded.provider_thread_namespace, \
                provider_thread_value=excluded.provider_thread_value, \
                rfc_message_id=excluded.rfc_message_id, subject_raw=excluded.subject_raw, \
                subject_normalized=excluded.subject_normalized, header_date=excluded.header_date, \
                raw_path=excluded.raw_path, raw_sha256=excluded.raw_sha256, \
                raw_size_bytes=excluded.raw_size_bytes, parse_defects_json=excluded.parse_defects_json",
        )
        .map_err(|source| sqlite_error("preparing message ingestion", source))?;
    for row in rows {
        statement
            .execute(params![
                request.source_snapshot_id,
                row.id,
                row.archive_id,
                row.account_id,
                row.provider_thread_namespace,
                row.provider_thread_value,
                row.rfc_message_id,
                row.subject_raw,
                row.subject_normalized,
                row.header_date,
                row.raw_path,
                row.raw_sha256,
                row.raw_size_bytes
                    .map(|value| to_i64(value, "raw_size_bytes"))
                    .transpose()?,
                valid_json_or_default(&row.parse_defects_json, "[]"),
            ])
            .map_err(|source| sqlite_error("ingesting source message", source))?;
    }
    Ok(())
}

fn ingest_occurrences(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    rows: Vec<profiler_core::SourceOccurrenceRecord>,
) -> ProfilerResult<()> {
    let mut statement = transaction
        .prepare_cached(
            "INSERT INTO source_message_occurrences(\
                snapshot_id, source_occurrence_id, source_message_id, generation_id, uid, \
                labels_json, internal_date, fetch_status\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(snapshot_id, source_occurrence_id) DO UPDATE SET \
                source_message_id=excluded.source_message_id, generation_id=excluded.generation_id, \
                uid=excluded.uid, labels_json=excluded.labels_json, \
                internal_date=excluded.internal_date, fetch_status=excluded.fetch_status",
        )
        .map_err(|source| sqlite_error("preparing occurrence ingestion", source))?;
    for row in rows {
        statement
            .execute(params![
                request.source_snapshot_id,
                row.id,
                row.message_id,
                row.generation_id,
                row.uid,
                valid_json_or_default(&row.labels_json, "[]"),
                row.internal_date,
                row.fetch_status,
            ])
            .map_err(|source| sqlite_error("ingesting source occurrence", source))?;
    }
    Ok(())
}

fn ingest_participants(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    rows: Vec<profiler_core::SourceParticipantRecord>,
) -> ProfilerResult<()> {
    let mut statement = transaction
        .prepare_cached(
            "INSERT INTO source_participants(\
                snapshot_id, source_participant_id, source_message_id, role, ordinal, name, address, domain\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(snapshot_id, source_participant_id) DO UPDATE SET \
                source_message_id=excluded.source_message_id, role=excluded.role, ordinal=excluded.ordinal, \
                name=excluded.name, address=excluded.address, domain=excluded.domain",
        )
        .map_err(|source| sqlite_error("preparing participant ingestion", source))?;
    for row in rows {
        statement
            .execute(params![
                request.source_snapshot_id,
                row.id,
                row.message_id,
                row.role,
                row.ordinal,
                row.name,
                row.address,
                row.domain,
            ])
            .map_err(|source| sqlite_error("ingesting source participant", source))?;
    }
    Ok(())
}

fn ingest_blobs(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    rows: Vec<profiler_core::SourceBlobRecord>,
) -> ProfilerResult<()> {
    let now = now_text();
    let mut source_statement = transaction
        .prepare_cached(
            "INSERT INTO source_blobs(
                snapshot_id, sha256, size_bytes, detected_mime_type, storage_path, first_seen_at, last_verified_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(snapshot_id, sha256) DO UPDATE SET
                size_bytes=excluded.size_bytes, detected_mime_type=excluded.detected_mime_type,
                storage_path=excluded.storage_path, first_seen_at=excluded.first_seen_at,
                last_verified_at=excluded.last_verified_at",
        )
        .map_err(|source| sqlite_error("preparing source blob ingestion", source))?;
    let mut statement = transaction
        .prepare_cached(
            "INSERT INTO content_objects(\
                id, collection_id, sha256, expected_size_bytes, source_detected_mime_type, \
                canonical_path_display, availability_state, integrity_state, security_state, \
                first_seen_at, last_seen_at, occurrence_count, created_at, updated_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, 'uninspected', ?7, 'unknown', ?8, ?8, 0, ?9, ?9) \
             ON CONFLICT(collection_id, sha256) DO UPDATE SET \
                expected_size_bytes=excluded.expected_size_bytes, \
                source_detected_mime_type=excluded.source_detected_mime_type, \
                canonical_path_display=excluded.canonical_path_display, \
                integrity_state=excluded.integrity_state, \
                first_seen_at=COALESCE(content_objects.first_seen_at, excluded.first_seen_at), \
                updated_at=excluded.updated_at",
        )
        .map_err(|source| sqlite_error("preparing blob ingestion", source))?;
    for row in rows {
        if !is_sha256(&row.sha256) {
            insert_finding(
                transaction,
                request,
                None,
                "INVALID_BLOB_SHA256",
                "error",
                "MailVault blob row contains an invalid SHA-256 value",
                serde_json::json!({"sha256": row.sha256}),
            )?;
            continue;
        }
        source_statement
            .execute(params![
                request.source_snapshot_id,
                row.sha256,
                to_i64(row.size_bytes, "blob size")?,
                row.detected_mime_type,
                row.storage_path,
                row.first_seen_at,
                row.last_verified_at,
            ])
            .map_err(|source| sqlite_error("ingesting source blob", source))?;
        let content_id = content_object_id(&request.collection_id, &row.sha256);
        statement
            .execute(params![
                content_id,
                request.collection_id,
                row.sha256,
                to_i64(row.size_bytes, "blob size")?,
                row.detected_mime_type,
                row.storage_path,
                if row.last_verified_at.is_some() {
                    "source_verified"
                } else {
                    "not_verified"
                },
                row.first_seen_at,
                now,
            ])
            .map_err(|source| sqlite_error("ingesting content object", source))?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn ingest_parts(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    rows: Vec<profiler_core::SourcePartRecord>,
) -> ProfilerResult<()> {
    let now = now_text();
    let mut part_statement = transaction
        .prepare_cached(
            "INSERT INTO source_parts(\
                snapshot_id, source_part_id, source_message_id, part_path, parent_part_path, role, \
                declared_mime_type, detected_mime_type, content_disposition, content_id, \
                filename_original, filename_safe, size_bytes, sha256, blob_path, defects_json, \
                charset, transfer_encoding\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18) \
             ON CONFLICT(snapshot_id, source_part_id) DO UPDATE SET \
                source_message_id=excluded.source_message_id, part_path=excluded.part_path, \
                parent_part_path=excluded.parent_part_path, role=excluded.role, \
                declared_mime_type=excluded.declared_mime_type, detected_mime_type=excluded.detected_mime_type, \
                content_disposition=excluded.content_disposition, content_id=excluded.content_id, \
                filename_original=excluded.filename_original, filename_safe=excluded.filename_safe, \
                size_bytes=excluded.size_bytes, sha256=excluded.sha256, blob_path=excluded.blob_path, \
                defects_json=excluded.defects_json, charset=excluded.charset, \
                transfer_encoding=excluded.transfer_encoding",
        )
        .map_err(|source| sqlite_error("preparing part ingestion", source))?;
    let mut fallback_content_statement = transaction
        .prepare_cached(
            "INSERT INTO content_objects(\
                id, collection_id, sha256, expected_size_bytes, source_detected_mime_type, \
                canonical_path_display, availability_state, integrity_state, security_state, \
                first_seen_at, last_seen_at, occurrence_count, created_at, updated_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, 'uninspected', 'not_verified', 'unknown', ?7, ?7, 0, ?8, ?8) \
             ON CONFLICT(collection_id, sha256) DO NOTHING",
        )
        .map_err(|source| sqlite_error("preparing fallback content ingestion", source))?;
    let mut occurrence_statement = transaction
        .prepare_cached(
            "INSERT INTO content_occurrences(\
                id, snapshot_id, content_object_id, source_message_id, source_part_id, part_path, \
                filename_original, filename_normalized, role, message_date, sender_domain, created_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
             ON CONFLICT(snapshot_id, source_message_id, part_path) DO UPDATE SET \
                content_object_id=excluded.content_object_id, source_part_id=excluded.source_part_id, \
                filename_original=excluded.filename_original, filename_normalized=excluded.filename_normalized, \
                role=excluded.role, message_date=excluded.message_date, sender_domain=excluded.sender_domain",
        )
        .map_err(|source| sqlite_error("preparing content occurrence ingestion", source))?;

    for row in rows {
        part_statement
            .execute(params![
                request.source_snapshot_id,
                row.id,
                row.message_id,
                row.part_path,
                row.parent_part_path,
                row.role,
                row.declared_mime_type,
                row.detected_mime_type,
                row.content_disposition,
                row.content_id,
                row.filename_original,
                row.filename_safe,
                to_i64(row.size_bytes, "part size")?,
                row.sha256,
                row.blob_path,
                valid_json_or_default(&row.defects_json, "[]"),
                row.charset,
                row.transfer_encoding,
            ])
            .map_err(|source| sqlite_error("ingesting source part", source))?;

        let Some(sha256) = row.sha256.as_deref() else {
            continue;
        };
        if !is_sha256(sha256) {
            insert_finding(
                transaction,
                request,
                None,
                "INVALID_PART_SHA256",
                "error",
                "MailVault MIME part contains an invalid SHA-256 value",
                serde_json::json!({
                    "sourceMessageId": row.message_id,
                    "sourcePartId": row.id,
                    "partPath": row.part_path,
                    "sha256": sha256,
                }),
            )?;
            continue;
        }

        let content_id = content_object_id(&request.collection_id, sha256);
        fallback_content_statement
            .execute(params![
                content_id,
                request.collection_id,
                sha256,
                to_i64(row.size_bytes, "part size")?,
                row.detected_mime_type
                    .as_deref()
                    .unwrap_or(&row.declared_mime_type),
                row.blob_path.as_deref().unwrap_or(""),
                row.message_date,
                now,
            ])
            .map_err(|source| sqlite_error("ingesting fallback content object", source))?;

        let occurrence_id = stable_id(&format!(
            "occurrence:{}:{}:{}",
            request.source_snapshot_id, row.message_id, row.part_path
        ));
        let preferred_filename = row
            .filename_original
            .as_deref()
            .or(row.filename_safe.as_deref());
        let normalized_filename = preferred_filename.and_then(normalize_filename);
        occurrence_statement
            .execute(params![
                occurrence_id,
                request.source_snapshot_id,
                content_id,
                row.message_id,
                row.id,
                row.part_path,
                preferred_filename,
                normalized_filename,
                row.role,
                row.message_date,
                row.sender_domain,
                now,
            ])
            .map_err(|source| sqlite_error("ingesting content occurrence", source))?;
    }
    Ok(())
}

fn ingest_relations(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    rows: Vec<profiler_core::SourceRelationRecord>,
) -> ProfilerResult<()> {
    let mut statement = transaction
        .prepare_cached(
            "INSERT INTO source_message_relations(\
                snapshot_id, source_relation_id, source_message_id, target_message_id, \
                relation_type, evidence_type, confidence, source_created_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(snapshot_id, source_relation_id) DO UPDATE SET \
                source_message_id=excluded.source_message_id, target_message_id=excluded.target_message_id, \
                relation_type=excluded.relation_type, evidence_type=excluded.evidence_type, \
                confidence=excluded.confidence, source_created_at=excluded.source_created_at",
        )
        .map_err(|source| sqlite_error("preparing relation ingestion", source))?;
    for row in rows {
        statement
            .execute(params![
                request.source_snapshot_id,
                row.id,
                row.source_message_id,
                row.target_message_id,
                row.relation_type,
                row.evidence_type,
                row.confidence,
                row.created_at,
            ])
            .map_err(|source| sqlite_error("ingesting source relation", source))?;
    }
    Ok(())
}

fn persist_checkpoint(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    checkpoint: &InventoryCheckpoint,
) -> ProfilerResult<()> {
    let payload = to_json(checkpoint, "serializing inventory checkpoint")?;
    transaction
        .execute(
            "INSERT INTO run_checkpoints(\
                run_id, stage, sequence, stable_cursor, tool_versions_json, \
                configuration_fingerprint, committed_at\
             ) VALUES(?1, ?2, ?3, ?4, '{}', 'metadata-inventory-v1', ?5) \
             ON CONFLICT(run_id, stage) DO UPDATE SET \
                sequence=excluded.sequence, stable_cursor=excluded.stable_cursor, \
                tool_versions_json=excluded.tool_versions_json, \
                configuration_fingerprint=excluded.configuration_fingerprint, \
                committed_at=excluded.committed_at",
            params![
                request.run_id,
                checkpoint_stage(checkpoint.table),
                to_i64(checkpoint.sequence, "checkpoint sequence")?,
                payload,
                now_text(),
            ],
        )
        .map_err(|source| sqlite_error("persisting inventory checkpoint", source))?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn persist_file_stat_observations(
    transaction: &Transaction<'_>,
    request: &FileStatRequest,
    observations: &[FileStatObservation],
) -> ProfilerResult<()> {
    let observed_at = now_text();
    let mut observation_statement = transaction
        .prepare_cached(
            "INSERT INTO file_stat_observations(\
                id, run_id, content_object_id, sha256, source_locator, expected_locator, \
                availability_state, size_state, expected_size_bytes, actual_size_bytes, \
                modified_unix_ns, error_kind, error_message, observed_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
             ON CONFLICT(run_id, content_object_id) DO UPDATE SET \
                sha256=excluded.sha256, source_locator=excluded.source_locator, \
                expected_locator=excluded.expected_locator, \
                availability_state=excluded.availability_state, size_state=excluded.size_state, \
                expected_size_bytes=excluded.expected_size_bytes, \
                actual_size_bytes=excluded.actual_size_bytes, \
                modified_unix_ns=excluded.modified_unix_ns, error_kind=excluded.error_kind, \
                error_message=excluded.error_message, observed_at=excluded.observed_at",
        )
        .map_err(|source| sqlite_error("preparing file-stat observation", source))?;
    let mut content_statement = transaction
        .prepare_cached(
            "UPDATE content_objects SET \
                actual_size_bytes=?1, availability_state=?2, size_state=?3, \
                integrity_state=CASE \
                    WHEN ?2<>'available' THEN 'unavailable' \
                    WHEN ?3='mismatch' THEN 'size_mismatch' \
                    WHEN integrity_state='source_verified' THEN 'source_verified' \
                    ELSE 'size_verified' \
                END, \
                modified_unix_ns=?4, last_stat_run_id=?5, last_stat_at=?6, updated_at=?6 \
             WHERE id=?7 AND collection_id=?8",
        )
        .map_err(|source| sqlite_error("preparing content-object file-stat update", source))?;
    let mut delete_finding_statement = transaction
        .prepare_cached(
            "DELETE FROM findings \
             WHERE run_id=?1 AND content_object_id=?2 AND code IN (\
                'MISSING_BLOB', 'UNREADABLE_BLOB', 'INVALID_BLOB_LOCATOR', \
                'NON_REGULAR_BLOB', 'UNSAFE_REPARSE_POINT', 'BLOB_STAT_IO_ERROR', \
                'BLOB_SIZE_MISMATCH'\
             )",
        )
        .map_err(|source| sqlite_error("preparing file-stat finding reset", source))?;
    let mut event_statement = transaction
        .prepare_cached(
            "INSERT INTO processing_events(\
                id, run_id, content_object_id, event_type, agent_name, agent_version, \
                outcome, detail_json, occurred_at\
             ) VALUES(?1, ?2, ?3, 'file_stat', ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(id) DO UPDATE SET outcome=excluded.outcome, \
                detail_json=excluded.detail_json, occurred_at=excluded.occurred_at",
        )
        .map_err(|source| sqlite_error("preparing file-stat processing event", source))?;

    for observation in observations {
        let observation_id = stable_id(&format!(
            "file-stat:{}:{}",
            request.run_id, observation.content_object_id
        ));
        let actual_size = observation
            .actual_size_bytes
            .map(|value| to_i64(value, "actual_size_bytes"))
            .transpose()?;
        observation_statement
            .execute(params![
                observation_id,
                request.run_id,
                observation.content_object_id,
                observation.sha256,
                observation.source_locator,
                observation.expected_locator,
                observation.availability_state.as_str(),
                observation.size_state.as_str(),
                to_i64(observation.expected_size_bytes, "expected_size_bytes")?,
                actual_size,
                observation.modified_unix_ns,
                observation.error_kind,
                observation.error_message,
                observed_at,
            ])
            .map_err(|source| sqlite_error("persisting file-stat observation", source))?;

        let updated = content_statement
            .execute(params![
                actual_size,
                observation.availability_state.as_str(),
                observation.size_state.as_str(),
                observation.modified_unix_ns,
                request.run_id,
                observed_at,
                observation.content_object_id,
                request.collection_id,
            ])
            .map_err(|source| sqlite_error("updating content-object file-stat state", source))?;
        if updated != 1 {
            return Err(ProfilerError::Internal(format!(
                "file-stat observation references an unknown content object: {}",
                observation.content_object_id
            )));
        }

        delete_finding_statement
            .execute(params![request.run_id, observation.content_object_id])
            .map_err(|source| sqlite_error("resetting file-stat findings", source))?;

        if let Some((code, severity, message)) = file_stat_finding(observation) {
            insert_file_stat_finding(transaction, request, observation, code, severity, message)?;
        }

        let outcome = if observation.availability_state == AvailabilityState::Available
            && observation.size_state == SizeState::Match
        {
            "success"
        } else {
            "warning"
        };
        let detail = serde_json::json!({
            "sha256": observation.sha256,
            "sourceLocator": observation.source_locator,
            "expectedLocator": observation.expected_locator,
            "availabilityState": observation.availability_state,
            "sizeState": observation.size_state,
            "expectedSizeBytes": observation.expected_size_bytes,
            "actualSizeBytes": observation.actual_size_bytes,
            "modifiedUnixNs": observation.modified_unix_ns,
            "errorKind": observation.error_kind,
        });
        let event_id = stable_id(&format!(
            "event:{}:file-stat:{}",
            request.run_id, observation.content_object_id
        ));
        event_statement
            .execute(params![
                event_id,
                request.run_id,
                observation.content_object_id,
                request.agent_name,
                request.agent_version,
                outcome,
                detail.to_string(),
                observed_at,
            ])
            .map_err(|source| sqlite_error("persisting file-stat processing event", source))?;
    }
    Ok(())
}

fn file_stat_finding(
    observation: &FileStatObservation,
) -> Option<(&'static str, &'static str, &'static str)> {
    match observation.availability_state {
        AvailabilityState::Missing => Some((
            "MISSING_BLOB",
            "warning",
            "Canonical MailVault blob object is missing from the archive",
        )),
        AvailabilityState::Unreadable => Some((
            "UNREADABLE_BLOB",
            "error",
            "Canonical MailVault blob object cannot be opened for reading",
        )),
        AvailabilityState::InvalidLocator => Some((
            "INVALID_BLOB_LOCATOR",
            "error",
            "MailVault blob locator failed strict content-addressed path validation",
        )),
        AvailabilityState::NonRegular => Some((
            "NON_REGULAR_BLOB",
            "error",
            "MailVault blob locator does not resolve to a regular file",
        )),
        AvailabilityState::UnsafeReparsePoint => Some((
            "UNSAFE_REPARSE_POINT",
            "error",
            "MailVault blob path traverses a symbolic link or reparse point",
        )),
        AvailabilityState::IoError => Some((
            "BLOB_STAT_IO_ERROR",
            "error",
            "MailVault blob metadata inspection failed",
        )),
        AvailabilityState::Available if observation.size_state == SizeState::Mismatch => Some((
            "BLOB_SIZE_MISMATCH",
            "error",
            "MailVault blob size does not match the canonical database value",
        )),
        AvailabilityState::Available => None,
        AvailabilityState::Uninspected => Some((
            "FILE_STAT_UNINSPECTED",
            "error",
            "File-stat observation remained uninspected",
        )),
    }
}

fn insert_file_stat_finding(
    transaction: &Transaction<'_>,
    request: &FileStatRequest,
    observation: &FileStatObservation,
    code: &str,
    severity: &str,
    message: &str,
) -> ProfilerResult<()> {
    let evidence = serde_json::json!({
        "sha256": observation.sha256,
        "sourceLocator": observation.source_locator,
        "expectedLocator": observation.expected_locator,
        "availabilityState": observation.availability_state,
        "sizeState": observation.size_state,
        "expectedSizeBytes": observation.expected_size_bytes,
        "actualSizeBytes": observation.actual_size_bytes,
        "errorKind": observation.error_kind,
        "errorMessage": observation.error_message,
    });
    let id = stable_id(&format!(
        "finding:{}:{code}:{}",
        request.run_id, observation.content_object_id
    ));
    transaction
        .execute(
            "INSERT INTO findings(\
                id, run_id, content_object_id, code, severity, message, evidence_json, created_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(id) DO UPDATE SET severity=excluded.severity, \
                message=excluded.message, evidence_json=excluded.evidence_json, \
                created_at=excluded.created_at, resolved_at=NULL",
            params![
                id,
                request.run_id,
                observation.content_object_id,
                code,
                severity,
                message,
                evidence.to_string(),
                now_text(),
            ],
        )
        .map_err(|source| sqlite_error("recording file-stat finding", source))?;
    Ok(())
}

fn persist_file_stat_checkpoint(
    transaction: &Transaction<'_>,
    request: &FileStatRequest,
    checkpoint: &FileStatCheckpoint,
) -> ProfilerResult<()> {
    let payload = to_json(checkpoint, "serializing file-stat checkpoint")?;
    let tool_versions = serde_json::json!({
        request.agent_name.clone(): request.agent_version,
    });
    transaction
        .execute(
            "INSERT INTO run_checkpoints(\
                run_id, stage, sequence, stable_cursor, tool_versions_json, \
                configuration_fingerprint, committed_at\
             ) VALUES(?1, 'file_stat', ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(run_id, stage) DO UPDATE SET \
                sequence=excluded.sequence, stable_cursor=excluded.stable_cursor, \
                tool_versions_json=excluded.tool_versions_json, \
                configuration_fingerprint=excluded.configuration_fingerprint, \
                committed_at=excluded.committed_at",
            params![
                request.run_id,
                to_i64(checkpoint.sequence, "file-stat checkpoint sequence")?,
                payload,
                tool_versions.to_string(),
                request.configuration_fingerprint,
                now_text(),
            ],
        )
        .map_err(|source| sqlite_error("persisting file-stat checkpoint", source))?;
    Ok(())
}

fn file_stat_summary(
    connection: &Connection,
    request: &FileStatRequest,
) -> ProfilerResult<FileStatSummary> {
    let values = connection
        .query_row(
            "SELECT \
                COUNT(*), \
                COALESCE(SUM(availability_state='available'), 0), \
                COALESCE(SUM(availability_state='missing'), 0), \
                COALESCE(SUM(availability_state='unreadable'), 0), \
                COALESCE(SUM(availability_state='invalid_locator'), 0), \
                COALESCE(SUM(availability_state='non_regular'), 0), \
                COALESCE(SUM(availability_state='unsafe_reparse_point'), 0), \
                COALESCE(SUM(availability_state='io_error'), 0), \
                COALESCE(SUM(size_state='match'), 0), \
                COALESCE(SUM(size_state='mismatch'), 0), \
                COALESCE(SUM(expected_size_bytes), 0), \
                COALESCE(SUM(CASE WHEN availability_state='available' \
                                  THEN actual_size_bytes ELSE 0 END), 0) \
             FROM file_stat_observations WHERE run_id=?1",
            [request.run_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        )
        .map_err(|source| sqlite_error("reading file-stat summary", source))?;

    let convert = |value: i64, field: &str| {
        u64::try_from(value)
            .map_err(|_| ProfilerError::Internal(format!("negative {field} returned")))
    };
    Ok(FileStatSummary {
        total_objects: convert(values.0, "file-stat total")?,
        available_objects: convert(values.1, "available object count")?,
        missing_objects: convert(values.2, "missing object count")?,
        unreadable_objects: convert(values.3, "unreadable object count")?,
        invalid_locator_objects: convert(values.4, "invalid locator count")?,
        non_regular_objects: convert(values.5, "non-regular object count")?,
        unsafe_reparse_objects: convert(values.6, "unsafe reparse count")?,
        io_error_objects: convert(values.7, "file-stat I/O error count")?,
        size_matches: convert(values.8, "size match count")?,
        size_mismatches: convert(values.9, "size mismatch count")?,
        expected_bytes: convert(values.10, "expected byte count")?,
        available_bytes: convert(values.11, "available byte count")?,
    })
}

fn replace_inventory_findings(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
) -> ProfilerResult<()> {
    transaction
        .execute(
            "DELETE FROM findings WHERE run_id=?1 AND code IN (\
                'ZERO_BYTE_CONTENT', 'SAME_HASH_DIFFERENT_NAMES', 'SAME_NAME_DIFFERENT_HASHES'\
             )",
            [request.run_id.as_str()],
        )
        .map_err(|source| sqlite_error("resetting inventory findings", source))?;

    let mut zero_statement = transaction
        .prepare(
            "SELECT id, sha256, occurrence_count FROM content_objects \
             WHERE collection_id=?1 AND expected_size_bytes=0 ORDER BY sha256",
        )
        .map_err(|source| sqlite_error("preparing zero-byte findings", source))?;
    let zero_rows = zero_statement
        .query_map([request.collection_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|source| sqlite_error("querying zero-byte findings", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting zero-byte findings", source))?;
    drop(zero_statement);
    for (content_id, sha256, occurrences) in zero_rows {
        insert_finding(
            transaction,
            request,
            Some(&content_id),
            "ZERO_BYTE_CONTENT",
            "warning",
            "Content object has a zero-byte payload",
            serde_json::json!({"sha256": sha256, "occurrenceCount": occurrences}),
        )?;
    }

    let mut variants_statement = transaction
        .prepare(
            "SELECT content_object_id, COUNT(*) AS variants \
             FROM filename_variants \
             WHERE content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1) \
             GROUP BY content_object_id HAVING COUNT(*) > 1",
        )
        .map_err(|source| sqlite_error("preparing filename-variant findings", source))?;
    let variant_rows = variants_statement
        .query_map([request.collection_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|source| sqlite_error("querying filename-variant findings", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting filename-variant findings", source))?;
    drop(variants_statement);
    for (content_id, variants) in variant_rows {
        insert_finding(
            transaction,
            request,
            Some(&content_id),
            "SAME_HASH_DIFFERENT_NAMES",
            "info",
            "Exact binary content was observed under multiple normalized filenames",
            serde_json::json!({"filenameVariantCount": variants}),
        )?;
    }

    let mut collision_statement = transaction
        .prepare(
            "SELECT normalized_filename, COUNT(DISTINCT content_object_id) AS binaries \
             FROM filename_variants \
             WHERE content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1) \
             GROUP BY normalized_filename HAVING COUNT(DISTINCT content_object_id) > 1",
        )
        .map_err(|source| sqlite_error("preparing filename-collision findings", source))?;
    let collisions = collision_statement
        .query_map([request.collection_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|source| sqlite_error("querying filename-collision findings", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting filename-collision findings", source))?;
    drop(collision_statement);
    for (filename, binaries) in collisions {
        insert_finding(
            transaction,
            request,
            None,
            "SAME_NAME_DIFFERENT_HASHES",
            "info",
            "One normalized filename refers to multiple exact binaries",
            serde_json::json!({"normalizedFilename": filename, "binaryCount": binaries}),
        )?;
    }
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn insert_finding(
    transaction: &Transaction<'_>,
    request: &InventoryRequest,
    content_object_id: Option<&str>,
    code: &str,
    severity: &str,
    message: &str,
    evidence: serde_json::Value,
) -> ProfilerResult<()> {
    let discriminator = evidence.to_string();
    let id = stable_id(&format!(
        "finding:{}:{code}:{}:{discriminator}",
        request.run_id,
        content_object_id.unwrap_or("collection")
    ));
    transaction
        .execute(
            "INSERT INTO findings(\
                id, run_id, content_object_id, code, severity, message, evidence_json, created_at\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(id) DO UPDATE SET severity=excluded.severity, message=excluded.message, \
                evidence_json=excluded.evidence_json",
            params![
                id,
                request.run_id,
                content_object_id,
                code,
                severity,
                message,
                evidence.to_string(),
                now_text(),
            ],
        )
        .map_err(|source| sqlite_error("recording inventory finding", source))?;
    Ok(())
}

fn inventory_summary(
    connection: &Connection,
    request: &InventoryRequest,
) -> ProfilerResult<InventorySummary> {
    Ok(InventorySummary {
        messages: count_where(
            connection,
            "SELECT COUNT(*) FROM source_messages WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        message_occurrences: count_where(
            connection,
            "SELECT COUNT(*) FROM source_message_occurrences WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        participants: count_where(
            connection,
            "SELECT COUNT(*) FROM source_participants WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        parts: count_where(
            connection,
            "SELECT COUNT(*) FROM source_parts WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        attachment_occurrences: count_where(
            connection,
            "SELECT COUNT(*) FROM source_parts WHERE snapshot_id=?1 AND role='attachment'",
            &request.source_snapshot_id,
        )?,
        blob_rows: count_where(
            connection,
            "SELECT COUNT(*) FROM source_blobs WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        content_objects: count_where(
            connection,
            "SELECT COUNT(*) FROM content_objects WHERE collection_id=?1",
            &request.collection_id,
        )?,
        content_occurrences: count_where(
            connection,
            "SELECT COUNT(*) FROM content_occurrences WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        message_relations: count_where(
            connection,
            "SELECT COUNT(*) FROM source_message_relations WHERE snapshot_id=?1",
            &request.source_snapshot_id,
        )?,
        zero_byte_content_objects: count_where(
            connection,
            "SELECT COUNT(*) FROM content_objects WHERE collection_id=?1 AND expected_size_bytes=0",
            &request.collection_id,
        )?,
        same_hash_different_names: count_where(
            connection,
            "SELECT COUNT(*) FROM (\
                SELECT content_object_id FROM filename_variants \
                WHERE content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1) \
                GROUP BY content_object_id HAVING COUNT(*) > 1\
             )",
            &request.collection_id,
        )?,
        same_name_different_hashes: count_where(
            connection,
            "SELECT COUNT(*) FROM (\
                SELECT normalized_filename FROM filename_variants \
                WHERE content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1) \
                GROUP BY normalized_filename HAVING COUNT(DISTINCT content_object_id) > 1\
             )",
            &request.collection_id,
        )?,
    })
}

fn verify_store_identity(connection: &Connection, existed: bool) -> ProfilerResult<()> {
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|source| sqlite_error("reading profiler application_id", source))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(ProfilerError::IncompatibleSource(format!(
            "workspace database has application_id {application_id}, expected {APPLICATION_ID}"
        )));
    }
    if existed && application_id == 0 {
        let user_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("checking profiler database ownership", source))?;
        if user_tables > 0 {
            return Err(ProfilerError::IncompatibleSource(
                "workspace database is not an initialized MailVault Profiler store".into(),
            ));
        }
    }
    Ok(())
}

fn configure_connection(connection: &Connection) -> ProfilerResult<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|source| sqlite_error("configuring busy timeout", source))?;
    connection
        .execute_batch(
            "PRAGMA foreign_keys=ON;\n\
             PRAGMA journal_mode=WAL;\n\
             PRAGMA synchronous=NORMAL;\n\
             PRAGMA temp_store=MEMORY;\n\
             PRAGMA trusted_schema=OFF;\n\
             PRAGMA wal_autocheckpoint=1000;",
        )
        .map_err(|source| sqlite_error("configuring profiler SQLite", source))?;
    Ok(())
}

fn read_user_version(connection: &Connection) -> ProfilerResult<i64> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|source| sqlite_error("reading profiler user_version", source))
}

fn verify_integrity(connection: &Connection) -> ProfilerResult<()> {
    let result: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|source| sqlite_error("checking profiler database integrity", source))?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(ProfilerError::contract(
            profiler_core::ErrorCode::WorkspaceDatabaseCorrupted,
            "profiler database integrity check failed",
            false,
        ))
    }
}

fn workspace_metadata_table_exists(connection: &Connection) -> ProfilerResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_meta')",
            [],
            |row| row.get(0),
        )
        .map_err(|source| sqlite_error("checking workspace metadata table", source))
}

fn set_existing_migration_state(connection: &Connection, state: &str) -> ProfilerResult<()> {
    if !workspace_metadata_table_exists(connection)? {
        return Ok(());
    }
    connection
        .execute(
            "UPDATE workspace_meta SET migration_state=?1 WHERE singleton_id=1",
            [state],
        )
        .map_err(|source| sqlite_error("updating workspace migration state", source))?;
    Ok(())
}

fn verify_workspace_ready(connection: &Connection) -> ProfilerResult<()> {
    if !workspace_metadata_table_exists(connection)? {
        return Ok(());
    }
    let state = connection
        .query_row(
            "SELECT migration_state FROM workspace_meta WHERE singleton_id=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|source| sqlite_error("reading workspace migration state", source))?
        .ok_or_else(|| {
            ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceInvalidLayout,
                "workspace metadata row is missing",
                false,
            )
        })?;
    if state == "ready" {
        Ok(())
    } else {
        Err(ProfilerError::contract(
            profiler_core::ErrorCode::WorkspaceMigrationFailed,
            format!("workspace migration state is {state}"),
            false,
        ))
    }
}

fn verify_required_workspace_tables(connection: &Connection) -> ProfilerResult<()> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name IN (
                'workspace_meta', 'finding_review_events', 'finding_review_state'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(|source| sqlite_error("checking workspace review tables", source))?;
    if count == 3 {
        Ok(())
    } else {
        Err(ProfilerError::contract(
            profiler_core::ErrorCode::WorkspaceInvalidLayout,
            "workspace review schema is incomplete",
            false,
        ))
    }
}

fn ensure_workspace_meta(connection: &Connection) -> ProfilerResult<()> {
    let table_exists = workspace_metadata_table_exists(connection)?;
    if !table_exists {
        return Ok(());
    }

    let now = now_text();
    connection
        .execute(
            "INSERT INTO workspace_meta(
                singleton_id, workspace_id, schema_version, created_at, created_by_version,
                last_migrated_at, last_migrated_by_version, migration_state
             ) VALUES(1, ?1, ?2, ?3, ?4, ?3, ?4, 'ready')
             ON CONFLICT(singleton_id) DO UPDATE SET
                schema_version=excluded.schema_version,
                last_migrated_at=excluded.last_migrated_at,
                last_migrated_by_version=excluded.last_migrated_by_version,
                migration_state='ready'",
            params![
                Uuid::now_v7().to_string(),
                CURRENT_USER_VERSION,
                now,
                env!("CARGO_PKG_VERSION")
            ],
        )
        .map_err(|source| sqlite_error("initializing workspace metadata", source))?;
    Ok(())
}

fn write_migration_failure_marker(path: &Path, error: &ProfilerError) -> ProfilerResult<()> {
    let payload = serde_json::json!({
        "format": 1,
        "failedAt": now_text(),
        "applicationVersion": env!("CARGO_PKG_VERSION"),
        "errorCode": error.report().code,
        "message": "Workspace migration failed. Review the retained backup and application logs."
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|source| {
        ProfilerError::Internal(format!("serializing migration failure marker: {source}"))
    })?;
    fs::write(path, bytes).map_err(|source| ProfilerError::Io {
        operation: "writing migration failure marker",
        path: path.to_path_buf(),
        source,
    })?;
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|source| ProfilerError::Io {
            operation: "opening migration failure marker for durable sync",
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| ProfilerError::Io {
        operation: "syncing migration failure marker",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn backup_before_migration(
    source: &Connection,
    database_path: &Path,
    from_schema: i64,
) -> ProfilerResult<PathBuf> {
    let workspace_root = database_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceInvalidLayout,
                "profiler database is not inside the expected workspace layout",
                false,
            )
        })?;
    let backup_directory = workspace_root.join("backups");
    fs::create_dir_all(&backup_directory).map_err(|source| ProfilerError::Io {
        operation: "creating workspace migration backup directory",
        path: backup_directory.clone(),
        source,
    })?;

    let base = backup_directory.join(format!(
        "profiler-before-workspace-schema-{from_schema}.sqlite3"
    ));
    let final_path = if base.exists() {
        let token = OffsetDateTime::now_utc().unix_timestamp_nanos();
        backup_directory.join(format!(
            "profiler-before-workspace-schema-{from_schema}-{token}.sqlite3"
        ))
    } else {
        base
    };
    let partial_path = final_path.with_extension("sqlite3.partial");
    if partial_path.exists() {
        fs::remove_file(&partial_path).map_err(|source| ProfilerError::Io {
            operation: "removing stale workspace migration backup",
            path: partial_path.clone(),
            source,
        })?;
    }

    let mut destination = Connection::open_with_flags(
        &partial_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|source| sqlite_error("opening workspace migration backup", source))?;
    {
        let backup = Backup::new(source, &mut destination)
            .map_err(|source| sqlite_error("initializing workspace migration backup", source))?;
        loop {
            match backup
                .step(256)
                .map_err(|source| sqlite_error("copying workspace migration backup", source))?
            {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    thread::sleep(Duration::from_millis(25));
                }
                _ => {
                    return Err(ProfilerError::contract(
                        profiler_core::ErrorCode::WorkspaceMigrationFailed,
                        "workspace migration backup returned an unsupported SQLite state",
                        true,
                    ));
                }
            }
        }
    }
    drop(destination);

    let mut backup_file = OpenOptions::new()
        .write(true)
        .open(&partial_path)
        .map_err(|source| ProfilerError::Io {
            operation: "opening completed workspace migration backup",
            path: partial_path.clone(),
            source,
        })?;
    backup_file.flush().map_err(|source| ProfilerError::Io {
        operation: "flushing workspace migration backup",
        path: partial_path.clone(),
        source,
    })?;
    backup_file.sync_all().map_err(|source| ProfilerError::Io {
        operation: "syncing workspace migration backup",
        path: partial_path.clone(),
        source,
    })?;
    drop(backup_file);
    fs::rename(&partial_path, &final_path).map_err(|source| ProfilerError::Io {
        operation: "publishing workspace migration backup",
        path: final_path.clone(),
        source,
    })?;
    Ok(final_path)
}

fn migrate(connection: &Connection) -> ProfilerResult<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (\n\
                version TEXT PRIMARY KEY,\n\
                applied_at TEXT NOT NULL\n\
             ) STRICT;",
        )
        .map_err(|source| sqlite_error("creating migration table", source))?;

    for migration in MIGRATIONS {
        let applied = connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version=?1",
                [migration.id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|source| sqlite_error("checking profiler migration", source))?
            .is_some();
        if applied {
            continue;
        }

        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|source| sqlite_error("starting profiler migration", source))?;
        let result = (|| {
            connection.execute_batch(migration.sql)?;
            connection.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES(?1, ?2)",
                params![migration.id, now_text()],
            )?;
            connection.pragma_update(None, "application_id", APPLICATION_ID)?;
            connection.pragma_update(None, "user_version", migration.user_version)?;
            Ok::<(), rusqlite::Error>(())
        })();

        match result {
            Ok(()) => connection
                .execute_batch("COMMIT")
                .map_err(|source| sqlite_error("committing profiler migration", source))?,
            Err(source) => {
                let _ = connection.execute_batch("ROLLBACK");
                return Err(sqlite_error("applying profiler migration", source));
            }
        }
    }

    verify_current_schema(connection)
}

fn verify_current_schema(connection: &Connection) -> ProfilerResult<()> {
    let user_version = read_user_version(connection)?;
    if user_version > CURRENT_USER_VERSION {
        return Err(ProfilerError::contract(
            profiler_core::ErrorCode::WorkspaceSchemaNewerThanApplication,
            format!(
                "workspace schema {user_version} is newer than supported schema {CURRENT_USER_VERSION}"
            ),
            false,
        ));
    }
    if user_version < CURRENT_USER_VERSION {
        return Err(ProfilerError::contract(
            profiler_core::ErrorCode::WorkspaceMigrationRequired,
            format!("workspace schema {user_version} must be migrated to {CURRENT_USER_VERSION}"),
            false,
        ));
    }
    Ok(())
}

fn parse_run_state(value: &str) -> ProfilerResult<RunState> {
    match value {
        "pending" => Ok(RunState::Pending),
        "preflighting" => Ok(RunState::Preflighting),
        "snapshotting" => Ok(RunState::Snapshotting),
        "ready" => Ok(RunState::Ready),
        "running" => Ok(RunState::Running),
        "pausing" => Ok(RunState::Pausing),
        "paused" => Ok(RunState::Paused),
        "cancelling" => Ok(RunState::Cancelling),
        "cancelled" => Ok(RunState::Cancelled),
        "succeeded" => Ok(RunState::Succeeded),
        "failed" => Ok(RunState::Failed),
        other => Err(ProfilerError::Internal(format!(
            "unknown persisted run state: {other}"
        ))),
    }
}

fn checkpoint_stage(table: InventoryTable) -> String {
    format!("metadata_inventory:{table}")
}

fn content_object_id(collection_id: &str, sha256: &str) -> String {
    stable_id(&format!("content:{collection_id}:{sha256}"))
}

fn stable_id(value: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, value.as_bytes()).to_string()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn normalize_filename(value: &str) -> Option<String> {
    let normalized = value
        .nfkc()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let whitespace_collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = whitespace_collapsed.trim_matches(|character| matches!(character, ' ' | '.'));
    (!trimmed.is_empty()).then(|| trimmed.to_lowercase())
}

fn valid_json_or_default(value: &str, fallback: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .map_or_else(|_| fallback.to_owned(), |_| value.to_owned())
}

fn count_where(connection: &Connection, sql: &str, value: &str) -> ProfilerResult<u64> {
    let count: i64 = connection
        .query_row(sql, [value], |row| row.get(0))
        .map_err(|source| sqlite_error("reading inventory summary", source))?;
    u64::try_from(count)
        .map_err(|_| ProfilerError::Internal("negative inventory count returned".into()))
}

pub(crate) fn to_u64(value: i64, field: &str) -> ProfilerResult<u64> {
    u64::try_from(value).map_err(|_| ProfilerError::Internal(format!("negative {field} returned")))
}

fn to_i64(value: u64, field: &str) -> ProfilerResult<i64> {
    i64::try_from(value).map_err(|_| {
        ProfilerError::Internal(format!("{field} exceeds SQLite signed integer capacity"))
    })
}

fn to_json<T: serde::Serialize>(value: &T, operation: &str) -> ProfilerResult<String> {
    serde_json::to_string(value)
        .map_err(|error| ProfilerError::Internal(format!("{operation}: {error}")))
}

fn now_text() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting is infallible for OffsetDateTime")
}

#[allow(clippy::needless_pass_by_value)]
fn sqlite_error(operation: &'static str, source: rusqlite::Error) -> ProfilerError {
    ProfilerError::Sqlite {
        operation,
        message: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrations_are_idempotent() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("profiler.sqlite3");
        drop(ProfilerStore::open(&path).unwrap());
        let second = ProfilerStore::open(&path).unwrap();
        let count: i64 = second
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 5);
        let user_version: i64 = second
            .connection()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(user_version, CURRENT_USER_VERSION);
    }

    #[test]
    fn read_only_store_requires_an_existing_profiler_database_and_rejects_writes() {
        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing.sqlite3");
        assert!(ProfilerStore::open_read_only(&missing).is_err());
        assert!(!missing.exists());

        let path = directory.path().join("profiler.sqlite3");
        drop(ProfilerStore::open(&path).unwrap());
        let read_only = ProfilerStore::open_read_only(&path).unwrap();
        let write_result = read_only.connection().execute(
            "INSERT INTO collections(                id, adapter_kind, archive_identity, archive_root_display, created_at, updated_at             ) VALUES('forbidden', 'test', 'test', 'test', 'now', 'now')",
            [],
        );
        assert!(write_result.is_err());
    }

    #[test]
    fn run_state_is_enforced_before_persistence() {
        let directory = tempdir().unwrap();
        let store = ProfilerStore::open(&directory.path().join("profiler.sqlite3")).unwrap();
        let run_id = store.create_run(None, "0.1", "test").unwrap();
        assert!(store.transition_run(&run_id, RunState::Succeeded).is_err());
        store
            .transition_run(&run_id, RunState::Preflighting)
            .unwrap();
    }

    #[test]
    fn filename_normalization_preserves_business_tokens() {
        assert_eq!(
            normalize_filename("  INV–PR–12–014–1 .PDF "),
            Some("inv–pr–12–014–1 .pdf".into())
        );
    }

    #[test]
    fn review_events_are_append_only_and_survive_reopen() {
        use profiler_core::{ReviewActorKind, ReviewStatus};

        let directory = tempdir().unwrap();
        let path = directory.path().join("profiler.sqlite3");
        let mut store = ProfilerStore::open(&path).unwrap();
        seed_review_fixture(&store, "run-1", "finding-1");
        let history = store
            .set_finding_review_status(
                "run-1",
                "finding-1",
                ReviewStatus::NeedsInvestigation,
                Some("Verify the physical object against the retained backup."),
                ReviewActorKind::LocalInteractiveUser,
                None,
            )
            .unwrap();
        assert_eq!(history.events.len(), 1);
        let event_id = history.events[0].event_id.clone();
        assert!(
            store
                .connection()
                .execute(
                    "UPDATE finding_review_events SET note='tampered' WHERE event_id=?1",
                    [&event_id],
                )
                .is_err()
        );
        assert!(
            store
                .connection()
                .execute(
                    "DELETE FROM finding_review_events WHERE event_id=?1",
                    [&event_id]
                )
                .is_err()
        );
        drop(store);

        let reopened = ProfilerStore::open_existing(&path).unwrap();
        let history = reopened
            .finding_review_history("run-1", "finding-1")
            .unwrap();
        assert_eq!(
            history.current_status,
            Some(ReviewStatus::NeedsInvestigation)
        );
        assert!(history.integrity_valid);
        assert_eq!(history.events.len(), 1);
    }

    #[test]
    fn tampered_review_history_fails_integrity_validation() {
        use profiler_core::{ReviewActorKind, ReviewStatus};

        let directory = tempdir().unwrap();
        let path = directory.path().join("profiler.sqlite3");
        let mut store = ProfilerStore::open(&path).unwrap();
        seed_review_fixture(&store, "run-1", "finding-1");
        store
            .set_finding_review_status(
                "run-1",
                "finding-1",
                ReviewStatus::Expected,
                Some("Known archive relationship."),
                ReviewActorKind::LocalInteractiveUser,
                None,
            )
            .unwrap();
        store
            .connection()
            .execute_batch("DROP TRIGGER finding_review_events_no_update;")
            .unwrap();
        store
            .connection()
            .execute(
                "UPDATE finding_review_events SET event_hash=?1 WHERE finding_id='finding-1'",
                ["ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"],
            )
            .unwrap();
        assert!(store.validate_all_review_history().is_err());
    }

    #[test]
    fn failed_projection_write_rolls_back_the_review_event() {
        use profiler_core::{ReviewActorKind, ReviewStatus};

        let directory = tempdir().unwrap();
        let path = directory.path().join("profiler.sqlite3");
        let mut store = ProfilerStore::open(&path).unwrap();
        seed_review_fixture(&store, "run-1", "finding-1");
        store
            .connection()
            .execute_batch(
                "CREATE TRIGGER reject_review_projection
                 BEFORE INSERT ON finding_review_state
                 BEGIN
                    SELECT RAISE(ABORT, 'projection rejected for test');
                 END;",
            )
            .unwrap();

        assert!(
            store
                .set_finding_review_status(
                    "run-1",
                    "finding-1",
                    ReviewStatus::Acknowledged,
                    None,
                    ReviewActorKind::LocalInteractiveUser,
                    None,
                )
                .is_err()
        );
        let event_count: i64 = store
            .connection()
            .query_row("SELECT COUNT(*) FROM finding_review_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        let state_count: i64 = store
            .connection()
            .query_row("SELECT COUNT(*) FROM finding_review_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(event_count, 0);
        assert_eq!(state_count, 0);
    }

    #[test]
    fn clearing_a_review_returns_the_finding_to_unreviewed_without_losing_history() {
        use profiler_core::{ReviewActorKind, ReviewStatus};

        let directory = tempdir().unwrap();
        let path = directory.path().join("profiler.sqlite3");
        let mut store = ProfilerStore::open(&path).unwrap();
        seed_review_fixture(&store, "run-1", "finding-1");
        store
            .set_finding_review_status(
                "run-1",
                "finding-1",
                ReviewStatus::Expected,
                Some("Known relationship."),
                ReviewActorKind::LocalInteractiveUser,
                None,
            )
            .unwrap();
        let history = store
            .clear_finding_review_status(
                "run-1",
                "finding-1",
                Some("Reopened for a future review cycle."),
                ReviewActorKind::LocalInteractiveUser,
                None,
            )
            .unwrap();

        assert_eq!(history.current_status, None);
        assert_eq!(history.events.len(), 2);
        assert_eq!(
            history.events[1].previous_status,
            Some(ReviewStatus::Expected)
        );
        assert_eq!(history.events[1].new_status, None);
        assert!(history.integrity_valid);
        let summary = store.review_summary("run-1").unwrap();
        assert_eq!(summary.unreviewed, 1);
        assert_eq!(summary.reviewed_findings, 0);
    }

    #[test]
    fn migration_from_schema_four_creates_backup_and_review_tables() {
        let directory = tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let profiler_directory = workspace.join("profiler");
        fs::create_dir_all(&profiler_directory).unwrap();
        let path = profiler_directory.join("profiler.sqlite3");
        let connection = Connection::open(&path).unwrap();
        configure_connection(&connection).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version TEXT PRIMARY KEY, applied_at TEXT NOT NULL) STRICT;",
            )
            .unwrap();
        for migration in &MIGRATIONS[..4] {
            connection.execute_batch(migration.sql).unwrap();
            connection
                .execute(
                    "INSERT INTO schema_migrations(version, applied_at) VALUES(?1, 'now')",
                    [migration.id],
                )
                .unwrap();
            connection
                .pragma_update(None, "application_id", APPLICATION_ID)
                .unwrap();
            connection
                .pragma_update(None, "user_version", migration.user_version)
                .unwrap();
        }
        drop(connection);

        let migrated = ProfilerStore::open_existing(&path).unwrap();
        let table_count: i64 = migrated
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('workspace_meta','finding_review_events','finding_review_state')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 3);
        assert!(
            workspace
                .join("backups/profiler-before-workspace-schema-4.sqlite3")
                .is_file()
        );
    }

    #[test]
    fn newer_workspace_schema_fails_closed() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("profiler.sqlite3");
        let store = ProfilerStore::open(&path).unwrap();
        store
            .connection()
            .pragma_update(None, "user_version", CURRENT_USER_VERSION + 1)
            .unwrap();
        drop(store);
        let error = ProfilerStore::open_existing(&path).unwrap_err();
        assert_eq!(
            error.report().code,
            profiler_core::ErrorCode::WorkspaceSchemaNewerThanApplication
        );
    }

    fn seed_review_fixture(store: &ProfilerStore, run_id: &str, finding_id: &str) {
        store
            .connection()
            .execute(
                "INSERT INTO profiler_runs(
                    id, state, pipeline_version, configuration_fingerprint, created_at, updated_at
                 ) VALUES(?1, 'succeeded', 'test', 'test', '2026-07-19T12:00:00Z', '2026-07-19T12:00:00Z')",
                [run_id],
            )
            .unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO findings(
                    id, run_id, code, severity, message, evidence_json, created_at
                 ) VALUES(?1, ?2, 'TEST_FINDING', 'warning', 'test finding', '{}', '2026-07-19T12:00:00Z')",
                (finding_id, run_id),
            )
            .unwrap();
    }
}
