use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use fs2::FileExt;
use profiler_core::{
    ActiveRunContext, ErrorCode, FindingDetail, FindingReviewHistory, FindingsPage,
    FindingsPageRequest, InventoryPage, InventoryPageRequest, OpenWorkspaceResult, ProfilerError,
    ProfilerResult, ReviewActorKind, ReviewStatus, RunCatalogEntry, SanitizedRunSummary,
    WorkspaceAccessMode, WorkspaceCompatibility, WorkspaceDatabaseInspection, WorkspaceDescriptor,
    WorkspaceInspection, WorkspaceOpenMode,
};
use profiler_storage_sqlite::{
    ProfilerStore, current_workspace_schema, expected_application_id, migration_failure_marker_path,
};
use serde::Serialize;
use time::OffsetDateTime;

const DATABASE_RELATIVE_PATH: &str = "profiler/profiler.sqlite3";
const LOCK_FILE_NAME: &str = ".mailvault-profiler.workspace.lock";

#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    pub root_path: PathBuf,
    pub profiler_database: PathBuf,
    pub access_mode: WorkspaceAccessMode,
    pub review_integrity_valid: bool,
    pub source_roots: Vec<PathBuf>,
}

impl WorkspaceContext {
    pub const fn allows_review_write(&self) -> bool {
        self.access_mode.allows_review_write() && self.review_integrity_valid
    }

    pub fn open_reader(&self) -> ProfilerResult<ProfilerStore> {
        ProfilerStore::open_read_only(&self.profiler_database)
    }

    pub fn open_writer(&self) -> ProfilerResult<ProfilerStore> {
        if !self.allows_review_write() {
            return Err(ProfilerError::contract(
                ErrorCode::ReviewWriteNotAllowed,
                "workspace is open read-only; review changes are disabled",
                false,
            ));
        }
        ProfilerStore::open_existing(&self.profiler_database)
    }
}

#[derive(Debug)]
pub struct WorkspaceSession {
    context: WorkspaceContext,
    descriptor: WorkspaceDescriptor,
    lock_file: Option<File>,
}

impl WorkspaceSession {
    pub fn inspect(root: &Path) -> ProfilerResult<WorkspaceInspection> {
        let root_path = validate_workspace_root(root)?;
        let lock_active = lock_is_active(&root_path)?;
        let profiler_database = root_path.join(DATABASE_RELATIVE_PATH);

        if migration_failure_marker_path(&profiler_database).is_file() {
            return Ok(inspection_without_database(
                root_path,
                profiler_database,
                lock_active,
                WorkspaceCompatibility::IncompleteMigration,
                "workspace contains a retained migration-failure marker",
            ));
        }
        if !profiler_database.is_file() {
            return Ok(inspection_without_database(
                root_path,
                profiler_database,
                lock_active,
                WorkspaceCompatibility::MissingProfilerDatabase,
                "expected profiler/profiler.sqlite3 was not found",
            ));
        }

        let database = ProfilerStore::inspect_database(&profiler_database)?;
        let source_roots = ProfilerStore::inspect_source_archive_roots(&profiler_database)?;
        let run_count = ProfilerStore::inspect_run_count(&profiler_database)?;
        let compatibility = if paths_overlap_any(&root_path, &source_roots)? {
            WorkspaceCompatibility::SourceWorkspaceOverlap
        } else {
            classify_workspace_compatibility(&database)
        };

        Ok(inspection_from_database(
            root_path,
            profiler_database,
            lock_active,
            run_count,
            database,
            compatibility,
        ))
    }

    pub fn open(
        root: &Path,
        mode: WorkspaceOpenMode,
        allow_migration: bool,
    ) -> ProfilerResult<Self> {
        let inspection = Self::inspect(root)?;
        validate_workspace_open(&inspection, allow_migration)?;

        let root_path = inspection.root_path;
        let profiler_database = inspection.profiler_database;
        let source_roots = ProfilerStore::inspect_source_archive_roots(&profiler_database)?;
        let (access_mode, lock_file) = select_workspace_access(&root_path, mode)?;

        if inspection.migration_required && !access_mode.allows_review_write() {
            return Err(ProfilerError::contract(
                ErrorCode::WorkspaceLocked,
                "workspace migration requires an exclusive review-writer lock",
                true,
            ));
        }

        let store = open_workspace_store(&profiler_database, access_mode)?;
        let review_integrity_valid = store.validate_all_review_history().is_ok();
        let (effective_access_mode, lock_file) =
            enforce_review_integrity(access_mode, lock_file, review_integrity_valid);
        let descriptor = store.workspace_descriptor(
            &root_path,
            &profiler_database,
            effective_access_mode,
            review_integrity_valid,
        )?;

        Ok(Self {
            context: WorkspaceContext {
                root_path,
                profiler_database,
                access_mode: effective_access_mode,
                review_integrity_valid,
                source_roots,
            },
            descriptor,
            lock_file,
        })
    }

