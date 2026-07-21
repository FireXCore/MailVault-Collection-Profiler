use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use fs2::available_space;
use profiler_core::{
    LockState, ProfilerError, ProfilerResult, ProgressEvent, ProgressSink, ProgressUnit, RunStage,
    SnapshotRequest, SnapshotResult, SourceSnapshotManifest, StageState,
};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tracing::info;

use crate::{
    MailVaultAdapter,
    layout::MailVaultLayout,
    preflight::{
        open_read_only, read_archive_identity_value, read_metrics, read_schema_version_value,
        run_preflight,
    },
};

#[allow(clippy::too_many_lines)]
pub(crate) fn create_snapshot(
    request: &SnapshotRequest,
    sink: &dyn ProgressSink,
) -> ProfilerResult<SnapshotResult> {
    validate_request(request)?;
    let preflight = run_preflight(&request.archive_root)?;
    if !preflight.compatible {
        return Err(ProfilerError::IncompatibleSource(format!(
            "preflight failed with {} required errors",
            preflight.errors_count
        )));
    }
    if matches!(
        preflight.lock_state,
        LockState::Active | LockState::Indeterminate
    ) {
        return Err(ProfilerError::SourceBusy(format!(
            "writer lock state is {:?}",
            preflight.lock_state
        )));
    }

    let layout = MailVaultLayout::inspect(&request.archive_root)?;
    ensure_workspace_outside_source(&layout.root, &request.workspace_root)?;
    let source_database_bytes = fs::metadata(&layout.database)
        .map_err(|source| ProfilerError::Io {
            operation: "reading MailVault database size",
            path: layout.database.clone(),
            source,
        })?
        .len();
    let required_space = source_database_bytes
        .saturating_mul(2)
        .saturating_add(64 * 1024 * 1024);
    let available =
        available_space(&request.workspace_root).map_err(|source| ProfilerError::Io {
            operation: "checking profiler workspace free space",
            path: request.workspace_root.clone(),
            source,
        })?;
    if available < required_space {
        return Err(ProfilerError::InsufficientSpace {
            required_bytes: required_space,
            available_bytes: available,
        });
    }

    let snapshot_directory = request
        .workspace_root
        .join("snapshots")
        .join(&request.run_id);
    fs::create_dir_all(&snapshot_directory).map_err(|source| ProfilerError::Io {
        operation: "creating snapshot directory",
        path: snapshot_directory.clone(),
        source,
    })?;
    let snapshot_database = snapshot_directory.join("mailvault.sqlite3");
    let partial_database = snapshot_directory.join("mailvault.sqlite3.partial");
    let manifest_path = snapshot_directory.join("snapshot-manifest.json");
    let archive_identity = preflight.archive_identity.clone().ok_or_else(|| {
        ProfilerError::IncompatibleSource("preflight did not produce an archive identity".into())
    })?;

    if snapshot_database.exists() {
        return recover_or_reuse_snapshot(
            request,
            &layout,
            &snapshot_database,
            &manifest_path,
            &archive_identity,
            &preflight.metrics,
            sink,
        );
    }
    if manifest_path.exists() {
        return Err(ProfilerError::InvalidPath {
            message: "snapshot manifest exists but the snapshot database is missing".into(),
            path: manifest_path,
        });
    }
    remove_if_exists(&partial_database)?;

    let source = open_read_only(&layout.database)?;
    let source_metrics_before = read_metrics(&source)?;
    let schema_version = read_schema_version_value(&source)?;
    let source_archive_identity = read_archive_identity_value(&source)?;
    if source_archive_identity != archive_identity {
        return Err(ProfilerError::SourceChanged);
    }
    let mut destination = Connection::open_with_flags(
        &partial_database,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|source| sqlite_error("opening snapshot destination", source))?;
    destination
        .busy_timeout(Duration::from_secs(5))
        .map_err(|source| sqlite_error("configuring snapshot destination", source))?;

    let page_size: u64 = source
        .pragma_query_value(None, "page_size", |row| row.get::<_, i64>(0))
        .map_err(|source| sqlite_error("reading source SQLite page size", source))
        .and_then(|value| {
            u64::try_from(value).map_err(|_| {
                ProfilerError::IncompatibleSource("SQLite page_size cannot be negative".into())
            })
        })?;
    let started = Instant::now();
    let mut sequence = 0_u64;
    let backup = Backup::new(&source, &mut destination)
        .map_err(|source| sqlite_error("initializing SQLite online backup", source))?;
    let deadline = started + Duration::from_millis(request.options.busy_timeout_ms);

    loop {
        let outcome = backup
            .step(request.options.pages_per_step)
            .map_err(|source| sqlite_error("copying SQLite snapshot pages", source))?;
        let state = backup.progress();
        sequence += 1;
        let page_count = u64::try_from(state.pagecount.max(0)).unwrap_or_default();
        let remaining = u64::try_from(state.remaining.max(0)).unwrap_or_default();
        let completed = page_count.saturating_sub(remaining);
        sink.send(ProgressEvent {
            run_id: request.run_id.clone(),
            sequence,
            stage: RunStage::SourceSnapshot,
            stage_state: StageState::Running,
            unit: ProgressUnit::Pages,
            completed_items: completed,
            total_items: (page_count > 0).then_some(page_count),
            completed_bytes: completed.saturating_mul(page_size),
            total_bytes: (page_count > 0).then_some(page_count.saturating_mul(page_size)),
            elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            instant_throughput: None,
            smoothed_throughput: None,
            eta_ms: None,
            active_workers: 1,
            queue_depth: 0,
            warnings: 0,
            errors: 0,
            current_object_display: Some("mailvault.sqlite3".into()),
            checkpoint_sequence: sequence,
        })?;

        match outcome {
            StepResult::Done => break,
            StepResult::More => {}
            StepResult::Busy | StepResult::Locked => {
                if Instant::now() >= deadline {
                    return Err(ProfilerError::SourceBusy(
                        "SQLite source remained busy beyond the snapshot timeout".into(),
                    ));
                }
                thread::sleep(Duration::from_millis(request.options.busy_retry_ms));
            }
            _ => {
                return Err(ProfilerError::Internal(
                    "SQLite backup returned an unknown future step result".into(),
                ));
            }
        }
    }
    drop(backup);
    drop(destination);
    sync_completed_file(&partial_database, "syncing completed source snapshot")?;

    let source_metrics_after = read_metrics(&source)?;
    if source_metrics_before != source_metrics_after {
        remove_if_exists(&partial_database)?;
        return Err(ProfilerError::SourceChanged);
    }
    drop(source);

    let snapshot = open_read_only(&partial_database)?;
    let quick_check: String = snapshot
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|source| sqlite_error("validating source snapshot", source))?;
    if quick_check != "ok" {
        return Err(ProfilerError::IncompatibleSource(format!(
            "snapshot quick_check failed: {quick_check}"
        )));
    }
    let snapshot_metrics = read_metrics(&snapshot)?;
    if snapshot_metrics != source_metrics_before {
        return Err(ProfilerError::SourceChanged);
    }
    drop(snapshot);

    fs::rename(&partial_database, &snapshot_database).map_err(|source| ProfilerError::Io {
        operation: "publishing source snapshot",
        path: snapshot_database.clone(),
        source,
    })?;

    let snapshot_sha256 = sha256_file(&snapshot_database)?;
    let snapshot_bytes = fs::metadata(&snapshot_database)
        .map_err(|source| ProfilerError::Io {
            operation: "reading published snapshot metadata",
            path: snapshot_database.clone(),
            source,
        })?
        .len();
    let manifest = SourceSnapshotManifest {
        adapter: "mailvault".into(),
        adapter_version: MailVaultAdapter::ADAPTER_VERSION.into(),
        run_id: request.run_id.clone(),
        archive_identity,
        archive_root: layout.root.to_string_lossy().into_owned(),
        source_database: layout.database.to_string_lossy().into_owned(),
        snapshot_database: snapshot_database.to_string_lossy().into_owned(),
        snapshot_sha256,
        snapshot_bytes,
        schema_version,
        source_metrics: source_metrics_before,
        snapshot_metrics,
        created_at: OffsetDateTime::now_utc(),
    };
    write_json_atomic(&manifest_path, &manifest)?;

    sequence += 1;
    sink.send(ProgressEvent {
        run_id: request.run_id.clone(),
        sequence,
        stage: RunStage::SourceSnapshot,
        stage_state: StageState::Completed,
        unit: ProgressUnit::Pages,
        completed_items: 1,
        total_items: Some(1),
        completed_bytes: snapshot_bytes,
        total_bytes: Some(snapshot_bytes),
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        instant_throughput: None,
        smoothed_throughput: None,
        eta_ms: Some(0),
        active_workers: 0,
        queue_depth: 0,
        warnings: 0,
        errors: 0,
        current_object_display: None,
        checkpoint_sequence: sequence,
    })?;

    info!(
        run_id = %request.run_id,
        snapshot = %snapshot_database.display(),
        bytes = snapshot_bytes,
        "MailVault source snapshot published"
    );

    Ok(SnapshotResult {
        snapshot_directory: snapshot_directory.to_string_lossy().into_owned(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        manifest,
    })
}

