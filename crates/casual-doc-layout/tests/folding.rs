//! Folding — ADR-049, `docs/157`.
//!
//! A collapsed heading hides the blocks of its own subtree from the **flow**, so
//! they contribute no fragments and no height and pagination closes up. The
//! point of the design, and the thing every guard here is about, is that this is
//! **reflow and not blanking**: the page count falls. A filter that merely
//! skipped painting would leave a blank band where the content was, and that is
//! the one wrong answer with a plausible shape — it is also the only answer
//! ONLYOFFICE's architecture could reach, since their pagination loop
//! (`word/Editor/Document.js:3707`) has no visibility filter at the block tier
//! at all. They have no folding whatever: zero `collaps` hits across the 232-file
//! Word engine, and they discard `w15:collapsed` structurally.
//!
//! Every guard here was driven **red** by mutating the engine before it was
//! trusted; the mutation is named on each test.
//!
//! The properties, in the order they matter:
//!
//! 1. **Inertness.** An empty [`FoldSet`] produces byte-for-byte what the driver
//!    produced before the parameter existed. Half a guard on its own; the other
//!    half is `geometry_snapshot.golden`, which cannot move with the code.
//! 2. **Reflow, not blanking.** The page count falls, and the visible content is
//!    continuous across the fold.
//! 3. **The range is the outline's.** The collapsed heading's own paragraph stays
//!    visible; hiding runs to the next heading whose level number is the same or
//!    lower.
//! 4. **Content is filtered, structure is not.** A hidden block still closes its
//!    section, so the geometry and running content of the visible pages *before*
//!    the fold do not move; and it still advances the list counters the visible
//!    items read.
//! 5. **Complexity.** Guarded by doubling, not by a millisecond threshold.
//! 6. **A float in a hidden paragraph does not paint.** `anchor::locate` falls
//!    back to page 1 when a paragraph was not placed, so without the gate the
//!    picture inside a folded section appears on the first page of the document.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::{
    LayoutView, paginate_document_after_edit_folded, paginate_document_folded, paginate_document_in,
};
use casual_doc_layout::flow::ReviewView;
use casual_doc_layout::fold::FoldSet;
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, AnchoredDrawing, BlockNode, DefinitionMap, Definitions,
    Document, DrawingAnchor, Extent, GridColumn, HeaderFooter, HeaderFooterId, HeaderFooterKind,
    HeaderFooterRef, HorizontalAnchor, HorizontalPosition, InlineNode, MediaId, MediaReference,
    PageMargins, PageSize, Paragraph, ParagraphProperties, Run, RunProperties, SectionBoundary,
    SectionColumns, SectionId, SectionType, Table, TableCell, TableCellProperties, TableProperties,
    TableRow, TableRowProperties, VerticalAnchor, VerticalPosition, WrapMode,
};
// Separate `use` lines (the repo's anti-conflict convention for new v1 imports).
use casual_doc_model::v1::AbstractNumbering;
use casual_doc_model::v1::AbstractNumberingId;
use casual_doc_model::v1::Indentation;
use casual_doc_model::v1::LevelSuffix;
use casual_doc_model::v1::NumberFormat;
use casual_doc_model::v1::NumberingInstance;
use casual_doc_model::v1::NumberingInstanceId;
use casual_doc_model::v1::NumberingLevel;
use casual_doc_model::v1::NumberingRef;
use casual_doc_model::v1::TabAlignment;
use casual_doc_model::v1::TabStop;

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------

const LINE: &str = "The quick brown fox jumps over the lazy dog and keeps on running. ";

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).expect("a non-zero test node id")
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn paragraph_with(id: u64, properties: ParagraphProperties, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines: vec![run(id + 1_000_000, text)],
    })
}

/// A body paragraph with three wrapped lines of prose.
fn body(id: u64) -> BlockNode {
    paragraph_with(id, ParagraphProperties::default(), &LINE.repeat(3))
}

