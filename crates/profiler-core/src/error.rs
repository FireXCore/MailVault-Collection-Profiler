use std::{collections::BTreeMap, io, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type ProfilerResult<T> = Result<T, ProfilerError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArgument,
    InvalidPath,
    Io,
    Sqlite,
    IncompatibleSource,
    SourceBusy,
    SourceChanged,
    InsufficientSpace,
    InvalidRunTransition,
    ProgressDelivery,
    WorkspaceNotFound,
    WorkspaceInvalidLayout,
    WorkspaceDatabaseMissing,
    WorkspaceDatabaseCorrupted,
    WorkspaceSchemaNewerThanApplication,
    WorkspaceMigrationRequired,
    WorkspaceMigrationFailed,
    WorkspaceLocked,
    WorkspaceOpenedReadOnly,
    WorkspaceSourceOverlap,
    RunNotFound,
    RunNotBrowsable,
    FindingNotFound,
    InvalidReviewStatus,
    ReviewNoteRequired,
    ReviewNoteTooLong,
    ReviewHistoryIntegrityFailure,
    ReviewWriteNotAllowed,
    SanitizedExportFailed,
    FormatToolNotFound,
    FormatToolIncompatible,
    FormatRunNotFound,
    FormatRunFailed,
    FormatRunAlreadyActive,
    FormatOutputInvalid,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReport {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum ProfilerError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("invalid path: {message}")]
    InvalidPath { message: String, path: PathBuf },

    #[error("I/O failure while {operation} at {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("SQLite failure while {operation}: {message}")]
    Sqlite {
        operation: &'static str,
        message: String,
    },

    #[error("incompatible MailVault source: {0}")]
    IncompatibleSource(String),

    #[error("MailVault source is busy: {0}")]
    SourceBusy(String),

    #[error("MailVault source changed while a snapshot was being created")]
    SourceChanged,

    #[error(
        "insufficient workspace space: required {required_bytes} bytes, available {available_bytes} bytes"
    )]
    InsufficientSpace {
        required_bytes: u64,
        available_bytes: u64,
    },

    #[error("invalid run-state transition from {from} to {to}")]
    InvalidRunTransition { from: String, to: String },

    #[error("failed to deliver progress event: {0}")]
    ProgressDelivery(String),

    #[error("{message}")]
    Contract {
        code: ErrorCode,
        message: String,
        retryable: bool,
        context: BTreeMap<String, String>,
    },

    #[error("internal profiler error: {0}")]
    Internal(String),
}

impl ProfilerError {
    pub fn contract(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self::Contract {
            code,
            message: message.into(),
            retryable,
            context: BTreeMap::new(),
        }
    }

    pub fn contract_with_context(
        code: ErrorCode,
        message: impl Into<String>,
        retryable: bool,
        context: BTreeMap<String, String>,
    ) -> Self {
        Self::Contract {
            code,
            message: message.into(),
            retryable,
            context,
        }
    }

    pub fn report(&self) -> ErrorReport {
        let mut context = BTreeMap::new();
        let (code, retryable) = match self {
            Self::InvalidArgument(_) => (ErrorCode::InvalidArgument, false),
            Self::InvalidPath { path, .. } => {
                context.insert("path".into(), path.to_string_lossy().into_owned());
                (ErrorCode::InvalidPath, false)
            }
            Self::Io {
                operation, path, ..
            } => {
                context.insert("operation".into(), (*operation).into());
                context.insert("path".into(), path.to_string_lossy().into_owned());
                (ErrorCode::Io, true)
            }
            Self::Sqlite { operation, .. } => {
                context.insert("operation".into(), (*operation).into());
                (ErrorCode::Sqlite, true)
            }
            Self::IncompatibleSource(_) => (ErrorCode::IncompatibleSource, false),
            Self::SourceBusy(_) => (ErrorCode::SourceBusy, true),
            Self::SourceChanged => (ErrorCode::SourceChanged, true),
            Self::InsufficientSpace {
                required_bytes,
                available_bytes,
            } => {
                context.insert("requiredBytes".into(), required_bytes.to_string());
                context.insert("availableBytes".into(), available_bytes.to_string());
                (ErrorCode::InsufficientSpace, true)
            }
            Self::InvalidRunTransition { from, to } => {
                context.insert("from".into(), from.clone());
                context.insert("to".into(), to.clone());
                (ErrorCode::InvalidRunTransition, false)
            }
            Self::ProgressDelivery(_) => (ErrorCode::ProgressDelivery, true),
            Self::Contract {
                code,
                retryable,
                context: contract_context,
                ..
            } => {
                context.extend(contract_context.clone());
                (*code, *retryable)
            }
            Self::Internal(_) => (ErrorCode::Internal, false),
        };

        ErrorReport {
            code,
            message: self.to_string(),
            retryable,
            context,
        }
    }
}
