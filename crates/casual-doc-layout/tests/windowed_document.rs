//! The windowed driver (`docs/113` §6 steps 4 and 5), against the one property
//! that makes it safe to ship:
//!
//! > **A windowed page equals the same page of a full `paginate_document`,
//! > field for field.**
//!
//! Field for field means the whole [`Page`]: its placed content with every
//! glyph, its running header and footer, its resolved `PAGE`/`NUMPAGES` fields,
//! its page borders. Not "the same boundaries" — a window that paginated
//! identically but resolved `NUMPAGES` from its own length would still show the
//! user a wrong number, which is the specific trap `docs/113` §7's fourth
//! unknown names.
//!
//! The corpus is deliberately built so that the assertions can fail:
//!
//! - documents long enough to have a **checkpoint** before the window, so the
//!   resume path is exercised rather than the trivial "window starts at page 0";
//! - a document with a **header and footer carrying `PAGE of NUMPAGES`**, so a
//!   window that took the total from its own page count is caught;
//! - a document whose section **restarts page numbering at 7 in upper-Roman**,
//!   so the closed-form page label is checked against the driver's running fold;
//! - documents that are **refused**, each for its own reason, so the refusals
//!   are not silently unreachable.

use casual_doc_layout::document_layout::document_page_config;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::incremental::PageRange;
use casual_doc_layout::measure::PageOutline;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_layout::windowed::DEFAULT_PAINT_BUDGET_BYTES;
use casual_doc_layout::windowed::DocumentMeasures;
use casual_doc_layout::windowed::FlowResume;
use casual_doc_layout::windowed::NotWindowable;
use casual_doc_layout::windowed::WindowPolicy;
use casual_doc_layout::windowed::measure_document;
use casual_doc_layout::windowed::page_paint_bytes;
use casual_doc_layout::windowed::window_of;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, Field, FieldKind, HeaderFooter, HeaderFooterId,
    HeaderFooterKind, HeaderFooterRef, InlineNode, LineNumberRestart, LineNumbering, NumberFormat,
    PageMargins, PageNumbering, PageSize, Paragraph, ParagraphProperties, Run, RunProperties,
    SectionBoundary, SectionColumns, SectionId, Spacing,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

const RUN_BAND: u64 = 5_000_000;

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default(),
        text: text.to_owned(),
    })
}

fn paragraph(id: u64, properties: ParagraphProperties, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties,
        inlines: vec![run(id + RUN_BAND, text)],
    })
}

fn section(id: u64) -> SectionBoundary {
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
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

/// `count` prose paragraphs — enough of them that the document paginates on
/// shaped heights rather than on forced breaks.
fn prose(count: u64, base: u64) -> Vec<BlockNode> {
    (0..count)
        .map(|i| {
            paragraph(
                base + i * 2,
                ParagraphProperties {
                    spacing: Some(Spacing {
                        before_twips: Some(120),
                        after_twips: Some(180),
                        ..Spacing::default()
                    }),
                    ..ParagraphProperties::default()
                },
                "The quick brown fox jumps over the lazy dog while the editor \
                 reflows this paragraph and every other one on the page.",
            )
        })
        .collect()
}

fn document_with(body: Vec<BlockNode>, definitions: Definitions) -> Document {
    Document::new(node(1), body, definitions).expect("the fixture body is valid")
}

fn plain(count: u64) -> Document {
    document_with(
        prose(count, 10_000),
        Definitions {
            sections: vec![section(9)],
            ..Definitions::default()
        },
    )
}

/// A `PAGE of NUMPAGES` footer and a plain header, so a window that resolved
/// `NUMPAGES` from its own page count is caught.
fn with_running_content(count: u64) -> Document {
    let header_id = HeaderFooterId::new(node(400));
    let footer_id = HeaderFooterId::new(node(410));
    let mut definitions = Definitions::default();
    definitions.headers.insert(
        header_id,
        HeaderFooter {
            blocks: vec![paragraph(420, ParagraphProperties::default(), "Chapter one")],
        },
    );
    definitions.footers.insert(
        footer_id,
        HeaderFooter {
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: node(430),
                properties: ParagraphProperties::default(),
                inlines: vec![
                    InlineNode::Field(Field {
                        id: node(431),
                        kind: FieldKind::Page,
                        instruction: "PAGE".to_owned(),
                        inlines: vec![run(432, "1")],
                        form: None,
                    }),
                    run(433, " of "),
                    InlineNode::Field(Field {
                        id: node(434),
                        kind: FieldKind::NumPages,
                        instruction: "NUMPAGES".to_owned(),
                        inlines: vec![run(435, "1")],
                        form: None,
                    }),
                ],
            })],
        },
    );
    let mut boundary = section(9);
    boundary.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: header_id,
    }];
    boundary.footers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: footer_id,
    }];
    definitions.sections = vec![boundary];
    document_with(prose(count, 20_000), definitions)
}

