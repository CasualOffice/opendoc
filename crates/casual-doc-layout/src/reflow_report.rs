//! What a reflowed column is approximating **in this document** — the survey
//! behind [`LayoutView::approximations`](crate::document_layout::LayoutView::approximations).
//!
//! The list it feeds used to be a constant: three sentences returned for every
//! reflowed document, keyed on nothing but the view. A document with no
//! footnotes was told about footnote placement and a document with no `PAGE`
//! field about page numbers, and the one approximation that actually loses
//! something a reader can see — content wider than the measure — was not in it
//! at all (`docs/163` R-7, and R-1 for the omission).
//!
//! So the report is derived. One walk answers four yes/no questions and stops as
//! soon as all four are answered, which is the shape a report of this kind
//! wants: a document either has the thing or it does not, and the sentence is
//! only true if it does.
//!
//! **This is not a renderer and must never become one.** It reads declared
//! widths, not resolved ones: a table whose `w:tblW` is wider than the measure
//! *will* have been fitted, which is what the sentence says, and a table that
//! fits says nothing. Asking the layout instead would mean laying the document
//! out twice.

use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::GroupChild;
use casual_doc_model::v1::HorizontalAnchor;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::PaginatedField;
use casual_doc_model::v1::Table;
use casual_doc_model::v1::VerticalAnchor;

use crate::units::{Twip, emu_to_twip_extent};

/// The four questions a reflow report asks of a document. Every field is "this
/// document contains at least one of these", so a `false` is a sentence the host
/// must not show.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Survey {
    /// A table or an inline drawing that declares itself wider than the measure,
    /// and has therefore been fitted to it (`docs/163` R-1).
    pub(crate) over_wide_content: bool,
    /// A drawing anchored to the page or to a margin, whose paper-relative
    /// position a tile cannot honour (`docs/151` §8 item 1).
    pub(crate) page_anchored_drawing: bool,
    /// A footnote or endnote reference, whose note body lands at a tile boundary
    /// rather than a page bottom.
    pub(crate) note: bool,
    /// A `PAGE` or `NUMPAGES` field, which has no tile answer (`docs/151` §6.5).
    pub(crate) page_field: bool,
}

impl Survey {
    /// Whether every question is answered `true`, so the walk can stop.
    const fn complete(self) -> bool {
        self.over_wide_content && self.page_anchored_drawing && self.note && self.page_field
    }
}

/// Surveys `document`'s body for the four things a reflowed column at `measure`
/// approximates.
///
/// Complexity: `O(document)` in the worst case — one visit per block and inline,
/// descending into tables, content controls and text boxes — and it short-circuits
/// the moment all four answers are known, so the common real document (which has
/// a `PAGE` field in a header, not in the body, and no page-anchored art) costs
/// the walk and nothing else. Called once per view change; never on an edit path.
pub(crate) fn survey(document: &Document, measure: Twip) -> Survey {
    let mut found = Survey::default();
    visit_blocks(document.body(), measure, &mut found);
    found
}

fn visit_blocks(blocks: &[BlockNode], measure: Twip, found: &mut Survey) {
    for block in blocks {
        if found.complete() {
            return;
        }
        match block {
            BlockNode::Paragraph(paragraph) => visit_inlines(&paragraph.inlines, measure, found),
            BlockNode::Table(table) => {
                if !found.over_wide_content && table_is_over_wide(table, measure) {
                    found.over_wide_content = true;
                }
                for row in &table.rows {
                    for cell in &row.cells {
                        visit_blocks(&cell.blocks, measure, found);
                    }
                }
            }
            BlockNode::Sdt(sdt) => visit_blocks(&sdt.blocks, measure, found),
            // An `w:altChunk` references a preserved part; it carries no inline
            // content here to survey.
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// Whether a table declares itself wider than `measure`.
///
/// Both of the solver's over-wide arms are asked, because either one of them
/// alone produces the loss: an explicit `w:tblW` in `dxa` is consulted by
/// `solve_column_widths` without reference to the available width, and a declared
/// grid is taken verbatim under a fixed layout. A `pct` width resolves against
/// the available width and can never exceed it, so it is not asked.
fn table_is_over_wide(table: &Table, measure: Twip) -> bool {
    let declared = table
        .properties
        .width
        .and_then(|width| width.dxa_twips())
        .unwrap_or(0);
    let grid: i32 = table
        .grid
        .iter()
        .filter_map(|column| column.width_twips)
        .sum();
    declared > measure.raw() || grid > measure.raw()
}

fn visit_inlines(inlines: &[InlineNode], measure: Twip, found: &mut Survey) {
    for inline in inlines {
        if found.complete() {
            return;
        }
        match inline {
            InlineNode::Drawing(drawing) => {
                if !found.over_wide_content
                    && let Some(extent) = drawing.extent.as_ref()
                    && emu_to_twip_extent(extent.width_emu) > measure
                {
                    found.over_wide_content = true;
                }
            }
            InlineNode::AnchoredDrawing(drawing) => {
                if anchor_is_paper_relative(
                    drawing.anchor.horizontal.relative_from,
                    drawing.anchor.vertical.relative_from,
                ) {
                    found.page_anchored_drawing = true;
                }
            }
            InlineNode::Group(group) => {
                if let Some(anchor) = group.anchor.as_ref()
                    && anchor_is_paper_relative(
                        anchor.horizontal.relative_from,
                        anchor.vertical.relative_from,
                    )
                {
                    found.page_anchored_drawing = true;
                }
                for child in &group.children {
                    if let GroupChild::TextBox(text_box) = child {
                        visit_blocks(&text_box.blocks, measure, found);
                    }
                }
            }
            InlineNode::TextBox(text_box) => {
                if let Some(anchor) = text_box.anchor.as_ref()
                    && anchor_is_paper_relative(
                        anchor.horizontal.relative_from,
                        anchor.vertical.relative_from,
                    )
                {
                    found.page_anchored_drawing = true;
                }
                visit_blocks(&text_box.blocks, measure, found);
            }
            InlineNode::NoteReference(_) => found.note = true,
            InlineNode::Field(field) => {
                if matches!(
                    PaginatedField::parse(&field.instruction),
                    Some(PaginatedField::PageNumber | PaginatedField::PageCount)
                ) {
                    found.page_field = true;
                }
                visit_inlines(&field.inlines, measure, found);
            }
            InlineNode::Hyperlink(hyperlink) => visit_inlines(&hyperlink.inlines, measure, found),
            InlineNode::Revision(revision) => visit_inlines(&revision.inlines, measure, found),
            InlineNode::Sdt(sdt) => visit_inlines(&sdt.inlines, measure, found),
            _ => {}
        }
    }
}

/// Whether either axis of an anchor is relative to the paper rather than to the
/// text — the anchors a tile has no edge for.
const fn anchor_is_paper_relative(horizontal: HorizontalAnchor, vertical: VerticalAnchor) -> bool {
    matches!(
        horizontal,
        HorizontalAnchor::Page
            | HorizontalAnchor::Margin
            | HorizontalAnchor::LeftMargin
            | HorizontalAnchor::RightMargin
            | HorizontalAnchor::InsideMargin
            | HorizontalAnchor::OutsideMargin
    ) || matches!(
        vertical,
        VerticalAnchor::Page
            | VerticalAnchor::Margin
            | VerticalAnchor::TopMargin
            | VerticalAnchor::BottomMargin
            | VerticalAnchor::InsideMargin
            | VerticalAnchor::OutsideMargin
    )
}
