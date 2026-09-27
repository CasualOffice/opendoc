//! The host contract's vocabulary, declared here and checked against the editor.
//!
//! `docs/126` phase 2 asks for **one schema and two transports**. Both transports
//! are JavaScript — the in-process session a host holds a reference to, and the
//! `postMessage` bridge — and both are built from `webapp/src/host_contract.mjs`.
//! This module is the same vocabulary in Rust, and a test beside it reads that
//! file and fails when the two disagree in either direction.
//!
//! # Why the vocabulary lives in this crate rather than only in the editor
//!
//! `docs/125` §2.2 F1 records the problem plainly: this crate is the typed host
//! facade `docs/83` promises, and the product does not use it — the only thing
//! that depends on it is `tools/opendoc-benchmark`. §8 of the same document
//! prescribes that host events be *"derived from `casual_doc_sdk::RuntimeEvent`
//! rather than invented"* and that error codes be this crate's, not new ones.
//!
//! Full convergence is not this phase. It is `docs/125` §9 row 5 — size **L** —
//! and it is blocked by `109` CQ-002: the live editing path applies
//! `casual-doc-edit`'s ops directly and references `casual_doc_transaction` zero
//! times, so this crate's five typed requests over the v0 normalized `Document`
//! are not the ops the editor runs. Wiring the editor to `DocumentSession` today
//! would mean reimplementing the v1 editing surface on a v0 model, which is a
//! migration and not a wiring.
//!
//! What phase 2 *can* do, and what this module is, is refuse to let the two grow
//! separate vocabularies while that convergence waits. So:
//!
//! * the seven host events are declared here, and [`HostEvent::from`] maps this
//!   crate's existing [`RuntimeEvent`](crate::RuntimeEvent) onto two of them,
//!   which is the derivation `docs/125` §8 asked for;
//! * the seven refusal codes are declared here, and [`HostRefusal::for_error`]
//!   maps this crate's [`ErrorCode`](crate::ErrorCode) onto them, so a native host
//!   branches on the same code a browser host does rather than on a second
//!   taxonomy;
//! * the four request verbs and the contract version are declared here too,
//!   because a transport that gained a verb on one side only is the exact drift
//!   this phase exists to make impossible.
//!
//! # What this module deliberately does NOT claim
//!
//! It does not claim the editor calls this crate. It does not carry command ids:
//! a command id addresses the *editor's* registry (`runCommandById`, 111 ribbon
//! controls with `data-command`), which is a chrome surface and has no meaning at
//! this layer. And it adds no engine operation — ADR-030 I2 — because it adds no
//! behaviour at all: it is a vocabulary plus two mappings.

use crate::error::ErrorCode;
use crate::event::RuntimeEvent;

/// Version of the host contract this crate declares.
///
/// Bumped when a command's requirement changes meaning, an event's payload loses
/// a field, or the envelope shape changes — never for an added command, event or
/// refusal code, which are additive by `docs/05` §12. The editor's
/// `CONTRACT_VERSION` must equal this.
pub const HOST_CONTRACT_VERSION: u32 = 1;

/// An event a host can hear from a document session.
///
/// The set is `docs/126` phase 2's minimum, in the order the editor declares it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HostEvent {
    /// The document is open and on screen. Carries the contract version, the
    /// resolved capabilities, the editing mode and how many commands are offered.
    Ready,
    /// An edit landed. Carries a revision handle and a dirty flag — never a
    /// document snapshot, because per-interaction work is O(1) in document size
    /// (`docs/107` §4).
    Change,
    /// The caret or the selection moved. Two positions.
    Selection,
    /// Bytes left the editor because the user saved.
    Save,
    /// Bytes left the editor because the user exported.
    Export,
    /// Something failed that was not a command's refusal.
    Error,
    /// A command was refused, with a code a host can branch on.
    Refusal,
}

impl HostEvent {
    /// Every host event, in the order the contract declares them.
    pub const ALL: [Self; 7] = [
        Self::Ready,
        Self::Change,
        Self::Selection,
        Self::Save,
        Self::Export,
        Self::Error,
        Self::Refusal,
    ];

    /// The event's name on the wire, which is also its name in the editor.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Change => "change",
            Self::Selection => "selection",
            Self::Save => "save",
            Self::Export => "export",
            Self::Error => "error",
            Self::Refusal => "refusal",
        }
    }
}