/// A heading at 1-based outline `level` (`w:outlineLvl` is 0-based).
fn heading(id: u64, level: u8, text: &str) -> BlockNode {
    paragraph_with(
        id,
        ParagraphProperties {
            outline_level: Some(level - 1),
            ..ParagraphProperties::default()
        },
        text,
    )
}

fn letter_section(id: u64) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(id)),
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
        page_numbering: Default::default(),
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

fn document(body: Vec<BlockNode>, definitions: Definitions) -> Document {
    Document::new(node(9_000_000), body, definitions).expect("a well-formed test document")
}

/// `H1 "Alpha"` + `fill` prose paragraphs + `H1 "Omega"` + one prose paragraph,
/// on one Letter section — a long enough section that folding it must drop whole
/// pages, with a heading after it so the fold has a real end.
fn two_section_report(fill: usize) -> Document {
    let mut blocks = vec![heading(10, 1, "Alpha")];
    blocks.extend((0..fill).map(|i| body(100 + i as u64)));
    blocks.push(heading(20, 1, "Omega"));
    blocks.push(body(900));
    document(
        blocks,
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    )
}

fn pages(layout: &PaginatedLayout) -> usize {
    layout.pages.len()
}

/// Every paragraph id the layout actually placed, in page order — "what the
/// reader can see", which is what folding is about.
///
/// Consecutive duplicates are collapsed: a paragraph that straddles a page
/// boundary is placed as two fragments and is still one paragraph, and the
/// question these guards ask is which paragraphs are visible, not how many
/// pieces each was cut into.
fn placed_paragraphs(layout: &PaginatedLayout) -> Vec<NodeId> {
    let mut out = Vec::new();
    for page in &layout.pages {
        for placed in &page.placed {
            collect_paragraph_ids(&placed.fragment, &mut out);
        }
    }
    out.dedup();
    out
}

fn collect_paragraph_ids(fragment: &BlockFragment, out: &mut Vec<NodeId>) {
    match fragment {
        BlockFragment::Paragraph { id, .. } => out.push(*id),
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                for nested in &cell.blocks {
                    collect_paragraph_ids(nested, out);
                }
            }
        }
    }
}

fn fold(ids: &[u64]) -> FoldSet {
    ids.iter().map(|id| node(*id)).collect()
}

fn laid_out(document: &Document, folds: &FoldSet) -> PaginatedLayout {
    let shaper = ParleyShaper::new();
    paginate_document_folded(
        document,
        &shaper,
        ReviewView::Editing,
        LayoutView::Paged,
        folds,
    )
}

// --------------------------------------------------------------------------
// 1. Inertness
// --------------------------------------------------------------------------

/// An empty fold set changes **nothing**: the fold parameter is as inert as the
/// `LayoutView` parameter was when it arrived.
///
/// The assertion that carries the weight is the **absolute** one — with an empty
/// fold set, *every* paragraph of the body is laid out, in document order. A
/// before-versus-after comparison alone cannot prove inertness, because both
/// sides run the same engine: a filter that hid content for an unfolded document
/// would hide it identically on both sides and the comparison would stay green.
/// That was found by mutation rather than reasoned about, and this test is the
/// shape it had to be rewritten into.
///
/// The other half of the claim is `geometry_snapshot.golden`, a committed
/// artifact that cannot move with the code — this file cannot replace it.
///
/// Mutation that reddens it: in `fold::step`, remove the `folds.is_empty()` fast
/// exit and arm on any heading rather than on a folded one. The body of `Alpha`
/// then disappears from a document nobody folded.
#[test]
fn an_empty_fold_set_is_byte_for_byte_the_old_layout() {
    let document = two_section_report(30);
    let shaper = ParleyShaper::new();
    let after = laid_out(&document, &FoldSet::EMPTY);
    assert_eq!(
        placed_paragraphs(&after),
        every_body_paragraph(&document),
        "with nothing folded, every body paragraph is laid out, in document order",
    );
    // And the two entry points agree, so the delegating one cannot drift from
    // the one that takes the fold set.
    let before = paginate_document_in(&document, &shaper, ReviewView::Editing, LayoutView::Paged);
    assert_eq!(
        format!("{before:?}"),
        format!("{after:?}"),
        "the whole layout, not just its page count",
    );
}

