// SPDX-License-Identifier: Apache-2.0

//! What an [`InlineNode`] CONTAINS — the one place in the workspace that knows the
//! inline container set (`docs/109` HF-212).
//!
//! # The class this closes
//!
//! An inline can contain other inlines, or block content of its own. The complete
//! container set is **six**: `Hyperlink`, `Field`, `Revision`, `Sdt` (inline in
//! inline) and `TextBox`, `Group` (block in inline). Written out ad hoc, each walk
//! implemented three or four of the six and stopped behind a `_ =>` arm — and the
//! matrix over `casual-doc-edit` said so: of the functions naming `InlineNode::` in
//! its `lib.rs`, a single-figure minority entered all six and roughly a third
//! carried a catch-all. Every walk decided the set for itself, silently, which is
//! why one defect kept arriving in a new place: HF-191 (25 of 29 inline kinds
//! dropped by the only deep copy), HF-194/195 (`Sdt`/`Revision`, so a text box in a
//! content control is not an editing surface), HF-196 (`Group`, exporting a dangling
//! relationship), HF-209 (an entity reference in a footer). Four in one day, all the
//! same shape.
//!
//! The fix is not another arm. It is one declared set that a walk consults, plus
//! [`crate::container_audit`], which fails the build when a walk over `InlineNode`
//! neither consults this module nor states in one line why it does not.
//!
//! # Why the declaration lives HERE and not in a consumer
//!
//! It was written twice before it was written once: `casual-doc-wasm` closed the
//! class in its own crate (#651) and `casual-doc-edit` closed it in its own (#731),
//! each with its own copy of the same six-arm match. The module header on the second
//! copy said so and reported the move as a row, because the container set is a fact
//! about [`InlineNode`] and belongs in the crate that defines it — `SKILL` §8:
//! *prefer one mechanism over two; when a design needs a parallel path, that is
//! evidence the abstraction is wrong.* Two copies of one rule drift, and a rule that
//! has drifted is worse than no rule, because both copies are still cited as
//! evidence.
//!
//! So this is the declaration, and a consumer imports it rather than restating it:
//! `use casual_doc_model::v1::inline_descent;`, plus an arming of
//! [`crate::container_audit`] over that crate's own source.
//!
//! **Three of the four consumers have moved; `casual-doc-wasm` has not.** This
//! crate, `casual-doc-edit` and `casual-doc-transaction` import this declaration
//! and arm the audit. `casual-doc-wasm/src/lib.rs` still carries its own
//! `inline_descent` (and a *third* spelling in its `contained_inlines_mut`, which
//! writes the four-wrapper match out again rather than going through a mutable
//! descent) with its own guard holding its own copy to the same shape. That is one
//! copy too many and it is stated here rather than implied: the remaining migration
//! is mechanical — delete the copy, import this one — and it is left as a row
//! because that file is 47,000 lines and shared with other lanes.
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

use super::{BlockNode, GroupChild, InlineNode};

/// What an [`InlineNode`] contains, by descent axis.
///
/// The match in [`inline_descent`] is **exhaustive with no wildcard arm**: a
/// wildcard is how the next inline kind gets silently treated as a leaf, which is
/// this same defect arriving by omission.
#[derive(Debug)]
pub enum InlineDescent<'a> {
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
/// decides for itself whether a non-contributing revision counts, but a walk
/// looking for an *id* must descend one regardless, because a tracked deletion's
/// nodes are still in the tree.
#[must_use]
pub fn inline_descent(inline: &InlineNode) -> InlineDescent<'_> {
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
#[derive(Debug)]
pub enum InlineDescentMut<'a> {
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
#[must_use]
pub fn inline_descent_mut(inline: &mut InlineNode) -> InlineDescentMut<'_> {
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
#[must_use]
pub fn contained_inlines(inline: &InlineNode) -> Option<&[InlineNode]> {
    match inline_descent(inline) {
        InlineDescent::Inlines(inlines) => Some(inlines),
        InlineDescent::Blocks(_) | InlineDescent::Group(_) | InlineDescent::Leaf => None,
    }
}

