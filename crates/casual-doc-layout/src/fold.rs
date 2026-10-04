// SPDX-License-Identifier: Apache-2.0

//! Outline folding: the per-viewer block visibility filter of ADR-049.
//!
//! A reader collapses a heading and the blocks of its subtree contribute **no
//! fragments and no height**, so pagination closes up and the page count falls.
//! That is Word's behaviour and it is the whole point: a filter that only
//! skipped *painting* would leave a blank band where the content was.
//! ONLYOFFICE has no folding at all — zero `collaps` hits in their 232-file Word
//! engine, and their pagination loop has no block-tier visibility filter — so
//! this is a competitive opening rather than catch-up (`docs/157`).
//!
//! ## Two things, deliberately kept apart
//!
//! - [`FoldSet`] — **which headings are folded.** A layout *input*, never
//!   derived: the set of collapsed heading [`NodeId`]s, O(folded headings),
//!   which is per-viewer state the shell already owns. Deriving it inside layout
//!   would mean walking the document to find collapsed headings on every pass,
//!   which is O(document) and breaches `docs/107` B1.
//! - [`FoldState`] — **the hidden range**, which *is* derived, but inside the
//!   document-order walk that already happens. The named pattern is a **lexer
//!   resume state** (equally, the paginator's existing continuation state): one
//!   integer, carried on the walk the flow pass already makes. That is why
//!   folding is not a second flow path — it is a `continue` and an integer.
//!
//! ## The fold range
//!
//! The collapsed heading's own paragraph **stays visible**; hidden is every
//! block from `heading + 1` through `next_sibling_or_higher − 1`, where the next
//! sibling is the first following block whose outline-level *number* is the same
//! or lower. A fold with no following sibling runs to the end of its container.
//! This is Microsoft's wording for `w15:collapsed` ("immediately subsequent
//! paragraphs with a higher heading level number appear collapsed"), and it is
//! the same arithmetic ONLYOFFICE compute for *selection* in
//! `private_GetNextSiblingOrHigher` (`word/Editor/DocumentOutline.js:400`).
//!
//! Nested folds inside a suppressed range need no stack: the outer level already
//! dominates, so a deeper fold inside a hidden range changes nothing.
//!
//! ## What folding does NOT filter
//!
//! **Content, never document structure.** A hidden block still closes its
//! section, still advances list counters, note numbering, bookmark and field
//! state — because dropping those would change the geometry, columns and running
//! content of *visible* pages before the fold, which is a change to content the
//! reader did not fold. Section partitioning is computed from the model by
//! `document_layout::build_section_plans` and is untouched by this filter; the
//! filter only decides whether a block in an already-partitioned sequence is
//! laid out.
//!
//! The one thing that necessarily does change is the **page count**: collapsed
//! content occupies no pages, so the on-screen count is lower than the printed
//! one. Word has the same property, and print/PDF/DOCX export are always fully
//! expanded.
//!
//! ## Complexity, stated honestly
//!
//! [`FoldState::visit`] is O(1) per visited block, so a full pass stays
//! O(blocks) and a window stays **O(blocks in the window + hidden blocks
//! stepped over)** — *not* O(viewport). Stepping a hidden block is one outline
//! level read with no shaping and no allocation, but a single fold over a
//! million paragraphs is still a million cheap visits. If that proves too slow
//! the textbook escape hatch is a skip list over fold boundaries, rebuilt on a
//! fold toggle (a user gesture, which may be O(document) off the main thread)
//! and never on a keystroke. An empty [`FoldSet`] short-circuits the whole
//! mechanism, so a document with nothing folded pays nothing at all.

use std::collections::BTreeSet;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document};

/// The headings a viewer has folded — a layout **input**.
///
/// Holds collapsed heading [`NodeId`]s only, so it is O(folded headings) and a
/// handful in practice. Empty means nothing is folded, which is every existing
/// caller's behaviour and why `tests/geometry_snapshot.golden` cannot move.
///
/// The live set is per-viewer state the shell owns; `w15:collapsed`
/// ([`ParagraphProperties::collapsed`](casual_doc_model::v1::ParagraphProperties::collapsed))
/// is the *document default* a shell seeds it from at open
/// ([`FoldSet::from_document_defaults`]), and a viewer's own toggling is never
/// written back into the file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FoldSet {
    collapsed: BTreeSet<NodeId>,
}