/// Every paragraph id in the document body, in document order, descending into
/// tables and content controls — "what a layout of an unfolded document must
/// contain".
fn every_body_paragraph(document: &Document) -> Vec<NodeId> {
    fn walk(block: &BlockNode, out: &mut Vec<NodeId>) {
        match block {
            BlockNode::Paragraph(paragraph) => out.push(paragraph.id),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        for nested in &cell.blocks {
                            walk(nested, out);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                for nested in &sdt.blocks {
                    walk(nested, out);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    let mut out = Vec::new();
    for block in document.body() {
        walk(block, &mut out);
    }
    out
}

// --------------------------------------------------------------------------
// 2. Reflow, not blanking — the milestone
// --------------------------------------------------------------------------

/// **The milestone.** Fold one heading and the page count falls.
///
/// Mutation that reddens it: in `flow_blocks_into`, replace the hidden-block
/// `continue` with flowing the block anyway (or push an empty fragment of the
/// block's own height). The page count then stays where it was, which is the
/// "blanking" behaviour the design rejects.
#[test]
fn folding_a_heading_drops_pages() {
    let document = two_section_report(40);
    let unfolded = laid_out(&document, &FoldSet::EMPTY);
    let folded = laid_out(&document, &fold(&[10]));
    assert!(
        pages(&unfolded) >= 3,
        "the fixture must be long enough for pages to be lost: {} page(s)",
        pages(&unfolded),
    );
    assert!(
        pages(&folded) < pages(&unfolded),
        "folding `Alpha` must DROP pages: {} folded vs {} unfolded",
        pages(&folded),
        pages(&unfolded),
    );
    assert_eq!(
        pages(&folded),
        1,
        "with the whole body of `Alpha` hidden, two headings and one paragraph fit on one page",
    );
    // And what is left is exactly the visible blocks, in document order, with no
    // gap where the content was.
    assert_eq!(
        placed_paragraphs(&folded),
        vec![node(10), node(20), node(900)],
        "the collapsed heading, the next heading and its paragraph — nothing else",
    );
}

/// The collapsed heading's **own paragraph** stays visible. Folding must never
/// move or hide the thing you folded, or there would be nothing to unfold.
///
/// Mutation that reddens it: arm the suppression *before* returning `Visible` in
/// `fold::step` — i.e. hide the heading too.
#[test]
fn the_collapsed_heading_stays_visible() {
    let document = two_section_report(12);
    let folded = laid_out(&document, &fold(&[10]));
    assert!(
        placed_paragraphs(&folded).contains(&node(10)),
        "the folded heading is the affordance; it cannot be what disappears",
    );
}

/// Hiding runs to the next heading whose outline level **number** is the same or
/// lower, transitively including the deeper headings inside it — ADR-049's range,
/// and the arithmetic ONLYOFFICE compute for *selection* in
/// `private_GetNextSiblingOrHigher` (`DocumentOutline.js:400`).
///
/// Mutation that reddens it: compare with `<` instead of `<=` in `fold::step`, so
/// a following **sibling** heading no longer ends the fold and the rest of the
/// document vanishes.
#[test]
fn a_fold_ends_at_the_next_sibling_or_higher_heading() {
    let document = document(
        vec![
            heading(10, 1, "Alpha"),
            heading(11, 2, "Alpha.one"),
            body(12),
            heading(13, 3, "Alpha.one.a"),
            body(14),
            heading(20, 1, "Omega"),
            body(21),
        ],
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    );
    assert_eq!(
        placed_paragraphs(&laid_out(&document, &fold(&[10]))),
        vec![node(10), node(20), node(21)],
        "folding the H1 hides every deeper heading and body under it, and stops at the next H1",
    );
    assert_eq!(
        placed_paragraphs(&laid_out(&document, &fold(&[11]))),
        vec![node(10), node(11), node(20), node(21)],
        "folding the H2 hides only its own subtree",
    );
    assert_eq!(
        placed_paragraphs(&laid_out(&document, &fold(&[13]))),
        vec![node(10), node(11), node(12), node(13), node(20), node(21)],
        "folding the H3 hides only the paragraph under it",
    );
}

/// A `w:pageBreakBefore` **inside** the folded range goes with it. Otherwise
/// folding a section leaves a blank page — visible evidence of invisible
/// content, and worse than either alternative.
///
/// Mutation that reddens it: stamp the break onto the galley before the fold
/// filter runs (move the `continue` below the break handling). A blank page then
/// survives the fold.
#[test]
fn a_hidden_blocks_page_break_hides_with_it() {
    let document = document(
        vec![
            heading(10, 1, "Alpha"),
            paragraph_with(
                11,
                ParagraphProperties {
                    page_break_before: Some(true),
                    ..ParagraphProperties::default()
                },
                "after a forced break",
            ),
            heading(20, 1, "Omega"),
        ],
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    );
    assert_eq!(
        pages(&laid_out(&document, &FoldSet::EMPTY)),
        2,
        "unfolded, the forced break makes a second page",
    );
    assert_eq!(
        pages(&laid_out(&document, &fold(&[10]))),
        1,
        "folded, the break belongs to a block that laid nothing out, so it lays nothing out",
    );
}

// --------------------------------------------------------------------------
// 3. Content is filtered; structure is not
// --------------------------------------------------------------------------

/// A hidden block still **closes its section**. Dropping the section break would
/// change the page geometry and running content of the *visible* pages that
/// precede the fold — a change to content the reader did not fold.
///
/// This is the subtle half of the design, and the guard is deliberately strong:
/// the first page's size and its header text must be identical folded and
/// unfolded.
///
/// Mutation that reddens it: in `flow_blocks_into`, move the fold filter ABOVE
/// the section-break bookkeeping and skip the block entirely (which is what a
/// naive "skip the block" does if the break is stamped by the block's own flow).
/// Page 1 then inherits the second section's geometry and header.
#[test]
fn a_hidden_block_still_closes_its_section() {
    let mut first = letter_section(7_000_001);
    first.section_type = Some(SectionType::NextPage);
    first.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(500)),
    }];
    // The second section is LANDSCAPE with its own header, so inheriting it
    // would be loudly visible on page 1.
    let mut second = letter_section(7_000_002);
    second.page_size = PageSize {
        width_twips: 15_840,
        height_twips: 12_240,
    };
    second.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(501)),
    }];

    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(500)),
        HeaderFooter {
            blocks: vec![paragraph_with(
                600,
                ParagraphProperties::default(),
                "FIRST HEADER",
            )],
        },
    );
    headers.insert(
        HeaderFooterId::new(node(501)),
        HeaderFooter {
            blocks: vec![paragraph_with(
                601,
                ParagraphProperties::default(),
                "SECOND HEADER",
            )],
        },
    );

    // The block that carries the first section's break is INSIDE the folded
    // range: H1 "Alpha", then a paragraph whose `w:pPr` ends section one.
    let closer = BlockNode::Paragraph(Paragraph {
        id: node(11),
        properties: ParagraphProperties {
            section_break: Some(SectionId::new(node(7_000_001))),
            ..ParagraphProperties::default()
        }
        .into(),
        inlines: vec![run(1_011, "the last paragraph of section one")],
    });
    let document = document(
        vec![
            heading(10, 1, "Alpha"),
            closer,
            heading(20, 1, "Omega"),
            body(21),
        ],
        Definitions {
            sections: vec![first, second],
            headers,
            ..Definitions::default()
        },
    );

    let unfolded = laid_out(&document, &FoldSet::EMPTY);
    let folded = laid_out(&document, &fold(&[10]));
    assert!(
        pages(&unfolded) >= 2 && pages(&folded) >= 2,
        "both layouts must still have the two sections' pages: {} / {}",
        pages(&unfolded),
        pages(&folded),
    );
    assert_eq!(
        folded.pages[0].page_size, unfolded.pages[0].page_size,
        "page 1's geometry must not move because a block AFTER it was folded",
    );
    assert_eq!(
        folded.pages[0].content_area, unfolded.pages[0].content_area,
        "nor its content area",
    );
    assert_eq!(
        header_paragraphs(&folded, 0),
        header_paragraphs(&unfolded, 0),
        "page 1's running content must not change either",
    );
    assert_eq!(
        header_paragraphs(&folded, 0),
        vec![node(600)],
        "and it must still be the FIRST section's header (node 600), not the \
         second's (601)",
    );
}

