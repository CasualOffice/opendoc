// SPDX-License-Identifier: Apache-2.0

//! Folding: the per-viewer block visibility filter keyed on the outline
//! (ADR-049).
//!
//! A [`FoldSet`] is the set of **collapsed heading** [`NodeId`]s a viewer is
//! looking at. It is a layout *input*, never derived inside layout: deriving it
//! would mean walking the document to find collapsed headings, which is
//! `O(document)` per pass and breaches `docs/107` B1.
//!
//! The hidden **range**, by contrast, is derived — but inside the walk that
//! already happens. The named pattern is a **lexer resume state**: an
//! incremental tokenizer's line-start state, equally the paginator's existing
//! continuation state. [`flow_blocks_into`](crate::flow) already visits blocks
//! in document order, so suppression is a one-integer state machine on that
//! walk ([`step`]), and the integer crosses a chunk seam inside
//! [`MeasureResume`](crate::flow::MeasureResume) rather than in a parallel
//! mechanism of folding's own.
//!
//! What folding filters is **content, never document structure**: a hidden
//! block still closes its section, still advances list counters, note numbering
//! and field state. Only the fragments it would have produced are absent, so
//! pagination closes up and the page count falls — reflow, not blanking.
//!
//! The one thing that *does* change is page numbers: collapsed content occupies
//! no pages, so the on-screen page count is lower than the printed one. Word has
//! the same property, and print/PDF/DOCX export are always fully expanded, so
//! the printed numbers are the true ones. Telling the reader that on screen is
//! the chrome's job.
//!
//! ONLYOFFICE has no folding at all — zero `collaps` hits across their 232-file
//! Word engine, and `CDocumentOutline` exposes no Collapse or Expand — and they
//! discard `w15:collapsed` structurally (`docs/157`). So Word is the standard
//! here, not a competitor's code.

use std::collections::BTreeSet;

use casual_doc_model::NodeId;
use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::Definitions;
use casual_doc_model::v1::ParagraphProperties;

use crate::cascade::StyleCascade;

/// The headings a viewer has collapsed, by [`NodeId`].
///
/// Per-viewer and per-document session state: it is **never** written to the
/// file. `NodeId`s are minted at import, so a persisted set would restore one
/// document's folds onto whatever ids a later open happened to assign.
/// `w15:collapsed` is the cross-session tier — the document's saved default,
/// read at open and exported on save.
///
/// Sized `O(folded headings)`, which is a handful even in a document of a
/// million paragraphs, and membership is `O(log folded)`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FoldSet(BTreeSet<NodeId>);

impl FoldSet {
    /// The empty set, as a constant, so every layout entry point that does not
    /// take a fold set can borrow one rather than build one per call.
    pub const EMPTY: Self = Self(BTreeSet::new());

    /// The empty set — nothing folded, which is what every existing layout
    /// entry point passes and why folding cannot move an unfolded document's
    /// geometry.
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Whether nothing is folded. The filter's fast exit: an unfolded document
    /// pays one `bool` per visited block and resolves no outline levels at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `node` is a collapsed heading. `O(log folded)`.
    #[must_use]
    pub fn contains(&self, node: NodeId) -> bool {
        self.0.contains(&node)
    }

