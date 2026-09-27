//! What an [`InlineNode`] CONTAINS — the one place in this crate that knows the
//! inline container set (`docs/109` HF-212).
//!
//! # The class this closes
//!
//! An inline can contain other inlines, or block content of its own. The complete
//! container set is **six**: `Hyperlink`, `Field`, `Revision`, `Sdt` (inline in
//! inline) and `TextBox`, `Group` (block in inline). Written out ad hoc, each walk
//! implemented three or four of the six and stopped behind a `_ =>` arm — and the
//! matrix over this crate said so: of the walks over `InlineNode` in
//! `lib.rs`, **exactly one entered all six, and 23 carried a catch-all**. Every
//! walk decided the set for itself, silently, which is why one defect kept
//! arriving in a new place: HF-191 (25 of 29 inline kinds dropped by the only deep
//! copy), HF-194/195 (`Sdt`/`Revision`, so a text box in a content control is not
//! an editing surface), HF-196 (`Group`, exporting a dangling relationship),
//! HF-209 (an entity reference in a footer). Four in one day, all the same shape.
//!
//! The fix is not another arm. It is one declared set that a walk consults, plus
//! `every_inline_walk_consults_the_container_set_or_says_why_not` in `lib.rs`,
//! which fails the build when a walk over `InlineNode` neither consults this
//! module nor states in one line why it does not.
//!
//! # This is `casual-doc-wasm`'s shape, deliberately — not a second one
//!
//! `casual-doc-wasm` closed the same class in its own crate first (#651) with
//! `InlineDescent`/`inline_descent`, `contained_inlines`, `contained_inlines_mut`,
//! `inline_block_stories` and `group_block_stories`. This module carries the same
//! names, the same four axes and the same no-wildcard rule, because two solutions
//! to one problem is the thing this repository keeps paying for. It adds only what
//! an *editing* crate additionally needs and a read-only façade does not: the
//! **mutable** descent ([`InlineDescentMut`]), which hands out `&mut` children on
//! all three axes rather than only the inline one.
//!
//! The two crates cannot share one copy today: the container set is a fact about
//! `casual_doc_model::v1::InlineNode` and belongs in `casual-doc-model`, which is
//! a model-crate change owned by another lane. Reported as a row rather than
//! taken; until then there are two copies of one rule, and
//! `the_container_set_matches_the_wasm_facades` in `lib.rs`'s tests pins the part
//! of it a reader cannot check by eye.
//!
//! # The axes, and why a walk must pick one
//!
//! Not every walk should descend everything, and the difference is not taste:
//!
//! * **inline axis** ([`InlineDescent::Inlines`]) — children in the SAME paragraph
//!   and the same model-offset space. Anything that measures or addresses byte
//!   offsets within one paragraph follows only this one.
//! * **block-story axis** ([`InlineDescent::Blocks`], [`InlineDescent::Group`]) — a
//!   text box's or a group's own paragraphs. These are *separate stories*: their
//!   paragraphs have ids in the same document-wide space but offsets in a
//!   different space, so a length walk that followed them would report a
//!   paragraph longer than any offset the host can produce, and a caption inside a
//!   text box would be counted twice.
//!
//! So "the choke point" does not mean "descend everything". It means one place
//! that says, per variant, *what descending would be*, with each caller picking
//! the axis its question needs.
//!
//! # Complexity
//!
//! Every function here is **O(1)** except [`find_in_group_block_stories`] and
//! [`find_in_group_block_stories_mut`], which are O(children in the group
//! subtree). Nothing here resolves a `NodeId`, so nothing here is a document
//! scan: these are the primitives that let a per-interaction walk stay O(1) in
//! document size (`docs/107` §4).

use casual_doc_model::v1::{BlockNode, GroupChild, InlineNode};

