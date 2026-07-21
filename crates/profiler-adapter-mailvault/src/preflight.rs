use std::{collections::BTreeSet, fs::OpenOptions, path::Path, time::Duration};

use fs2::FileExt;
use profiler_core::{
    ArchiveMetrics, CheckLevel, CheckStatus, LockState, PreflightCheck, PreflightReport,
    ProfilerError, ProfilerResult,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::{
    contract::{RECOMMENDED_INDEXES, REQUIRED_TABLES, SUPPORTED_SCHEMA_MAX, SUPPORTED_SCHEMA_MIN},
    layout::MailVaultLayout,
};

pub(crate) fn run_preflight(archive_root: &Path) -> ProfilerResult<PreflightReport> {
    let layout = MailVaultLayout::inspect(archive_root)?;
    let mut checks = Vec::new();

    check_required_path(&layout.database, "database", false, &mut checks);
    check_required_path(&layout.raw_objects, "raw_object_store", true, &mut checks);
    check_required_path(&layout.blob_objects, "blob_object_store", true, &mut checks);
    check_required_path(&layout.state, "state_directory", true, &mut checks);

    if !layout.database.is_file() {
        return Ok(finalize_report(
            &layout,
            None,
            None,
            None,
            LockState::Indeterminate,
            ArchiveMetrics::default(),
            checks,
        ));
    }

    let database_bytes = layout
        .database
        .metadata()
        .map_err(|source| ProfilerError::Io {
            operation: "reading MailVault database metadata",
            path: layout.database.clone(),
            source,
        })?
        .len();

    let connection = open_read_only(&layout.database)?;
    let schema_version = read_schema_version(&connection, &mut checks);
    check_schema_contract(&connection, &mut checks)?;
    let archive_identity = read_archive_identity(&connection, &mut checks);
    check_recommended_indexes(&connection, &mut checks)?;
    let journal_mode = read_journal_mode(&connection, &mut checks);
    check_quick_integrity(&connection, &mut checks)?;
    let metrics = read_metrics(&connection)?;
    let lock_state = inspect_writer_lock(&layout, &mut checks);

    let mut report = finalize_report(
        &layout,
        archive_identity,
        schema_version,
        journal_mode,
        lock_state,
        metrics,
        checks,
    );
    report.database_bytes = database_bytes;
    Ok(report)
}

pub(crate) fn open_read_only(path: &Path) -> ProfilerResult<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|source| sqlite_error("opening MailVault database read-only", source))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|source| sqlite_error("configuring MailVault busy timeout", source))?;
    connection
        .execute_batch(
            "PRAGMA query_only=ON;\n\
             PRAGMA trusted_schema=OFF;\n\
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|source| sqlite_error("hardening MailVault read-only connection", source))?;
    Ok(connection)
}

pub(crate) fn read_metrics(connection: &Connection) -> ProfilerResult<ArchiveMetrics> {
    Ok(ArchiveMetrics {
        accounts: count(connection, "SELECT COUNT(*) FROM accounts")?,
        messages: count(connection, "SELECT COUNT(*) FROM messages")?,
        message_occurrences: count(connection, "SELECT COUNT(*) FROM message_occurrences")?,
        mime_parts: count(connection, "SELECT COUNT(*) FROM message_parts")?,
        attachment_occurrences: count(
            connection,
            "SELECT COUNT(*) FROM message_parts WHERE role='attachment'",
        )?,
        blobs: count(connection, "SELECT COUNT(*) FROM blobs")?,
        blob_bytes: count(connection, "SELECT COALESCE(SUM(size_bytes), 0) FROM blobs")?,
        message_relations: count(connection, "SELECT COUNT(*) FROM message_relations")?,
        participants: count(connection, "SELECT COUNT(*) FROM message_participants")?,
    })
}

pub(crate) fn read_schema_version_value(connection: &Connection) -> ProfilerResult<u32> {
    connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|source| sqlite_error("reading MailVault schema version", source))?
        .ok_or_else(|| {
            ProfilerError::IncompatibleSource("schema_meta.schema_version is missing".into())
        })?
        .parse::<u32>()
        .map_err(|_| {
            ProfilerError::IncompatibleSource("schema version is not an unsigned integer".into())
        })
}

