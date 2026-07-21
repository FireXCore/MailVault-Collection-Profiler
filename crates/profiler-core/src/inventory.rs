use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{ArchiveMetrics, ProfilerResult, ProgressSink};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryTable {
    Messages,
    MessageOccurrences,
    Participants,
    Blobs,
    Parts,
    MessageRelations,
}

impl InventoryTable {
    pub const ORDERED: [Self; 6] = [
        Self::Messages,
        Self::MessageOccurrences,
        Self::Participants,
        Self::Blobs,
        Self::Parts,
        Self::MessageRelations,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::MessageOccurrences => "message_occurrences",
            Self::Participants => "participants",
            Self::Blobs => "blobs",
            Self::Parts => "parts",
            Self::MessageRelations => "message_relations",
        }
    }
}

impl std::fmt::Display for InventoryTable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOptions {
    pub batch_size: u32,
}

impl Default for InventoryOptions {
    fn default() -> Self {
        Self { batch_size: 1_000 }
    }
}

#[derive(Debug, Clone)]
pub struct InventoryRequest {
    pub run_id: String,
    pub collection_id: String,
    pub source_snapshot_id: String,
    pub snapshot_database: PathBuf,
    pub archive_root: PathBuf,
    pub expected_metrics: ArchiveMetrics,
    pub options: InventoryOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryCheckpoint {
    pub table: InventoryTable,
    pub last_integer_key: Option<i64>,
    pub last_text_key: Option<String>,
    pub completed_rows: u64,
    pub sequence: u64,
}

impl InventoryCheckpoint {
    pub const fn empty(table: InventoryTable) -> Self {
        Self {
            table,
            last_integer_key: None,
            last_text_key: None,
            completed_rows: 0,
            sequence: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMessageRecord {
    pub id: i64,
    pub archive_id: String,
    pub account_id: i64,
    pub provider_thread_namespace: Option<String>,
    pub provider_thread_value: Option<String>,
    pub rfc_message_id: Option<String>,
    pub subject_raw: String,
    pub subject_normalized: String,
    pub header_date: Option<String>,
    pub raw_path: Option<String>,
    pub raw_sha256: Option<String>,
    pub raw_size_bytes: Option<u64>,
    pub parse_defects_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOccurrenceRecord {
    pub id: i64,
    pub message_id: i64,
    pub generation_id: i64,
    pub uid: i64,
    pub labels_json: String,
    pub internal_date: Option<String>,
    pub fetch_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceParticipantRecord {
    pub id: i64,
    pub message_id: i64,
    pub role: String,
    pub ordinal: i64,
    pub name: String,
    pub address: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBlobRecord {
    pub sha256: String,
    pub size_bytes: u64,
    pub detected_mime_type: String,
    pub storage_path: String,
    pub first_seen_at: String,
    pub last_verified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePartRecord {
    pub id: i64,
    pub message_id: i64,
    pub part_path: String,
    pub parent_part_path: Option<String>,
    pub role: String,
    pub declared_mime_type: String,
    pub detected_mime_type: Option<String>,
    pub content_disposition: Option<String>,
    pub content_id: Option<String>,
    pub filename_original: Option<String>,
    pub filename_safe: Option<String>,
    pub charset: Option<String>,
    pub transfer_encoding: Option<String>,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub blob_path: Option<String>,
    pub defects_json: String,
    pub message_date: Option<String>,
    pub sender_domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceRelationRecord {
    pub id: i64,
    pub source_message_id: i64,
    pub target_message_id: i64,
    pub relation_type: String,
    pub evidence_type: String,
    pub confidence: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InventoryBatch {
    Messages(Vec<SourceMessageRecord>),
    MessageOccurrences(Vec<SourceOccurrenceRecord>),
    Participants(Vec<SourceParticipantRecord>),
    Blobs(Vec<SourceBlobRecord>),
    Parts(Vec<SourcePartRecord>),
    MessageRelations(Vec<SourceRelationRecord>),
}

impl InventoryBatch {
    pub const fn table(&self) -> InventoryTable {
        match self {
            Self::Messages(_) => InventoryTable::Messages,
            Self::MessageOccurrences(_) => InventoryTable::MessageOccurrences,
            Self::Participants(_) => InventoryTable::Participants,
            Self::Blobs(_) => InventoryTable::Blobs,
            Self::Parts(_) => InventoryTable::Parts,
            Self::MessageRelations(_) => InventoryTable::MessageRelations,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Messages(rows) => rows.len(),
            Self::MessageOccurrences(rows) => rows.len(),
            Self::Participants(rows) => rows.len(),
            Self::Blobs(rows) => rows.len(),
            Self::Parts(rows) => rows.len(),
            Self::MessageRelations(rows) => rows.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub trait InventorySink {
    fn load_checkpoint(
        &self,
        run_id: &str,
        table: InventoryTable,
    ) -> ProfilerResult<Option<InventoryCheckpoint>>;

    fn ingest_batch(
        &mut self,
        request: &InventoryRequest,
        batch: InventoryBatch,
        checkpoint: &InventoryCheckpoint,
    ) -> ProfilerResult<()>;

    fn finalize_inventory(
        &mut self,
        request: &InventoryRequest,
    ) -> ProfilerResult<InventorySummary>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySummary {
    pub messages: u64,
    pub message_occurrences: u64,
    pub participants: u64,
    pub parts: u64,
    pub attachment_occurrences: u64,
    pub blob_rows: u64,
    pub content_objects: u64,
    pub content_occurrences: u64,
    pub message_relations: u64,
    pub zero_byte_content_objects: u64,
    pub same_hash_different_names: u64,
    pub same_name_different_hashes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryResult {
    pub run_id: String,
    pub source_snapshot_id: String,
    pub summary: InventorySummary,
}

pub trait InventorySource: Send + Sync {
    fn inventory(
        &self,
        request: &InventoryRequest,
        sink: &mut dyn InventorySink,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<InventoryResult>;
}
