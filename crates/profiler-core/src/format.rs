use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatState {
    Uninspected,
    Identified,
    Unknown,
    Ambiguous,
    Empty,
    SkippedUnavailable,
    ToolError,
}

impl FormatState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uninspected => "uninspected",
            Self::Identified => "identified",
            Self::Unknown => "unknown",
            Self::Ambiguous => "ambiguous",
            Self::Empty => "empty",
            Self::SkippedUnavailable => "skipped_unavailable",
            Self::ToolError => "tool_error",
        }
    }
}

impl std::fmt::Display for FormatState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatToolIdentity {
    pub tool_name: String,
    pub tool_version: String,
    pub executable_path: String,
    pub executable_sha256: String,
    pub signature_path: String,
    pub signature_sha256: Option<String>,
    pub signature_version: String,
    pub signature_created: Option<String>,
    pub identifiers: Vec<FormatIdentifierIdentity>,
    pub probed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatIdentifierIdentity {
    pub name: String,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOptions {
    pub batch_size: u32,
    pub workers: u32,
    pub timeout_seconds: u64,
    pub resume: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            batch_size: 2_048,
            workers: 0,
            timeout_seconds: 900,
            resume: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormatIdentificationRequest {
    pub baseline_run_id: String,
    pub workspace_root: PathBuf,
    pub profiler_database: PathBuf,
    pub archive_root: PathBuf,
    pub siegfried_path: Option<PathBuf>,
    pub signature_path: Option<PathBuf>,
    pub options: FormatOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatWorkItem {
    pub content_object_id: String,
    pub sha256: String,
    pub expected_size_bytes: u64,
    pub source_mime_type: String,
    pub canonical_path_display: String,
    pub availability_state: String,
    pub preferred_extension: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatRunRegistration {
    pub format_run_id: String,
    pub collection_id: String,
    pub total_objects: u64,
    pub eligible_objects: u64,
    pub total_bytes: u64,
    pub resume_after_sha256: Option<String>,
    pub completed_objects: u64,
    pub completed_bytes: u64,
    pub checkpoint_sequence: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct FormatRunStartRequest<'a> {
    pub baseline_run_id: &'a str,
    pub tool: &'a FormatToolIdentity,
    pub configuration_fingerprint: &'a str,
    pub batch_size: u32,
    pub worker_count: u32,
    pub timeout_seconds: u64,
    pub resume: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatMatch {
    pub namespace: String,
    pub identifier: String,
    pub format_name: String,
    pub format_version: String,
    pub mime_type: String,
    pub format_class: Option<String>,
    pub basis: String,
    pub warning: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatObservation {
    pub content_object_id: String,
    pub sha256: String,
    pub state: FormatState,
    pub source_mime_type: String,
    pub preferred_extension: Option<String>,
    pub staging_mode: String,
    pub primary_identifier: Option<String>,
    pub primary_format_name: Option<String>,
    pub primary_format_version: Option<String>,
    pub primary_mime_type: Option<String>,
    pub match_count: u64,
    pub extension_checked: bool,
    pub extension_mismatch: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub matches: Vec<FormatMatch>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatSummary {
    pub baseline_run_id: String,
    pub latest_format_run_id: Option<String>,
    pub latest_run_state: Option<String>,
    pub total_objects: u64,
    pub eligible_objects: u64,
    pub completed_objects: u64,
    pub total_bytes: u64,
    pub completed_bytes: u64,
    pub identified: u64,
    pub unknown: u64,
    pub ambiguous: u64,
    pub empty: u64,
    pub skipped_unavailable: u64,
    pub tool_errors: u64,
    pub extension_mismatches: u64,
    pub distinct_puids: u64,
    pub tool: Option<FormatToolIdentity>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<FormatState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub puid: Option<String>,
    #[serde(default)]
    pub mismatch_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatPageRequest {
    pub baseline_run_id: String,
    #[serde(default)]
    pub filters: FormatFilters,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_sha256: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatObjectRow {
    pub content_object_id: String,
    pub sha256: String,
    pub primary_filename: String,
    pub expected_size_bytes: u64,
    pub source_mime_type: String,
    pub state: FormatState,
    pub primary_identifier: Option<String>,
    pub primary_format_name: Option<String>,
    pub primary_format_version: Option<String>,
    pub primary_mime_type: Option<String>,
    pub match_count: u64,
    pub extension_checked: bool,
    pub extension_mismatch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatPage {
    pub items: Vec<FormatObjectRow>,
    pub total_filtered: u64,
    pub next_after_sha256: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatIdentificationResult {
    pub format_run_id: String,
    pub baseline_run_id: String,
    pub configuration_fingerprint: String,
    pub summary: FormatSummary,
}