impl FoldSet {
    /// An empty set: nothing folded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The set a document asks for when it is opened: every top-level body
    /// paragraph whose effective `w15:collapsed` is on.
    ///
    /// **O(top-level body blocks)**, and deliberately so: it is called once per
    /// open, by the shell, off the main thread — never per layout pass, which is
    /// the whole reason [`FoldSet`] is an input. Direct `w:pPr` only; a style
    /// cannot carry `w15:collapsed` in the schema.
    #[must_use]
    pub fn from_document_defaults(document: &Document) -> Self {
        document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => {
                    (paragraph.properties.collapsed == Some(true)).then_some(paragraph.id)
                }
                _ => None,
            })
            .collect()
    }

    /// Folds `heading`; returns whether it was not already folded.
    pub fn insert(&mut self, heading: NodeId) -> bool {
        self.collapsed.insert(heading)
    }

    /// Unfolds `heading`; returns whether it was folded.
    pub fn remove(&mut self, heading: NodeId) -> bool {
        self.collapsed.remove(&heading)
    }

    /// Whether `heading` is folded. O(log folded headings).
    #[must_use]
    pub fn contains(&self, heading: NodeId) -> bool {
        self.collapsed.contains(&heading)
    }

    /// Whether nothing is folded. The flow pass checks this first and skips the
    /// filter entirely when it is true.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.collapsed.is_empty()
    }

    /// How many headings are folded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.collapsed.len()
    }

    /// The folded headings, in [`NodeId`] order.
    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.collapsed.iter().copied()
    }
}

impl FromIterator<NodeId> for FoldSet {
    fn from_iter<I: IntoIterator<Item = NodeId>>(iter: I) -> Self {
        Self {
            collapsed: iter.into_iter().collect(),
        }
    }
}

impl Extend<NodeId> for FoldSet {
    fn extend<I: IntoIterator<Item = NodeId>>(&mut self, iter: I) {
        self.collapsed.extend(iter);
    }
}

/// The outline-fold inputs of one flow pass, bundled so every builder threads
/// the same pair — the [`NoteFlow`](crate::flow::NoteFlow) precedent, and the
/// reason folding adds one parameter to the flow seam rather than two.
///
/// [`Default`] is "nothing folded, not inside a fold", which is what every
/// entry point that does not name a fold set passes, so an unfolded document is
/// byte-for-byte what it was before this type existed.
#[derive(Clone, Copy, Debug, Default)]
pub struct FoldFlow<'a> {
    /// The folded headings. `None` short-circuits the filter entirely.
    pub set: Option<&'a FoldSet>,
    /// The state the OUTERMOST block sequence starts in — non-default only for
    /// a windowed pass resuming inside a folded range.
    pub resume: FoldState,
}

impl<'a> FoldFlow<'a> {
    /// A whole-document pass over `set`: start unsuppressed, and treat an empty
    /// set as no fold set at all so the filter costs nothing.
    #[must_use]
    pub fn new(set: Option<&'a FoldSet>) -> Self {
        Self {
            set: set.filter(|folds| !folds.is_empty()),
            resume: FoldState::default(),
        }
    }

    /// The same inputs resumed at `resume` — what a window mid-document needs.
    #[must_use]
    pub fn resumed(set: Option<&'a FoldSet>, resume: FoldState) -> Self {
        Self {
            resume,
            ..Self::new(set)
        }
    }
}

/// Whether a block is laid out or stepped over.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Visibility {
    /// Lay the block out normally.
    Visible,
    /// Inside a folded range: contribute no fragments and no height.
    Hidden,
}

/// The fold filter's whole state: **one integer**.
///
/// `Some(level)` means "suppress every following block until one whose outline
/// level is `<= level`". This is the lexer-resume-state pattern, and it is what
/// a windowed paginator must carry across a window boundary: a window starting
/// mid-document cannot know whether its first block is inside a fold, and
/// recomputing that from the start of the document is exactly the O(document)
/// cost [`FoldSet`] being an input avoids. It therefore rides in the measure
/// pass's existing continuation state ([`crate::flow::MeasureResume`]) rather
/// than in a parallel one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FoldState {
    suppress_above_level: Option<u8>,
}

