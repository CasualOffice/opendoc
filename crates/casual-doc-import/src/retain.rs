//! Source retained for round-trip (Retention mode).
//!
//! This is the D5 tier-1 "byte floor": the original source is kept verbatim so
//! an unedited document can be reproduced exactly. Edit-tolerant tier-2
//! provenance (per-construct offset-span anchoring) is a later slice.

use std::collections::BTreeMap;

/// The original source bytes retained for a no-edit round trip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedSource {
    /// Original main-document part bytes, byte-identical to the import input.
    pub main_document: Vec<u8>,
    /// All admitted source parts by normalized name, byte-identical to the
    /// package. Populated by `import_package`; empty for the XML-only
    /// `import_main_document_xml` entry point (no package available).
    pub parts: BTreeMap<String, Vec<u8>>,
}