/// Page numbering restarted at 7 in upper Roman, so the window's closed-form
/// label has something to get wrong.
fn with_restarted_page_numbers(count: u64) -> Document {
    let mut boundary = section(9);
    boundary.page_numbering = PageNumbering {
        start: Some(7),
        format: Some(NumberFormat::UpperRoman),
        ..PageNumbering::default()
    };
    let header_id = HeaderFooterId::new(node(500));
    let mut definitions = Definitions::default();
    definitions.headers.insert(
        header_id,
        HeaderFooter {
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: node(510),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Field(Field {
                    id: node(511),
                    kind: FieldKind::Page,
                    instruction: "PAGE".to_owned(),
                    inlines: vec![run(512, "1")],
                    form: None,
                })],
            })],
        },
    );
    boundary.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: header_id,
    }];
    definitions.sections = vec![boundary];
    document_with(prose(count, 30_000), definitions)
}

/// The documents the windowed path must handle, each named for the thing it
/// would catch.
fn windowable_corpus() -> Vec<(&'static str, Document)> {
    vec![
        ("plain prose, one page", plain(6)),
        ("plain prose, many pages", plain(200)),
        ("page and numpages in a footer", with_running_content(200)),
        (
            "page numbers restarted at VII",
            with_restarted_page_numbers(200),
        ),
    ]
}

fn measures_of(document: &Document, shaper: &ParleyShaper) -> DocumentMeasures {
    measure_document(document, shaper).expect("the corpus document is windowable")
}

#[test]
fn measure_document_reports_exactly_the_pages_a_full_paginate_does() {
    let shaper = ParleyShaper::new();
    let mut multi_page = 0;
    for (name, doc) in windowable_corpus() {
        let full = paginate_document(&doc, &shaper);
        let measures = measures_of(&doc, &shaper);
        let expected: Vec<PageOutline> = full.pages.iter().map(PageOutline::of).collect();
        assert_eq!(
            measures.page_count(),
            full.pages.len(),
            "{name}: the measure tier must agree on the page count"
        );
        assert_eq!(
            measures.pages, expected,
            "{name}: every page boundary must match a full paginate, field for field"
        );
        if full.pages.len() > 1 {
            multi_page += 1;
        }
    }
    assert!(
        multi_page >= 3,
        "the corpus must paginate onto many pages, or agreeing proves nothing ({multi_page})"
    );
}

#[test]
fn every_window_equals_the_same_pages_of_a_full_paginate() {
    let shaper = ParleyShaper::new();
    let policy = WindowPolicy::default();
    let mut windows_past_a_checkpoint = 0;
    for (name, doc) in windowable_corpus() {
        let full = paginate_document(&doc, &shaper);
        let measures = measures_of(&doc, &shaper);
        let total = full.pages.len();
        // A sample of three-page windows, plus the two ends.
        let starts: Vec<usize> = (0..total).step_by(4).chain([total.saturating_sub(1)]).collect();
        for start in starts {
            let visible = PageRange::new(start, (start + 3).min(total));
            let window = window_of(&doc, &shaper, &measures, visible, policy);
            assert!(
                !window.viewport.pages.is_empty(),
                "{name}: window at page {start} built nothing"
            );
            assert_eq!(
                window.viewport.total_pages, total,
                "{name}: a window must report the document's real page count"
            );
            for (offset, visible_page) in window.viewport.pages.iter().enumerate() {
                let index = window.range.start + offset;
                assert_eq!(
                    &visible_page.page, &full.pages[index],
                    "{name}: windowed page {index} differs from the full layout"
                );
            }
            if window.resumed_at_page > 0 {
                windows_past_a_checkpoint += 1;
            }
        }
    }
    // The corpus documents here are shorter than one checkpoint interval, so
    // every window resumes from the document start. The deep-window test below
    // is the one that exercises a recorded checkpoint, and it asserts so.
    assert_eq!(
        windows_past_a_checkpoint, 0,
        "these fixtures are shorter than a checkpoint interval; a window that \
         resumed from a recorded checkpoint means the fixtures changed and this \
         test is no longer covering what its name says"
    );
}