impl FoldState {
    /// Whether this state is suppressing anything — i.e. whether the next block
    /// visited lands inside a folded range.
    #[must_use]
    pub fn is_suppressing(&self) -> bool {
        self.suppress_above_level.is_some()
    }

    /// Decides one block of a sequence, in document order, and advances.
    ///
    /// `folded` is whether this block's own id is in the [`FoldSet`];
    /// `outline_level` is its **effective** `w:outlineLvl` (`None` for anything
    /// that is not a heading, including every non-paragraph block).
    ///
    /// A folded heading is itself [`Visibility::Visible`] — folding must never
    /// move the heading the reader clicked. Suppression clears on the first
    /// following block at the same or a lower level number, and that block may
    /// immediately re-arm it if it too is folded.
    ///
    /// O(1).
    pub(crate) fn visit(&mut self, folded: bool, outline_level: Option<u8>) -> Visibility {
        if let Some(suppressed) = self.suppress_above_level {
            match outline_level {
                // A sibling or an ancestor heading ends the fold and is visible.
                Some(level) if level <= suppressed => self.suppress_above_level = None,
                // Deeper headings and all non-heading blocks stay hidden; a fold
                // nested inside them needs no stack because this outer level
                // already dominates.
                _ => return Visibility::Hidden,
            }
        }
        if folded && let Some(level) = outline_level {
            self.suppress_above_level = Some(level);
        }
        Visibility::Visible
    }
}

#[cfg(test)]
mod tests {
    use super::{FoldState, Visibility};

    /// Walks `blocks` — `(folded, outline_level)` pairs — and returns the
    /// indices that stay visible.
    fn visible(blocks: &[(bool, Option<u8>)]) -> Vec<usize> {
        let mut state = FoldState::default();
        blocks
            .iter()
            .enumerate()
            .filter(|(_, (folded, level))| state.visit(*folded, *level) == Visibility::Visible)
            .map(|(index, _)| index)
            .collect()
    }

    #[test]
    fn nothing_folded_hides_nothing() {
        assert_eq!(
            visible(&[(false, Some(0)), (false, None), (false, Some(1))]),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn a_folded_heading_stays_visible_and_hides_through_its_next_sibling() {
        // H1(folded) body body H2 body H1 body
        let visible = visible(&[
            (true, Some(0)),
            (false, None),
            (false, None),
            (false, Some(1)),
            (false, None),
            (false, Some(0)),
            (false, None),
        ]);
        assert_eq!(
            visible,
            vec![0, 5, 6],
            "the subtree hides, the sibling does not"
        );
    }

    #[test]
    fn a_fold_with_no_following_sibling_runs_to_the_end() {
        assert_eq!(
            visible(&[
                (true, Some(1)),
                (false, None),
                (false, Some(2)),
                (false, None)
            ]),
            vec![0]
        );
    }

    #[test]
    fn an_ancestor_heading_also_ends_a_fold() {
        // Folding an H2 must not swallow the H1 that follows it.
        assert_eq!(
            visible(&[(true, Some(1)), (false, None), (false, Some(0))]),
            vec![0, 2]
        );
    }

    #[test]
    fn a_sibling_that_is_itself_folded_re_arms_immediately() {
        let visible = visible(&[
            (true, Some(0)),
            (false, None),
            (true, Some(0)),
            (false, None),
            (false, Some(0)),
        ]);
        assert_eq!(visible, vec![0, 2, 4]);
    }

    #[test]
    fn a_nested_fold_inside_a_hidden_range_needs_no_stack() {
        // The inner H2 is folded too, but it is hidden, so it never arms. The
        // outer fold must still end at the following H1 — a stack that pushed
        // the inner level would end one heading early.
        let visible = visible(&[
            (true, Some(0)),
            (true, Some(1)),
            (false, None),
            (false, Some(0)),
        ]);
        assert_eq!(visible, vec![0, 3]);
    }

    #[test]
    fn a_folded_block_that_is_not_a_heading_arms_nothing() {
        assert_eq!(
            visible(&[(true, None), (false, None), (false, Some(3))]),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn suppression_is_visible_to_a_resuming_window() {
        let mut state = FoldState::default();
        assert!(!state.is_suppressing());
        state.visit(true, Some(0));
        assert!(
            state.is_suppressing(),
            "a window resuming here must know it starts inside a fold"
        );
        state.visit(false, Some(0));
        assert!(!state.is_suppressing());
    }
}