    pub fn context(&self) -> WorkspaceContext {
        self.context.clone()
    }

    pub fn descriptor(&self) -> WorkspaceDescriptor {
        self.descriptor.clone()
    }

    pub fn open_result(&self) -> ProfilerResult<OpenWorkspaceResult> {
        let store = self.context.open_reader()?;
        Ok(OpenWorkspaceResult {
            descriptor: self.descriptor(),
            runs: store.list_runs()?,
        })
    }
}

impl Drop for WorkspaceSession {
    fn drop(&mut self) {
        if let Some(file) = self.lock_file.take() {
            let _ = FileExt::unlock(&file);
        }
    }
}

pub fn list_runs(context: &WorkspaceContext) -> ProfilerResult<Vec<RunCatalogEntry>> {
    context.open_reader()?.list_runs()
}

pub fn open_run(context: &WorkspaceContext, run_id: &str) -> ProfilerResult<ActiveRunContext> {
    context.open_reader()?.active_run_context(run_id)
}

pub fn inventory_page(
    context: &WorkspaceContext,
    request: &InventoryPageRequest,
) -> ProfilerResult<InventoryPage> {
    context.open_reader()?.inventory_page(request)
}

pub fn findings_page(
    context: &WorkspaceContext,
    request: &FindingsPageRequest,
) -> ProfilerResult<FindingsPage> {
    context.open_reader()?.findings_page(request)
}

pub fn finding_detail(
    context: &WorkspaceContext,
    run_id: &str,
    finding_id: &str,
) -> ProfilerResult<FindingDetail> {
    context.open_reader()?.finding_detail(run_id, finding_id)
}

pub fn set_review_status(
    context: &WorkspaceContext,
    run_id: &str,
    finding_id: &str,
    status: ReviewStatus,
    note: Option<&str>,
    actor_kind: ReviewActorKind,
) -> ProfilerResult<FindingReviewHistory> {
    let mut store = context.open_writer()?;
    store.set_finding_review_status(run_id, finding_id, status, note, actor_kind, None)
}

pub fn clear_review_status(
    context: &WorkspaceContext,
    run_id: &str,
    finding_id: &str,
    note: Option<&str>,
    actor_kind: ReviewActorKind,
) -> ProfilerResult<FindingReviewHistory> {
    let mut store = context.open_writer()?;
    store.clear_finding_review_status(run_id, finding_id, note, actor_kind, None)
}

pub fn add_review_note(
    context: &WorkspaceContext,
    run_id: &str,
    finding_id: &str,
    note: &str,
    actor_kind: ReviewActorKind,
) -> ProfilerResult<FindingReviewHistory> {
    let mut store = context.open_writer()?;
    store.add_finding_review_note(run_id, finding_id, note, actor_kind, None)
}

