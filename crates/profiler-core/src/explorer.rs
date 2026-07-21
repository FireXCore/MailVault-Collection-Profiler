use serde::{Deserialize, Serialize};

use crate::{AvailabilityState, FindingCategory, ReviewStatus, SizeState};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_state: Option<AvailabilityState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_state: Option<SizeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPageRequest {
    pub collection_id: String,
    pub run_id: String,
    #[serde(default)]
    pub filters: InventoryFilters,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_sha256: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryObjectRow {
    pub id: String,
    pub sha256: String,
    pub primary_filename: String,
    pub source_detected_mime_type: String,
    pub expected_size_bytes: u64,
    pub actual_size_bytes: Option<u64>,
    pub occurrence_count: u64,
    pub filename_variant_count: u64,
    pub message_count: u64,
    pub thread_count: u64,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub availability_state: AvailabilityState,
    pub size_state: SizeState,
    pub finding_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPage {
    pub items: Vec<InventoryObjectRow>,
    pub total_filtered: u64,
    pub next_after_sha256: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilenameVariantView {
    pub normalized_filename: String,
    pub display_filename: String,
    pub occurrence_count: u64,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OccurrenceView {
    pub occurrence_id: String,
    pub source_message_id: i64,
    pub source_part_id: i64,
    pub part_path: String,
    pub filename_original: Option<String>,
    pub role: String,
    pub sender_domain: Option<String>,
    pub message_date: Option<String>,
    pub subject: String,
    pub provider_thread_namespace: Option<String>,
    pub provider_thread_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingView {
    pub id: String,
    pub content_object_id: Option<String>,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub evidence: serde_json::Value,
    pub created_at: String,
    pub review_status: Option<ReviewStatus>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentObjectDetail {
    pub object: InventoryObjectRow,
    pub filename_variants: Vec<FilenameVariantView>,
    pub occurrences: Vec<OccurrenceView>,
    pub occurrence_total: u64,
    pub occurrences_truncated: bool,
    pub findings: Vec<FindingView>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingsSummary {
    pub total: u64,
    pub warnings: u64,
    pub errors: u64,
    pub informational: u64,
    pub zero_byte: u64,
    pub same_hash_different_names: u64,
    pub same_name_different_hashes: u64,
    pub missing: u64,
    pub size_mismatch: u64,
    pub invalid_locator: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingsPageRequest {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<FindingCategory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingsPage {
    pub items: Vec<FindingView>,
    pub summary: FindingsSummary,
    pub next_after_id: Option<String>,
    pub has_more: bool,
}
