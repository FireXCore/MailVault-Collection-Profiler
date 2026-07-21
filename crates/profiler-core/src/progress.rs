use serde::{Deserialize, Serialize};

use crate::{ProfilerError, ProfilerResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStage {
    Preflight,
    SourceSnapshot,
    MetadataInventory,
    Reconciliation,
    FileStat,
    Fixity,
    FormatIdentification,
    Aggregation,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Planned,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressUnit {
    Checks,
    Pages,
    Rows,
    Objects,
    Bytes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub run_id: String,
    pub sequence: u64,
    pub stage: RunStage,
    pub stage_state: StageState,
    pub unit: ProgressUnit,
    pub completed_items: u64,
    pub total_items: Option<u64>,
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
    pub elapsed_ms: u64,
    pub instant_throughput: Option<f64>,
    pub smoothed_throughput: Option<f64>,
    pub eta_ms: Option<u64>,
    pub active_workers: u32,
    pub queue_depth: u32,
    pub warnings: u64,
    pub errors: u64,
    pub current_object_display: Option<String>,
    pub checkpoint_sequence: u64,
}

impl ProgressEvent {
    pub fn stage_started(run_id: impl Into<String>, stage: RunStage, unit: ProgressUnit) -> Self {
        Self {
            run_id: run_id.into(),
            sequence: 0,
            stage,
            stage_state: StageState::Running,
            unit,
            completed_items: 0,
            total_items: None,
            completed_bytes: 0,
            total_bytes: None,
            elapsed_ms: 0,
            instant_throughput: None,
            smoothed_throughput: None,
            eta_ms: None,
            active_workers: 1,
            queue_depth: 0,
            warnings: 0,
            errors: 0,
            current_object_display: None,
            checkpoint_sequence: 0,
        }
    }

    pub fn validate_monotonic_after(&self, previous: &Self) -> ProfilerResult<()> {
        if self.run_id != previous.run_id || self.stage != previous.stage {
            return Ok(());
        }
        if self.sequence <= previous.sequence
            || self.completed_items < previous.completed_items
            || self.completed_bytes < previous.completed_bytes
            || self.checkpoint_sequence < previous.checkpoint_sequence
        {
            return Err(ProfilerError::Internal(
                "non-monotonic progress event rejected".into(),
            ));
        }
        Ok(())
    }
}

pub trait ProgressSink: Send + Sync {
    fn send(&self, event: ProgressEvent) -> ProfilerResult<()>;
}

#[derive(Debug, Default)]
pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn send(&self, _event: ProgressEvent) -> ProfilerResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_must_be_monotonic_within_a_stage() {
        let mut previous =
            ProgressEvent::stage_started("run", RunStage::SourceSnapshot, ProgressUnit::Pages);
        previous.sequence = 1;
        previous.completed_items = 10;

        let mut next = previous.clone();
        next.sequence = 2;
        next.completed_items = 9;

        assert!(next.validate_monotonic_after(&previous).is_err());
    }
}
