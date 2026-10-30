// SPDX-License-Identifier: Apache-2.0

//! The SDK's one error type.

use std::fmt;

/// What went wrong, as a stable machine code plus one sentence for a person.
///
/// The code is the contract a host branches on (it is the same string in every
/// language binding); the message is for a log or a dialog and may be reworded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    code: ErrorCode,
    message: String,
}

/// The stable error codes. New codes are additive; a code is never repurposed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The bytes are not a document this build opens, or the open failed.
    Open,
    /// The requested format is not registered for this direction.
    UnsupportedFormat,
    /// The save failed, or was refused (for example an exact save of a changed document).
    Save,
    /// An edit was refused by the engine (bad position, protection, invariants).
    Edit,
    /// An argument was invalid: an unknown node id, a page past the end, bad JSON.
    InvalidArgument,
    /// Building a document from a specification failed.
    Build,
    /// A template could not be parsed or filled.
    Template,
    /// Rendering a page, or writing a PDF, failed.
    Render,
    /// The document's identity space is exhausted.
    Exhausted,
}

impl ErrorCode {
    /// The stable string form (`"open"`, `"unsupported-format"`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::UnsupportedFormat => "unsupported-format",
            Self::Save => "save",
            Self::Edit => "edit",
            Self::InvalidArgument => "invalid-argument",
            Self::Build => "build",
            Self::Template => "template",
            Self::Render => "render",
            Self::Exhausted => "exhausted",
        }
    }
}

impl Error {
    /// An error with a code and a message.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub(crate) fn edit(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Edit, message)
    }

    /// The stable code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// The sentence for a person.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for Error {}

/// The SDK's result type.
pub type Result<T> = std::result::Result<T, Error>;