#[allow(clippy::too_many_arguments)]
fn recover_or_reuse_snapshot(
    request: &SnapshotRequest,
    layout: &MailVaultLayout,
    snapshot_database: &Path,
    manifest_path: &Path,
    archive_identity: &str,
    expected_metrics: &profiler_core::ArchiveMetrics,
    sink: &dyn ProgressSink,
) -> ProfilerResult<SnapshotResult> {
    let started = Instant::now();
    let snapshot = open_read_only(snapshot_database)?;
    let quick_check: String = snapshot
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|source| sqlite_error("validating reusable source snapshot", source))?;
    if quick_check != "ok" {
        return Err(ProfilerError::IncompatibleSource(format!(
            "existing snapshot quick_check failed: {quick_check}"
        )));
    }
    let snapshot_metrics = read_metrics(&snapshot)?;
    if &snapshot_metrics != expected_metrics {
        return Err(ProfilerError::SourceChanged);
    }
    let schema_version = read_schema_version_value(&snapshot)?;
    let snapshot_archive_identity = read_archive_identity_value(&snapshot)?;
    if snapshot_archive_identity != archive_identity {
        return Err(ProfilerError::SourceChanged);
    }
    drop(snapshot);

    let snapshot_sha256 = sha256_file(snapshot_database)?;
    let snapshot_bytes = fs::metadata(snapshot_database)
        .map_err(|source| ProfilerError::Io {
            operation: "reading reusable snapshot metadata",
            path: snapshot_database.to_path_buf(),
            source,
        })?
        .len();

    let manifest = if manifest_path.is_file() {
        let existing: SourceSnapshotManifest = read_json(manifest_path)?;
        if existing.run_id != request.run_id
            || existing.archive_identity != archive_identity
            || existing.snapshot_sha256 != snapshot_sha256
            || existing.snapshot_metrics != snapshot_metrics
            || existing.schema_version != schema_version
        {
            return Err(ProfilerError::IncompatibleSource(
                "existing snapshot manifest does not match the reusable snapshot".into(),
            ));
        }
        existing
    } else {
        let recovered = SourceSnapshotManifest {
            adapter: "mailvault".into(),
            adapter_version: MailVaultAdapter::ADAPTER_VERSION.into(),
            run_id: request.run_id.clone(),
            archive_identity: archive_identity.to_owned(),
            archive_root: layout.root.to_string_lossy().into_owned(),
            source_database: layout.database.to_string_lossy().into_owned(),
            snapshot_database: snapshot_database.to_string_lossy().into_owned(),
            snapshot_sha256,
            snapshot_bytes,
            schema_version,
            source_metrics: expected_metrics.clone(),
            snapshot_metrics,
            created_at: OffsetDateTime::now_utc(),
        };
        write_json_atomic(manifest_path, &recovered)?;
        recovered
    };

    sink.send(ProgressEvent {
        run_id: request.run_id.clone(),
        sequence: 1,
        stage: RunStage::SourceSnapshot,
        stage_state: StageState::Completed,
        unit: ProgressUnit::Pages,
        completed_items: 1,
        total_items: Some(1),
        completed_bytes: snapshot_bytes,
        total_bytes: Some(snapshot_bytes),
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        instant_throughput: None,
        smoothed_throughput: None,
        eta_ms: Some(0),
        active_workers: 0,
        queue_depth: 0,
        warnings: 0,
        errors: 0,
        current_object_display: Some("reused verified snapshot".into()),
        checkpoint_sequence: 1,
    })?;

    Ok(SnapshotResult {
        snapshot_directory: snapshot_database
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .into_owned(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        manifest,
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> ProfilerResult<T> {
    let payload = fs::read(path).map_err(|source| ProfilerError::Io {
        operation: "reading snapshot manifest",
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&payload).map_err(|source| {
        ProfilerError::IncompatibleSource(format!("invalid snapshot manifest: {source}"))
    })
}

fn validate_request(request: &SnapshotRequest) -> ProfilerResult<()> {
    if request.run_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument(
            "run_id cannot be empty".into(),
        ));
    }
    if request.options.pages_per_step <= 0 {
        return Err(ProfilerError::InvalidArgument(
            "pages_per_step must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn ensure_workspace_outside_source(
    source_root: &Path,
    workspace_root: &Path,
) -> ProfilerResult<()> {
    fs::create_dir_all(workspace_root).map_err(|source| ProfilerError::Io {
        operation: "creating profiler workspace",
        path: workspace_root.to_path_buf(),
        source,
    })?;
    let workspace = fs::canonicalize(workspace_root).map_err(|source| ProfilerError::Io {
        operation: "canonicalizing profiler workspace",
        path: workspace_root.to_path_buf(),
        source,
    })?;
    if workspace.starts_with(source_root) {
        return Err(ProfilerError::InvalidPath {
            message: "profiler workspace must be outside the MailVault archive root".into(),
            path: workspace,
        });
    }
    Ok(())
}

fn sync_completed_file(path: &Path, operation: &'static str) -> ProfilerResult<()> {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|source| ProfilerError::Io {
            operation: "opening completed file for durable sync",
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| ProfilerError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    })
}

fn sha256_file(path: &Path) -> ProfilerResult<String> {
    let file = File::open(path).map_err(|source| ProfilerError::Io {
        operation: "opening snapshot for SHA-256",
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::with_capacity(4 * 1024 * 1024, file);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 4 * 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| ProfilerError::Io {
                operation: "hashing source snapshot",
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> ProfilerResult<()> {
    let temporary = path.with_extension("json.partial");
    let payload = serde_json::to_vec_pretty(value).map_err(|source| {
        ProfilerError::Internal(format!("serializing snapshot manifest: {source}"))
    })?;
    let mut file = File::create(&temporary).map_err(|source| ProfilerError::Io {
        operation: "creating snapshot manifest",
        path: temporary.clone(),
        source,
    })?;
    file.write_all(&payload)
        .map_err(|source| ProfilerError::Io {
            operation: "writing snapshot manifest",
            path: temporary.clone(),
            source,
        })?;
    file.sync_all().map_err(|source| ProfilerError::Io {
        operation: "syncing snapshot manifest",
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| ProfilerError::Io {
        operation: "publishing snapshot manifest",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> ProfilerResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ProfilerError::Io {
            operation: "removing stale partial snapshot",
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn sqlite_error(operation: &'static str, source: rusqlite::Error) -> ProfilerError {
    ProfilerError::Sqlite {
        operation,
        message: source.to_string(),
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use tempfile::tempdir;

    use super::sync_completed_file;

    #[test]
    fn completed_snapshot_file_can_be_durably_synced_on_windows() {
        let directory = tempdir().expect("create temporary snapshot directory");
        let snapshot = directory.path().join("mailvault.sqlite3.partial");
        std::fs::write(&snapshot, b"mailvault snapshot").expect("write temporary snapshot file");

        // Windows FlushFileBuffers requires a write-capable handle. This test prevents a
        // regression to File::open(...).sync_all(), which fails with ERROR_ACCESS_DENIED.
        sync_completed_file(&snapshot, "syncing test snapshot")
            .expect("sync completed snapshot with a write-capable handle");
    }
}
