use serde::{Deserialize, Serialize};

use crate::{ProfilerError, ProfilerResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Pending,
    Preflighting,
    Snapshotting,
    Ready,
    Running,
    Pausing,
    Paused,
    Cancelling,
    Cancelled,
    Succeeded,
    Failed,
}

impl RunState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Succeeded | Self::Failed)
    }
}

impl std::fmt::Display for RunState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Pending => "pending",
            Self::Preflighting => "preflighting",
            Self::Snapshotting => "snapshotting",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        })
    }
}

pub fn validate_transition(from: RunState, to: RunState) -> ProfilerResult<()> {
    let valid = match from {
        RunState::Pending => matches!(to, RunState::Preflighting | RunState::Cancelled),
        RunState::Preflighting => {
            matches!(
                to,
                RunState::Snapshotting | RunState::Failed | RunState::Cancelled
            )
        }
        RunState::Snapshotting => {
            matches!(
                to,
                RunState::Ready | RunState::Failed | RunState::Cancelling
            )
        }
        RunState::Ready => matches!(to, RunState::Running | RunState::Cancelled),
        RunState::Running => matches!(
            to,
            RunState::Pausing | RunState::Cancelling | RunState::Succeeded | RunState::Failed
        ),
        RunState::Pausing => matches!(to, RunState::Paused | RunState::Failed),
        RunState::Paused => matches!(to, RunState::Running | RunState::Cancelling),
        RunState::Cancelling => matches!(to, RunState::Cancelled | RunState::Failed),
        RunState::Cancelled | RunState::Succeeded | RunState::Failed => false,
    };

    if valid {
        Ok(())
    } else {
        Err(ProfilerError::InvalidRunTransition {
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states_reject_all_transitions() {
        for terminal in [RunState::Cancelled, RunState::Succeeded, RunState::Failed] {
            assert!(validate_transition(terminal, RunState::Pending).is_err());
        }
    }

    #[test]
    fn snapshot_happy_path_is_valid() {
        validate_transition(RunState::Pending, RunState::Preflighting).unwrap();
        validate_transition(RunState::Preflighting, RunState::Snapshotting).unwrap();
        validate_transition(RunState::Snapshotting, RunState::Ready).unwrap();
    }
}
