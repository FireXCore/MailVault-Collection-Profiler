use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ContentObjectDetail, FindingView, FindingsSummary, InventorySummary, ProfilerError,
    ProfilerResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAccessMode {
    ReadWrite,
    ReadOnlyLocked,
    ReadOnlyCompatibility,
}

impl WorkspaceAccessMode {
    pub const fn allows_review_write(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceOpenMode {
    ReadWritePreferred,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCompatibility {
    Compatible,
    MigrationRequired,
    NewerThanApplication,
    InvalidLayout,
    MissingProfilerDatabase,
    CorruptedProfilerDatabase,
    SourceWorkspaceOverlap,
    IncompleteMigration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDatabaseInspection {
    pub application_id: i64,
    pub schema_version: i64,
    pub integrity_ok: bool,
    pub migration_state: Option<String>,
    pub workspace_id: Option<String>,
    pub created_by_version: Option<String>,
    pub last_migrated_by_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInspection {
    pub root_path: PathBuf,
    pub profiler_database: PathBuf,
    pub compatibility: WorkspaceCompatibility,
    pub schema_version: Option<i64>,
    pub supported_schema_version: i64,
    pub migration_required: bool,
    pub lock_active: bool,
    pub run_count: u64,
    pub workspace_id: Option<String>,
    pub created_by_version: Option<String>,
    pub last_migrated_by_version: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    pub workspace_id: String,
    pub root_path: PathBuf,
    pub profiler_database: PathBuf,
    pub schema_version: i64,
    pub created_at: String,
    pub created_by_version: String,
    pub last_migrated_at: Option<String>,
    pub last_migrated_by_version: Option<String>,
    pub access_mode: WorkspaceAccessMode,
    pub review_integrity_valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunCatalogStatus {
    Completed,
    Failed,
    Interrupted,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    pub total_findings: u64,
    pub reviewable_findings: u64,
    pub unreviewed: u64,
    pub acknowledged: u64,
    pub expected: u64,
    pub needs_investigation: u64,
    pub resolved_externally: u64,
    pub reviewed_findings: u64,
    pub review_completion_percent: u32,
    pub warnings_remaining: u64,
    pub errors_remaining: u64,
    pub informational_evidence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCatalogEntry {
    pub run_id: String,
    pub collection_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub status: RunCatalogStatus,
    pub persisted_state: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub app_version: String,
    pub archive_fingerprint: Option<String>,
    pub source_schema_version: Option<u32>,
    pub messages: u64,
    pub mime_parts: u64,
    pub blobs: u64,
    pub findings: u64,
    pub errors: u64,
    pub warnings: u64,
    pub review_summary: ReviewSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenWorkspaceResult {
    pub descriptor: WorkspaceDescriptor,
    pub runs: Vec<RunCatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRunContext {
    pub run: RunCatalogEntry,
    pub collection_id: String,
    pub source_snapshot_id: String,
    pub inventory: InventorySummary,
    pub findings: FindingsSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Acknowledged,
    Expected,
    NeedsInvestigation,
    ResolvedExternally,
}

impl ReviewStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Acknowledged => "acknowledged",
            Self::Expected => "expected",
            Self::NeedsInvestigation => "needs_investigation",
            Self::ResolvedExternally => "resolved_externally",
        }
    }

    pub const fn requires_note(self) -> bool {
        matches!(self, Self::NeedsInvestigation | Self::ResolvedExternally)
    }
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ReviewStatus {
    type Err = ProfilerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "acknowledged" => Ok(Self::Acknowledged),
            "expected" => Ok(Self::Expected),
            "needs_investigation" => Ok(Self::NeedsInvestigation),
            "resolved_externally" => Ok(Self::ResolvedExternally),
            _ => Err(ProfilerError::contract(
                crate::ErrorCode::InvalidReviewStatus,
                "review status is not supported",
                false,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    StatusSet,
    StatusCleared,
    NoteAdded,
}

impl ReviewAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StatusSet => "status_set",
            Self::StatusCleared => "status_cleared",
            Self::NoteAdded => "note_added",
        }
    }
}

impl FromStr for ReviewAction {
    type Err = ProfilerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "status_set" => Ok(Self::StatusSet),
            "status_cleared" => Ok(Self::StatusCleared),
            "note_added" => Ok(Self::NoteAdded),
            _ => Err(ProfilerError::contract(
                crate::ErrorCode::ReviewHistoryIntegrityFailure,
                "review history contains an unknown action",
                false,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewActorKind {
    LocalInteractiveUser,
    LocalCliUser,
}

impl ReviewActorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalInteractiveUser => "local_interactive_user",
            Self::LocalCliUser => "local_cli_user",
        }
    }
}

impl FromStr for ReviewActorKind {
    type Err = ProfilerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "local_interactive_user" => Ok(Self::LocalInteractiveUser),
            "local_cli_user" => Ok(Self::LocalCliUser),
            _ => Err(ProfilerError::contract(
                crate::ErrorCode::ReviewHistoryIntegrityFailure,
                "review history contains an unknown actor kind",
                false,
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingReviewEvent {
    pub event_id: String,
    pub run_id: String,
    pub finding_id: String,
    pub sequence: u64,
    pub action: ReviewAction,
    pub previous_status: Option<ReviewStatus>,
    pub new_status: Option<ReviewStatus>,
    pub note: Option<String>,
    pub actor_kind: ReviewActorKind,
    pub actor_label: Option<String>,
    pub occurred_at: String,
    pub previous_event_hash: Option<String>,
    pub event_hash: String,
}

impl FindingReviewEvent {
    pub fn compute_hash(&self) -> ProfilerResult<String> {
        let canonical = serde_json::to_vec(&(
            self.event_id.as_str(),
            self.run_id.as_str(),
            self.finding_id.as_str(),
            self.sequence,
            self.action.as_str(),
            self.previous_status.map(ReviewStatus::as_str),
            self.new_status.map(ReviewStatus::as_str),
            self.note.as_deref(),
            self.actor_kind.as_str(),
            self.actor_label.as_deref(),
            self.occurred_at.as_str(),
            self.previous_event_hash.as_deref(),
        ))
        .map_err(|error| {
            ProfilerError::Internal(format!("serializing review hash input: {error}"))
        })?;
        let mut digest = Sha256::new();
        digest.update(canonical);
        Ok(hex::encode(digest.finalize()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingReviewHistory {
    pub finding_id: String,
    pub current_status: Option<ReviewStatus>,
    pub latest_note: Option<String>,
    pub integrity_valid: bool,
    pub events: Vec<FindingReviewEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingDetail {
    pub finding: FindingView,
    pub object: Option<ContentObjectDetail>,
    pub review: FindingReviewHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCategory {
    RequiresAttention,
    InformationalEvidence,
    Reviewed,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedFindingRow {
    pub finding_token: String,
    pub object_token: Option<String>,
    pub code: String,
    pub severity: String,
    pub review_status: Option<ReviewStatus>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedRunSummary {
    pub format_version: u32,
    pub generated_at: String,
    pub run_id: String,
    pub application_version: String,
    pub workspace_schema_version: i64,
    pub run_status: RunCatalogStatus,
    pub source_mutation: String,
    pub inventory: InventorySummary,
    pub findings: FindingsSummary,
    pub review: ReviewSummary,
    pub findings_by_code: BTreeMap<String, u64>,
    pub review_by_status: BTreeMap<String, u64>,
}

pub fn normalize_review_note(note: Option<&str>) -> ProfilerResult<Option<String>> {
    let Some(note) = note else {
        return Ok(None);
    };
    let normalized = note.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 4_000 {
        return Err(ProfilerError::contract(
            crate::ErrorCode::ReviewNoteTooLong,
            "review notes cannot exceed 4,000 characters",
            false,
        ));
    }
    if trimmed
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(ProfilerError::InvalidArgument(
            "review notes contain unsupported control characters".into(),
        ));
    }
    Ok(Some(trimmed.to_owned()))
}

pub fn validate_status_note(
    status: ReviewStatus,
    note: Option<&str>,
) -> ProfilerResult<Option<String>> {
    let note = normalize_review_note(note)?;
    if status.requires_note() && note.is_none() {
        return Err(ProfilerError::contract(
            crate::ErrorCode::ReviewNoteRequired,
            "a review note is required for this status",
            false,
        ));
    }
    Ok(note)
}

pub fn redaction_token(value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())[..12].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_status_and_note_contract_is_enforced() {
        assert_eq!(
            ReviewStatus::from_str("needs_investigation").unwrap(),
            ReviewStatus::NeedsInvestigation
        );
        assert!(validate_status_note(ReviewStatus::NeedsInvestigation, None).is_err());
        assert_eq!(
            validate_status_note(
                ReviewStatus::NeedsInvestigation,
                Some("  verify backup\r\nthen compare metadata  ")
            )
            .unwrap(),
            Some("verify backup\nthen compare metadata".into())
        );
        assert!(normalize_review_note(Some(&"x".repeat(4_001))).is_err());
    }

    #[test]
    fn review_event_hash_is_stable_and_change_sensitive() {
        let event = FindingReviewEvent {
            event_id: "event-1".into(),
            run_id: "run-1".into(),
            finding_id: "finding-1".into(),
            sequence: 1,
            action: ReviewAction::StatusSet,
            previous_status: None,
            new_status: Some(ReviewStatus::Expected),
            note: Some("known filename relationship".into()),
            actor_kind: ReviewActorKind::LocalInteractiveUser,
            actor_label: None,
            occurred_at: "2026-07-19T12:00:00Z".into(),
            previous_event_hash: None,
            event_hash: String::new(),
        };
        let first = event.compute_hash().unwrap();
        let second = event.compute_hash().unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);

        let mut changed = event;
        changed.sequence = 2;
        assert_ne!(first, changed.compute_hash().unwrap());
    }

    #[test]
    fn redaction_tokens_are_short_stable_and_non_reversible_identifiers() {
        let token = redaction_token("objects/blobs/sha256/private-locator");
        assert_eq!(token.len(), 12);
        assert_eq!(
            token,
            redaction_token("objects/blobs/sha256/private-locator")
        );
        assert_ne!(token, redaction_token("objects/blobs/sha256/other-locator"));
    }
}