/// The header paragraphs placed on page `index`.
///
/// Asserted by node id rather than by text because a shaped `GlyphRun` carries
/// glyphs and no string: the id is the stronger claim anyway, since the two
/// headers in this fixture are different parts.
fn header_paragraphs(layout: &PaginatedLayout, index: usize) -> Vec<NodeId> {
    let mut out = Vec::new();
    for placed in &layout.pages[index].header {
        collect_paragraph_ids(&placed.fragment, &mut out);
    }
    out
}

/// A fold inside a numbered list keeps the **visible** items' numbers: a hidden
/// block still advances the list counters the following items read, because
/// folding filters content and never document structure.
///
/// Asserted by the width of the marker, in glyphs: eleven items, with items 2–10
/// hidden by folding the H2 above them, so item 11's marker is `11.` — three
/// glyphs. A filter that skipped the counters as well as the content would print
/// `2.`, two glyphs, and the assertion goes red. (A shaped `GlyphRun` carries
/// glyphs and no string, so the glyph count IS the observable here.)
///
/// Mutation that reddens it: in `flow_blocks_into`'s hidden arm, also skip
/// `prepare_list_marker`'s counter advance for the hidden paragraphs — i.e. treat
/// a fold as removing the blocks from the document rather than from the flow.
#[test]
fn a_fold_inside_a_numbered_list_keeps_the_visible_numbers() {
    let abstract_id = AbstractNumberingId::new(node(300));
    let instance_id = NumberingInstanceId::new(node(301));
    let mut definitions = Definitions {
        sections: vec![letter_section(7_000_001)],
        ..Definitions::default()
    };
    definitions.abstract_numbering.insert(
        abstract_id,
        AbstractNumbering {
            levels: vec![NumberingLevel {
                level: 0,
                start: 1,
                num_fmt: Some(NumberFormat::Decimal),
                lvl_text: Some("%1.".to_owned()),
                lvl_jc: None,
                suff: Some(LevelSuffix::Tab),
                is_lgl: false,
                paragraph_properties: Some(ParagraphProperties {
                    indentation: Some(Indentation {
                        start_twips: Some(720),
                        hanging_twips: Some(360),
                        ..Indentation::default()
                    }),
                    tabs: vec![TabStop {
                        position_twips: 720,
                        alignment: TabAlignment::Start,
                        leader: None,
                    }],
                    ..ParagraphProperties::default()
                }),
                run_properties: None,
                style_ref: None,
                lvl_restart: None,
                pstyle: None,
            }],
            multi_level_type: None,
            num_style_link: None,
            style_link: None,
        },
    );
    definitions.numbering.insert(
        instance_id,
        NumberingInstance {
            abstract_ref: abstract_id,
            overrides: Vec::new(),
        },
    );
    let item = |id: u64| {
        paragraph_with(
            id,
            ParagraphProperties {
                numbering: Some(NumberingRef {
                    instance: instance_id,
                    level: 0,
                }),
                ..ParagraphProperties::default()
            },
            "an item",
        )
    };
    // item 1, H2 "Middle", items 2..=10 (hidden by the fold), H1 "End", item 11.
    let mut blocks = vec![item(31), heading(33, 2, "Middle")];
    blocks.extend((2..=10).map(|n| item(40 + n)));
    blocks.push(heading(35, 1, "End"));
    blocks.push(item(60));
    let document = document(blocks, definitions);

    let unfolded = laid_out(&document, &FoldSet::EMPTY);
    let folded = laid_out(&document, &fold(&[33]));
    assert!(
        !placed_paragraphs(&folded).contains(&node(44)),
        "a hidden item must not be laid out",
    );
    let unfolded_marker = marker_glyphs(&unfolded, node(60));
    let folded_marker = marker_glyphs(&folded, node(60));
    assert_eq!(
        unfolded_marker, 3,
        "the fixture must really reach a two-digit marker (`11.` is 3 glyphs); \
         saw {unfolded_marker}",
    );
    assert_eq!(
        folded_marker, unfolded_marker,
        "the last visible item keeps the number the HIDDEN items advanced the \
         counter to: {folded_marker} glyph(s) folded against {unfolded_marker} \
         unfolded",
    );
}

