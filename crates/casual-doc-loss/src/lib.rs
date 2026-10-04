// SPDX-License-Identifier: Apache-2.0

//! The format-neutral conversion-loss taxonomy: `35-DISPOSITION-TAXONOMY.md` as
//! types, the preservation ledger that licenses its `preserved` claims, and the
//! aggregating sink every format adapter reports through.
//!
//! # Why this is its own crate
//!
//! "Direct OOXML with verbatim retention" is one of this project's structural
//! advantages over a converter pipeline — but **only if the loss that retention
//! does not cover is detected, named, and auditable**. A converter that round
//! trips through an intermediate model silently drops whatever that model lacks;
//! the thing that makes this engine different is not that it loses less, it is
//! that it says what it lost and can prove where the remainder went.
//!
//! That proof used to live inside `casual-doc-import`, so it was reachable only
//! from the DOCX path. Two consequences followed, and the second is the
//! expensive one:
//!
//! - ODT could not make ledger-validated preservation claims at all, because the
//!   taxonomy it would have reported into was DOCX-private. It therefore claimed
//!   `preserved` on the honour system, with nothing a host could audit.
//! - A second document-shaped importer — a presentation reader, say — would have
//!   hit the same wall, and the obvious workaround is a second taxonomy. The
//!   repository already carries one of those (`casual-doc-rtf`'s private
//!   `RtfModelOutcome`/`RtfRetentionOutcome` pair), which is the shape this crate
//!   exists to stop multiplying.
//!
//! So the vocabulary lives here, below every format adapter, and the adapters
//! depend on it rather than on each other.
//!
//! # What is deliberately *not* here
//!
//! Nothing in this crate's public API names a markup vocabulary. There are no
//! XML byte-slice names, no element/attribute parsing, no no-op classification,
//! and no `w:`/`a:`/`text:` prefixed identifiers. A finding arrives as a feature
//! string, a [`FeatureLocation`] and a [`Finding`], all of which the *adapter*
//! builds — because deciding that `<a:effectLst/>` means "no effects" while
//! `<a:effectLst>…</a:effectLst>` means a lost shadow is a DrawingML fact, and a
//! format-neutral crate that knew it would be a format crate wearing a neutral
//! name.
//!
//! The seam is drawn exactly there, and the test of it is that this crate
//! compiles with no dependencies at all.
//!
//! # The three properties the taxonomy is built around
//!
//! - **Only unrecovered meaning is enumerated.** A construct an adapter captures
//!   in full raises nothing, so an ordinary document imports with an *empty*
//!   report. A report that fires on healthy documents is one every caller learns
//!   to ignore, and an ignored report is worse than none. The corollary is that
//!   [`ModelOutcome::Mapped`] is unreachable in a report by construction, not by
//!   omission.
//! - **The disposition pair is chosen per construct, never per mode.** Each call
//!   site states what happened to *that* construct as a [`Finding`], and
//!   [`SourceRetention`] resolves it against what retains the source. One
//!   semantic import legitimately yields `not-retained`, `rejected`, `blocked`
//!   and `preserved` on different constructs.
//! - **Only the nine legal pairs are representable.** `35` says any other
//!   pairing "is an internal error and must fail import, not be reported";
//!   naming the legal pairs ([`Disposition`]) makes the illegal ones impossible
//!   to construct, which is stronger than checking for them afterwards. What is
//!   left to validate is the preservation rule, which a type cannot enforce:
//!   `preserved` is legal *only* when the entry references a validated
//!   [`PreservationLedger`] record ([`CompatibilityReport::validate`]).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod ledger;
mod report;
mod reporter;
mod taxonomy;

pub use ledger::{LedgerId, LedgerRecord, PreservationKind, PreservationLedger};
pub use report::{
    CompatibilityEntry, CompatibilityReport, DispositionViolation, FeatureLocation,
    PartConstructDisposition, PartDisposition, WholePartDisposition,
};
pub use reporter::{Finding, LossReporter, SourceRetention};
pub use taxonomy::{Disposition, ModelOutcome, RetentionOutcome};
