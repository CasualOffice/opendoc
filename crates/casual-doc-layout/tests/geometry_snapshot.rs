//! Phase H1 of the oracle visual-fidelity harness (docs/94): a self-referential
//! geometry-snapshot regression gate.
//!
//! It paginates a set of small, single-concern fixtures through the real driver
//! and dumps the page/section/content geometry and every placed block's rect
//! (rounded to points, matching the design's ±1pt tolerance) to a stable text
//! form, then compares that to a committed golden. A layout change that moves,
//! resizes, or re-paginates content — the class of bug the rendering-fidelity
//! audit found — fails the build with a readable diff showing exactly what moved.
//!
//! This is the oracle-free tier: it locks *our own* current geometry. The
//! LibreOffice oracle (docs/94 H2) diffs the same geometry against an
//! independent reference and is added later.
//!
//! Re-bless after an intended change: `REBLESS_GEOMETRY=1 cargo test -p
//! casual-doc-layout --test geometry_snapshot` rewrites the golden; review the
//! diff like any baseline change.

use std::fmt::Write as _;
use std::path::PathBuf;

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, PageMargins, PageSize, PageVerticalAlignment,
    Paragraph, ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
    Spacing,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

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
        inlines: vec![run(id + 1, text)],
    })
}

/// A US-Letter, 1-inch-margin single section with the given id and vertical
/// alignment (all other fields default).
fn section(id: u64, valign: Option<PageVerticalAlignment>) -> SectionBoundary {
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
        vertical_alignment: valign,
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
    }
}

fn document(body: Vec<BlockNode>, section: SectionBoundary) -> Document {
    Document::new(
        node(1),
        body,
        Definitions {
            sections: vec![section],
            ..Definitions::default()
        },
    )
    .unwrap()
}

fn spacing(before: i32, after: i32) -> Spacing {
    Spacing {
        before_twips: Some(before),
        after_twips: Some(after),
        ..Spacing::default()
    }
}

/// Twips → a stable point string (the design's snapshot precision).
fn pt(value: Twip) -> String {
    format!("{:.2}", value.raw() as f64 / 20.0)
}

/// The single-concern fixtures. Each name maps to a document exercising one
/// behavior whose geometry a regression would perturb.
fn fixtures() -> Vec<(&'static str, Document)> {
    // 1. contextualSpacing: three same-(default)-style paragraphs each with
    //    240-twip before/after AND the flag. The inter-paragraph gaps collapse,
    //    so paragraphs 2 and 3 stack tightly against their predecessor.
    let contextual = {
        let props = || ParagraphProperties {
            spacing: Some(spacing(240, 240)),
            contextual_spacing: true,
            ..ParagraphProperties::default()
        };
        document(
            vec![
                paragraph(100, props(), "First"),
                paragraph(110, props(), "Second"),
                paragraph(120, props(), "Third"),
            ],
            section(9, None),
        )
    };

    // 2. The same three paragraphs WITHOUT the flag: the gaps stay, a direct
    //    contrast that makes the collapse visible in the golden.
    let spaced = document(
        vec![
            paragraph(
                200,
                ParagraphProperties {
                    spacing: Some(spacing(240, 240)),
                    ..ParagraphProperties::default()
                },
                "First",
            ),
            paragraph(
                210,
                ParagraphProperties {
                    spacing: Some(spacing(240, 240)),
                    ..ParagraphProperties::default()
                },
                "Second",
            ),
        ],
        section(9, None),
    );

    // 3. Section vAlign=Bottom: a single short paragraph is pushed so its bottom
    //    meets the content-area bottom.
    let valign_bottom = document(
        vec![paragraph(
            300,
            ParagraphProperties::default(),
            "Bottom aligned",
        )],
        section(9, Some(PageVerticalAlignment::Bottom)),
    );

    // 4. Section vAlign=Center: the same paragraph lands halfway down.
    let valign_center = document(
        vec![paragraph(400, ParagraphProperties::default(), "Centered")],
        section(9, Some(PageVerticalAlignment::Center)),
    );

    // 5. Pagination: three forced page breaks → three pages, each carrying its
    //    one paragraph at the content-area top.
    let paginated = document(
        vec![
            paragraph(500, ParagraphProperties::default(), "Page one"),
            paragraph(
                510,
                ParagraphProperties {
                    page_break_before: true,
                    ..ParagraphProperties::default()
                },
                "Page two",
            ),
            paragraph(
                520,
                ParagraphProperties {
                    page_break_before: true,
                    ..ParagraphProperties::default()
                },
                "Page three",
            ),
        ],
        section(9, None),
    );

    vec![
        ("contextual-spacing", contextual),
        ("spaced-no-contextual", spaced),
        ("valign-bottom", valign_bottom),
        ("valign-center", valign_center),
        ("paginated", paginated),
    ]
}

fn dump_layout(name: &str, layout: &PaginatedLayout, out: &mut String) {
    writeln!(out, "== fixture: {name} ==").unwrap();
    for (page_index, page) in layout.pages.iter().enumerate() {
        writeln!(
            out,
            "  page {} size=({}x{}) content=({},{} {}x{})",
            page_index + 1,
            pt(page.page_size.width),
            pt(page.page_size.height),
            pt(page.content_area.origin.x),
            pt(page.content_area.origin.y),
            pt(page.content_area.size.width),
            pt(page.content_area.size.height),
        )
        .unwrap();
        for placed in &page.placed {
            let kind = match &placed.fragment {
                BlockFragment::Paragraph { .. } => "para",
                BlockFragment::TableRow { .. } => "row",
            };
            writeln!(
                out,
                "    {kind} rect=({},{} {}x{})",
                pt(placed.rect.origin.x),
                pt(placed.rect.origin.y),
                pt(placed.rect.size.width),
                pt(placed.rect.size.height),
            )
            .unwrap();
        }
    }
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/geometry_snapshot.golden")
}

// The golden encodes shaped line metrics (line heights, glyph advances), which
// are byte-identical on the Linux and macOS CI runners but differ on Windows,
// whose text stack rasterizes/shapes the bundled faces slightly differently.
// The repo's other visual checks (e.g. `casual-doc-render`'s containment test)
// use tolerant bounds for the same reason; this exact-value snapshot is pinned
// to the deterministic platforms and skipped on Windows.
#[cfg_attr(
    target_os = "windows",
    ignore = "shaped line metrics differ on Windows; golden is blessed on Linux/macOS"
)]
#[test]
fn geometry_of_fixtures_matches_the_golden_snapshot() {
    let shaper = ParleyShaper::new();
    let mut actual = String::new();
    for (name, doc) in fixtures() {
        let layout = paginate_document(&doc, &shaper);
        dump_layout(name, &layout, &mut actual);
    }

    let path = golden_path();
    if std::env::var_os("REBLESS_GEOMETRY").is_some() {
        std::fs::write(&path, &actual).expect("write geometry golden");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing geometry golden {}; create it with REBLESS_GEOMETRY=1",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "geometry drifted from the golden (docs/94 H1). If intended, re-bless with \
         REBLESS_GEOMETRY=1 and review the diff."
    );
}
