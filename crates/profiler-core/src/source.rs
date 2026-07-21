use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckLevel {
    Required,
    Recommended,
    Informational,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Warning,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    Absent,
    Idle,
    Active,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheck {
    pub code: String,
    pub label: String,
    pub level: CheckLevel,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMetrics {
    pub accounts: u64,
    pub messages: u64,
    pub message_occurrences: u64,
    pub mime_parts: u64,
    pub attachment_occurrences: u64,
    pub blobs: u64,
    pub blob_bytes: u64,
    pub message_relations: u64,
    pub participants: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub adapter: String,
    pub compatible: bool,
    pub archive_root: String,
    pub database_path: String,
    pub database_bytes: u64,
    pub archive_identity: Option<String>,
    pub schema_version: Option<u32>,
    pub journal_mode: Option<String>,
    pub lock_state: LockState,
    pub metrics: ArchiveMetrics,
    pub checks: Vec<PreflightCheck>,
    pub warnings_count: u64,
    pub errors_count: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub inspected_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshotManifest {
    pub adapter: String,
    pub adapter_version: String,
    pub run_id: String,
    pub archive_identity: String,
    pub archive_root: String,
    pub source_database: String,
    pub snapshot_database: String,
    pub snapshot_sha256: String,
    pub snapshot_bytes: u64,
    pub schema_version: u32,
    pub source_metrics: ArchiveMetrics,
    pub snapshot_metrics: ArchiveMetrics,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
