use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use profiler_adapter_mailvault::MailVaultAdapter;
use profiler_core::{
    CollectionAdapter, FormatIdentificationRequest, FormatOptions, PreflightReport, ProfilerError,
    ProfilerResult, ProgressSink, RunState,
};
use profiler_storage_sqlite::ProfilerStore;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ExactFormatEngine, ProfileEngine, ProfileOptions, ProfileRequest, ProfileResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuousProfileOptions {
    pub profile: ProfileOptions,
    pub format: FormatOptions,
    pub format_required: bool,
    pub rebuild: bool,
    pub retain_successful_snapshots: u32,
}

impl Default for ContinuousProfileOptions {
    fn default() -> Self {
        Self {
            profile: ProfileOptions::default(),
            format: FormatOptions::default(),
            format_required: true,
            rebuild: false,
            retain_successful_snapshots: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContinuousProfileRequest {
    pub archive_root: PathBuf,
    pub workspace_root: PathBuf,
    pub siegfried_path: Option<PathBuf>,
    pub signature_path: Option<PathBuf>,
    pub options: ContinuousProfileOptions,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ContinuousProfileEngine;

impl ContinuousProfileEngine {
    pub fn profile(
        &self,
        request: &ContinuousProfileRequest,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<ProfileResult> {
        validate_request(request)?;
        fs::create_dir_all(&request.workspace_root).map_err(|source| ProfilerError::Io {
            operation: "creating continuous profiler workspace",
            path: request.workspace_root.clone(),
            source,
        })?;
        let _lock = ProfileLock::acquire(&request.workspace_root)?;

        let adapter = MailVaultAdapter;
        let preflight = adapter.preflight(&request.archive_root)?;
        if !preflight.compatible {
            return Err(ProfilerError::IncompatibleSource(format!(
                "preflight failed with {} required errors",
                preflight.errors_count
            )));
        }
        let archive_identity = preflight.archive_identity.as_deref().ok_or_else(|| {
            ProfilerError::IncompatibleSource(
                "preflight did not produce an archive identity".into(),
            )
        })?;

        let database = request
            .workspace_root
            .join("profiler")
            .join("profiler.sqlite3");
        let mut store = ProfilerStore::open(&database)?;
        let collection_id = store.register_continuous_collection(
            adapter.kind(),
            archive_identity,
            &preflight.archive_identity_aliases,
            &preflight.archive_root,
        )?;
        let configuration_fingerprint = semantic_configuration_fingerprint(request)?;
        let source_revision = source_revision_fingerprint(&preflight)?;
        let registration = begin_or_resume_run(
            &store,
            &collection_id,
            &configuration_fingerprint,
            &source_revision,
            request.options.rebuild,
        )?;
        let run_id = registration.run_id.clone();

        let physical = ProfileEngine::profile_registered_run(
            &ProfileRequest {
                archive_root: request.archive_root.clone(),
                workspace_root: request.workspace_root.clone(),
                options: request.options.profile.clone(),
            },
            progress,
            adapter,
            &preflight,
            &mut store,
            &run_id,
            &collection_id,
            &database,
            &configuration_fingerprint,
            false,
        );
        let mut result = match physical {
            Ok(result) => result,
            Err(error) => {
                mark_failure(&store, &run_id, &error)?;
                return Err(error);
            }
        };

        if request.options.format_required {
            let format = ExactFormatEngine.identify(
                &FormatIdentificationRequest {
                    baseline_run_id: run_id.clone(),
                    workspace_root: request.workspace_root.clone(),
                    profiler_database: database.clone(),
                    archive_root: request.archive_root.clone(),
                    siegfried_path: request.siegfried_path.clone(),
                    signature_path: request.signature_path.clone(),
                    options: request.options.format,
                },
                progress,
            );
            match format {
                Ok(format) => result.format = Some(format),
                Err(error) => {
                    mark_failure(&store, &run_id, &error)?;
                    return Err(error);
                }
            }
        }

        store.transition_run(&run_id, RunState::Succeeded)?;
        result.run_kind = registration.run_kind;
        result.generation = registration.generation;
        result.resumed = registration.resumed;
        result.resume_count = registration.resume_count;
        result.source_revision_fingerprint = source_revision;
        result.snapshots_deleted = prune_snapshots(
            &store,
            &request.workspace_root,
            &collection_id,
            request.options.retain_successful_snapshots.max(1),
        )?;
        Ok(result)
    }
}

#[derive(Debug)]
struct RunRegistration {
    run_id: String,
    run_kind: String,
    generation: u64,
    resumed: bool,
    resume_count: u64,
}

pub fn profile_needed(workspace: &Path, source_revision: &str) -> ProfilerResult<bool> {
    let database = workspace.join("profiler").join("profiler.sqlite3");
    if !database.is_file() {
        return Ok(true);
    }
    let store = ProfilerStore::open_existing(&database)?;
    let connection = store.connection();
    let resumable: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM profiler_runs
             WHERE state IN ('pending','preflighting','snapshotting','ready','running',
                             'pausing','paused','cancelling')
                OR (state='failed' AND failure_retryable=1))",
            [],
            |row| row.get(0),
        )
        .map_err(|source| sqlite_error("checking resumable profile work", &source))?;
    if resumable {
        return Ok(true);
    }
    let latest: Option<String> = connection
        .query_row(
            "SELECT source_revision_fingerprint FROM profiler_runs
             WHERE state='succeeded' AND source_revision_fingerprint IS NOT NULL
             ORDER BY generation DESC, created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|source| sqlite_error("loading latest successful source revision", &source))?;
    Ok(latest.as_deref() != Some(source_revision))
}

fn begin_or_resume_run(
    store: &ProfilerStore,
    collection_id: &str,
    configuration_fingerprint: &str,
    source_revision_fingerprint: &str,
    rebuild: bool,
) -> ProfilerResult<RunRegistration> {
    let connection = store.connection();
    let candidate = connection
        .query_row(
            "SELECT id, run_kind, generation, resume_count FROM profiler_runs
             WHERE collection_id=?1 AND configuration_fingerprint=?2
               AND (state IN ('pending','preflighting','snapshotting','ready','running',
                              'pausing','paused','cancelling')
                    OR (state='failed' AND failure_retryable=1))
             ORDER BY generation DESC, created_at DESC LIMIT 1",
            rusqlite::params![collection_id, configuration_fingerprint],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|source| sqlite_error("finding resumable profile run", &source))?;

    if let Some((run_id, run_kind, generation, resume_count)) = candidate {
        if rebuild {
            return Err(ProfilerError::InvalidArgument(
                "a resumable run exists; resume or remove it before a rebuild".into(),
            ));
        }
        let now = now_text();
        connection
            .execute(
                "UPDATE profiler_runs SET state='pending', resume_count=resume_count+1,
                    last_resumed_at=?1, last_heartbeat_at=?1, failure_code=NULL,
                    failure_message=NULL, failure_retryable=0, finished_at=NULL,
                    source_revision_fingerprint=?2, updated_at=?1 WHERE id=?3",
                rusqlite::params![now, source_revision_fingerprint, run_id],
            )
            .map_err(|source| sqlite_error("resuming profile run", &source))?;
        return Ok(RunRegistration {
            run_id,
            run_kind,
            generation: to_u64(generation)?,
            resumed: true,
            resume_count: to_u64(resume_count + 1)?,
        });
    }

    let previous = connection
        .query_row(
            "SELECT id, generation FROM profiler_runs
             WHERE collection_id=?1 AND state='succeeded'
             ORDER BY generation DESC, created_at DESC LIMIT 1",
            [collection_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|source| sqlite_error("loading previous profile run", &source))?;
    let (parent_run_id, generation, run_kind) = match previous {
        Some((id, generation)) => (
            Some(id),
            generation + 1,
            if rebuild { "rebuild" } else { "incremental" },
        ),
        None => (None, 1, if rebuild { "rebuild" } else { "initial" }),
    };
    let run_id = Uuid::now_v7().to_string();
    let now = now_text();
    connection
        .execute(
            "INSERT INTO profiler_runs(
                id, collection_id, state, pipeline_version, configuration_fingerprint,
                created_at, updated_at, run_kind, parent_run_id, generation,
                last_heartbeat_at, source_revision_fingerprint
             ) VALUES(?1,?2,'pending',?3,?4,?5,?5,?6,?7,?8,?5,?9)",
            rusqlite::params![
                run_id,
                collection_id,
                env!("CARGO_PKG_VERSION"),
                configuration_fingerprint,
                now,
                run_kind,
                parent_run_id,
                generation,
                source_revision_fingerprint,
            ],
        )
        .map_err(|source| sqlite_error("creating continuous profile run", &source))?;
    Ok(RunRegistration {
        run_id,
        run_kind: run_kind.into(),
        generation: to_u64(generation)?,
        resumed: false,
        resume_count: 0,
    })
}

fn mark_failure(store: &ProfilerStore, run_id: &str, error: &ProfilerError) -> ProfilerResult<()> {
    let report = error.report();
    let now = now_text();
    store
        .connection()
        .execute(
            "UPDATE profiler_runs SET state='failed', failure_code=?1, failure_message=?2,
                failure_retryable=?3, finished_at=?4, updated_at=?4
             WHERE id=?5 AND state NOT IN ('cancelled','succeeded')",
            rusqlite::params![
                format!("{:?}", report.code).to_ascii_lowercase(),
                report.message,
                i64::from(report.retryable),
                now,
                run_id,
            ],
        )
        .map_err(|source| sqlite_error("recording continuous profile failure", &source))?;
    Ok(())
}

pub fn source_revision_fingerprint(preflight: &PreflightReport) -> ProfilerResult<String> {
    let mut digest = Sha256::new();
    digest.update(b"mailvault-source-revision-v3\0");

    // SQLite's shared-memory file is coordination state, not archive content.
    // Read-only connections may update its metadata/read marks, so including
    // `-shm` makes an unchanged WAL archive appear revised on every poll.
    // Database and WAL metadata plus stable archive metrics form the watch
    // signal; the consistent snapshot remains the authoritative content proof.
    for path in [
        PathBuf::from(&preflight.database_path),
        PathBuf::from(format!("{}-wal", preflight.database_path)),
    ] {
        digest.update(path.to_string_lossy().as_bytes());
        match fs::metadata(&path) {
            Ok(metadata) => {
                digest.update(metadata.len().to_be_bytes());
                if let Ok(modified) = metadata.modified()
                    && let Ok(delta) = modified.duration_since(std::time::UNIX_EPOCH)
                {
                    digest.update(delta.as_nanos().to_be_bytes());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                digest.update(b"missing");
            }
            Err(source) => {
                return Err(ProfilerError::Io {
                    operation: "reading MailVault revision metadata",
                    path,
                    source,
                });
            }
        }
    }
    let metrics = serde_json::to_vec(&preflight.metrics).map_err(|error| {
        ProfilerError::Internal(format!("serializing source revision metrics: {error}"))
    })?;
    digest.update(metrics);
    Ok(hex::encode(digest.finalize()))
}

fn semantic_configuration_fingerprint(
    request: &ContinuousProfileRequest,
) -> ProfilerResult<String> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "contract": "mailvault-continuous-profile-v6",
        "formatRequired": request.options.format_required,
        "containerExpansion": false,
        "contentIdentity": "sha256",
        "metadataReconciliation": "full-consistent-snapshot",
    }))
    .map_err(|error| ProfilerError::Internal(format!("serializing profile contract: {error}")))?;
    let mut digest = Sha256::new();
    digest.update(b"mailvault-continuous-profile-v6\0");
    digest.update(payload);
    Ok(hex::encode(digest.finalize()))
}