pub(crate) fn read_archive_identity_value(connection: &Connection) -> ProfilerResult<String> {
    let mut statement = connection
        .prepare("SELECT archive_id FROM accounts ORDER BY archive_id")
        .map_err(|source| sqlite_error("preparing MailVault archive identity", source))?;
    let archive_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|source| sqlite_error("querying MailVault archive identity", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting MailVault archive identity", source))?;
    if archive_ids.is_empty() {
        return Err(ProfilerError::IncompatibleSource(
            "MailVault archive contains no account identities".into(),
        ));
    }

    let mut digest = Sha256::new();
    digest.update(b"mailvault-profiler-collection-v1\0");
    for archive_id in archive_ids {
        let bytes = archive_id.as_bytes();
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Ok(hex::encode(digest.finalize()))
}

fn read_archive_identity(
    connection: &Connection,
    checks: &mut Vec<PreflightCheck>,
) -> Option<String> {
    match read_archive_identity_value(connection) {
        Ok(identity) => {
            checks.push(passed(
                "archive_identity",
                "Stable archive identity",
                CheckLevel::Required,
                format!("{}…", &identity[..12]),
            ));
            Some(identity)
        }
        Err(error) => {
            checks.push(failed(
                "archive_identity",
                "Stable archive identity",
                CheckLevel::Required,
                error.to_string(),
            ));
            None
        }
    }
}

fn read_schema_version(connection: &Connection, checks: &mut Vec<PreflightCheck>) -> Option<u32> {
    match read_schema_version_value(connection) {
        Ok(version) if (SUPPORTED_SCHEMA_MIN..=SUPPORTED_SCHEMA_MAX).contains(&version) => {
            checks.push(passed(
                "schema_version",
                "MailVault schema version",
                CheckLevel::Required,
                format!("schema version {version} is supported"),
            ));
            Some(version)
        }
        Ok(version) => {
            checks.push(failed(
                "schema_version",
                "MailVault schema version",
                CheckLevel::Required,
                format!(
                    "schema version {version} is unsupported; adapter supports {SUPPORTED_SCHEMA_MIN}..={SUPPORTED_SCHEMA_MAX}"
                ),
            ));
            Some(version)
        }
        Err(error) => {
            checks.push(failed(
                "schema_version",
                "MailVault schema version",
                CheckLevel::Required,
                error.to_string(),
            ));
            None
        }
    }
}

fn check_schema_contract(
    connection: &Connection,
    checks: &mut Vec<PreflightCheck>,
) -> ProfilerResult<()> {
    for &(table, required_columns) in REQUIRED_TABLES {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("checking MailVault table", source))?;
        if !exists {
            checks.push(failed(
                format!("table_{table}"),
                format!("Required table: {table}"),
                CheckLevel::Required,
                "table is missing",
            ));
            continue;
        }

        let escaped = table.replace('"', "\"\"");
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info(\"{escaped}\")"))
            .map_err(|source| sqlite_error("reading MailVault table contract", source))?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|source| sqlite_error("querying MailVault columns", source))?
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|source| sqlite_error("collecting MailVault columns", source))?;

        let missing: Vec<_> = required_columns
            .iter()
            .copied()
            .filter(|column| !columns.contains(*column))
            .collect();
        if missing.is_empty() {
            checks.push(passed(
                format!("table_{table}"),
                format!("Required table: {table}"),
                CheckLevel::Required,
                format!("{} required columns present", required_columns.len()),
            ));
        } else {
            checks.push(failed(
                format!("table_{table}"),
                format!("Required table: {table}"),
                CheckLevel::Required,
                format!("missing columns: {}", missing.join(", ")),
            ));
        }
    }
    Ok(())
}

fn check_recommended_indexes(
    connection: &Connection,
    checks: &mut Vec<PreflightCheck>,
) -> ProfilerResult<()> {
    for &index in RECOMMENDED_INDEXES {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                [index],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("checking MailVault index", source))?;
        checks.push(if exists {
            passed(
                format!("index_{index}"),
                format!("Recommended source index: {index}"),
                CheckLevel::Recommended,
                "index is present",
            )
        } else {
            warning(
                format!("index_{index}"),
                format!("Recommended source index: {index}"),
                CheckLevel::Recommended,
                "index is missing; source remains compatible but profiling may be slower",
            )
        });
    }
    Ok(())
}

fn check_quick_integrity(
    connection: &Connection,
    checks: &mut Vec<PreflightCheck>,
) -> ProfilerResult<()> {
    let result: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|source| sqlite_error("running MailVault quick_check", source))?;
    checks.push(if result == "ok" {
        passed(
            "sqlite_quick_check",
            "SQLite structural quick check",
            CheckLevel::Required,
            "database returned ok",
        )
    } else {
        failed(
            "sqlite_quick_check",
            "SQLite structural quick check",
            CheckLevel::Required,
            result,
        )
    });
    Ok(())
}

