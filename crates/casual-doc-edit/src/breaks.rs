//! Authoring the three breaks a word processor offers from Insert and Layout —
//! **page**, **column**, and **section** (`docs/130` §4.3, OO-022).
//!
//! Before this module the model could *read* every one of them and could author
//! none: `BreakKind::{Page, Column}` imported, paginated and exported, and
//! `SectionBoundary` carried a full `w:sectPr`, but no operation put either into
//! a document. What was missing was never the model.
//!
//! # What each break is here
//!
//! A **page break** and a **column break** are inline nodes — Word writes
//! `<w:br w:type="page"/>` inside a run — so authoring one is
//! [`Operation::InsertInlineObject`] carrying an [`InlineNode::Break`], the same
//! operation Shift+Enter already uses for a line break, with its existing
//! `RemoveInlineObject` inverse. **No new operation, and no new layout code**:
//! `casual-doc-layout`'s flow already turns a trailing `BreakKind::Page` into
//! `LineBreak::Page` with `page_break_after`, and the column paginator already
//! answers `LineBreak::Page` with a new page and `LineBreak::Column` with the
//! next column.
//!
//! A **section break** is not an inline. It splits the body into two sections,
//! each with its own [`SectionBoundary`], and the model states the binding
//! precisely: a paragraph's `section_break` names the section that paragraph
//! **ends**, and the document's final section is the trailing `sections` entry no
//! paragraph references. Per ECMA-376 a boundary's `section_type` says how *that*
//! section **starts** relative to the one before it, which is why the kind the
//! user picks (next page / continuous / even / odd) lands on the **new, second**
//! section and not on the one being split.
//!
//! # Which half is new, and what it inherits
//!
//! The caret's section keeps its identity and keeps the content **before** the
//! break; the content **after** the break becomes a fresh section. That is Word's
//! numbering ("Section 1 of 2" — the first half is still section 1), and it is
//! also what keeps this to one new operation: the existing boundary is not
//! rewritten at all, so nothing has to set its start type.
//!
//! The new boundary is a clone of the caret's, except for four fields:
//!
//! | Field | New section | Why |
//! | --- | --- | --- |
//! | `id` | fresh | it is a new section |
//! | `section_type` | the kind the user picked | this is the whole point of the gesture |
//! | `page_numbering.start` | **cleared** | `w:start` means *restart numbering here*. Copying it would restart page numbers at the break and silently renumber every page after it. Word does not restart numbering when you insert a section break |
//! | `section_change` | **cleared** | `w:sectPrChange` is a tracked *format change* recorded against the section it sits on. Copying it would fabricate a second identical revision, with the same author, date and id, for a section that has never been reformatted |
//!
//! Everything else is inherited verbatim, and deliberately: page size, margins,
//! orientation, columns, gutter, header and footer **references** (so both
//! sections keep the same running content — Word's "link to previous" default),
//! `title_page`, vertical alignment, document grid, paper source, page borders,
//! line numbering, the watermark (a section field here, so a new section without
//! it would drop the watermark after the break), footnote and endnote properties,
//! text direction and `bidi`. Identical geometry on both sides is the point: a
//! break must not move the text that follows it anywhere except onto the page its
//! start type asks for.
//!
//! Deliberate difference from Word, recorded rather than left ambiguous:
//! `title_page` is inherited, so a document with a distinct first-page header gets
//! a distinct first page in the new section too. Word copies the whole `sectPr`
//! and behaves the same way; not inheriting it would make the new section's
//! running content differ from the section it was split out of, which is the
//! worse surprise.
//!
//! # Where a break refuses
//!
//! Layout honours a section break only on a **top-level body paragraph**
//! (`document_layout::section_break_points` walks `body` without recursing), and
//! only the body's column paginator consumes `page_break_after`. So a break
//! inside a table cell, a text box, a content control, a header or footer, a
//! note, or a comment would be stored and then ignored. That is silent loss, so
//! it is refused with a reason instead ([`BreakRefusal`]) — never inserted, never
//! a no-op.
//!
//! Refusing inside a **body-level content control** is stricter than Word, which
//! allows it. It is deliberate: those paragraphs are not direct children of the
//! body, so layout would not charge a section break on one to any section.

use crate::{Operation, Pos, SplitProperties};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, Document, InlineNode, ParagraphProperties, SectionBoundary,
    SectionId, SectionType,
};

