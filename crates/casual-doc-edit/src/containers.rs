//! This crate's arming of the inline container set (`docs/109` HF-212).
//!
//! # The declaration is not here any more, and that is the fix
//!
//! An inline can contain other inlines, or block content of its own. The complete
//! container set is **six**: `Hyperlink`, `Field`, `Revision`, `Sdt` (inline in
//! inline) and `TextBox`, `Group` (block in inline). Written out ad hoc, each walk
//! implemented three or four of the six and stopped behind a `_ =>` arm — and the
//! matrix over this crate's `lib.rs` said so. Every walk decided the set for
//! itself, silently, which is why one defect kept arriving in a new place: HF-191,
//! HF-194/195, HF-196, HF-209. Four in one day, all the same shape.
//!
//! This module held the declaration when it was first closed, and its own header
//! reported the remaining defect as a row: `casual-doc-wasm` had closed the same
//! class with its own copy of the same six-arm match, so one rule existed twice,
//! and the set is a fact about `casual_doc_model::v1::InlineNode` rather than about
//! editing. It now lives in the crate that defines the type
//! ([`casual_doc_model::v1::inline_descent`]), and what is left here is the two
//! things that are genuinely this crate's: the names it re-exports so every call
//! site in `lib.rs` reads the same as before, and the **arming** of
//! [`casual_doc_model::container_audit`] over this crate's own source.
//!
//! # Complexity
//!
//! Every re-exported function is **O(1)** except `find_in_group_block_stories`
//! and its mutable twin, which are O(children in the group subtree). Nothing here
//! resolves a `NodeId`, so nothing here is a document scan.

pub(crate) use casual_doc_model::v1::{
    InlineDescent, InlineDescentMut, contained_inlines, contained_inlines_mut,
    find_in_group_block_stories, find_in_group_block_stories_mut, inline_descent,
    inline_descent_mut,
};

#[cfg(test)]
mod tests {
    use casual_doc_model::container_audit::{Audit, SourceFile};

    /// Every `.rs` in this crate's `src/`.
    ///
    /// A walk added to a new module would escape a per-file list, so if one is
    /// added this function is what has to be extended, and
    /// `assert_covers_declared_modules` fails until it is.
    fn audit() -> Audit<'static> {
        Audit::new(vec![
            SourceFile::new("lib.rs", include_str!("lib.rs")),
            SourceFile::new("containers.rs", include_str!("containers.rs")),
            SourceFile::new("clone.rs", include_str!("clone.rs")),
            SourceFile::new("references.rs", include_str!("references.rs")),
            SourceFile::new("breaks.rs", include_str!("breaks.rs")),
            SourceFile::new("refusal.rs", include_str!("refusal.rs")),
            SourceFile::new("mint.rs", include_str!("mint.rs")),
            SourceFile::new("protection.rs", include_str!("protection.rs")),
            SourceFile::new("access.rs", include_str!("access.rs")),
        ])
        // Two crate-local *axes of* the declaration rather than second copies of
        // it: the children come from `contained_inlines` and only the policy —
        // whether a non-contributing tracked revision counts as content — is
        // theirs.
        .also_consulting(&["transparent_children", "editing_transparent"])
        // This crate is the editing engine over `v1::InlineNode`; it cannot stop
        // naming it. A scan that found fewer than this has stopped reading.
        .expecting_at_least(80)
    }

    /// Every module of this crate is in the scan.
    #[test]
    fn the_scan_covers_every_module() {
        audit().assert_covers_declared_modules();
    }

    /// The rule, on this crate: a walk over `InlineNode` either consults the
    /// declared container set or says in one line why it does not (`docs/109`
    /// HF-212).
    ///
    /// This is the deliverable, not the arms. `casual-doc-wasm` fixed nine walks and
    /// this crate's matrix found the same shape in thirty-odd more, because nothing
    /// stopped the next one being written the same way. The mutation that proves it
    /// works is the obvious one: add a walk with a wildcard and no stated reason.
    #[test]
    fn every_inline_walk_consults_the_container_set_or_says_why_not() {
        audit().run().assert_clean();
    }
}
