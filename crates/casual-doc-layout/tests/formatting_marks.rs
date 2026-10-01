//! Non-printing characters (`docs/153` `shell.formatting-marks`), end to end
//! through the real driver.
//!
//! The geometry half of the proof — that marks move nothing and the geometry
//! golden does not budge — lives in `geometry_snapshot.rs`, next to the golden it
//! is held against. This file holds the other half: that the marks are actually
//! *there*, one per non-printing character, in every container the document flows
//! through, and that each mark set paints only its own mark.
//!
//! Every assertion reads the **suffix** of the page's display list — the items
//! past the end of the marks-off list — which is the invariant
//! `casual_doc_layout::formatting_marks` establishes, so these tests cannot pass
//! by accident on an item the content pass emitted.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::{ComposeOptions, compose_page, compose_page_with};
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::formatting_marks::FormattingMarks;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, DefinitionMap, Definitions, Document, GridColumn,
    HeaderFooter as ModelHeaderFooter, HeaderFooterId, HeaderFooterKind, HeaderFooterRef,
    InlineNode, PageMargins, PageSize, Paragraph, ParagraphProperties, Run, RunProperties,
    SectionBoundary, SectionColumns, SectionId, Tab, TabAlignment, TabStop, Table, TableCell,
    TableCellProperties, TableProperties, TableRow, TableRowProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).expect("a valid node id")
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default().into(),
        inlines,
    })
}

/// A paragraph with one explicit left tab stop at 2 inches.
fn tabbed(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            tabs: vec![TabStop {
                position_twips: 2_880,
                alignment: TabAlignment::Start,
                leader: None,
            }],
            ..ParagraphProperties::default()
        }
        .into(),
        inlines,
    })
}

/// US-Letter, one-inch margins, single column, with the given header refs.
fn section(headers: Vec<HeaderFooterRef>) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(9)),
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
        headers,
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

fn document(body: Vec<BlockNode>) -> Document {
    Document::new(
        node(1),
        body,
        Definitions {
            sections: vec![section(Vec::new())],
            ..Definitions::default()
        },
    )
    .expect("a valid document")
}

/// How many paint items of each mark-bearing kind the overlay produced.
///
/// The mark shapes are distinguishable by primitive: a space dot and a pilcrow's
/// lobe are the only ellipses, an arrowhead is the only polygon, and the
/// page/column-break rule is the only `Shape`. Counting by primitive is what lets
/// each mark set be asserted on its own.
#[derive(Debug, Default, Eq, PartialEq)]
struct MarkCounts {
    ellipses: usize,
    rects: usize,
    polygons: usize,
    shapes: usize,
}

/// The marks a document's first page produced, read as the SUFFIX of the
/// marks-on display list.
fn marks_of(doc: &Document, marks: FormattingMarks) -> (MarkCounts, Vec<PaintItem>) {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(doc, &shaper);
    marks_of_layout(&layout, 0, marks)
}

fn marks_of_layout(
    layout: &PaginatedLayout,
    page_index: usize,
    marks: FormattingMarks,
) -> (MarkCounts, Vec<PaintItem>) {
    let page = &layout.pages[page_index];
    let plain = compose_page(page);
    let marked = compose_page_with(page, &ComposeOptions { marks });
    assert!(
        marked.items.len() >= plain.items.len(),
        "marks must only add items"
    );
    let suffix: Vec<PaintItem> = marked.items[plain.items.len()..].to_vec();
    let mut counts = MarkCounts::default();
    for item in &suffix {
        match item {
            PaintItem::Ellipse { .. } => counts.ellipses += 1,
            PaintItem::Rect { .. } => counts.rects += 1,
            PaintItem::Polygon { .. } => counts.polygons += 1,
            PaintItem::Shape { .. } => counts.shapes += 1,
            other => panic!("the overlay emitted an unexpected primitive: {other:?}"),
        }
    }
    (counts, suffix)
}

#[test]
fn the_default_mark_set_changes_nothing_at_all() {
    let doc = document(vec![paragraph(100, vec![run(101, "one two three")])]);
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    let page = &layout.pages[0];
    let plain = serde_json::to_string(&compose_page(page)).expect("a list serializes");
    let defaulted = serde_json::to_string(&compose_page_with(page, &ComposeOptions::default()))
        .expect("a list serializes");
    assert_eq!(
        plain, defaulted,
        "compose_page_with(default) must be byte-identical to compose_page"
    );
}