pub fn export_sanitized_run(
    context: &WorkspaceContext,
    run_id: &str,
    destination: &Path,
) -> ProfilerResult<PathBuf> {
    validate_export_destination(context, destination)?;
    let store = context.open_reader()?;
    let extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if destination.exists() {
        return Err(ProfilerError::contract(
            ErrorCode::SanitizedExportFailed,
            "sanitized export destination already exists",
            false,
        ));
    }

    let payload =
        match extension.as_str() {
            "json" => serde_json::to_vec_pretty(&store.sanitized_run_summary(run_id)?).map_err(
                |error| ProfilerError::Internal(format!("serializing sanitized summary: {error}")),
            )?,
            "csv" => sanitized_csv(&store.sanitized_findings(run_id)?).into_bytes(),
            _ => {
                return Err(ProfilerError::contract(
                    ErrorCode::SanitizedExportFailed,
                    "sanitized export destination must use .json or .csv",
                    false,
                ));
            }
        };
    let temporary =
        destination.with_extension(format!("{extension}.{}.partial", uuid::Uuid::now_v7()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| ProfilerError::Io {
                operation: "creating sanitized export",
                path: temporary.clone(),
                source,
            })?;
        file.write_all(&payload)
            .map_err(|source| ProfilerError::Io {
                operation: "writing sanitized export",
                path: temporary.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| ProfilerError::Io {
            operation: "syncing sanitized export",
            path: temporary.clone(),
            source,
        })?;
    }
    if let Err(source) = std::fs::rename(&temporary, destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(ProfilerError::Io {
            operation: "publishing sanitized export",
            path: destination.to_path_buf(),
            source,
        });
    }
    Ok(destination.to_path_buf())
}

pub fn sanitized_summary(
    context: &WorkspaceContext,
    run_id: &str,
) -> ProfilerResult<SanitizedRunSummary> {
    context.open_reader()?.sanitized_run_summary(run_id)
}

fn validate_workspace_root(root: &Path) -> ProfilerResult<PathBuf> {
    if !root.is_dir() {
        return Err(ProfilerError::contract(
            ErrorCode::WorkspaceNotFound,
            "workspace directory does not exist",
            false,
        ));
    }
    canonicalize_directory(root, "canonicalizing workspace directory")
}

fn inspection_without_database(
    root_path: PathBuf,
    profiler_database: PathBuf,
    lock_active: bool,
    compatibility: WorkspaceCompatibility,
    detail: &str,
) -> WorkspaceInspection {
    WorkspaceInspection {
        root_path,
        profiler_database,
        compatibility,
        schema_version: None,
        supported_schema_version: current_workspace_schema(),
        migration_required: false,
        lock_active,
        run_count: 0,
        workspace_id: None,
        created_by_version: None,
        last_migrated_by_version: None,
        detail: detail.into(),
    }
}

fn classify_workspace_compatibility(
    database: &WorkspaceDatabaseInspection,
) -> WorkspaceCompatibility {
    if !database.integrity_ok {
        WorkspaceCompatibility::CorruptedProfilerDatabase
    } else if database.application_id != expected_application_id() {
        WorkspaceCompatibility::InvalidLayout
    } else if database.schema_version > current_workspace_schema() {
        WorkspaceCompatibility::NewerThanApplication
    } else if database.schema_version < current_workspace_schema() {
        WorkspaceCompatibility::MigrationRequired
    } else if database
        .migration_state
        .as_deref()
        .is_some_and(|state| state != "ready")
    {
        WorkspaceCompatibility::IncompleteMigration
    } else {
        WorkspaceCompatibility::Compatible
    }
}

fn inspection_from_database(
    root_path: PathBuf,
    profiler_database: PathBuf,
    lock_active: bool,
    run_count: u64,
    database: WorkspaceDatabaseInspection,
    compatibility: WorkspaceCompatibility,
) -> WorkspaceInspection {
    let migration_required = matches!(compatibility, WorkspaceCompatibility::MigrationRequired);
    let detail = workspace_compatibility_detail(compatibility, database.schema_version);
    WorkspaceInspection {
        root_path,
        profiler_database,
        compatibility,
        schema_version: Some(database.schema_version),
        supported_schema_version: current_workspace_schema(),
        migration_required,
        lock_active,
        run_count,
        workspace_id: database.workspace_id,
        created_by_version: database.created_by_version,
        last_migrated_by_version: database.last_migrated_by_version,
        detail,
    }
}

fn workspace_compatibility_detail(
    compatibility: WorkspaceCompatibility,
    schema_version: i64,
) -> String {
    match compatibility {
        WorkspaceCompatibility::Compatible => "workspace is compatible".into(),
        WorkspaceCompatibility::MigrationRequired => format!(
            "workspace schema {schema_version} must be migrated to {}",
            current_workspace_schema()
        ),
        WorkspaceCompatibility::NewerThanApplication => format!(
            "workspace schema {schema_version} is newer than supported schema {}",
            current_workspace_schema()
        ),
        WorkspaceCompatibility::CorruptedProfilerDatabase => {
            "profiler database integrity check failed".into()
        }
        WorkspaceCompatibility::SourceWorkspaceOverlap => {
            "workspace and MailVault source paths overlap".into()
        }
        WorkspaceCompatibility::IncompleteMigration => {
            "workspace metadata reports an incomplete migration".into()
        }
        WorkspaceCompatibility::InvalidLayout | WorkspaceCompatibility::MissingProfilerDatabase => {
            "workspace layout is invalid".into()
        }
    }
}

fn validate_workspace_open(
    inspection: &WorkspaceInspection,
    allow_migration: bool,
) -> ProfilerResult<()> {
    let error_code = match inspection.compatibility {
        WorkspaceCompatibility::Compatible => return Ok(()),
        WorkspaceCompatibility::MigrationRequired if allow_migration => return Ok(()),
        WorkspaceCompatibility::MigrationRequired => ErrorCode::WorkspaceMigrationRequired,
        WorkspaceCompatibility::NewerThanApplication => {
            ErrorCode::WorkspaceSchemaNewerThanApplication
        }
        WorkspaceCompatibility::CorruptedProfilerDatabase => ErrorCode::WorkspaceDatabaseCorrupted,
        WorkspaceCompatibility::SourceWorkspaceOverlap => ErrorCode::WorkspaceSourceOverlap,
        WorkspaceCompatibility::IncompleteMigration => ErrorCode::WorkspaceMigrationFailed,
        WorkspaceCompatibility::InvalidLayout | WorkspaceCompatibility::MissingProfilerDatabase => {
            ErrorCode::WorkspaceInvalidLayout
        }
    };
    Err(ProfilerError::contract(
        error_code,
        inspection.detail.clone(),
        false,
    ))
}

fn select_workspace_access(
    root: &Path,
    mode: WorkspaceOpenMode,
) -> ProfilerResult<(WorkspaceAccessMode, Option<File>)> {
    match mode {
        WorkspaceOpenMode::ReadOnly => Ok((WorkspaceAccessMode::ReadOnlyCompatibility, None)),
        WorkspaceOpenMode::ReadWritePreferred => match acquire_workspace_lock(root)? {
            Some(file) => Ok((WorkspaceAccessMode::ReadWrite, Some(file))),
            None => Ok((WorkspaceAccessMode::ReadOnlyLocked, None)),
        },
    }
}

fn open_workspace_store(
    profiler_database: &Path,
    access_mode: WorkspaceAccessMode,
) -> ProfilerResult<ProfilerStore> {
    if access_mode.allows_review_write() {
        ProfilerStore::open_existing(profiler_database)
    } else {
        ProfilerStore::open_read_only(profiler_database)
    }
}

fn enforce_review_integrity(
    access_mode: WorkspaceAccessMode,
    lock_file: Option<File>,
    review_integrity_valid: bool,
) -> (WorkspaceAccessMode, Option<File>) {
    if review_integrity_valid {
        return (access_mode, lock_file);
    }
    if let Some(file) = lock_file {
        let _ = FileExt::unlock(&file);
    }
    (WorkspaceAccessMode::ReadOnlyCompatibility, None)
}

fn is_workspace_lock_contended(error: &std::io::Error) -> bool {
    let expected = fs2::lock_contended_error();
    match expected.raw_os_error() {
        Some(expected_code) => error.raw_os_error() == Some(expected_code),
        None => error.kind() == expected.kind(),
    }
}

fn acquire_workspace_lock(root: &Path) -> ProfilerResult<Option<File>> {
    let lock_path = root.join(LOCK_FILE_NAME);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        // Do not truncate before acquiring the OS lock. Existing metadata may
        // belong to another live writer. The file is reset only after the
        // exclusive lock has been acquired successfully.
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| ProfilerError::Io {
            operation: "opening workspace lock",
            path: lock_path.clone(),
            source,
        })?;
    match file.try_lock_exclusive() {
        Ok(()) => {
            file.set_len(0).map_err(|source| ProfilerError::Io {
                operation: "resetting workspace lock metadata",
                path: lock_path.clone(),
                source,
            })?;
            file.seek(SeekFrom::Start(0))
                .map_err(|source| ProfilerError::Io {
                    operation: "seeking workspace lock metadata",
                    path: lock_path.clone(),
                    source,
                })?;
            let metadata = WorkspaceLockMetadata {
                format: 1,
                process_id: std::process::id(),
                application_version: env!("CARGO_PKG_VERSION"),
                opened_at: OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .map_err(|error| {
                        ProfilerError::Internal(format!("formatting lock time: {error}"))
                    })?,
                purpose: "review_write",
            };
            let payload = serde_json::to_vec_pretty(&metadata).map_err(|error| {
                ProfilerError::Internal(format!("serializing lock metadata: {error}"))
            })?;
            file.write_all(&payload)
                .map_err(|source| ProfilerError::Io {
                    operation: "writing workspace lock metadata",
                    path: lock_path.clone(),
                    source,
                })?;
            file.sync_all().map_err(|source| ProfilerError::Io {
                operation: "syncing workspace lock metadata",
                path: lock_path,
                source,
            })?;
            Ok(Some(file))
        }
        Err(error) if is_workspace_lock_contended(&error) => Ok(None),
        Err(source) => Err(ProfilerError::Io {
            operation: "acquiring workspace lock",
            path: lock_path,
            source,
        }),
    }
}