fn read_journal_mode(connection: &Connection, checks: &mut Vec<PreflightCheck>) -> Option<String> {
    match connection.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0)) {
        Ok(mode) => {
            checks.push(passed(
                "journal_mode",
                "SQLite journal mode",
                CheckLevel::Informational,
                mode.clone(),
            ));
            Some(mode)
        }
        Err(error) => {
            checks.push(warning(
                "journal_mode",
                "SQLite journal mode",
                CheckLevel::Informational,
                error.to_string(),
            ));
            None
        }
    }
}

fn inspect_writer_lock(layout: &MailVaultLayout, checks: &mut Vec<PreflightCheck>) -> LockState {
    if !layout.sync_lock.exists() {
        checks.push(passed(
            "writer_lock",
            "MailVault writer lock",
            CheckLevel::Required,
            "sync.lock is absent",
        ));
        return LockState::Absent;
    }

    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(&layout.sync_lock)
    {
        Ok(file) => file,
        Err(error) => {
            checks.push(failed(
                "writer_lock",
                "MailVault writer lock",
                CheckLevel::Required,
                format!("cannot prove lock state: {error}"),
            ));
            return LockState::Indeterminate;
        }
    };

    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            checks.push(passed(
                "writer_lock",
                "MailVault writer lock",
                CheckLevel::Required,
                "lock file exists but no writer holds it",
            ));
            LockState::Idle
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            checks.push(failed(
                "writer_lock",
                "MailVault writer lock",
                CheckLevel::Required,
                "another MailVault process currently holds sync.lock",
            ));
            LockState::Active
        }
        Err(error) => {
            checks.push(failed(
                "writer_lock",
                "MailVault writer lock",
                CheckLevel::Required,
                format!("cannot prove lock state: {error}"),
            ));
            LockState::Indeterminate
        }
    }
}

fn check_required_path(
    path: &Path,
    code: &str,
    expect_directory: bool,
    checks: &mut Vec<PreflightCheck>,
) {
    let valid = if expect_directory {
        path.is_dir()
    } else {
        path.is_file()
    };
    checks.push(if valid {
        passed(
            code,
            format!("Required path: {code}"),
            CheckLevel::Required,
            path.to_string_lossy(),
        )
    } else {
        failed(
            code,
            format!("Required path: {code}"),
            CheckLevel::Required,
            format!("missing or wrong type: {}", path.to_string_lossy()),
        )
    });
}

fn finalize_report(
    layout: &MailVaultLayout,
    archive_identity: Option<String>,
    schema_version: Option<u32>,
    journal_mode: Option<String>,
    lock_state: LockState,
    metrics: ArchiveMetrics,
    checks: Vec<PreflightCheck>,
) -> PreflightReport {
    let errors_count = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Failed)
        .count() as u64;
    let warnings_count = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Warning)
        .count() as u64;
    PreflightReport {
        adapter: "mailvault".into(),
        compatible: errors_count == 0,
        archive_root: layout.root.to_string_lossy().into_owned(),
        database_path: layout.database.to_string_lossy().into_owned(),
        database_bytes: 0,
        archive_identity,
        schema_version,
        journal_mode,
        lock_state,
        metrics,
        checks,
        warnings_count,
        errors_count,
        inspected_at: OffsetDateTime::now_utc(),
    }
}

fn count(connection: &Connection, sql: &str) -> ProfilerResult<u64> {
    let value: i64 = connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(|source| sqlite_error("reading MailVault metric", source))?;
    u64::try_from(value).map_err(|_| {
        ProfilerError::IncompatibleSource(format!("negative metric returned for query: {sql}"))
    })
}

fn passed(
    code: impl Into<String>,
    label: impl Into<String>,
    level: CheckLevel,
    detail: impl Into<String>,
) -> PreflightCheck {
    check(code, label, level, CheckStatus::Passed, detail)
}

fn warning(
    code: impl Into<String>,
    label: impl Into<String>,
    level: CheckLevel,
    detail: impl Into<String>,
) -> PreflightCheck {
    check(code, label, level, CheckStatus::Warning, detail)
}

fn failed(
    code: impl Into<String>,
    label: impl Into<String>,
    level: CheckLevel,
    detail: impl Into<String>,
) -> PreflightCheck {
    check(code, label, level, CheckStatus::Failed, detail)
}

fn check(
    code: impl Into<String>,
    label: impl Into<String>,
    level: CheckLevel,
    status: CheckStatus,
    detail: impl Into<String>,
) -> PreflightCheck {
    PreflightCheck {
        code: code.into(),
        label: label.into(),
        level,
        status,
        detail: detail.into(),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn sqlite_error(operation: &'static str, source: rusqlite::Error) -> ProfilerError {
    ProfilerError::Sqlite {
        operation,
        message: source.to_string(),
    }
}
