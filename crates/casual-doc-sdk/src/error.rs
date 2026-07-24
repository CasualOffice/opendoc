//! Stable public error taxonomy and internal error mapping.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use casual_doc_model as model;
use casual_doc_selection as selection;
use casual_doc_transaction as transaction;

use crate::value::Revision;

/// Stable public error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    /// `ODC-0001`.
    InvalidArgument,
    /// `ODC-0002`.
    InvalidConfiguration,
    /// `ODC-1001`.
    MalformedDocument,
    /// `ODC-1003`.
    ResourceLimit,
    /// `ODC-2001`.
    StaleRevision,
    /// `ODC-2002`.
    InvalidPosition,
    /// `ODC-2003`.
    EmptyTransaction,
    /// `ODC-2004`.
    InvalidTextInput,
    /// `ODC-2005`.
    InvariantViolation,
    /// `ODC-2006`.
    HistoryEmpty,
    /// `ODC-9001`.
    Internal,
}

impl ErrorCode {
    /// Returns the non-recycled registry code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "ODC-0001",
            Self::InvalidConfiguration => "ODC-0002",
            Self::MalformedDocument => "ODC-1001",
            Self::ResourceLimit => "ODC-1003",
            Self::StaleRevision => "ODC-2001",
            Self::InvalidPosition => "ODC-2002",
            Self::EmptyTransaction => "ODC-2003",
            Self::InvalidTextInput => "ODC-2004",
            Self::InvariantViolation => "ODC-2005",
            Self::HistoryEmpty => "ODC-2006",
            Self::Internal => "ODC-9001",
        }
    }
}

/// Public failure severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorSeverity {
    /// Recoverable limitation.
    Warning,
    /// Requested operation failed; session remains valid.
    Error,
    /// Session cannot safely continue.
    Fatal,
}

/// Error crossing the Rust SDK facade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkError {
    code: ErrorCode,
    severity: ErrorSeverity,
    message: String,
    context: BTreeMap<String, String>,
}

impl SdkError {
    pub(crate) fn new(
        code: ErrorCode,
        severity: ErrorSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, ErrorSeverity::Fatal, message)
    }

    pub(crate) fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Returns the stable error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the public severity.
    #[must_use]
    pub const fn severity(&self) -> ErrorSeverity {
        self.severity
    }

    /// Returns a safe host-facing message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns redacted structured context.
    #[must_use]
    pub const fn context(&self) -> &BTreeMap<String, String> {
        &self.context
    }
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl Error for SdkError {}

pub(crate) fn map_transaction_error(error: transaction::TransactionError) -> SdkError {
    match error {
        transaction::TransactionError::StaleRevision { expected, actual } => SdkError::new(
            ErrorCode::StaleRevision,
            ErrorSeverity::Error,
            "transaction base revision does not match the session",
        )
        .with_context("expected_revision", expected.get().to_string())
        .with_context("actual_revision", actual.get().to_string()),
        transaction::TransactionError::EmptyTransaction => SdkError::new(
            ErrorCode::EmptyTransaction,
            ErrorSeverity::Error,
            "transaction has no effective operation",
        ),
        transaction::TransactionError::InvalidPosition { node, offset } => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "position does not resolve to a valid grapheme boundary",
        )
        .with_context("node_id", node.to_string())
        .with_context("grapheme_offset", offset.to_string()),
        transaction::TransactionError::InvalidRange {
            start_node,
            start,
            end_node,
            end,
        } => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "range is not valid for this operation",
        )
        .with_context("start_node_id", start_node.to_string())
        .with_context("start_offset", start.to_string())
        .with_context("end_node_id", end_node.to_string())
        .with_context("end_offset", end.to_string()),
        transaction::TransactionError::InvalidStructure => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "paragraph structure does not satisfy the operation",
        ),
        transaction::TransactionError::InvalidTextInput => SdkError::new(
            ErrorCode::InvalidTextInput,
            ErrorSeverity::Error,
            "text contains a control that requires a structural command",
        ),
        transaction::TransactionError::TextTooLong => SdkError::new(
            ErrorCode::InvalidArgument,
            ErrorSeverity::Error,
            "inserted text exceeds the supported length",
        ),
        transaction::TransactionError::RevisionExhausted => {
            SdkError::internal("session revision is exhausted")
        }
        transaction::TransactionError::Model(_) => SdkError::new(
            ErrorCode::InvariantViolation,
            ErrorSeverity::Fatal,
            "transaction application failed a document invariant",
        ),
    }
}

pub(crate) fn stale_revision_error(
    expected: Revision,
    actual: transaction::RevisionId,
) -> SdkError {
    SdkError::new(
        ErrorCode::StaleRevision,
        ErrorSeverity::Error,
        "base revision does not match the session",
    )
    .with_context("expected_revision", expected.get().to_string())
    .with_context("actual_revision", actual.get().to_string())
}

pub(crate) fn map_requested_selection_error(error: selection::SelectionError) -> SdkError {
    match error {
        selection::SelectionError::InvalidPosition { node, offset } => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "selection endpoint does not resolve to a valid grapheme boundary",
        )
        .with_context("node_id", node.to_string())
        .with_context("grapheme_offset", offset.to_string()),
        selection::SelectionError::EmptyDocument => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "selection requires a document paragraph",
        ),
        selection::SelectionError::OffsetOverflow { node } => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "selection endpoint exceeds the supported offset range",
        )
        .with_context("node_id", node.to_string()),
    }
}

pub(crate) fn map_initial_selection_error(_error: selection::SelectionError) -> SdkError {
    SdkError::new(
        ErrorCode::InvariantViolation,
        ErrorSeverity::Fatal,
        "validated document could not produce an initial selection",
    )
}

pub(crate) fn map_mapped_selection_error(_error: selection::SelectionError) -> SdkError {
    SdkError::new(
        ErrorCode::InvariantViolation,
        ErrorSeverity::Fatal,
        "transaction produced an invalid mapped selection",
    )
}

pub(crate) fn map_snapshot_error(error: model::SnapshotError) -> SdkError {
    match error {
        model::SnapshotError::InvalidLimitConfiguration {
            limit,
            value,
            hard_ceiling,
        } => SdkError::new(
            ErrorCode::InvalidConfiguration,
            ErrorSeverity::Error,
            "normalized snapshot limit exceeds the runtime hard ceiling",
        )
        .with_context("limit_name", limit)
        .with_context("limit_value", value.to_string())
        .with_context("hard_ceiling", hard_ceiling.to_string()),
        model::SnapshotError::LimitExceeded {
            limit,
            observed,
            allowed,
        } => SdkError::new(
            ErrorCode::ResourceLimit,
            ErrorSeverity::Error,
            "normalized snapshot resource limit exceeded",
        )
        .with_context("limit_name", limit)
        .with_context("observed_value", observed.to_string())
        .with_context("limit_value", allowed.to_string()),
        model::SnapshotError::MalformedJson | model::SnapshotError::InvalidModel(_) => {
            SdkError::new(
                ErrorCode::MalformedDocument,
                ErrorSeverity::Error,
                "normalized document is malformed or violates schema v0",
            )
        }
        model::SnapshotError::Serialization => {
            SdkError::internal("normalized document serialization failed")
        }
    }
}