/// How many glyphs the list marker injected ahead of paragraph `target` holds.
///
/// The marker is the first glyph run of the paragraph's first line, which is what
/// `ListMarker::inject` puts there.
fn marker_glyphs(layout: &PaginatedLayout, target: NodeId) -> usize {
    for page in &layout.pages {
        for placed in &page.placed {
            if let BlockFragment::Paragraph { id, lines, .. } = &placed.fragment
                && *id == target
                && let Some(line) = lines.lines.first()
                && let Some(marker) = line.runs.first()
            {
                return marker.glyphs.len();
            }
        }
    }
    0
}

/// A heading **inside a table cell** can be folded, and its range is the rest of
/// that cell — nothing outside it.
///
/// This is `SKILL`'s uniform-flow rule applied to folding: a cell flows through
/// the same pipeline the body does, so it gets the same feature, not a
/// context-limited subset. It is also the reachable half of the
/// suppression-scoping decision. The *other* half — a fold armed in the body
/// leaking into a cell — is unreachable by construction and is stated here rather
/// than asserted: a suppressed block is never descended into, so the body walk
/// can never recurse while armed, and `flow_blocks_into` TAKES the integer at
/// entry and writes it back at exit so a nested sequence cannot see it even if
/// that ever changed.
///
/// Mutation that reddens it: in `flow_blocks_into`, return `Visible`
/// unconditionally when `ctx.table_depth > 0` — i.e. "no folding inside tables",
/// the context-limited feature set the uniform-flow rule forbids. The cell's
/// hidden paragraph then reappears.
#[test]
fn a_heading_inside_a_table_cell_folds_within_the_cell() {
    let table = BlockNode::Table(Box::new(Table {
        id: node(40),
        properties: TableProperties::default(),
        grid: vec![GridColumn {
            width_twips: Some(6_000),
        }],
        rows: vec![TableRow {
            id: node(42),
            properties: TableRowProperties::default(),
            cells: vec![TableCell {
                id: node(43),
                properties: TableCellProperties::default(),
                blocks: vec![
                    heading(44, 2, "Cell heading"),
                    paragraph_with(45, ParagraphProperties::default(), "under the cell heading"),
                ],
            }],
        }],
        grid_change: None,
    }));
    let document = document(
        vec![
            heading(10, 1, "Alpha"),
            table,
            paragraph_with(46, ParagraphProperties::default(), "after the table"),
        ],
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    );
    let unfolded = placed_paragraphs(&laid_out(&document, &FoldSet::EMPTY));
    assert!(
        unfolded.contains(&node(45)),
        "the fixture must lay the cell's second paragraph out when nothing is folded",
    );
    let folded = placed_paragraphs(&laid_out(&document, &fold(&[44])));
    assert!(
        folded.contains(&node(44)),
        "the cell's folded heading stays visible",
    );
    assert!(
        !folded.contains(&node(45)),
        "and the paragraph under it, in the same cell, hides",
    );
    assert!(
        folded.contains(&node(46)),
        "while the body paragraph after the table is untouched — the cell's fold \
         does not escape the cell",
    );
}

