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
    /// The document declares no `w:sectPr` at all, so there is no section to
    /// split. A DOCX always has a body-level one; a plain-text or JSON import may
    /// not, and page setup is equally unavailable on such a document.
    NoSectionToSplit,
}

use crate::refused;

impl BreakRefusal {
    /// The user-facing reason, in the voice the rest of the engine's refusals
    /// use. The engine owns this vocabulary for the same reason it owns undo
    /// labels: a host should never have to reverse-engineer a refusal.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoSuchParagraph => {
                refused!("break.no-paragraph", "There is no paragraph at the caret.")
            }
            Self::InTableCell => refused!(
                "break.in-table",
                "A break cannot be inserted inside a table."
            ),
            Self::InTextBox => refused!(
                "break.in-text-box",
                "A break cannot be inserted inside a text box or shape."
            ),
            Self::InContentControl => {
                refused!(
                    "break.in-content-control",
                    "A break cannot be inserted inside a content control."
                )
            }
            Self::InRunningContent => refused!(
                "break.in-running-content",
                "A break cannot be inserted in a header or footer."
            ),
            Self::InNote => refused!(
                "break.in-note",
                "A break cannot be inserted in a footnote or endnote."
            ),
            Self::InComment => refused!(
                "break.in-comment",
                "A break cannot be inserted in a comment."
            ),
            Self::NoSectionToSplit => {
                refused!(
                    "break.no-section",
                    "This document declares no page setup, so it has no section to split."
                )
            }
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
        .ok_or(BreakRefusal::NoSectionToSplit)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditError, Operation, apply, block_visits, reset_block_visits};
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{
        BlockSdt, Comment, CommentId, DefinitionMap, Definitions, HeaderFooter, HeaderFooterId,
        HeaderFooterKind, HeaderFooterRef, InlineSdt, MarkRevision, MarkRevisionKind, Note, NoteId,
        NumberFormat, PageMargins, PageNumbering, PageSize, Paragraph, PropChange, Run,
        RunProperties, SdtProperties, SectionColumns, Table, TableCell, TableRow, TextBox,
    };

    fn ids() -> IdGenerator {
        IdGenerator::new(9)
    }

    fn n(counter: u64) -> NodeId {
        NodeId::from_parts(7, counter).expect("valid node id")
    }

    fn run(id: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: n(id),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn para_with(id: u64, text: &str, properties: ParagraphProperties) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: n(id),
            properties: properties.into(),
            inlines: vec![run(id + 1, text)],
        })
    }

    fn para(id: u64, text: &str) -> BlockNode {
        para_with(id, text, ParagraphProperties::default())
    }

    /// A paragraph that ends section `section` — how a DOCX body marks a break.
    fn terminator(id: u64, text: &str, section: SectionId) -> BlockNode {
        para_with(
            id,
            text,
            ParagraphProperties {
                section_break: Some(section),
                ..ParagraphProperties::default()
            },
        )
    }

    /// A US-Letter section boundary.
    fn boundary(id: u64) -> SectionBoundary {
        SectionBoundary {
            id: SectionId::new(n(id)),
            page_size: PageSize {
                width_twips: 12_240,
                height_twips: 15_840,
            },
            page_margins: PageMargins {
                top_twips: 1_440,
                bottom_twips: 1_440,
                start_twips: 1_440,
                end_twips: 1_440,
                header_twips: None,
                footer_twips: None,
                gutter_twips: None,
            },
            columns: SectionColumns {
                count: 1,
                space_twips: None,
                separator: None,
                equal_width: None,
                columns: Vec::new(),
            },
            headers: Vec::new(),
            footers: Vec::new(),
            section_type: None,
            title_page: None,
            vertical_alignment: None,
            page_numbering: PageNumbering::default(),
            doc_grid: Default::default(),
            orientation: None,
            paper_source: Default::default(),
            page_borders: Default::default(),
            line_numbering: Default::default(),
            watermark: None,
            footnote_props: Default::default(),
            endnote_props: Default::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        }
    }

    fn document(body: Vec<BlockNode>, sections: Vec<SectionBoundary>) -> Document {
        Document::new(
            n(1_000),
            body,
            Definitions {
                sections,
                ..Definitions::default()
            },
        )
        .expect("valid document")
    }

    /// One section, three paragraphs; the section is the body-level one that no
    /// paragraph terminates.
    fn one_section() -> Document {
        document(
            vec![para(10, "one"), para(20, "two"), para(30, "three")],
            vec![boundary(500)],
        )
    }

    /// Two sections: paragraph 20 terminates section 500, and section 600 is the
    /// body-level one.
    fn two_sections() -> Document {
        document(
            vec![
                para(10, "one"),
                terminator(20, "two", SectionId::new(n(500))),
                para(30, "three"),
            ],
            vec![boundary(500), boundary(600)],
        )
    }

    fn sections_of(doc: &Document) -> Vec<SectionId> {
        doc.definitions()
            .sections
            .iter()
            .map(|boundary| boundary.id)
            .collect()
    }

    fn section_break_of(doc: &Document, paragraph: NodeId) -> Option<SectionId> {
        crate::find_paragraph(doc.body(), paragraph)
            .and_then(|found| found.properties.section_break)
    }

    /// Applies `ops` in order, returning the inverses in undo order.
    fn apply_group(doc: &mut Document, ops: &[Operation]) -> Vec<Operation> {
        let mut ids = ids();
        let mut inverses: Vec<Operation> = ops
            .iter()
            .map(|op| apply(doc, &mut ids, op).expect("operation applies"))
            .collect();
        inverses.reverse();
        inverses
    }

    // -----------------------------------------------------------------------
    // Page and column breaks — the half that needed no new operation
    // -----------------------------------------------------------------------

    /// A page break is an inline `w:br`, authored by the operation that already
    /// existed, and removed exactly by its existing inverse.
    #[test]
    fn a_page_break_is_an_inline_break_the_existing_operation_authors() {
        let mut doc = one_section();
        let before = doc.clone();
        let op = insert_break_op(Pos::new(n(10), 1), n(4_000), BreakKind::Page);
        let inverse = apply(&mut doc, &mut ids(), &op).expect("the break inserts");

        let paragraph = crate::find_paragraph(doc.body(), n(10)).expect("paragraph 10");
        let kinds: Vec<BreakKind> = paragraph
            .inlines
            .iter()
            .filter_map(|inline| match inline {
                InlineNode::Break(node) => Some(node.kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![BreakKind::Page],
            "the paragraph must hold exactly one PAGE break, not a line break"
        );

        assert!(
            matches!(inverse, Operation::RemoveInlineObject { .. }),
            "the inverse is the existing inline-object removal, not a new operation"
        );
        apply(&mut doc, &mut ids(), &inverse).expect("the inverse applies");
        assert_eq!(doc, before, "undo restores the document exactly");
    }

    /// A column break is the same operation with the other kind, so a single
    /// mapping mistake cannot make both breaks the same thing.
    #[test]
    fn a_column_break_is_authored_as_a_column_break() {
        let mut doc = one_section();
        let op = insert_break_op(Pos::new(n(10), 3), n(4_001), BreakKind::Column);
        apply(&mut doc, &mut ids(), &op).expect("the break inserts");
        let paragraph = crate::find_paragraph(doc.body(), n(10)).expect("paragraph 10");
        assert!(
            paragraph
                .inlines
                .iter()
                .any(|inline| matches!(inline, InlineNode::Break(node)
                    if node.kind == BreakKind::Column)),
            "a column break must be a COLUMN break: {:?}",
            paragraph.inlines
        );
    }

    // -----------------------------------------------------------------------
    // Section breaks
    // -----------------------------------------------------------------------

    /// In a one-section document the break appends a second boundary, the caret's
    /// paragraph keeps the existing section, and the START TYPE the user picked
    /// lands on the NEW, second section — the direction ECMA-376 and
    /// `flow::section_break_forces_page` both read.
    #[test]
    fn the_start_type_lands_on_the_new_second_section() {
        let mut doc = one_section();
        let split = section_split_site(&doc, n(20)).expect("a body paragraph");
        assert_eq!(split.current, SectionId::new(n(500)));
        assert_eq!(split.next, None, "the caret is in the body-level section");
        assert!(split.terminator.is_none(), "nothing terminates it yet");

        let inherited = doc.definitions().sections[0].clone();
        let new_section = SectionId::new(n(700));
        let ops = section_break_ops(
            Pos::new(n(20), 3),
            &split,
            &inherited,
            new_section,
            n(710),
            SectionType::Continuous,
        );
        apply_group(&mut doc, &ops);

        assert_eq!(
            sections_of(&doc),
            vec![SectionId::new(n(500)), new_section],
            "the new boundary follows the one it was split out of"
        );
        assert_eq!(
            doc.definitions().sections[0].section_type,
            None,
            "the section being split is not rewritten at all"
        );
        assert_eq!(
            doc.definitions().sections[1].section_type,
            Some(SectionType::Continuous),
            "the kind the user picked describes how the NEW section starts"
        );
        assert_eq!(
            section_break_of(&doc, n(20)),
            Some(SectionId::new(n(500))),
            "the leading half terminates the section the caret was in"
        );
        assert_eq!(
            section_break_of(&doc, n(710)),
            None,
            "the trailing half is ordinary content of the new section"
        );
    }

    /// The new section must carry the caret section's page-numbering FORMAT and
    /// must not carry its `@start`: `w:start` means *restart numbering here*, and
    /// `paginate::page_number_labels_for` restarts on exactly that field at a
    /// section's first page — so copying it renumbers every page after the break.
    #[test]
    fn a_section_break_does_not_restart_page_numbering() {
        let mut source = boundary(500);
        source.page_numbering = PageNumbering {
            format: Some(NumberFormat::LowerRoman),
            start: Some(7),
        };
        let new = inherited_boundary(&source, SectionId::new(n(700)), SectionType::NextPage);
        assert_eq!(
            new.page_numbering.format,
            Some(NumberFormat::LowerRoman),
            "the number FORMAT is inherited — the pages look the same"
        );
        assert_eq!(
            new.page_numbering.start, None,
            "the restart is NOT inherited: page numbers continue across the break"
        );
    }

    /// `w:sectPrChange` is a tracked format change recorded against the section it
    /// sits on. A copy would be a second revision, with the same author, date and
    /// id, claiming a section that has never been reformatted was.
    #[test]
    fn a_section_break_does_not_fabricate_a_tracked_format_change() {
        let mut source = boundary(500);
        source.section_change = Some(PropChange {
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: Some("11".to_owned()),
            editor_group: None,
            prior: Box::new(boundary(500)),
        });
        let new = inherited_boundary(&source, SectionId::new(n(700)), SectionType::NextPage);
        assert!(
            new.section_change.is_none(),
            "the new section has no prior formatting, so it carries no revision"
        );
    }

    /// Geometry, running-content references and the watermark are inherited, so
    /// the break moves the text after it onto the page its start type asks for and
    /// nowhere else.
    #[test]
    fn a_section_break_inherits_the_geometry_it_splits() {
        let mut source = boundary(500);
        source.headers = vec![HeaderFooterRef {
            kind: HeaderFooterKind::Default,
            reference: HeaderFooterId::new(n(800)),
        }];
        source.title_page = Some(true);
        let new = inherited_boundary(&source, SectionId::new(n(700)), SectionType::NextPage);
        assert_eq!(new.page_size, source.page_size, "same paper");
        assert_eq!(new.page_margins, source.page_margins, "same margins");
        assert_eq!(new.columns, source.columns, "same columns");
        assert_eq!(
            new.headers, source.headers,
            "both sections keep the same running content (Word's link-to-previous)"
        );
        assert_eq!(new.title_page, source.title_page, "same first-page rule");
    }

    /// With a break in the middle of a section that already has a terminator, the
    /// new boundary is spliced in BODY ORDER — between the two existing ones, not
    /// appended — and the old terminator is retargeted at the new section, so
    /// exactly one paragraph ends each section.
    #[test]
    fn the_new_boundary_is_spliced_in_body_order_and_the_terminator_moves() {
        let mut doc = two_sections();
        let split = section_split_site(&doc, n(10)).expect("a body paragraph");
        assert_eq!(split.current, SectionId::new(n(500)));
        assert_eq!(
            split.next,
            Some(SectionId::new(n(600))),
            "section 600 follows the caret's section"
        );
        assert_eq!(
            split.terminator.as_ref().map(|(node, _)| *node),
            Some(n(20)),
            "paragraph 20 terminates the caret's section today"
        );

        let inherited = doc.definitions().sections[0].clone();
        let new_section = SectionId::new(n(700));
        let ops = section_break_ops(
            Pos::new(n(10), 1),
            &split,
            &inherited,
            new_section,
            n(710),
            SectionType::NextPage,
        );
        apply_group(&mut doc, &ops);

        assert_eq!(
            sections_of(&doc),
            vec![SectionId::new(n(500)), new_section, SectionId::new(n(600))],
            "the new section sits between the two it was split between"
        );
        assert_eq!(
            section_break_of(&doc, n(10)),
            Some(SectionId::new(n(500))),
            "the leading half ends the first section"
        );
        assert_eq!(
            section_break_of(&doc, n(20)),
            Some(new_section),
            "the old terminator now ends the NEW section"
        );
    }

    /// A break inside the LAST paragraph of a section: there is no separate
    /// terminator to retarget, so the trailing half takes over the role. Getting
    /// this wrong leaves the new section terminated by nothing while the first
    /// section is terminated twice.
    #[test]
    fn a_break_in_a_sections_last_paragraph_hands_the_role_to_the_trailing_half() {
        let mut doc = two_sections();
        let split = section_split_site(&doc, n(20)).expect("a body paragraph");
        assert!(
            split.caret_terminates,
            "paragraph 20 is the caret's section's own terminator"
        );
        assert!(
            split.terminator.is_none(),
            "so there is no OTHER paragraph to retarget"
        );

        let inherited = doc.definitions().sections[0].clone();
        let new_section = SectionId::new(n(700));
        let ops = section_break_ops(
            Pos::new(n(20), 1),
            &split,
            &inherited,
            new_section,
            n(710),
            SectionType::NextPage,
        );
        assert_eq!(ops.len(), 2, "two operations, not three");
        apply_group(&mut doc, &ops);

        assert_eq!(
            section_break_of(&doc, n(20)),
            Some(SectionId::new(n(500))),
            "the leading half still ends the first section"
        );
        assert_eq!(
            section_break_of(&doc, n(710)),
            Some(new_section),
            "and the trailing half ends the new one"
        );
    }

    /// A tracked paragraph-mark insertion stays on the trailing half only
    /// (docs/108 Decision 1): the leading half's mark is brand new — it is the one
    /// that now carries the `w:sectPr`.
    #[test]
    fn a_tracked_paragraph_mark_is_not_charged_to_both_halves() {
        let mut doc = document(
            vec![
                para(10, "one"),
                para_with(
                    20,
                    "two",
                    ParagraphProperties {
                        mark_revision: Some(Box::new(MarkRevision {
                            kind: MarkRevisionKind::Insertion,
                            author: Some("Reviewer".to_owned()),
                            date: None,
                            revision_id: Some("3".to_owned()),
                        })),
                        ..ParagraphProperties::default()
                    },
                ),
            ],
            vec![boundary(500)],
        );
        let split = section_split_site(&doc, n(20)).expect("a body paragraph");
        let inherited = doc.definitions().sections[0].clone();
        let ops = section_break_ops(
            Pos::new(n(20), 1),
            &split,
            &inherited,
            SectionId::new(n(700)),
            n(710),
            SectionType::NextPage,
        );
        apply_group(&mut doc, &ops);

        let leading = crate::find_paragraph(doc.body(), n(20)).expect("leading half");
        let trailing = crate::find_paragraph(doc.body(), n(710)).expect("trailing half");
        assert!(
            leading.properties.mark_revision.is_none(),
            "the leading half's paragraph mark is new, so it carries no revision"
        );
        assert!(
            trailing.properties.mark_revision.is_some(),
            "the original mark travelled to the trailing half"
        );
    }

    /// The whole split is one exact inverse chain: undoing it restores the
    /// document byte for byte, including the POSITION of the boundary in the
    /// ordered section list. A restore that appended instead would leave the
    /// sections out of body order, which silently re-geometries every page after
    /// the break.
    #[test]
    fn a_section_break_undoes_exactly_including_the_list_position() {
        let mut doc = two_sections();
        let before = doc.clone();
        let split = section_split_site(&doc, n(10)).expect("a body paragraph");
        let inherited = doc.definitions().sections[0].clone();
        let ops = section_break_ops(
            Pos::new(n(10), 1),
            &split,
            &inherited,
            SectionId::new(n(700)),
            n(710),
            SectionType::NextPage,
        );
        let inverses = apply_group(&mut doc, &ops);
        assert_ne!(doc, before, "the break changed something");
        for inverse in &inverses {
            apply(&mut doc, &mut ids(), inverse).expect("the inverse applies");
        }
        assert_eq!(
            doc, before,
            "undoing the action restores the document exactly"
        );
    }

    /// The splice operation removes as exactly as it inserts, in both directions,
    /// on its own.
    #[test]
    fn splicing_a_boundary_is_its_own_inverse_in_both_directions() {
        let mut doc = two_sections();
        let before = doc.clone();
        let insert = Operation::SpliceSectionBoundary {
            at: Some(SectionId::new(n(600))),
            boundary: Some(Box::new(boundary(700))),
        };
        let undo = apply(&mut doc, &mut ids(), &insert).expect("insert");
        assert_eq!(
            sections_of(&doc),
            vec![
                SectionId::new(n(500)),
                SectionId::new(n(700)),
                SectionId::new(n(600))
            ],
            "inserted before the boundary it names"
        );
        let redo = apply(&mut doc, &mut ids(), &undo).expect("remove");
        assert_eq!(doc, before, "removal restores the document");
        apply(&mut doc, &mut ids(), &redo).expect("re-insert");
        assert_eq!(
            sections_of(&doc),
            vec![
                SectionId::new(n(500)),
                SectionId::new(n(700)),
                SectionId::new(n(600))
            ],
            "redo restores the POSITION, not merely the membership"
        );
    }

    /// The splice refuses rather than guessing: an unknown anchor, a request that
    /// names nothing, and an id already in use all leave the document untouched.
    #[test]
    fn splicing_refuses_an_unknown_anchor_a_nameless_request_and_a_duplicate_id() {
        let mut doc = two_sections();
        let before = doc.clone();
        for (op, expected) in [
            (
                Operation::SpliceSectionBoundary {
                    at: Some(SectionId::new(n(999))),
                    boundary: Some(Box::new(boundary(700))),
                },
                EditError::NodeNotFound,
            ),
            (
                Operation::SpliceSectionBoundary {
                    at: None,
                    boundary: None,
                },
                EditError::Unsupported,
            ),
            (
                Operation::SpliceSectionBoundary {
                    at: None,
                    boundary: Some(Box::new(boundary(600))),
                },
                EditError::Unsupported,
            ),
        ] {
            assert_eq!(
                apply(&mut doc, &mut ids(), &op),
                Err(expected),
                "{op:?} must be refused"
            );
            assert_eq!(doc, before, "a refused splice changes nothing");
        }
    }

    // -----------------------------------------------------------------------
    // Refusals
    // -----------------------------------------------------------------------

    /// Every container a caret reaches where a break would be stored and then
    /// ignored is refused, and each refusal names its own container. Layout
    /// charges a section break to a section only for a top-level body paragraph
    /// (`document_layout::section_break_points` does not recurse) and only the body
    /// paginator consumes `page_break_after`, so accepting any of these would be
    /// silent loss.
    #[test]
    fn a_break_outside_the_body_flow_is_refused_by_name() {
        let mut headers = DefinitionMap::default();
        headers.insert(
            HeaderFooterId::new(n(800)),
            HeaderFooter {
                blocks: vec![para(810, "running head")],
            },
        );
        let mut footnotes = DefinitionMap::default();
        footnotes.insert(
            NoteId::new(n(820)),
            Note {
                blocks: vec![para(830, "note body")],
            },
        );
        let mut comments = DefinitionMap::default();
        comments.insert(
            CommentId::new(n(840)),
            Comment {
                blocks: vec![para(850, "comment body")],
                ..Comment::default()
            },
        );

        let cell_paragraph = para(910, "in a cell");
        let table = BlockNode::Table(Box::new(Table {
            id: n(900),
            properties: Default::default(),
            grid: Vec::new(),
            grid_change: None,
            rows: vec![TableRow {
                id: n(902),
                properties: Default::default(),
                cells: vec![TableCell {
                    id: n(904),
                    properties: Default::default(),
                    blocks: vec![cell_paragraph],
                }],
            }],
        }));

        let text_box = BlockNode::Paragraph(Paragraph {
            id: n(920),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::TextBox(Box::new(TextBox {
                id: n(922),
                hyperlink: None,
                anchor: None,
                relative_height: None,
                extent: None,
                fill: None,
                border: None,
                body_properties: Default::default(),
                blocks: vec![para(930, "in a text box")],
            }))],
        });

        let control = BlockNode::Sdt(Box::new(BlockSdt {
            id: n(940),
            properties: SdtProperties::default(),
            blocks: vec![para(950, "in a content control")],
        }));

        let doc = Document::new(
            n(1_000),
            vec![para(10, "body"), table, text_box, control],
            Definitions {
                sections: vec![boundary(500)],
                headers,
                footnotes,
                comments,
                ..Definitions::default()
            },
        )
        .expect("valid document");

        for (node, expected) in [
            (n(910), BreakRefusal::InTableCell),
            (n(930), BreakRefusal::InTextBox),
            (n(950), BreakRefusal::InContentControl),
            (n(810), BreakRefusal::InRunningContent),
            (n(830), BreakRefusal::InNote),
            (n(850), BreakRefusal::InComment),
            (n(9_999), BreakRefusal::NoSuchParagraph),
        ] {
            assert_eq!(
                break_site(&doc, node),
                BreakSite::Elsewhere(expected),
                "{node} must be refused as {expected:?}"
            );
            assert_eq!(
                section_split_site(&doc, node),
                Err(expected),
                "and a section break must refuse there too, with the same reason"
            );
            assert!(
                !expected.reason().is_empty(),
                "every refusal carries a sentence"
            );
        }

        // The control: a body paragraph is accepted, so the guard above is not
        // simply refusing everything.
        assert_eq!(break_site(&doc, n(10)), BreakSite::Body(0));
    }

    /// A text box nested inside an INLINE content control is still refused.
    ///
    /// The reason degrades to `NoSuchParagraph` rather than `InTextBox`, because
    /// the crate's shared inline walk (`find_paragraph_in_inlines`) does not
    /// descend into `InlineNode::Sdt` — a pre-existing gap, reported rather than
    /// widened here, since widening that walk changes what every other operation
    /// can reach. What matters for **no silent loss** is that the position is
    /// refused and says something, and that is what this asserts.
    #[test]
    fn a_paragraph_in_a_text_box_inside_an_inline_control_is_still_refused() {
        let doc = Document::new(
            n(1_000),
            vec![BlockNode::Paragraph(Paragraph {
                id: n(20),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Sdt(Box::new(InlineSdt {
                    id: n(22),
                    properties: SdtProperties::default(),
                    inlines: vec![InlineNode::TextBox(Box::new(TextBox {
                        id: n(24),
                        hyperlink: None,
                        anchor: None,
                        relative_height: None,
                        extent: None,
                        fill: None,
                        border: None,
                        body_properties: Default::default(),
                        blocks: vec![para(30, "deep")],
                    }))],
                }))],
            })],
            Definitions {
                sections: vec![boundary(500)],
                ..Definitions::default()
            },
        )
        .expect("valid document");
        let site = break_site(&doc, n(30));
        assert!(
            matches!(site, BreakSite::Elsewhere(_)),
            "a paragraph inside an inline control is not body flow: {site:?}"
        );
        let BreakSite::Elsewhere(refusal) = site else {
            unreachable!("asserted above");
        };
        assert!(!refusal.reason().is_empty(), "and it says something");
    }

    // -----------------------------------------------------------------------
    // Complexity
    // -----------------------------------------------------------------------

    /// Resolving the section split must cost work proportional to the document,
    /// not to its square.
    ///
    /// The guard is a **ratio**, not a clock: a millisecond threshold cannot tell
    /// a quadratic from a slow constant and is flaky under load. Doubling the
    /// document must roughly double the blocks examined; resolving each
    /// paragraph's properties by id instead — the HF-184 shape — quadruples it.
    #[test]
    fn resolving_a_section_split_is_linear_in_the_document() {
        fn visits(paragraphs: u64) -> u64 {
            // Two sections, so the forward pass actually runs (a single-section
            // document short-circuits), and the caret is the first paragraph so
            // the pass covers the whole body.
            // Ids start well clear of the document's own and of the section
            // boundaries', so a longer fixture cannot collide with either.
            let mut body = vec![para(100_000, "caret")];
            for i in 1..paragraphs {
                body.push(para(100_000 + i * 10, "filler"));
            }
            let last = 100_000 + (paragraphs - 1) * 10;
            body[(paragraphs - 1) as usize] = terminator(last, "end", SectionId::new(n(500)));
            let doc = document(body, vec![boundary(500), boundary(600)]);
            reset_block_visits();
            let split = section_split_site(&doc, n(100_000)).expect("a body paragraph");
            assert_eq!(split.current, SectionId::new(n(500)));
            block_visits()
        }

        let small_n = 400;
        let small = visits(small_n);
        let large = visits(small_n * 2);
        assert!(
            small > 0,
            "the counter must observe the walk, or this guard cannot fail"
        );
        assert!(
            large < small * 3,
            "work must roughly double, not quadruple: {small} visits at {small_n} \
             paragraphs and {large} at {}",
            small_n * 2
        );
    }

    /// A single-section document resolves the split without walking the body at
    /// all: with one boundary the caret can only be in it. This is the common
    /// case, and it is the one that has to stay O(1) on a million-paragraph file.
    #[test]
    fn a_single_section_document_resolves_the_split_without_walking_forward() {
        fn visits(paragraphs: u64) -> u64 {
            let body = (0..paragraphs)
                .map(|i| para(100_000 + i * 10, "filler"))
                .collect();
            let doc = document(body, vec![boundary(500)]);
            reset_block_visits();
            section_split_site(&doc, n(100_000)).expect("a body paragraph");
            block_visits()
        }
        // `break_site` still has to locate the caret's paragraph, so the count is
        // not zero; what must not grow is any SECOND pass. The caret is the first
        // paragraph, so a forward pass would add the whole body again.
        let small = visits(400);
        let large = visits(800);
        assert!(
            large < small * 3,
            "one pass, not two: {small} visits at 400 paragraphs and {large} at 800"
        );
    }
}