/// Why a break cannot be inserted where the caret is.
///
/// Every arm is a real position a caret reaches in this editor, and each one
/// carries its own sentence: a refusal that cannot say *why* is indistinguishable
/// from a dead control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreakRefusal {
    /// The node is not a paragraph in this document.
    NoSuchParagraph,
    /// The paragraph is in a table cell.
    InTableCell,
    /// The paragraph is inside a text box or a shape.
    InTextBox,
    /// The paragraph is inside a block-level content control.
    InContentControl,
    /// The paragraph is in a header or footer definition.
    InRunningContent,
    /// The paragraph is in a footnote or endnote definition.
    InNote,
    /// The paragraph is in a comment definition.
    InComment,
}

impl BreakRefusal {
    /// The user-facing reason, in the voice the rest of the engine's refusals
    /// use. The engine owns this vocabulary for the same reason it owns undo
    /// labels: a host should never have to reverse-engineer a refusal.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoSuchParagraph => "refused: there is no paragraph at the caret",
            Self::InTableCell => "refused: a break cannot be inserted inside a table",
            Self::InTextBox => "refused: a break cannot be inserted inside a text box or shape",
            Self::InContentControl => {
                "refused: a break cannot be inserted inside a content control"
            }
            Self::InRunningContent => "refused: a break cannot be inserted in a header or footer",
            Self::InNote => "refused: a break cannot be inserted in a footnote or endnote",
            Self::InComment => "refused: a break cannot be inserted in a comment",
        }
    }
}

/// Where the caret's paragraph sits, as far as break insertion is concerned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreakSite {
    /// A direct child of the body, at this 0-based index in the body block list —
    /// the only place a break is honoured.
    Body(usize),
    /// Somewhere a break would be stored and then ignored.
    Elsewhere(BreakRefusal),
}

/// Where `paragraph` sits, for break insertion.
///
/// **Complexity.** The accept path is a single non-recursive pass over the body's
/// top-level block list: O(top-level body blocks), no id resolved inside a loop,
/// and no use of `find_paragraph`/`find_paragraph_mut` (HF-184's linear scans get
/// no new call site here). The refusal path — reached at most once per refused
/// gesture, and it mutates nothing — then walks the nested containers to name the
/// one the caret is in, which is O(document); a precise sentence is worth one walk
/// that no user waits on twice.
#[must_use]
pub fn break_site(doc: &Document, paragraph: NodeId) -> BreakSite {
    let body = doc.body();
    crate::note_blocks(body.len());
    for (index, block) in body.iter().enumerate() {
        if let BlockNode::Paragraph(candidate) = block
            && candidate.id == paragraph
        {
            return BreakSite::Body(index);
        }
    }
    BreakSite::Elsewhere(classify_elsewhere(doc, paragraph))
}

/// Names the container a paragraph that is not a direct child of the body is in.
/// Only the refusal path calls this. O(document).
fn classify_elsewhere(doc: &Document, paragraph: NodeId) -> BreakRefusal {
    if let Some(found) = blocks_refusal(doc.body(), paragraph) {
        return found;
    }
    let definitions = doc.definitions();
    for (_, header) in definitions.headers.iter() {
        if crate::find_paragraph(&header.blocks, paragraph).is_some() {
            return BreakRefusal::InRunningContent;
        }
    }
    for (_, footer) in definitions.footers.iter() {
        if crate::find_paragraph(&footer.blocks, paragraph).is_some() {
            return BreakRefusal::InRunningContent;
        }
    }
    for (_, note) in definitions
        .footnotes
        .iter()
        .chain(definitions.endnotes.iter())
    {
        if crate::find_paragraph(&note.blocks, paragraph).is_some() {
            return BreakRefusal::InNote;
        }
    }
    for (_, comment) in definitions.comments.iter() {
        if crate::find_paragraph(&comment.blocks, paragraph).is_some() {
            return BreakRefusal::InComment;
        }
    }
    BreakRefusal::NoSuchParagraph
}