/// What an [`InlineNode`] contains, by descent axis.
///
/// The match in [`inline_descent`] is **exhaustive with no wildcard arm**: a
/// wildcard is how the next inline kind gets silently treated as a leaf, which is
/// this same defect arriving by omission.
pub(crate) enum InlineDescent<'a> {
    /// Inline children in the SAME paragraph and the same model-offset space:
    /// `Hyperlink`, `Field`, `Revision`, `Sdt`.
    Inlines(&'a [InlineNode]),
    /// A block story of its own — `TextBox`.
    Blocks(&'a [BlockNode]),
    /// A DrawingML group's children — `Group`. Pictures and shapes are leaves; its
    /// text boxes and nested groups carry block stories.
    Group(&'a [GroupChild]),
    /// Everything else: a leaf, with nothing inside it to visit.
    Leaf,
}

/// The children of one inline node, by descent axis. **O(1)** — it returns
/// borrowed slices and visits nothing.
///
/// `Revision` is a container whatever its kind: a walk looking for *content*
/// decides for itself whether a non-contributing revision counts (see
/// `editing_transparent` in `lib.rs`), but a walk looking for an *id* must descend
/// one regardless, because a tracked deletion's nodes are still in the tree.
pub(crate) fn inline_descent(inline: &InlineNode) -> InlineDescent<'_> {
    match inline {
        InlineNode::Hyperlink(link) => InlineDescent::Inlines(&link.inlines),
        InlineNode::Field(field) => InlineDescent::Inlines(&field.inlines),
        InlineNode::Revision(revision) => InlineDescent::Inlines(&revision.inlines),
        InlineNode::Sdt(sdt) => InlineDescent::Inlines(&sdt.inlines),
        InlineNode::TextBox(text_box) => InlineDescent::Blocks(&text_box.blocks),
        InlineNode::Group(group) => InlineDescent::Group(&group.children),
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::PositionalTab(_)
        | InlineNode::Break(_)
        | InlineNode::Symbol(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::Math(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => InlineDescent::Leaf,
    }
}

/// [`InlineDescent`] for a walk that MUTATES what it finds.
///
/// Rust cannot share one match across mutability, so this is the one deliberate
/// second spelling of the container set, kept immediately beside the first so the
/// two cannot drift apart unnoticed — and
/// `the_two_faces_of_the_container_set_agree` fails the build if they ever do.
/// Adding a third spelling anywhere else is the defect this module is about.
pub(crate) enum InlineDescentMut<'a> {
    /// See [`InlineDescent::Inlines`].
    Inlines(&'a mut Vec<InlineNode>),
    /// See [`InlineDescent::Blocks`].
    Blocks(&'a mut Vec<BlockNode>),
    /// See [`InlineDescent::Group`].
    Group(&'a mut Vec<GroupChild>),
    /// See [`InlineDescent::Leaf`].
    Leaf,
}

/// [`inline_descent`] for a walk that MUTATES what it finds. **O(1)**.
pub(crate) fn inline_descent_mut(inline: &mut InlineNode) -> InlineDescentMut<'_> {
    match inline {
        InlineNode::Hyperlink(link) => InlineDescentMut::Inlines(&mut link.inlines),
        InlineNode::Field(field) => InlineDescentMut::Inlines(&mut field.inlines),
        InlineNode::Revision(revision) => InlineDescentMut::Inlines(&mut revision.inlines),
        InlineNode::Sdt(sdt) => InlineDescentMut::Inlines(&mut sdt.inlines),
        InlineNode::TextBox(text_box) => InlineDescentMut::Blocks(&mut text_box.blocks),
        InlineNode::Group(group) => InlineDescentMut::Group(&mut group.children),
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::PositionalTab(_)
        | InlineNode::Break(_)
        | InlineNode::Symbol(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::Math(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => InlineDescentMut::Leaf,
    }
}

/// The inline children an inline contains IN THE SAME PARAGRAPH, or `None` when it
/// has none. **O(1)**.
///
/// [`InlineDescent::Inlines`] on its own, for the walks that must follow only that
/// axis: anything measuring or addressing offsets within one paragraph. A
/// `TextBox`'s or a `Group`'s paragraphs are separate stories with their own
/// offset spaces, so following them would produce an offset in one paragraph for a
/// node that lives in another.
pub(crate) fn contained_inlines(inline: &InlineNode) -> Option<&[InlineNode]> {
    match inline_descent(inline) {
        InlineDescent::Inlines(inlines) => Some(inlines),
        InlineDescent::Blocks(_) | InlineDescent::Group(_) | InlineDescent::Leaf => None,
    }
}

/// [`contained_inlines`] for a walk that MUTATES what it finds. **O(1)**.
pub(crate) fn contained_inlines_mut(inline: &mut InlineNode) -> Option<&mut Vec<InlineNode>> {
    match inline_descent_mut(inline) {
        InlineDescentMut::Inlines(inlines) => Some(inlines),
        InlineDescentMut::Blocks(_) | InlineDescentMut::Group(_) | InlineDescentMut::Leaf => None,
    }
}

/// The first `Some` that `f` returns over the block stories a group's children
/// own, in paint order — the [`InlineDescent::Group`] axis flattened to the
/// stories inside it. **O(children in the subtree)**; a picture or a shape is a
/// leaf and carries no story.
///
/// For the walks whose question is about *block content*: a picture inside a text
/// box inside a group is inside a paragraph, and nothing but this reaches it.
/// A walk whose question is about the group children THEMSELVES (a shape's
/// geometry, a nested group's transform) takes [`InlineDescent::Group`] raw
/// instead and writes its own four-arm match over `GroupChild`, which the compiler
/// already makes exhaustive.
pub(crate) fn find_in_group_block_stories<'a, T>(
    children: &'a [GroupChild],
    f: &mut impl FnMut(&'a [BlockNode]) -> Option<T>,
) -> Option<T> {
    for child in children {
        let found = match child {
            GroupChild::TextBox(text_box) => f(&text_box.blocks),
            GroupChild::Group(nested) => find_in_group_block_stories(&nested.children, f),
            GroupChild::Picture(_) | GroupChild::Shape(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// [`find_in_group_block_stories`] for a walk that MUTATES what it finds.
/// **O(children in the subtree)**.
pub(crate) fn find_in_group_block_stories_mut<T>(
    children: &mut [GroupChild],
    f: &mut impl FnMut(&mut Vec<BlockNode>) -> Option<T>,
) -> Option<T> {
    for child in children {
        let found = match child {
            GroupChild::TextBox(text_box) => f(&mut text_box.blocks),
            GroupChild::Group(nested) => find_in_group_block_stories_mut(&mut nested.children, f),
            GroupChild::Picture(_) | GroupChild::Shape(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}