    /// How many headings are folded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The folded headings, in `NodeId` order.
    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.0.iter().copied()
    }

    /// Folds `node`, returning whether the set changed.
    pub fn insert(&mut self, node: NodeId) -> bool {
        self.0.insert(node)
    }

    /// Unfolds `node`, returning whether the set changed.
    pub fn remove(&mut self, node: NodeId) -> bool {
        self.0.remove(&node)
    }

    /// Unfolds everything.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// A generation stamp for the incremental galley cache.
    ///
    /// The cache retains the galley the previous build produced and reuses its
    /// fragments for blocks the edit did not name. A galley built under one fold
    /// set describes a different body from one built under another, so the stamp
    /// joins `width` and the note-label generation as a reuse precondition: a
    /// fold toggle invalidates the retained galley (a **gesture**, which may pay
    /// a re-shape) while a keystroke at a fixed fold set still costs `O(edit)`.
    ///
    /// Order-independent by construction — it is a sum over a set — and `0` for
    /// the empty set, so an unfolded document gets the stamp it always had.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        // FNV-1a over each id's 16 raw bytes, combined by wrapping addition so
        // the result cannot depend on iteration order even if `NodeId`'s
        // ordering ever changes. A collision degrades to a stale-galley reuse,
        // which the debug-build hash assertion in `build_galley_cached` catches.
        self.0.iter().fold(0_u64, |acc, id| {
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            for byte in id.as_u128().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            acc.wrapping_add(hash)
        })
    }
}

