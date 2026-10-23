// SPDX-License-Identifier: Apache-2.0

//! Refusals an agent can act on: a stable kebab-case code, one sentence, and
//! structured details (candidates for an ambiguous anchor, the current revision
//! for a stale plan, the losses a save would make).

use serde::Serialize;
use serde_json::Value;

/// A refusal from the AI layer. Never a panic and never a silent partial
/// result: every refusal leaves the document as it was.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AiError {
    /// Stable machine code, for example `anchor-ambiguous`.
    pub code: &'static str,
    /// One sentence a person or a model can act on.
    pub message: String,
    /// Structured context; `null` when there is none.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

impl AiError {
    /// A refusal with no details.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }

    /// The same refusal carrying `details`.
    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    /// An argument the caller must correct.
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid-argument", message)
    }

    /// The engine refused an operation. The engine's own sentence is kept; its
    /// internal `refused: …\u{1f}code` marker is split into message and code.
    #[must_use]
    pub fn engine(message: impl Into<String>) -> Self {
        let message = message.into();
        let stripped = message.strip_prefix("refused: ").unwrap_or(&message);
        match stripped.split_once('\u{1f}') {
            Some((sentence, code)) => Self::new("engine-refused", sentence.to_owned())
                .with_details(serde_json::json!({ "engineCode": code })),
            None => Self::new("engine-refused", stripped.to_owned()),
        }
    }
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AiError {}

/// Shorthand for results in this crate.
pub type AiResult<T> = Result<T, AiError>;
