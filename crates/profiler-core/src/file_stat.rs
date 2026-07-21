use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ProfilerResult, ProgressSink};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityState {
    Uninspected,
    Available,
    Missing,
    Unreadable,
    InvalidLocator,
    NonRegular,
    UnsafeReparsePoint,
    IoError,
}

impl AvailabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uninspected => "uninspected",
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
            Self::InvalidLocator => "invalid_locator",
            Self::NonRegular => "non_regular",
            Self::UnsafeReparsePoint => "unsafe_reparse_point",
            Self::IoError => "io_error",
        }
    }
}

impl std::fmt::Display for AvailabilityState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeState {
    Uninspected,
    Match,
    Mismatch,
    Unavailable,
}

impl SizeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uninspected => "uninspected",
            Self::Match => "match",
            Self::Mismatch => "mismatch",
            Self::Unavailable => "unavailable",
        }
    }
}

impl std::fmt::Display for SizeState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatOptions {
    /// Zero selects the provisional auto policy. The final default is benchmark-gated.
    pub workers: u32,
    pub batch_size: u32,
}

impl Default for FileStatOptions {
    fn default() -> Self {
        Self {
            workers: 0,
            batch_size: 512,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileStatRequest {
    pub run_id: String,
    pub collection_id: String,
    pub archive_root: PathBuf,
    pub agent_name: String,
    pub agent_version: String,
    pub configuration_fingerprint: String,
    pub options: FileStatOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatWorkItem {
    pub content_object_id: String,
    pub sha256: String,
    pub expected_size_bytes: u64,
    pub source_locator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatObservation {
    pub content_object_id: String,
    pub sha256: String,
    pub source_locator: String,
    pub expected_locator: String,
    pub availability_state: AvailabilityState,
    pub size_state: SizeState,
    pub expected_size_bytes: u64,
    pub actual_size_bytes: Option<u64>,
    pub modified_unix_ns: Option<i64>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
}

impl FileStatObservation {
    pub fn unavailable(
        item: &FileStatWorkItem,
        expected_locator: String,
        availability_state: AvailabilityState,
        error_kind: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            content_object_id: item.content_object_id.clone(),
            sha256: item.sha256.clone(),
            source_locator: item.source_locator.clone(),
            expected_locator,
            availability_state,
            size_state: SizeState::Unavailable,
            expected_size_bytes: item.expected_size_bytes,
            actual_size_bytes: None,
            modified_unix_ns: None,
            error_kind: Some(error_kind.into()),
            error_message: Some(error_message.into()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatSummary {
    pub total_objects: u64,
    pub available_objects: u64,
    pub missing_objects: u64,
    pub unreadable_objects: u64,
    pub invalid_locator_objects: u64,
    pub non_regular_objects: u64,
    pub unsafe_reparse_objects: u64,
    pub io_error_objects: u64,
    pub size_matches: u64,
    pub size_mismatches: u64,
    pub expected_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatResult {
    pub run_id: String,
    pub collection_id: String,
    pub summary: FileStatSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatCheckpoint {
    pub last_sha256: Option<String>,
    pub completed_objects: u64,
    pub completed_bytes: u64,
    pub warnings: u64,
    pub errors: u64,
    pub sequence: u64,
}

impl FileStatCheckpoint {
    pub const fn empty() -> Self {
        Self {
            last_sha256: None,
            completed_objects: 0,
            completed_bytes: 0,
            warnings: 0,
            errors: 0,
            sequence: 0,
        }
    }
}

pub trait FileStatStore {
    fn load_file_stat_checkpoint(&self, run_id: &str)
    -> ProfilerResult<Option<FileStatCheckpoint>>;

    fn count_file_stat_objects(&self, collection_id: &str) -> ProfilerResult<(u64, u64)>;

    fn load_file_stat_batch(
        &self,
        collection_id: &str,
        after_sha256: Option<&str>,
        limit: u32,
    ) -> ProfilerResult<Vec<FileStatWorkItem>>;

    fn commit_file_stat_batch(
        &mut self,
        request: &FileStatRequest,
        observations: &[FileStatObservation],
        checkpoint: &FileStatCheckpoint,
    ) -> ProfilerResult<()>;

    fn finalize_file_stat(&mut self, request: &FileStatRequest) -> ProfilerResult<FileStatSummary>;
}

pub trait PhysicalObjectResolver: Send + Sync {
    fn inspect(&self, archive_root: &Path, item: &FileStatWorkItem) -> FileStatObservation;
}

pub trait FileStatRunner: Send + Sync {
    fn file_stat(
        &self,
        request: &FileStatRequest,
        store: &mut dyn FileStatStore,
        resolver: &dyn PhysicalObjectResolver,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<FileStatResult>;
}