// --------------------------------------------------------------------------
// 4. The float gate
// --------------------------------------------------------------------------

/// A square-wrapped picture anchored in a **hidden** paragraph must not paint.
///
/// `anchor::locate` returns `(0, fallback, fallback)` when it cannot find the
/// anchoring paragraph in the layout, so without the gate the drawing lands on
/// **page 1** of the document — a picture from a folded section appearing above
/// the fold, which is the plainest possible "invisible content is visible"
/// failure.
///
/// Mutation that reddens it: delete the `block_is_placed` guard from
/// `place_floats`'s body loop. The anchored drawing then appears on page 1.
#[test]
fn a_float_in_a_folded_paragraph_does_not_paint_on_page_one() {
    let media_id = MediaId::new(node(70));
    let mut media = DefinitionMap::default();
    media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId7".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/image1.png".to_owned(),
        },
    );
    let anchored = BlockNode::Paragraph(Paragraph {
        id: node(50),
        properties: ParagraphProperties::default().into(),
        inlines: vec![
            run(1_050, "a paragraph with a floating picture"),
            InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
                hyperlink: None,
                opacity: None,
                id: node(51),
                media: media_id,
                extent: Extent {
                    width_emu: 914_400,
                    height_emu: 914_400,
                },
                anchor: DrawingAnchor {
                    horizontal: AnchorHorizontal {
                        relative_from: HorizontalAnchor::Page,
                        position: HorizontalPosition::Offset(914_400),
                    },
                    vertical: AnchorVertical {
                        relative_from: VerticalAnchor::Page,
                        position: VerticalPosition::Offset(914_400),
                    },
                    wrap: WrapMode::Square,
                    wrap_text: None,
                    wrap_distances: Default::default(),
                    wrap_polygon: None,
                    behind_doc: false,
                },
                descr: Some("A floating logo".to_owned()),
                relative_height: None,
                crop: None,
                border: None,
                flip_h: false,
                flip_v: false,
                rotation: None,
            })),
        ],
    });
    let document = document(
        vec![
            heading(10, 1, "Alpha"),
            anchored,
            heading(20, 1, "Omega"),
            body(21),
        ],
        Definitions {
            sections: vec![letter_section(7_000_001)],
            media,
            ..Definitions::default()
        },
    );
    let unfolded = laid_out(&document, &FoldSet::EMPTY);
    assert_eq!(
        anchored_count(&unfolded),
        1,
        "the fixture must really place a float when nothing is folded",
    );
    let folded = laid_out(&document, &fold(&[10]));
    assert_eq!(
        anchored_count(&folded),
        0,
        "a float whose paragraph was not laid out must not be placed anywhere, \
         least of all on page 1",
    );
}

