//! Source retained for round-trip (Retention mode).
//!
//! This is the D5 tier-1 "byte floor": the original main-document bytes are kept
//! verbatim so an unedited document can be reproduced exactly. Edit-tolerant
//! tier-2 provenance (per-construct offset-span anchoring) is a later slice.

/// The original source bytes retained for a no-edit round trip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedSource {
    /// Original main-document part bytes, byte-identical to the import input.
    pub main_document: Vec<u8>,
}