impl FromIterator<NodeId> for FoldSet {
    fn from_iter<I: IntoIterator<Item = NodeId>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Whether a block contributes fragments to the galley.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlockVisibility {
    /// Flow it as usual.
    Visible,
    /// Inside a collapsed heading's range: contribute **no fragments and no
    /// height**, and do not descend into it.
    Hidden,
}

/// What one step of the fold filter decided, plus the work it already did.
pub(crate) struct FoldStep {
    /// Whether the block contributes fragments.
    pub(crate) visibility: BlockVisibility,
    /// A paragraph's **effective** properties, already resolved through the style
    /// cascade.
    ///
    /// Handed back rather than discarded so a HIDDEN paragraph's
    /// document-structure side effects — the list counters the visible items
    /// after the fold read — can be applied without a second cascade walk. `None`
    /// for a non-paragraph block, and `None` on the inert fast path, where the
    /// visible flow resolves the properties itself exactly as it always did.
    pub(crate) effective: Option<ParagraphProperties>,
}

/// One step of the suppression state machine, over the block the flow walk has
/// just reached.
///
/// `suppress` is the outline level of the innermost collapsed heading whose
/// range we are inside, or `None` outside any. Nested folds need no stack: the
/// outer level already dominates, so the first following heading at or above it
/// ends every fold it contains.
///
/// The range this implements is ADR-049's, stated exactly: the collapsed
/// heading's **own paragraph stays visible**, and the hidden range is every
/// block after it up to but excluding the next heading whose outline-level
/// *number* is the same or lower. A fold with no following sibling runs to the
/// end of its container. That agrees with Microsoft's Open Specifications
/// wording for `w15:collapsed` and it is derived from the outline rather than
/// stored, so the two can never disagree after an edit.
///
/// Complexity: `O(1)` per visited block, plus one style-cascade resolution for
/// a paragraph — and **zero** when `folds` is empty and no fold is armed, which
/// is the case for every document nobody has folded anything in.
pub(crate) fn step(
    suppress: &mut Option<u8>,
    block: &BlockNode,
    folds: &FoldSet,
    definitions: &Definitions,
    cascade: &StyleCascade<'_>,
) -> FoldStep {
    // The inert fast path. Resolving an outline level costs a cascade walk, and
    // a document with nothing folded must not pay one per block — that is the
    // inertness half of ADR-049, and it is what keeps `geometry_snapshot.golden`
    // from moving.
    if folds.is_empty() && suppress.is_none() {
        return FoldStep {
            visibility: BlockVisibility::Visible,
            effective: None,
        };
    }
    let paragraph = match block {
        BlockNode::Paragraph(paragraph) => Some(paragraph),
        // A table, a block content control or an `altChunk` carries no outline
        // level, so it can never END a fold — it is content inside one.
        _ => None,
    };
    let effective = paragraph.map(|p| cascade.resolve_paragraph(p.properties.get()));
    let level = match (paragraph, effective.as_ref()) {
        (Some(p), Some(effective)) => heading_level_of(effective, p.properties.get(), definitions),
        _ => None,
    };
    if let Some(armed) = *suppress {
        match level {
            // A heading at or above the fold's level ends it. It is then a
            // visible block in its own right, and may immediately re-arm below.
            Some(found) if found <= armed => *suppress = None,
            _ => {
                return FoldStep {
                    visibility: BlockVisibility::Hidden,
                    effective,
                };
            }
        }
    }
    if let (Some(found), Some(paragraph)) = (level, paragraph)
        && folds.contains(paragraph.id)
    {
        *suppress = Some(found);
    }
    FoldStep {
        visibility: BlockVisibility::Visible,
        effective,
    }
}

/// The outline level of a paragraph with these **direct** properties, 1-based
/// (1 = top), or `None` if it is not a heading.
///
/// This is the **one** heading-level rule in the product: the Outline panel, the
/// fold filter and the accessibility projection all answer the question here, so
/// the panel's idea of what can be folded and layout's idea of what a fold hides
/// cannot disagree. (`casual-doc-wasm`'s `documentOutline` delegates to it.)
/// Robust across how producers mark headings:
///
/// 1. the **effective** `w:outlineLvl`, resolved through the whole style chain;
/// 2. otherwise the paragraph's style and its `w:basedOn` ancestors, for a style
///    that carries its own `w:outlineLvl` — so a custom style *based on*
///    Heading 2 is still found;
/// 3. otherwise a `Title` / `Heading N` style **name**, which is how a producer
///    that writes no `outlineLvl` at all still gets an outline.
///
/// Takes the properties rather than a [`NodeId`] on purpose: every caller
/// already holds the paragraph, and resolving a `NodeId` back to its paragraph
/// is a walk of every block surface — the shape that made the outline panel and
/// the accessibility mirror quadratic in document size (`docs/116`).
///
/// Complexity: `O(style chain)`, bounded at 24 links.
#[must_use]
pub fn heading_level(
    direct: &ParagraphProperties,
    definitions: &Definitions,
    cascade: &StyleCascade<'_>,
) -> Option<u8> {
    heading_level_of(&cascade.resolve_paragraph(direct), direct, definitions)
}

/// [`heading_level`] for a caller that has **already** resolved the effective
/// properties, so the fold filter pays one cascade walk per paragraph rather
/// than two.
#[must_use]
pub fn heading_level_of(
    effective: &ParagraphProperties,
    direct: &ParagraphProperties,
    definitions: &Definitions,
) -> Option<u8> {
    if let Some(level) = effective.outline_level.filter(|level| *level <= 8) {
        return Some(level + 1);
    }
    let mut style_id = direct.style_ref;
    for _ in 0..24 {
        let style = style_id.and_then(|id| definitions.styles.get(&id))?;
        if let Some(level) = style
            .paragraph
            .as_ref()
            .and_then(|paragraph| paragraph.outline_level)
            .filter(|level| *level <= 8)
        {
            return Some(level + 1);
        }
        if let Some(level) = heading_level_from_name(style.name.as_deref()) {
            return Some(level);
        }
        style_id = style.based_on;
    }
    None
}

/// The heading level a style **name** implies: `Title` is 1, `Heading N` is `N`
/// (clamped to 9), anything else is not a heading.
///
/// Case- and separator-insensitive, because producers write `Heading 1`,
/// `heading1` and `Heading1` interchangeably.
#[must_use]
pub fn heading_level_from_name(name: Option<&str>) -> Option<u8> {
    let name = name?.trim();
    if name.eq_ignore_ascii_case("title") {
        return Some(1);
    }
    let rest = name
        .strip_prefix("Heading")
        .or_else(|| name.strip_prefix("heading"))
        .or_else(|| {
            name.get(..7)
                .filter(|head| head.eq_ignore_ascii_case("heading"))
                .map(|_| &name[7..])
        })?;
    let digits = rest.trim_start_matches([' ', '-', '_']).trim();
    let level: u8 = digits.parse().ok()?;
    (1..=9).contains(&level).then_some(level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::Paragraph;
    use casual_doc_model::v1::SharedParagraphProperties;

    fn node(raw: u128) -> NodeId {
        NodeId::new(raw).expect("non-zero")
    }

    fn heading(id: u128, level: u8) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: SharedParagraphProperties::new(ParagraphProperties {
                outline_level: Some(level - 1),
                ..ParagraphProperties::default()
            }),
            inlines: Vec::new(),
        })
    }

    fn body(id: u128) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: SharedParagraphProperties::default(),
            inlines: Vec::new(),
        })
    }

    #[test]
    fn a_fold_hides_to_the_next_sibling_or_higher_heading() {
        let definitions = Definitions::default();
        let cascade = StyleCascade::new(&definitions);
        // H1(1) H2(2) body(3) H2(4) H1(5) body(6), with H1(1) folded.
        let blocks = [
            heading(1, 1),
            heading(2, 2),
            body(3),
            heading(4, 2),
            heading(5, 1),
            body(6),
        ];
        let folds: FoldSet = [node(1)].into_iter().collect();
        let mut suppress = None;
        let seen: Vec<BlockVisibility> = blocks
            .iter()
            .map(|block| step(&mut suppress, block, &folds, &definitions, &cascade).visibility)
            .collect();
        use BlockVisibility::{Hidden, Visible};
        assert_eq!(
            seen,
            vec![Visible, Hidden, Hidden, Hidden, Visible, Visible],
            "the folded heading stays visible, its whole subtree hides, and the \
             next same-level heading ends the fold",
        );
        assert_eq!(suppress, None, "the fold cleared at the following H1");
    }

    #[test]
    fn an_empty_fold_set_hides_nothing_and_resolves_no_levels() {
        let definitions = Definitions::default();
        let cascade = StyleCascade::new(&definitions);
        let blocks = [heading(1, 1), body(2)];
        let folds = FoldSet::new();
        let mut suppress = None;
        for block in &blocks {
            assert_eq!(
                step(&mut suppress, block, &folds, &definitions, &cascade).visibility,
                BlockVisibility::Visible
            );
        }
        assert_eq!(suppress, None);
        assert_eq!(
            folds.fingerprint(),
            0,
            "the empty set stamps as it always did"
        );
    }

    #[test]
    fn a_deeper_fold_inside_a_folded_range_needs_no_stack() {
        let definitions = Definitions::default();
        let cascade = StyleCascade::new(&definitions);
        let blocks = [heading(1, 1), heading(2, 2), body(3), heading(4, 1)];
        let folds: FoldSet = [node(1), node(2)].into_iter().collect();
        let mut suppress = None;
        let seen: Vec<BlockVisibility> = blocks
            .iter()
            .map(|block| step(&mut suppress, block, &folds, &definitions, &cascade).visibility)
            .collect();
        use BlockVisibility::{Hidden, Visible};
        assert_eq!(seen, vec![Visible, Hidden, Hidden, Visible]);
    }

    #[test]
    fn a_fingerprint_separates_two_different_fold_sets() {
        let one: FoldSet = [node(7)].into_iter().collect();
        let two: FoldSet = [node(8)].into_iter().collect();
        assert_ne!(one.fingerprint(), two.fingerprint());
        let both: FoldSet = [node(7), node(8)].into_iter().collect();
        assert_ne!(both.fingerprint(), one.fingerprint());
        assert_eq!(both.len(), 2);
        assert!(both.contains(node(7)));
    }

    #[test]
    fn a_heading_style_name_is_an_outline_level() {
        assert_eq!(heading_level_from_name(Some("Heading 3")), Some(3));
        assert_eq!(heading_level_from_name(Some("heading1")), Some(1));
        assert_eq!(heading_level_from_name(Some("HEADING 9")), Some(9));
        assert_eq!(heading_level_from_name(Some("Title")), Some(1));
        assert_eq!(heading_level_from_name(Some("Heading 10")), None);
        assert_eq!(heading_level_from_name(Some("Body Text")), None);
        assert_eq!(heading_level_from_name(None), None);
    }
}