/// [`contained_inlines`] for a walk that MUTATES what it finds. **O(1)**.
#[must_use]
pub fn contained_inlines_mut(inline: &mut InlineNode) -> Option<&mut Vec<InlineNode>> {
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
pub fn find_in_group_block_stories<'a, T>(
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
pub fn find_in_group_block_stories_mut<T>(
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

#[cfg(test)]
mod tests {
    use crate::container_audit::{Audit, SourceFile, production_half};

    /// Every v1 module, for the container-set audit.
    ///
    /// **v1 only, deliberately.** The crate root carries the v0 model, whose own
    /// `InlineNode` has no containers at all — there is no set to decide there, and
    /// a `container-set:` declaration on a v0 walk would be noise asserting
    /// something false. The audit is about `v1::InlineNode`, which is the type this
    /// module declares the set for.
    fn audit() -> Audit<'static> {
        Audit::new(vec![
            SourceFile::new("v1/mod.rs", include_str!("mod.rs")),
            SourceFile::new("v1/body.rs", include_str!("body.rs")),
            SourceFile::new("v1/chart.rs", include_str!("chart.rs")),
            SourceFile::new("v1/containers.rs", include_str!("containers.rs")),
            SourceFile::new("v1/definitions.rs", include_str!("definitions.rs")),
            SourceFile::new("v1/document.rs", include_str!("document.rs")),
            SourceFile::new("v1/ids.rs", include_str!("ids.rs")),
            SourceFile::new("v1/intern.rs", include_str!("intern.rs")),
            SourceFile::new("v1/metadata.rs", include_str!("metadata.rs")),
            SourceFile::new("v1/migration.rs", include_str!("migration.rs")),
            SourceFile::new("v1/numbering.rs", include_str!("numbering.rs")),
            SourceFile::new("v1/properties.rs", include_str!("properties.rs")),
            SourceFile::new("v1/table.rs", include_str!("table.rs")),
        ])
        // Fourteen at the time of writing: validation, the snapshot limits, the id
        // recorder, the chart-anchor recorder and the field-range tally in
        // `document.rs`, this module's own two faces, `InlineNode::id` in `body.rs`,
        // and the v0 bridge in `migration.rs`. A scan that reads fewer has stopped
        // reading the source it was handed.
        .expecting_at_least(14)
    }

    /// Every v1 module is in the scan.
    ///
    /// A per-file list is the one way the audit can be escaped without touching
    /// it: add `foo.rs` with a wildcard walk and nothing reads it.
    #[test]
    fn the_scan_covers_every_v1_module() {
        audit().assert_covers_declared_modules();
    }

    /// The rule, on the crate that owns the type: a walk over `InlineNode` either
    /// consults the declared container set or says in one line why it does not
    /// (`docs/109` HF-212).
    ///
    /// Armed here and not only in `casual-doc-edit` because the crate that declares
    /// the set was itself deciding it per walk: `record_inline_ids` — the walk that
    /// puts every node into the document-wide uniqueness set — entered all six
    /// behind a `_ => {}`, so the next variant would have been omitted from the one
    /// check that makes a `NodeId` mean something, silently.
    #[test]
    fn every_inline_walk_consults_the_container_set_or_says_why_not() {
        audit().run().assert_clean();
    }

    /// The immutable and mutable faces of the container set classify every inline
    /// kind the same way.
    ///
    /// Rust cannot share one match across mutability, so `inline_descent` and
    /// `inline_descent_mut` are two spellings of one fact — the exact shape this
    /// module exists to remove. They are kept adjacent so a reader sees both at
    /// once; this fails the build if they ever disagree, which is the part a reader
    /// cannot be relied on for. Drift here is not theoretical: it is how a node gets
    /// FOUND by a reading walk and then silently not changed by a writing one.
    ///
    /// Checked over **every** variant without a fixture, by reading the two matches
    /// themselves: the compiler already guarantees each is exhaustive, so if the two
    /// assign the same axis to the same variant names, they agree everywhere. A
    /// 30th variant therefore arrives for free — `inline_descent` refuses to compile
    /// without a new arm, and this then checks the mutable face got the same one
    /// rather than being quietly swept into `Leaf`.
    #[test]
    fn the_two_faces_of_the_container_set_agree() {
        let source = production_half(include_str!("containers.rs"));
        let immutable = descent_map(body_of(&source, "fn inline_descent("));
        let mutable = descent_map(body_of(&source, "fn inline_descent_mut("));

        assert!(
            immutable.len() >= 25,
            "the parse found only {} arms in `inline_descent`, so this guard is not \
             reading the match and is checking nothing",
            immutable.len()
        );
        assert_eq!(
            immutable, mutable,
            "`inline_descent` and `inline_descent_mut` classify some variant \
             differently: a walk that reads the tree and one that rewrites it would \
             descend different containers, which is how a node gets found by a \
             reading walk and then silently not changed by a writing one"
        );

        // And the declared set is the one this module documents, named here so the
        // six are pinned by the guard and not only by prose.
        for (container, axis) in [
            ("Hyperlink", "Inlines"),
            ("Field", "Inlines"),
            ("Revision", "Inlines"),
            ("Sdt", "Inlines"),
            ("TextBox", "Blocks"),
            ("Group", "Group"),
        ] {
            assert_eq!(
                immutable
                    .iter()
                    .find(|(name, _)| name == container)
                    .map(|(_, axis)| axis.as_str()),
                Some(axis),
                "`{container}` must descend on the {axis} axis"
            );
        }
        let leaves = immutable.iter().filter(|(_, axis)| axis == "Leaf").count();
        assert_eq!(
            leaves,
            immutable.len() - 6,
            "exactly six variants are containers; everything else is a leaf"
        );
    }

    /// The text of one function, from its signature to the line that closes it.
    fn body_of<'a>(source: &'a str, signature: &str) -> &'a str {
        let start = source
            .find(signature)
            .unwrap_or_else(|| panic!("`{signature}` must be in this file"));
        let rest = &source[start..];
        let end = rest
            .find("\n}\n")
            .unwrap_or_else(|| panic!("`{signature}` must be closed at column zero"));
        &rest[..end]
    }

    /// `(variant name, axis)` for every arm of a descent match, from its source.
    ///
    /// Line-wise: variant names accumulate across an `|`-separated pattern and are
    /// assigned the axis named on the right of the `=>` that closes it. Comment
    /// lines are skipped, so prose naming a variant is not read as an arm.
    fn descent_map(body: &str) -> Vec<(String, String)> {
        let mut pending: Vec<String> = Vec::new();
        let mut out: Vec<(String, String)> = Vec::new();
        for line in body.split('\n') {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for (index, _) in line.match_indices("InlineNode::") {
                let name: String = line[index + "InlineNode::".len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    pending.push(name);
                }
            }
            let Some((_, right)) = line.split_once("=>") else {
                continue;
            };
            let axis = ["InlineDescentMut::", "InlineDescent::"]
                .iter()
                .find_map(|prefix| right.split_once(prefix))
                .map(|(_, rest)| -> String {
                    rest.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect()
                });
            let Some(axis) = axis else {
                continue;
            };
            for name in pending.drain(..) {
                out.push((name, axis.clone()));
            }
        }
        out.sort();
        out
    }
}
