//! Host-facing OpenDoc engine and document-session facade.
//!
//! The Phase 0 API creates a blank document and applies grapheme-aware text
//! insertions through atomic transactions:
//!
//! ```
//! use std::collections::BTreeSet;
//!
//! use casual_doc_sdk::{
//!     Affinity, BlockSnapshot, Engine, EngineConfig, InsertTextRequest, Position,
//! };
//!
//! let engine = Engine::new(EngineConfig::default())?;
//! let session = engine.create_blank()?;
//! let snapshot = session.snapshot()?;
//! let paragraph = match &snapshot.body[0] {
//!     BlockSnapshot::Paragraph(paragraph) => paragraph.id.clone(),
//! };
//!
//! session.insert_text(InsertTextRequest {
//!     base_revision: snapshot.revision,
//!     at: Position {
//!         node: paragraph,
//!         grapheme_offset: 0,
//!         affinity: Affinity::After,
//!     },
//!     text: "Hello".to_owned(),
//!     marks: BTreeSet::new(),
//! })?;
//!
//! # Ok::<(), casual_doc_sdk::SdkError>(())
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod command;
mod config;
mod error;
mod event;
mod selection;
mod session;
mod snapshot;
mod value;

#[cfg(test)]
mod tests;

pub use command::{
    DeleteRangeRequest, InsertTextRequest, JoinParagraphRequest, MappingStep, PositionMap,
    SetSelectionRequest, SplitParagraphRequest, TransactionResult,
};
pub use config::{
    Engine, EngineConfig, NormalizedSnapshotLimits, OpenNormalizedOptions, SessionId,
};
pub use error::{ErrorCode, ErrorSeverity, SdkError};
pub use event::{
    EVENT_JOURNAL_CAPACITY, EventBatch, EventSequence, RuntimeEvent, SelectionChangeReason,
    SelectionChangedEvent, SequencedEvent, TransactionCommittedEvent, TransactionOrigin,
};
pub use selection::{Affinity, Position, Range, SelectionSnapshot};
pub use session::{DocumentSession, Subscription};
pub use snapshot::{BlockSnapshot, DocumentSnapshot, InlineSnapshot, ParagraphSnapshot};
pub use value::{Mark, NodeId, Revision};