fn lock_is_active(root: &Path) -> ProfilerResult<bool> {
    let lock_path = root.join(LOCK_FILE_NAME);
    if !lock_path.exists() {
        return Ok(false);
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| ProfilerError::Io {
            operation: "inspecting workspace lock",
            path: lock_path.clone(),
            source,
        })?;
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file).map_err(|source| ProfilerError::Io {
                operation: "releasing inspected workspace lock",
                path: lock_path,
                source,
            })?;
            Ok(false)
        }
        Err(error) if is_workspace_lock_contended(&error) => Ok(true),
        Err(source) => Err(ProfilerError::Io {
            operation: "inspecting workspace lock state",
            path: lock_path,
            source,
        }),
    }
}

fn validate_export_destination(
    context: &WorkspaceContext,
    destination: &Path,
) -> ProfilerResult<()> {
    let parent = destination.parent().ok_or_else(|| {
        ProfilerError::contract(
            ErrorCode::SanitizedExportFailed,
            "export destination must have a parent directory",
            false,
        )
    })?;
    if !parent.is_dir() {
        return Err(ProfilerError::contract(
            ErrorCode::SanitizedExportFailed,
            "export destination directory does not exist",
            false,
        ));
    }
    let canonical_parent = canonicalize_directory(parent, "canonicalizing export directory")?;
    for source_root in &context.source_roots {
        if let Ok(canonical_source) = source_root.canonicalize()
            && path_is_within(&canonical_parent, &canonical_source)
        {
            return Err(ProfilerError::contract(
                ErrorCode::SanitizedExportFailed,
                "sanitized exports cannot be written inside the MailVault source archive",
                false,
            ));
        }
    }
    Ok(())
}