/// The nested container within `blocks` holding `paragraph`, or `None`.
///
/// Reuses [`crate::find_paragraph`] and the crate's inline walk rather than
/// growing a second traversal: two copies of "every container that can hold a
/// paragraph" diverge the first time a variant is added.
fn blocks_refusal(blocks: &[BlockNode], paragraph: NodeId) -> Option<BreakRefusal> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(candidate) => {
                if crate::find_paragraph_in_inlines(&candidate.inlines, paragraph).is_some() {
                    return Some(BreakRefusal::InTextBox);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if crate::find_paragraph(&cell.blocks, paragraph).is_some() {
                            return Some(BreakRefusal::InTableCell);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if crate::find_paragraph(&sdt.blocks, paragraph).is_some() {
                    // A nested table or text box inside the control is reported as
                    // the control: the outermost thing that makes the position
                    // unreachable is the one worth naming.
                    return Some(BreakRefusal::InContentControl);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// The operation that inserts an explicit break at the caret.
///
/// One existing operation, not a new one: `w:br` is an inline node, so a page or
/// column break is [`Operation::InsertInlineObject`] and its inverse is the
/// existing `RemoveInlineObject`. `id` must be fresh. O(1) in document size (the
/// operation it returns is not; see [`Operation::InsertInlineObject`]).
#[must_use]
pub fn insert_break_op(at: Pos, id: NodeId, kind: BreakKind) -> Operation {
    Operation::InsertInlineObject {
        at,
        node: Box::new(InlineNode::Break(Break { id, kind })),
    }
}

/// Everything a section split needs to know, resolved by ONE pass over the body's
/// top-level block list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SectionSplit {
    /// The section the caret's paragraph belongs to — the one the leading half of
    /// the split will terminate.
    pub current: SectionId,
    /// The section that follows `current` in the ordered list, or `None` when
    /// `current` is the document's final (body-level) section. The new boundary is
    /// spliced in immediately before it, so the list stays in body order.
    pub next: Option<SectionId>,
    /// The paragraph that terminates `current` today, with its properties — when
    /// that paragraph is *not* the caret's own. It has to be retargeted at the new
    /// section, because after the split the caret's leading half terminates
    /// `current` instead.
    pub terminator: Option<(NodeId, ParagraphProperties)>,
    /// Whether the caret's own paragraph is the one terminating `current` (a break
    /// inserted inside the last paragraph of a section). Then there is no separate
    /// terminator to retarget: the split's trailing half inherits the role.
    pub caret_terminates: bool,
    /// The caret paragraph's own properties, carried out of the same pass that
    /// located it. Both halves of the split inherit from these, so resolving the
    /// paragraph a second time to read them would be one more walk for something
    /// already in hand.
    pub caret_properties: ParagraphProperties,
}

/// Resolves the section a caret sits in, and the paragraph that currently
/// terminates it, in one forward pass.
///
/// The walk goes **forwards** from the caret, not from the start of the document,
/// because the model's binding makes that sufficient: the caret's section is the
/// one named by the first top-level paragraph at or after the caret that carries a
/// `section_break`, and if there is none the caret is in the final, body-level
/// section. Walking from the start of the document to count breaks — which is what
/// the facade's `section_of` does for section-scoped dialogs — would visit every
/// block before the caret for no gain.
///
/// **Complexity.** One non-recursive pass over the body's top-level block list:
/// O(top-level body blocks), and O(1) in the number of ids resolved (nothing is
/// looked up by id inside the loop). A single-section document short-circuits
/// before the pass, because with one boundary the caret can only be in it.
///
/// Returns a [`BreakRefusal`] when the caret is not on a top-level body paragraph
/// or the document has no section at all.
pub fn section_split_site(doc: &Document, paragraph: NodeId) -> Result<SectionSplit, BreakRefusal> {
    let index = match break_site(doc, paragraph) {
        BreakSite::Body(index) => index,
        BreakSite::Elsewhere(refusal) => return Err(refusal),
    };
    let body = doc.body();
    let BlockNode::Paragraph(caret) = &body[index] else {
        unreachable!("`break_site` returned the index of a paragraph");
    };
    let caret_properties = (*caret.properties).clone();
    let sections = doc.definitions().sections.as_slice();
    let (first, rest) = sections
        .split_first()
        .ok_or(BreakRefusal::NoSuchParagraph)?;
    let first_id = first.id;
    if rest.is_empty() {
        // One boundary is the body-level one, so the caret is in it and no
        // paragraph terminates it. No pass needed at all.
        return Ok(SectionSplit {
            current: first_id,
            next: None,
            terminator: None,
            caret_terminates: false,
            caret_properties,
        });
    }
    crate::note_blocks(body.len() - index);
    for (offset, block) in body[index..].iter().enumerate() {
        let BlockNode::Paragraph(candidate) = block else {
            continue;
        };
        let Some(current) = candidate.properties.section_break else {
            continue;
        };
        let next = sections
            .iter()
            .position(|boundary| boundary.id == current)
            .and_then(|found| sections.get(found + 1))
            .map(|boundary| boundary.id);
        return Ok(SectionSplit {
            current,
            next,
            // `properties` is interned (a `Shared`), so this is one clone of the
            // one paragraph whose section reference has to move — not a walk.
            terminator: (offset > 0).then(|| (candidate.id, (*candidate.properties).clone())),
            caret_terminates: offset == 0,
            caret_properties,
        });
    }
    // No terminator after the caret: the caret is in the final, body-level
    // section, which is the trailing entry.
    let last = sections.last().map_or(first_id, |boundary| boundary.id);
    Ok(SectionSplit {
        current: last,
        next: None,
        terminator: None,
        caret_terminates: false,
        caret_properties,
    })
}

/// The boundary a new section inherits from the one it is split out of. See this
/// module's header for the four fields that are not inherited and why. O(1) in
/// document size (one boundary clone).
#[must_use]
pub fn inherited_boundary(
    from: &SectionBoundary,
    id: SectionId,
    start: SectionType,
) -> SectionBoundary {
    let mut boundary = from.clone();
    boundary.id = id;
    boundary.section_type = Some(start);
    // `w:start` means *restart page numbering at this section*. Inheriting it
    // would renumber every page after the break.
    boundary.page_numbering.start = None;
    // `w:sectPrChange` is a tracked format change recorded against the section it
    // sits on; a copy would be a second, fabricated revision.
    boundary.section_change = None;
    boundary
}

/// The operations that insert a section break at `at`, as ONE undoable action.
///
/// `inherited` must be the boundary named by `split.current`. `new_section` and
/// `new_paragraph` must be fresh ids.
///
/// The order is forced: the boundary has to exist before a paragraph may reference
/// it, or the split is refused with a dangling section reference.
///
/// 1. splice the new boundary in after `split.current`;
/// 2. retarget the section's existing terminator, if it is a different paragraph,
///    at the new section — it is now the *last* paragraph of the *second* half;
/// 3. split the caret's paragraph, the leading half terminating `split.current`.
///
/// Applied as one group they are one history entry, so the whole split undoes in
/// one step. O(1) in document size (three operation values and two property
/// clones); the operations themselves cost what their own documentation says.
#[must_use]
pub fn section_break_ops(
    at: Pos,
    split: &SectionSplit,
    inherited: &SectionBoundary,
    new_section: SectionId,
    new_paragraph: NodeId,
    start: SectionType,
) -> Vec<Operation> {
    let mut leading = split.caret_properties.clone();
    leading.section_break = Some(split.current);
    // docs/108 Decision 1: splitting a paragraph leaves the ORIGINAL mark on the
    // trailing half and gives the leading half a brand-new one, so a tracked
    // paragraph-mark insertion must not be charged to both halves. The leading
    // half's mark is the one that now carries the `w:sectPr`, and it is new.
    leading.mark_revision = None;

    let mut trailing = split.caret_properties.clone();
    // When the caret's own paragraph was the section's terminator, the trailing
    // half inherits that role for the new section; otherwise the trailing half is
    // ordinary content and must not terminate anything.
    trailing.section_break = split.caret_terminates.then_some(new_section);
    // `w:pageBreakBefore` belongs to where the original paragraph STARTED. Copying
    // it onto the trailing half would force a second page break one paragraph
    // later, which is not what splitting a paragraph means.
    trailing.page_break_before = false;

    let mut ops = Vec::with_capacity(3);
    ops.push(Operation::SpliceSectionBoundary {
        at: split.next,
        boundary: Some(Box::new(inherited_boundary(inherited, new_section, start))),
    });
    if let Some((node, properties)) = &split.terminator {
        let mut properties = properties.clone();
        properties.section_break = Some(new_section);
        ops.push(Operation::SetParagraphProperties {
            node: *node,
            properties: Box::new(properties),
        });
    }
    ops.push(Operation::SplitParagraph {
        at,
        new_id: new_paragraph,
        properties: Some(Box::new(SplitProperties { leading, trailing })),
    });
    ops
}