/// The window that matters most: one far into a long document, reached without
/// paginating anything above it.
#[test]
fn a_window_deep_in_a_long_document_equals_the_full_layout() {
    let shaper = ParleyShaper::new();
    let doc = plain(3_000);
    let full = paginate_document(&doc, &shaper);
    let measures = measures_of(&doc, &shaper);
    assert!(
        full.pages.len() > 64,
        "the fixture must be longer than one checkpoint interval ({} pages)",
        full.pages.len()
    );
    assert!(
        !measures.checkpoints.is_empty(),
        "a document this long must have recorded checkpoints"
    );
    let last = full.pages.len() - 1;
    let mut resumed_from_a_checkpoint = 0;
    for start in [full.pages.len() / 2, last.saturating_sub(1), last] {
        let visible = PageRange::new(start, (start + 1).min(full.pages.len()));
        let window = window_of(&doc, &shaper, &measures, visible, WindowPolicy::default());
        assert!(
            !window.viewport.pages.is_empty(),
            "window at page {start} built nothing"
        );
        for (offset, visible_page) in window.viewport.pages.iter().enumerate() {
            let index = window.range.start + offset;
            assert_eq!(
                &visible_page.page, &full.pages[index],
                "windowed page {index} of {} differs from the full layout",
                full.pages.len()
            );
        }
        if window.resumed_at_page > 0 {
            resumed_from_a_checkpoint += 1;
        }
    }
    assert_eq!(
        resumed_from_a_checkpoint, 3,
        "every window this deep must have resumed from a recorded checkpoint, \
         or the resume path is not what produced these pages"
    );
}

#[test]
fn a_plain_prose_document_can_resume_a_flow_at_any_block() {
    let shaper = ParleyShaper::new();
    let measures = measures_of(&plain(200), &shaper);
    assert_eq!(
        measures.resume(),
        FlowResume::AnyBlock,
        "plain prose carries no cross-block flow state"
    );
}

/// The refusals. Each one must be reachable, and each must name its own
/// reason — an "unsupported" that always reports the same cause is a bug
/// waiting to be mistaken for coverage.
#[test]
fn each_refusal_is_reachable_and_names_its_own_reason() {
    let shaper = ParleyShaper::new();

    let mut two_sections = Definitions {
        sections: vec![section(9), section(11)],
        ..Definitions::default()
    };
    two_sections.settings = Default::default();
    let multi_section = document_with(prose(40, 60_000), two_sections);

    let mut columns = section(9);
    columns.columns.count = 2;
    let multi_column = document_with(
        prose(40, 61_000),
        Definitions {
            sections: vec![columns],
            ..Definitions::default()
        },
    );

    let mut numbered_lines = section(9);
    numbered_lines.line_numbering = LineNumbering {
        start: Some(1),
        count_by: Some(1),
        distance: None,
        restart: Some(LineNumberRestart::NewPage),
    };
    let line_numbered = document_with(
        prose(40, 62_000),
        Definitions {
            sections: vec![numbered_lines],
            ..Definitions::default()
        },
    );

    let cases: Vec<(&str, Document, NotWindowable)> = vec![
        ("two sections", multi_section, NotWindowable::MultipleSections),
        ("two columns", multi_column, NotWindowable::MultipleColumns),
        ("line numbering", line_numbered, NotWindowable::LineNumbering),
    ];
    for (name, doc, expected) in cases {
        let refusal = measure_document(&doc, &shaper).err();
        assert_eq!(
            refusal,
            Some(expected),
            "{name}: expected the windowed path to refuse with {expected:?}"
        );
        assert!(
            !expected.reason().is_empty(),
            "{name}: a refusal must be able to say what it found"
        );
        // A refused document still lays out; refusing is a fallback, not a
        // failure.
        assert!(
            !paginate_document(&doc, &shaper).pages.is_empty(),
            "{name}: the full driver must still be able to lay this out"
        );
    }
}

