use std::path::{Path, PathBuf};

use profiler_core::{
    ActiveRunContext, ErrorCode, InventorySummary, ProfilerError, ProfilerResult, RunCatalogEntry,
    RunCatalogStatus, WorkspaceAccessMode, WorkspaceDatabaseInspection, WorkspaceDescriptor,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use super::{APPLICATION_ID, CURRENT_USER_VERSION, ProfilerStore, now_text, sqlite_error, to_u64};

impl ProfilerStore {
    pub fn inspect_database(path: &Path) -> ProfilerResult<WorkspaceDatabaseInspection> {
        if !path.is_file() {
            return Err(ProfilerError::contract(
                ErrorCode::WorkspaceDatabaseMissing,
                "profiler database does not exist",
                false,
            ));
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|source| sqlite_error("inspecting profiler database", source))?;
        connection
            .execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")
            .map_err(|source| sqlite_error("hardening workspace inspection", source))?;
        let application_id: i64 = connection
            .pragma_query_value(None, "application_id", |row| row.get(0))
            .map_err(|source| sqlite_error("reading profiler application_id", source))?;
        let schema_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|source| sqlite_error("reading profiler user_version", source))?;
        let integrity: String = connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|source| sqlite_error("checking profiler database integrity", source))?;
        let has_meta: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_meta')",
                [],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("checking workspace metadata", source))?;
        let metadata = if has_meta {
            connection
                .query_row(
                    "SELECT migration_state, workspace_id, created_by_version, last_migrated_by_version
                     FROM workspace_meta WHERE singleton_id=1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(|source| sqlite_error("reading workspace metadata", source))?
        } else {
            None
        };
        Ok(WorkspaceDatabaseInspection {
            application_id,
            schema_version,
            integrity_ok: integrity.eq_ignore_ascii_case("ok"),
            migration_state: metadata.as_ref().map(|value| value.0.clone()),
            workspace_id: metadata.as_ref().map(|value| value.1.clone()),
            created_by_version: metadata.as_ref().map(|value| value.2.clone()),
            last_migrated_by_version: metadata.and_then(|value| value.3),
        })
    }

    pub fn inspect_source_archive_roots(path: &Path) -> ProfilerResult<Vec<PathBuf>> {
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|source| sqlite_error("inspecting workspace source paths", source))?;
        let has_collections: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='collections')",
                [],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("checking workspace collections table", source))?;
        if !has_collections {
            return Ok(Vec::new());
        }
        let mut statement = connection
            .prepare("SELECT DISTINCT archive_root_display FROM collections ORDER BY id")
            .map_err(|source| sqlite_error("preparing workspace source paths", source))?;
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| sqlite_error("querying workspace source paths", source))?
            .map(|row| row.map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting workspace source paths", source))
    }

    pub fn inspect_run_count(path: &Path) -> ProfilerResult<u64> {
        if !path.is_file() {
            return Ok(0);
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|source| sqlite_error("inspecting workspace runs", source))?;
        let has_runs: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='profiler_runs')",
                [],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("checking workspace run table", source))?;
        if !has_runs {
            return Ok(0);
        }
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM profiler_runs", [], |row| row.get(0))
            .map_err(|source| sqlite_error("counting inspected workspace runs", source))?;
        to_u64(count, "inspected workspace run count")
    }

    pub fn workspace_descriptor(
        &self,
        root_path: &Path,
        database_path: &Path,
        access_mode: WorkspaceAccessMode,
        review_integrity_valid: bool,
    ) -> ProfilerResult<WorkspaceDescriptor> {
        self.connection
            .query_row(
                "SELECT workspace_id, schema_version, created_at, created_by_version,
                        last_migrated_at, last_migrated_by_version
                 FROM workspace_meta WHERE singleton_id=1",
                [],
                |row| {
                    Ok(WorkspaceDescriptor {
                        workspace_id: row.get(0)?,
                        root_path: root_path.to_path_buf(),
                        profiler_database: database_path.to_path_buf(),
                        schema_version: row.get(1)?,
                        created_at: row.get(2)?,
                        created_by_version: row.get(3)?,
                        last_migrated_at: row.get(4)?,
                        last_migrated_by_version: row.get(5)?,
                        access_mode,
                        review_integrity_valid,
                    })
                },
            )
            .map_err(|source| sqlite_error("reading workspace descriptor", source))
    }

    pub fn run_count(&self) -> ProfilerResult<u64> {
        let value: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM profiler_runs", [], |row| row.get(0))
            .map_err(|source| sqlite_error("counting workspace runs", source))?;
        to_u64(value, "workspace run count")
    }

    pub fn source_archive_roots(&self) -> ProfilerResult<Vec<PathBuf>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT archive_root_display FROM collections ORDER BY id")
            .map_err(|source| sqlite_error("preparing source archive paths", source))?;
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| sqlite_error("querying source archive paths", source))?
            .map(|row| row.map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting source archive paths", source))
    }

    pub fn list_runs(&self) -> ProfilerResult<Vec<RunCatalogEntry>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id FROM profiler_runs
                 ORDER BY COALESCE(started_at, created_at) DESC, id DESC",
            )
            .map_err(|source| sqlite_error("preparing run catalog", source))?;
        let run_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| sqlite_error("querying run catalog", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting run catalog", source))?;
        run_ids
            .iter()
            .map(|run_id| self.run_catalog_entry(run_id))
            .collect()
    }

    pub fn run_catalog_entry(&self, run_id: &str) -> ProfilerResult<RunCatalogEntry> {
        let base = self
            .connection
            .query_row(
                "SELECT run.id, run.collection_id, run.source_snapshot_id, run.state,
                        COALESCE(run.started_at, run.created_at), run.finished_at,
                        run.pipeline_version, collection.archive_identity,
                        snapshot.source_schema_version,
                        COALESCE(CAST(json_extract(snapshot.snapshot_metrics_json, '$.messages') AS INTEGER), 0),
                        COALESCE(CAST(json_extract(snapshot.snapshot_metrics_json, '$.mimeParts') AS INTEGER), 0),
                        COALESCE(CAST(json_extract(snapshot.snapshot_metrics_json, '$.blobs') AS INTEGER), 0)
                 FROM profiler_runs AS run
                 LEFT JOIN collections AS collection ON collection.id=run.collection_id
                 LEFT JOIN source_snapshots AS snapshot ON snapshot.id=run.source_snapshot_id
                 WHERE run.id=?1",
                [run_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                    ))
                },
            )
            .optional()
            .map_err(|source| sqlite_error("reading run catalog entry", source))?
            .ok_or_else(|| {
                ProfilerError::contract(ErrorCode::RunNotFound, "profiler run was not found", false)
            })?;
        let findings = self.findings_summary(run_id)?;
        let review_summary = self.review_summary(run_id)?;
        Ok(RunCatalogEntry {
            run_id: base.0,
            collection_id: base.1,
            source_snapshot_id: base.2,
            status: catalog_status(&base.3),
            persisted_state: base.3,
            started_at: base.4,
            completed_at: base.5,
            app_version: base.6,
            archive_fingerprint: base.7,
            source_schema_version: base.8.and_then(|value| u32::try_from(value).ok()),
            messages: to_u64(base.9, "run message count")?,
            mime_parts: to_u64(base.10, "run MIME-part count")?,
            blobs: to_u64(base.11, "run blob count")?,
            findings: findings.total,
            errors: findings.errors,
            warnings: findings.warnings,
            review_summary,
        })
    }

    pub fn active_run_context(&self, run_id: &str) -> ProfilerResult<ActiveRunContext> {
        let run = self.run_catalog_entry(run_id)?;
        let collection_id = run.collection_id.clone().ok_or_else(|| {
            ProfilerError::contract(
                ErrorCode::RunNotBrowsable,
                "run is not associated with a collection",
                false,
            )
        })?;
        let source_snapshot_id = run.source_snapshot_id.clone().ok_or_else(|| {
            ProfilerError::contract(
                ErrorCode::RunNotBrowsable,
                "run has no completed source snapshot",
                false,
            )
        })?;
        let inventory = self.run_inventory_summary(&collection_id, &source_snapshot_id)?;
        let findings = self.findings_summary(run_id)?;
        Ok(ActiveRunContext {
            run,
            collection_id,
            source_snapshot_id,
            inventory,
            findings,
        })
    }

    pub fn run_inventory_summary(
        &self,
        collection_id: &str,
        snapshot_id: &str,
    ) -> ProfilerResult<InventorySummary> {
        Ok(InventorySummary {
            messages: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM source_messages WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            message_occurrences: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM source_message_occurrences WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            participants: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM source_participants WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            parts: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM source_parts WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            attachment_occurrences: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM content_occurrences WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            blob_rows: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM source_blobs WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            content_objects: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM content_objects WHERE collection_id=?1",
                collection_id,
            )?,
            content_occurrences: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM content_occurrences WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            message_relations: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM source_message_relations WHERE snapshot_id=?1",
                snapshot_id,
            )?,
            zero_byte_content_objects: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM content_objects WHERE collection_id=?1 AND expected_size_bytes=0",
                collection_id,
            )?,
            same_hash_different_names: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM (
                    SELECT content_object_id FROM filename_variants
                    WHERE content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1)
                    GROUP BY content_object_id HAVING COUNT(*) > 1
                 )",
                collection_id,
            )?,
            same_name_different_hashes: count_value(
                &self.connection,
                "SELECT COUNT(*) FROM (
                    SELECT normalized_filename FROM filename_variants
                    WHERE content_object_id IN (SELECT id FROM content_objects WHERE collection_id=?1)
                    GROUP BY normalized_filename HAVING COUNT(DISTINCT content_object_id) > 1
                 )",
                collection_id,
            )?,
        })
    }

    pub fn mark_migration_state(&self, state: &str) -> ProfilerResult<()> {
        if !matches!(state, "ready" | "migrating" | "failed") {
            return Err(ProfilerError::InvalidArgument(
                "unsupported workspace migration state".into(),
            ));
        }
        self.connection
            .execute(
                "UPDATE workspace_meta SET migration_state=?1, last_migrated_at=?2,
                        last_migrated_by_version=?3 WHERE singleton_id=1",
                params![state, now_text(), env!("CARGO_PKG_VERSION")],
            )
            .map_err(|source| sqlite_error("updating workspace migration state", source))?;
        Ok(())
    }
}

pub const fn current_workspace_schema() -> i64 {
    CURRENT_USER_VERSION
}

pub const fn expected_application_id() -> i64 {
    APPLICATION_ID
}

fn count_value(connection: &Connection, sql: &str, value: &str) -> ProfilerResult<u64> {
    let count: i64 = connection
        .query_row(sql, [value], |row| row.get(0))
        .map_err(|source| sqlite_error("reading run inventory summary", source))?;
    to_u64(count, "run inventory count")
}

fn catalog_status(state: &str) -> RunCatalogStatus {
    match state {
        "succeeded" => RunCatalogStatus::Completed,
        "failed" => RunCatalogStatus::Failed,
        "cancelled" => RunCatalogStatus::Cancelled,
        "pending" | "preflighting" | "snapshotting" | "ready" | "running" | "pausing"
        | "paused" | "cancelling" => RunCatalogStatus::Interrupted,
        _ => RunCatalogStatus::Unknown,
    }
}