impl From<&RuntimeEvent> for HostEvent {
    /// The host event a runtime event becomes.
    ///
    /// This is the derivation `docs/125` §8 asks for: the journal's two variants
    /// are host events already, and the other five are things this crate's Phase 0
    /// session has no notion of (opening, saving, exporting, refusing) rather than
    /// things it names differently.
    fn from(event: &RuntimeEvent) -> Self {
        match event {
            RuntimeEvent::TransactionCommitted(_) => Self::Change,
            RuntimeEvent::SelectionChanged(_) => Self::Selection,
        }
    }
}

/// Why a host command was refused.
///
/// A code, never a sentence: the sentence is localised and belongs to the chrome,
/// and a host writing a branch on a refusal must not be matching on English.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HostRefusal {
    /// No such command id, in this state.
    UnknownCommand,
    /// The host was not granted what the command requires. Refused *before*
    /// dispatch, so the capability gates the API and not only the chrome.
    CapabilityWithheld,
    /// The command exists but cannot run right now, and the chrome's own reason
    /// says why.
    Unavailable,
    /// It ran and the engine refused it.
    EngineRefused,
    /// It raised; the message is the exception's.
    Threw,
    /// The envelope or the arguments were not the contract's.
    BadRequest,
    /// The transport gave up waiting. Host side only.
    Timeout,
}

impl HostRefusal {
    /// Every refusal code, in the order the contract declares them.
    pub const ALL: [Self; 7] = [
        Self::UnknownCommand,
        Self::CapabilityWithheld,
        Self::Unavailable,
        Self::EngineRefused,
        Self::Threw,
        Self::BadRequest,
        Self::Timeout,
    ];

    /// The code's spelling on the wire, which is also its spelling in the editor.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCommand => "unknown-command",
            Self::CapabilityWithheld => "capability-withheld",
            Self::Unavailable => "unavailable",
            Self::EngineRefused => "engine-refused",
            Self::Threw => "threw",
            Self::BadRequest => "bad-request",
            Self::Timeout => "timeout",
        }
    }

    /// The refusal a host is told about when this crate's facade fails.
    ///
    /// So that a native host and a browser host branch on the SAME code: an
    /// `SdkError` is what this crate returns, and a host contract refusal is what
    /// a host reads, and without a mapping the two would be separate taxonomies
    /// for one outcome.
    ///
    /// The split is between *what the caller asked* and *what the document
    /// allowed*: an argument or configuration this facade cannot accept never
    /// reaches the document, so it is a `bad-request`; everything the transaction
    /// layer declines — a stale revision, a position that is not in the document,
    /// an empty transaction, text that is not admissible, an exhausted history, a
    /// violated invariant, a malformed document, a resource limit — ran and was
    /// refused. [`ErrorCode::Internal`] maps to [`HostRefusal::Threw`] because it
    /// is a bug rather than a policy: the editor reports a command that raised the
    /// same way.
    #[must_use]
    pub const fn for_error(code: ErrorCode) -> Self {
        match code {
            ErrorCode::InvalidArgument | ErrorCode::InvalidConfiguration => Self::BadRequest,
            ErrorCode::MalformedDocument
            | ErrorCode::ResourceLimit
            | ErrorCode::StaleRevision
            | ErrorCode::InvalidPosition
            | ErrorCode::EmptyTransaction
            | ErrorCode::InvalidTextInput
            | ErrorCode::InvariantViolation
            | ErrorCode::HistoryEmpty => Self::EngineRefused,
            ErrorCode::Internal => Self::Threw,
        }
    }
}

/// What a host may ask of an editor, over either transport.
///
/// Four, and the same four the in-process session exposes as methods, so neither
/// transport has a verb the other lacks.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HostRequest {
    /// The contract, resolved against the live command registry.
    Describe,
    /// Run one command by id.
    Execute,
    /// The state of one command.
    Query,
    /// Liveness, and the cheapest possible one.
    Ping,
}

impl HostRequest {
    /// Every request verb, in the order the contract declares them.
    pub const ALL: [Self; 4] = [Self::Describe, Self::Execute, Self::Query, Self::Ping];

    /// The verb's spelling on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Describe => "describe",
            Self::Execute => "execute",
            Self::Query => "query",
            Self::Ping => "ping",
        }
    }
}
