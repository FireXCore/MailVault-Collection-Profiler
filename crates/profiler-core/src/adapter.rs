use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{PreflightReport, ProfilerResult, ProgressSink, SourceSnapshotManifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotOptions {
    pub pages_per_step: i32,
    pub busy_retry_ms: u64,
    pub busy_timeout_ms: u64,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            pages_per_step: 256,
            busy_retry_ms: 50,
            busy_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotRequest {
    pub run_id: String,
    pub archive_root: PathBuf,
    pub workspace_root: PathBuf,
    pub options: SnapshotOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotResult {
    pub snapshot_directory: String,
    pub manifest_path: String,
    pub manifest: SourceSnapshotManifest,
}

pub trait CollectionAdapter: Send + Sync {
    fn kind(&self) -> &'static str;

    fn preflight(&self, archive_root: &Path) -> ProfilerResult<PreflightReport>;

    fn create_snapshot(
        &self,
        request: &SnapshotRequest,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<SnapshotResult>;
}
