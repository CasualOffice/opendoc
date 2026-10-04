// SPDX-License-Identifier: Apache-2.0

//! Node-id allocation for an imported deck.
//!
//! # Why the file's own ids are not reused
//!
//! A `p:cNvPr@id` is unique only within one slide part. Two slides in a normal
//! deck both carry a shape with `id="2"`, and `Presentation::validate` refuses a
//! node id that appears twice **anywhere** in the presentation — correctly, since
//! identity is a property of the id space and not of the part. Reusing the
//! authored ids would therefore make almost every real deck fail validation, and
//! a per-part offset would make the ids depend on part-read order.
//!
//! So every node gets a fresh id from one monotonic generator, in read order. The
//! authored `p:cNvPr@id` is not discarded quietly: there is nowhere in the reused
//! `v1` drawing types to keep it, so it is reported as a degraded attribute. That
//! matters for export — PowerPoint matches animation targets (`p:timing`'s
//! `spid`) by that id, so a writer that re-mints ids silently breaks every
//! animation in the deck. `docs/156` §8 already names `spid` remapping as a Tier 2
//! import/export requirement; this is the import half of why.

use casual_doc_model::{IdGenerator, NodeId};

use crate::ImportError;

/// The id-space namespace an imported presentation's nodes are minted in.
///
/// A fixed namespace, so two imports of the same package produce the same ids and
/// a snapshot comparison is meaningful. Collaboration rebases the space when a
/// participant joins; that is a later concern and `IdGenerator::rebase` exists
/// for it.
pub(crate) const IMPORT_NAMESPACE: u64 = 1;

/// Monotonic node-id source for one import.
#[derive(Debug)]
pub(crate) struct Ids {
    generator: IdGenerator,
}

impl Ids {
    /// A generator starting at the beginning of the import namespace.
    pub(crate) const fn new() -> Self {
        Self {
            generator: IdGenerator::new(IMPORT_NAMESPACE),
        }
    }

    /// The next id.
    ///
    /// Exhaustion is an error rather than a wrap: a wrapped id collides with one
    /// already in the deck, and `Presentation::validate` would then refuse the
    /// whole import with a duplicate-id failure whose cause is invisible.
    ///
    /// # Complexity
    ///
    /// O(1).
    pub(crate) fn next(&mut self) -> Result<NodeId, ImportError> {
        Ok(self.generator.next_id()?)
    }
}