#[test]
fn a_space_dot_is_painted_once_per_space_and_nowhere_else() {
    // Four spaces, and nothing else in the document that could be mistaken for
    // one: the paragraph mark is turned off for this assertion.
    let doc = document(vec![paragraph(
        100,
        vec![run(101, "one two three four five")],
    )]);
    let (counts, _) = marks_of(
        &doc,
        FormattingMarks {
            space: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        counts,
        MarkCounts {
            ellipses: 4,
            ..MarkCounts::default()
        },
        "one dot per space, and no other primitive"
    );
}

#[test]
fn a_tab_arrow_is_painted_once_per_tab_inside_its_own_advance() {
    let doc = document(vec![tabbed(
        100,
        vec![
            run(101, "Label"),
            InlineNode::Tab(Tab { id: node(102) }),
            run(103, "Value"),
        ],
    )]);
    let (counts, suffix) = marks_of(
        &doc,
        FormattingMarks {
            tab: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        counts,
        MarkCounts {
            rects: 1,
            polygons: 1,
            ..MarkCounts::default()
        },
        "one shaft and one head for the one tab"
    );
    // The arrow is inside the tab's advance: the tab ran from the end of "Label"
    // to the 2-inch stop, so the head must land at or before the stop's page x
    // (content origin 1440 + 2880).
    let head_x = suffix
        .iter()
        .find_map(|item| match item {
            PaintItem::Polygon { points, .. } => Some(points[0].x.raw()),
            _ => None,
        })
        .expect("the arrow has a head");
    assert!(
        head_x <= 1_440 + 2_880,
        "the head ({head_x}) must not pass the tab stop ({})",
        1_440 + 2_880
    );
    assert!(
        head_x > 1_440,
        "the head must be past the paragraph's left edge"
    );
}

#[test]
fn a_tab_free_paragraph_records_no_tab_and_paints_no_arrow() {
    // The negative half of the recording: `Line::tab_extents` must be empty for
    // ordinary text, or every paragraph in every document would pay for it.
    let doc = document(vec![paragraph(100, vec![run(101, "No tabs here at all")])]);
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    for placed in &layout.pages[0].placed {
        if let BlockFragment::Paragraph { lines, .. } = &placed.fragment {
            for line in &lines.lines {
                assert!(
                    line.tab_extents.is_empty(),
                    "a tab-free line recorded {:?}",
                    line.tab_extents
                );
            }
        }
    }
    let (counts, _) = marks_of(
        &doc,
        FormattingMarks {
            tab: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(counts, MarkCounts::default());
}

#[test]
fn two_tabs_on_one_line_are_recorded_and_marked_twice() {
    let doc = document(vec![tabbed(
        100,
        vec![
            run(101, "A"),
            InlineNode::Tab(Tab { id: node(102) }),
            run(103, "B"),
            InlineNode::Tab(Tab { id: node(104) }),
            run(105, "C"),
        ],
    )]);
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    let recorded: usize = layout.pages[0]
        .placed
        .iter()
        .filter_map(|placed| match &placed.fragment {
            BlockFragment::Paragraph { lines, .. } => Some(lines),
            _ => None,
        })
        .flat_map(|lines| &lines.lines)
        .map(|line| line.tab_extents.len())
        .sum();
    assert_eq!(recorded, 2, "both tabs are recorded");
    let (counts, _) = marks_of(
        &doc,
        FormattingMarks {
            tab: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(counts.polygons, 2, "two arrowheads");
}

#[test]
fn a_pilcrow_is_painted_once_per_paragraph_including_an_empty_one() {
    let doc = document(vec![
        paragraph(100, vec![run(101, "First")]),
        paragraph(110, Vec::new()),
        paragraph(120, vec![run(121, "Third")]),
    ]);
    let (counts, _) = marks_of(
        &doc,
        FormattingMarks {
            paragraph: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        counts,
        MarkCounts {
            // Three pilcrows: one lobe + two stems each.
            ellipses: 3,
            rects: 6,
            ..MarkCounts::default()
        },
        "the empty paragraph gets a pilcrow too — that is the whole point of the mark"
    );
}

#[test]
fn a_hard_break_gets_a_return_arrow_and_the_paragraph_still_gets_its_pilcrow() {
    let doc = document(vec![paragraph(
        100,
        vec![
            run(101, "First"),
            InlineNode::Break(Break {
                id: node(102),
                kind: BreakKind::Line,
            }),
            run(103, "Second"),
        ],
    )]);
    let (only_break, _) = marks_of(
        &doc,
        FormattingMarks {
            line_break: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        only_break,
        MarkCounts {
            rects: 2,
            polygons: 1,
            ..MarkCounts::default()
        },
        "the return arrow is a stem, a bar and a head"
    );
    let (only_paragraph, _) = marks_of(
        &doc,
        FormattingMarks {
            paragraph: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        only_paragraph.ellipses, 1,
        "a hard break is NOT a paragraph end: one pilcrow for the one paragraph"
    );
}

#[test]
fn a_page_break_gets_a_rule_and_a_soft_wrap_gets_nothing() {
    let doc = document(vec![paragraph(
        100,
        vec![
            run(101, "Before"),
            InlineNode::Break(Break {
                id: node(102),
                kind: BreakKind::Page,
            }),
            run(103, "After"),
        ],
    )]);
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    let (counts, suffix) = marks_of_layout(
        &layout,
        0,
        FormattingMarks {
            page_break: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        counts,
        MarkCounts {
            shapes: 1,
            ..MarkCounts::default()
        },
        "one dashed rule for the authored page break"
    );
    // It spans the content column, not a nominal stub.
    let span = suffix
        .iter()
        .find_map(|item| match item {
            PaintItem::Shape {
                geometry: casual_doc_layout::display::ShapeGeometry::Line { from, to },
                ..
            } => Some(to.x.raw() - from.x.raw()),
            _ => None,
        })
        .expect("the rule is a line");
    assert_eq!(span, 12_240 - 2 * 1_440, "the rule spans the text column");

    // A paragraph long enough to wrap produces no rule and no return arrow: a soft
    // wrap is not a non-printing character.
    let wrapping = document(vec![paragraph(200, vec![run(201, &"word ".repeat(200))])]);
    let (wrapped, _) = marks_of(
        &wrapping,
        FormattingMarks {
            page_break: true,
            line_break: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        wrapped,
        MarkCounts::default(),
        "a soft wrap must not be marked as a break"
    );
}

#[test]
fn marks_are_painted_inside_a_table_cell() {
    // The document's ONLY space is in the cell, so a non-empty suffix can only
    // have come from the cell's own content: the uniform-flow rule in practice.
    let doc = document(vec![
        paragraph(100, vec![run(101, "Outside")]),
        BlockNode::Table(Box::new(Table {
            id: node(200),
            grid: vec![GridColumn {
                width_twips: Some(4_000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: node(201),
                properties: TableRowProperties::default(),
                cells: vec![TableCell {
                    id: node(202),
                    properties: TableCellProperties::default(),
                    blocks: vec![paragraph(203, vec![run(204, "in cell")])],
                }],
            }],
        })),
    ]);
    let (counts, suffix) = marks_of(
        &doc,
        FormattingMarks {
            space: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        counts,
        MarkCounts {
            ellipses: 1,
            ..MarkCounts::default()
        },
        "the one space inside the cell is marked"
    );
    // And it is inside the table, not up with the body paragraph.
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    let row_top = layout.pages[0]
        .placed
        .iter()
        .find(|placed| matches!(placed.fragment, BlockFragment::TableRow { .. }))
        .expect("the table paginated to a row")
        .rect
        .origin
        .y
        .raw();
    let dot_y = suffix
        .iter()
        .find_map(|item| match item {
            PaintItem::Ellipse { rect, .. } => Some(rect.origin.y.raw()),
            _ => None,
        })
        .expect("a dot was painted");
    assert!(
        dot_y >= row_top,
        "the cell's dot ({dot_y}) sits in the row ({row_top}), not in the body above it"
    );
}

#[test]
fn marks_are_painted_in_a_running_header() {
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "in header")])],
        },
    );
    // The body carries no space at all, so any dot came from the header band.
    let doc = Document::new(
        node(1),
        vec![paragraph(100, vec![run(101, "Body")])],
        Definitions {
            sections: vec![section(vec![HeaderFooterRef {
                kind: HeaderFooterKind::Default,
                reference: HeaderFooterId::new(node(300)),
            }])],
            headers,
            ..Definitions::default()
        },
    )
    .expect("a valid document");
    let (counts, suffix) = marks_of(
        &doc,
        FormattingMarks {
            space: true,
            ..FormattingMarks::default()
        },
    );
    assert_eq!(
        counts,
        MarkCounts {
            ellipses: 1,
            ..MarkCounts::default()
        },
        "the header's one space is marked"
    );
    let dot_y = suffix
        .iter()
        .find_map(|item| match item {
            PaintItem::Ellipse { rect, .. } => Some(rect.origin.y.raw()),
            _ => None,
        })
        .expect("a dot was painted");
    assert!(
        dot_y < 1_440,
        "the header's dot ({dot_y}) sits above the top margin (1440)"
    );
}

#[test]
fn every_glyph_run_keeps_its_baseline_when_marks_are_turned_on() {
    // "Identical baselines", asserted on the painted output rather than inferred
    // from the layout: the Glyphs items of the marks-on list must be the same
    // runs, at the same origins, in the same order.
    let doc = document(vec![
        tabbed(
            100,
            vec![
                run(101, "Label"),
                InlineNode::Tab(Tab { id: node(102) }),
                run(103, "Value with spaces"),
            ],
        ),
        paragraph(110, Vec::new()),
        paragraph(120, vec![run(121, &"wrapping text ".repeat(40))]),
    ]);
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    for page in &layout.pages {
        let plain: Vec<String> = compose_page(page)
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Glyphs { run } => Some(serde_json::to_string(run).expect("serializes")),
                _ => None,
            })
            .collect();
        let marked: Vec<String> = compose_page_with(
            page,
            &ComposeOptions {
                marks: FormattingMarks::ALL,
            },
        )
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => Some(serde_json::to_string(run).expect("serializes")),
            _ => None,
        })
        .collect();
        assert!(!plain.is_empty(), "the fixture paints text");
        assert_eq!(
            plain, marked,
            "a glyph run moved, or changed, when marks were turned on"
        );
    }
}

#[test]
fn every_mark_set_paints_only_its_own_mark() {
    // One document carrying one of each non-printing character, composed once per
    // flag. A mark that leaked into another set's output would show up here as an
    // extra primitive, which is the failure a single all-on assertion would hide.
    let doc = document(vec![tabbed(
        100,
        vec![
            run(101, "A"),
            InlineNode::Tab(Tab { id: node(102) }),
            run(103, "B C"),
            InlineNode::Break(Break {
                id: node(104),
                kind: BreakKind::Line,
            }),
            run(105, "D"),
            InlineNode::Break(Break {
                id: node(106),
                kind: BreakKind::Page,
            }),
            run(107, "E"),
        ],
    )]);
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    let only = |marks: FormattingMarks| {
        let mut total = MarkCounts::default();
        for index in 0..layout.pages.len() {
            let (counts, _) = marks_of_layout(&layout, index, marks);
            total.ellipses += counts.ellipses;
            total.rects += counts.rects;
            total.polygons += counts.polygons;
            total.shapes += counts.shapes;
        }
        total
    };
    let space_only = only(FormattingMarks {
        space: true,
        ..FormattingMarks::default()
    });
    assert_eq!(space_only.ellipses, 1, "one space, in \"B C\"");
    assert_eq!(
        space_only.polygons + space_only.shapes + space_only.rects,
        0
    );

    let tab_only = only(FormattingMarks {
        tab: true,
        ..FormattingMarks::default()
    });
    assert_eq!((tab_only.rects, tab_only.polygons), (1, 1));
    assert_eq!(tab_only.ellipses + tab_only.shapes, 0);

    let paragraph_only = only(FormattingMarks {
        paragraph: true,
        ..FormattingMarks::default()
    });
    assert_eq!(
        (paragraph_only.ellipses, paragraph_only.rects),
        (1, 2),
        "one paragraph, so one pilcrow, however many breaks it holds"
    );
    assert_eq!(paragraph_only.polygons + paragraph_only.shapes, 0);

    let break_only = only(FormattingMarks {
        line_break: true,
        ..FormattingMarks::default()
    });
    assert_eq!((break_only.rects, break_only.polygons), (2, 1));

    let page_only = only(FormattingMarks {
        page_break: true,
        ..FormattingMarks::default()
    });
    assert_eq!(page_only.shapes, 1);
    assert_eq!(page_only.ellipses + page_only.rects + page_only.polygons, 0);

    // All on is exactly the sum of the parts: no mark depends on another being on.
    let all = only(FormattingMarks::ALL);
    assert_eq!(
        all,
        MarkCounts {
            ellipses: space_only.ellipses + paragraph_only.ellipses,
            rects: tab_only.rects + paragraph_only.rects + break_only.rects,
            polygons: tab_only.polygons + break_only.polygons,
            shapes: page_only.shapes,
        }
    );
}