fn anchored_count(layout: &PaginatedLayout) -> usize {
    layout.pages.iter().map(|page| page.anchored.len()).sum()
}

// --------------------------------------------------------------------------
// 5. Complexity, guarded by doubling
// --------------------------------------------------------------------------

/// The pass is `O(blocks + hidden blocks stepped)` and the claim is guarded by
/// **doubling**, not by a millisecond threshold: a timing threshold is flaky and
/// cannot tell a slow constant from a quadratic.
///
/// Doubling the hidden content must roughly double neither the page count nor the
/// laid-out block count — it must leave both **unchanged**, because the hidden
/// blocks contribute nothing. That is the strong form of the claim: the work the
/// pass does on the *output* is proportional to what is visible, and the stepping
/// over hidden blocks is the only term that grows.
///
/// Mutation that reddens it: flow hidden blocks at zero height instead of
/// skipping them. The laid-out block count then doubles with the hidden content.
#[test]
fn doubling_the_hidden_content_does_not_grow_the_layout() {
    let n = laid_out(&two_section_report(40), &fold(&[10]));
    let two_n = laid_out(&two_section_report(80), &fold(&[10]));
    assert_eq!(
        pages(&n),
        pages(&two_n),
        "twice as much folded content is still no pages",
    );
    assert_eq!(
        placed_paragraphs(&n).len(),
        placed_paragraphs(&two_n).len(),
        "and no extra laid-out blocks",
    );
    // The other direction, so the fixture is not vacuous: unfolded, 2n really is
    // about twice the pages of n.
    let unfolded_n = pages(&laid_out(&two_section_report(40), &FoldSet::EMPTY));
    let unfolded_2n = pages(&laid_out(&two_section_report(80), &FoldSet::EMPTY));
    assert!(
        unfolded_2n >= unfolded_n * 2 - 1,
        "the fixture must be sensitive to size at all: {unfolded_n} -> {unfolded_2n}",
    );
}