fn canonicalize_directory(path: &Path, operation: &'static str) -> ProfilerResult<PathBuf> {
    path.canonicalize().map_err(|source| ProfilerError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    })
}

fn paths_overlap_any(workspace: &Path, source_roots: &[PathBuf]) -> ProfilerResult<bool> {
    for source in source_roots {
        if !source.exists() {
            continue;
        }
        let source = canonicalize_directory(source, "canonicalizing MailVault source root")?;
        if path_is_within(workspace, &source) || path_is_within(&source, workspace) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn path_is_within(candidate: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        let candidate = candidate
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase();
        let parent = parent
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase();
        candidate == parent
            || candidate
                .strip_prefix(&parent)
                .is_some_and(|remainder| remainder.starts_with('\\'))
    }
    #[cfg(not(windows))]
    {
        candidate == parent || candidate.starts_with(parent)
    }
}

fn sanitized_csv(rows: &[profiler_core::SanitizedFindingRow]) -> String {
    let mut output =
        String::from("finding_token,object_token,code,severity,review_status,reviewed_at\n");
    for row in rows {
        let fields = [
            row.finding_token.as_str(),
            row.object_token.as_deref().unwrap_or(""),
            row.code.as_str(),
            row.severity.as_str(),
            row.review_status.map_or("unreviewed", ReviewStatus::as_str),
            row.reviewed_at.as_deref().unwrap_or(""),
        ];
        output.push_str(
            &fields
                .iter()
                .map(|value| csv_escape(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }
    output
}

fn csv_escape(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceLockMetadata<'a> {
    format: u32,
    process_id: u32,
    application_version: &'a str,
    opened_at: String,
    purpose: &'a str,
}

#[cfg(test)]
mod tests {
    use super::is_workspace_lock_contended;

    #[test]
    fn recognizes_the_platform_lock_contention_error_reported_by_fs2() {
        let error = fs2::lock_contended_error();
        assert!(is_workspace_lock_contended(&error));
    }

    #[test]
    fn does_not_treat_an_unrelated_io_error_as_lock_contention() {
        let error = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(!is_workspace_lock_contended(&error));
    }
}