fn prune_snapshots(
    store: &ProfilerStore,
    workspace: &Path,
    collection_id: &str,
    keep: u32,
) -> ProfilerResult<u64> {
    let connection = store.connection();
    let root = workspace.join("snapshots");
    let candidates = snapshot_retention_candidates(connection, collection_id, keep)?;
    let mut deleted = delete_snapshot_candidates(connection, &root, collection_id, candidates)?;
    let referenced = referenced_snapshot_directories(connection, collection_id)?;
    deleted = deleted.saturating_add(delete_orphan_snapshot_directories(&root, &referenced)?);
    Ok(deleted)
}

fn snapshot_retention_candidates(
    connection: &rusqlite::Connection,
    collection_id: &str,
    keep: u32,
) -> ProfilerResult<Vec<(String, String)>> {
    let mut statement = connection
        .prepare(
            "SELECT snapshot.id, snapshot.snapshot_database_path
             FROM source_snapshots AS snapshot
             WHERE snapshot.collection_id=?1
               AND snapshot.retention_state<>'pinned'
               AND snapshot.id NOT IN (
                   SELECT source_snapshot_id FROM profiler_runs
                   WHERE collection_id=?1 AND source_snapshot_id IS NOT NULL
                     AND state NOT IN ('succeeded','cancelled')
               )
               AND snapshot.id NOT IN (
                   SELECT source_snapshot_id FROM profiler_runs
                   WHERE collection_id=?1 AND state='succeeded'
                     AND source_snapshot_id IS NOT NULL
                   ORDER BY generation DESC LIMIT ?2
               )",
        )
        .map_err(|source| sqlite_error("preparing snapshot retention", &source))?;
    let rows = statement
        .query_map(rusqlite::params![collection_id, i64::from(keep)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|source| sqlite_error("querying snapshot retention", &source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting snapshot retention", &source))?;
    Ok(rows)
}

fn delete_snapshot_candidates(
    connection: &rusqlite::Connection,
    root: &Path,
    collection_id: &str,
    candidates: Vec<(String, String)>,
) -> ProfilerResult<u64> {
    let mut deleted = 0_u64;
    for (snapshot_id, database_path) in candidates {
        delete_snapshot_directory(root, &database_path)?;
        connection
            .execute("DELETE FROM source_snapshots WHERE id=?1", [&snapshot_id])
            .map_err(|source| sqlite_error("deleting expired source snapshot", &source))?;
        connection
            .execute(
                "INSERT INTO snapshot_retention_events(
                    id, snapshot_id, collection_id, action, reason, detail_json, occurred_at
                 ) VALUES(?1,?2,?3,'deleted','retention_limit','{}',?4)",
                rusqlite::params![
                    Uuid::now_v7().to_string(),
                    snapshot_id,
                    collection_id,
                    now_text(),
                ],
            )
            .map_err(|source| sqlite_error("recording snapshot retention", &source))?;
        deleted = deleted.saturating_add(1);
    }
    Ok(deleted)
}

fn delete_snapshot_directory(root: &Path, database_path: &str) -> ProfilerResult<()> {
    let Some(directory) = PathBuf::from(database_path).parent().map(Path::to_path_buf) else {
        return Ok(());
    };
    if !directory.starts_with(root) {
        return Err(ProfilerError::InvalidPath {
            message: "snapshot retention candidate escapes the workspace".into(),
            path: directory,
        });
    }
    if directory.exists() {
        fs::remove_dir_all(&directory).map_err(|source| ProfilerError::Io {
            operation: "deleting expired profiler snapshot",
            path: directory,
            source,
        })?;
    }
    Ok(())
}

fn referenced_snapshot_directories(
    connection: &rusqlite::Connection,
    collection_id: &str,
) -> ProfilerResult<BTreeSet<PathBuf>> {
    let mut statement = connection
        .prepare("SELECT snapshot_database_path FROM source_snapshots WHERE collection_id=?1")
        .map_err(|source| sqlite_error("preparing referenced snapshot query", &source))?;
    let paths = statement
        .query_map([collection_id], |row| row.get::<_, String>(0))
        .map_err(|source| sqlite_error("loading referenced snapshots", &source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting referenced snapshots", &source))?;
    Ok(paths
        .into_iter()
        .filter_map(|path| PathBuf::from(path).parent().map(Path::to_path_buf))
        .collect())
}

fn delete_orphan_snapshot_directories(
    root: &Path,
    referenced: &BTreeSet<PathBuf>,
) -> ProfilerResult<u64> {
    if !root.is_dir() {
        return Ok(0);
    }
    let mut deleted = 0_u64;
    for entry in fs::read_dir(root).map_err(|source| ProfilerError::Io {
        operation: "scanning snapshot directory",
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ProfilerError::Io {
            operation: "reading snapshot directory entry",
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) && !referenced.contains(&path) {
            fs::remove_dir_all(&path).map_err(|source| ProfilerError::Io {
                operation: "deleting orphan profiler snapshot",
                path: path.clone(),
                source,
            })?;
            deleted = deleted.saturating_add(1);
        }
    }
    Ok(deleted)
}

fn validate_request(request: &ContinuousProfileRequest) -> ProfilerResult<()> {
    if !request.archive_root.is_dir() {
        return Err(ProfilerError::InvalidPath {
            message: "MailVault archive root is not a directory".into(),
            path: request.archive_root.clone(),
        });
    }
    if request.workspace_root.starts_with(&request.archive_root)
        || request.archive_root.starts_with(&request.workspace_root)
    {
        return Err(ProfilerError::InvalidArgument(
            "profiler workspace and MailVault archive must not overlap".into(),
        ));
    }
    if request.options.retain_successful_snapshots == 0 {
        return Err(ProfilerError::InvalidArgument(
            "at least one successful snapshot must be retained".into(),
        ));
    }
    Ok(())
}

struct ProfileLock {
    file: File,
}

impl ProfileLock {
    fn acquire(workspace: &Path) -> ProfilerResult<Self> {
        let path = workspace.join(".mailvault-profiler.profile.lock");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| ProfilerError::Io {
                operation: "opening continuous profile lock",
                path: path.clone(),
                source,
            })?;
        file.try_lock_exclusive().map_err(|_| {
            ProfilerError::SourceBusy("another profile writer owns this workspace".into())
        })?;
        file.set_len(0).map_err(|source| ProfilerError::Io {
            operation: "resetting continuous profile lock",
            path: path.clone(),
            source,
        })?;
        writeln!(
            file,
            "pid={} version={}",
            std::process::id(),
            env!("CARGO_PKG_VERSION")
        )
        .map_err(|source| ProfilerError::Io {
            operation: "writing continuous profile lock",
            path,
            source,
        })?;
        Ok(Self { file })
    }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn sqlite_error(operation: &'static str, source: &rusqlite::Error) -> ProfilerError {
    ProfilerError::Sqlite {
        operation,
        message: source.to_string(),
    }
}

fn now_text() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn to_u64(value: i64) -> ProfilerResult<u64> {
    u64::try_from(value)
        .map_err(|_| ProfilerError::Internal("negative continuous-profile counter".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use profiler_core::{ArchiveMetrics, LockState};
    use time::OffsetDateTime;

    fn preflight_report(database: &Path) -> PreflightReport {
        PreflightReport {
            adapter: "mailvault".into(),
            compatible: true,
            archive_root: database
                .parent()
                .expect("database parent")
                .to_string_lossy()
                .into_owned(),
            database_path: database.to_string_lossy().into_owned(),
            database_bytes: fs::metadata(database).expect("database metadata").len(),
            archive_identity: Some("archive-id".into()),
            archive_identity_aliases: Vec::new(),
            schema_version: Some(3),
            journal_mode: Some("wal".into()),
            lock_state: LockState::Absent,
            metrics: ArchiveMetrics {
                accounts: 1,
                messages: 2,
                message_occurrences: 2,
                mime_parts: 3,
                attachment_occurrences: 1,
                blobs: 1,
                blob_bytes: 4,
                message_relations: 0,
                participants: 2,
            },
            checks: Vec::new(),
            warnings_count: 0,
            errors_count: 0,
            inspected_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn source_revision_ignores_sqlite_shared_memory_churn() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = directory.path().join("mailvault.sqlite3");
        let wal = PathBuf::from(format!("{}-wal", database.to_string_lossy()));
        let shm = PathBuf::from(format!("{}-shm", database.to_string_lossy()));

        fs::write(&database, b"database").expect("write database");
        fs::write(&wal, b"wal").expect("write wal");
        fs::write(&shm, b"shared-memory-a").expect("write shm");

        let report = preflight_report(&database);
        let before = source_revision_fingerprint(&report).expect("fingerprint before");

        fs::write(&shm, b"shared-memory-b-with-different-size").expect("rewrite shm");
        let after = source_revision_fingerprint(&report).expect("fingerprint after");

        assert_eq!(before, after);
    }

    #[test]
    fn source_revision_changes_when_wal_changes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = directory.path().join("mailvault.sqlite3");
        let wal = PathBuf::from(format!("{}-wal", database.to_string_lossy()));

        fs::write(&database, b"database").expect("write database");
        fs::write(&wal, b"wal-a").expect("write wal");

        let report = preflight_report(&database);
        let before = source_revision_fingerprint(&report).expect("fingerprint before");

        fs::write(&wal, b"wal-b-with-different-size").expect("rewrite wal");
        let after = source_revision_fingerprint(&report).expect("fingerprint after");

        assert_ne!(before, after);
    }

    #[test]
    fn source_revision_changes_when_logical_metrics_change() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = directory.path().join("mailvault.sqlite3");

        fs::write(&database, b"database").expect("write database");

        let before_report = preflight_report(&database);
        let mut after_report = before_report.clone();
        after_report.metrics.messages += 1;

        let before = source_revision_fingerprint(&before_report).expect("fingerprint before");
        let after = source_revision_fingerprint(&after_report).expect("fingerprint after");

        assert_ne!(before, after);
    }
}