// --------------------------------------------------------------------------
// 6. The incremental cache
// --------------------------------------------------------------------------

/// The decision about the galley cache, asserted both ways.
///
/// A fold set is a **generation input** to the retained galley. So:
///
/// - a fold **toggle** invalidates it and re-derives the body — one re-shape on a
///   gesture, which is allowed;
/// - typing at a **fixed** fold set reuses it, so a keystroke stays `O(edit)`
///   rather than re-shaping the body, which would breach `docs/107` §4.
///
/// Mutation that reddens the first half: drop `retained.folds == folds` from
/// `begin_build`'s reuse filter — the fold change then serves fragments for
/// invisible blocks. Mutation that reddens the second: make the fingerprint a
/// constant, or bypass the cache whenever `!folds.is_empty()` — the rebuilt count
/// then jumps to the whole body on every keystroke.
#[test]
fn the_galley_cache_treats_a_fold_set_as_a_generation() {
    let shaper = ParleyShaper::new();
    let document = two_section_report(30);
    let folds = fold(&[10]);
    let mut cache = GalleyCache::new();

    // Build once folded.
    let first = paginate_document_after_edit_folded(
        &document,
        &shaper,
        &mut cache,
        &DirtySet::complete(Vec::new()),
        ReviewView::Editing,
        None,
        LayoutView::Paged,
        &folds,
    );
    let folded_pages = first.layout.pages.len();
    assert_eq!(
        folded_pages, 1,
        "the incremental path must fold exactly as the fresh one does",
    );

    // A second build at the SAME fold set reuses the retained galley: nothing is
    // re-derived, which is the O(edit) guarantee.
    let _ = paginate_document_after_edit_folded(
        &document,
        &shaper,
        &mut cache,
        &DirtySet::complete(Vec::new()),
        ReviewView::Editing,
        Some(first.layout),
        LayoutView::Paged,
        &folds,
    );
    let reused = cache.rebuilt_last_build();

    // Now UNFOLD. The retained galley describes a different body, so it must not
    // be reused — and the layout must be the unfolded one.
    let unfolded = paginate_document_after_edit_folded(
        &document,
        &shaper,
        &mut cache,
        &DirtySet::complete(Vec::new()),
        ReviewView::Editing,
        None,
        LayoutView::Paged,
        &FoldSet::EMPTY,
    );
    assert!(
        unfolded.layout.pages.len() > folded_pages,
        "unfolding must bring the pages back: {} vs {}",
        unfolded.layout.pages.len(),
        folded_pages,
    );
    assert!(
        cache.rebuilt_last_build() > reused,
        "a fold change must re-derive the body ({} rebuilt) where a no-op edit at \
         a fixed fold set did not ({reused} rebuilt)",
        cache.rebuilt_last_build(),
    );
}