#[test]
fn the_window_policy_widens_by_a_screenful_and_clamps() {
    let policy = WindowPolicy {
        budget_bytes: DEFAULT_PAINT_BUDGET_BYTES,
        lead_pages: 2,
    };
    assert_eq!(
        policy.window_for(PageRange::new(10, 12), 100),
        PageRange::new(8, 14)
    );
    assert_eq!(
        policy.window_for(PageRange::new(0, 2), 100),
        PageRange::new(0, 4)
    );
    assert_eq!(
        policy.window_for(PageRange::new(98, 100), 100),
        PageRange::new(96, 100)
    );
}

/// The budget is in bytes and it is *counted*, not assumed.
#[test]
fn a_tight_byte_budget_still_builds_every_visible_page() {
    let shaper = ParleyShaper::new();
    let doc = plain(200);
    let measures = measures_of(&doc, &shaper);
    let full = paginate_document(&doc, &shaper);
    let one_page = page_paint_bytes(&full.pages[0]);
    assert!(one_page > 0, "a page of prose must cost something to paint");

    let visible = PageRange::new(10, 12);
    // A budget below even one page: the lead pages must go, the visible ones
    // must not.
    let starved = window_of(
        &doc,
        &shaper,
        &measures,
        visible,
        WindowPolicy {
            budget_bytes: 1,
            lead_pages: 4,
        },
    );
    assert!(
        starved.viewport.pages.len() < 10,
        "a starved window must drop its lead pages"
    );
    let built: Vec<u32> = starved
        .viewport
        .pages
        .iter()
        .map(|p| p.page.number)
        .collect();
    assert!(
        built.contains(&full.pages[10].number),
        "the visible page must be built whatever the budget says (built {built:?})"
    );

    let generous = window_of(
        &doc,
        &shaper,
        &measures,
        visible,
        WindowPolicy {
            budget_bytes: DEFAULT_PAINT_BUDGET_BYTES,
            lead_pages: 4,
        },
    );
    assert!(
        generous.viewport.pages.len() > starved.viewport.pages.len(),
        "a generous budget must build more than a starved one"
    );
    assert_eq!(
        generous.bytes,
        generous
            .viewport
            .pages
            .iter()
            .map(|p| page_paint_bytes(&p.page))
            .sum::<usize>(),
        "the reported byte cost must be the cost of what was built"
    );
}

/// Scrolling a long document must not shape the whole thing per window.
#[test]
fn a_window_in_a_resumable_document_shapes_only_what_it_needs() {
    let shaper = ParleyShaper::new();
    let doc = plain(3_000);
    let measures = measures_of(&doc, &shaper);
    assert_eq!(measures.resume(), FlowResume::AnyBlock);
    let total = measures.page_count();
    let window = window_of(
        &doc,
        &shaper,
        &measures,
        PageRange::new(total - 2, total),
        WindowPolicy::default(),
    );
    assert!(
        window.shaped_fragments < doc.body().len() / 4,
        "a window at the end of a {}-block document re-shaped {} fragments; \
         resuming from a checkpoint must cost far less than the document",
        doc.body().len(),
        window.shaped_fragments
    );
}

#[test]
fn the_measure_tier_costs_orders_of_magnitude_less_than_the_paint_tier() {
    let shaper = ParleyShaper::new();
    let doc = plain(400);
    let measures = measures_of(&doc, &shaper);
    let full = paginate_document(&doc, &shaper);
    let paint: usize = full.pages.iter().map(page_paint_bytes).sum();
    let measure = measures.resident_bytes();
    assert!(
        measure * 10 < paint,
        "the measure tier ({measure} B) must be far cheaper than the paint tier \
         ({paint} B) or windowing buys nothing"
    );
}

#[test]
fn the_page_geometry_a_window_uses_is_the_document_geometry() {
    let shaper = ParleyShaper::new();
    let doc = plain(20);
    let measures = measures_of(&doc, &shaper);
    assert_eq!(
        measures.config().page_size,
        document_page_config(&doc).page_size
    );
    assert!(measures.config().content_area().size.width > Twip::ZERO);
}
