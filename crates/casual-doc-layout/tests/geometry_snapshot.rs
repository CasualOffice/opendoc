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
use casual_doc_layout::page::{PaginatedLayout, PlacedFragment};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, BlockNode, Definitions, Document, Indentation,
    InlineNode, LevelSuffix, NumberFormat, NumberingInstance, NumberingInstanceId, NumberingLevel,
    NumberingRef, PageMargins, PageSize, PageVerticalAlignment, Paragraph, ParagraphProperties,
    Run, RunProperties, SectionBoundary, SectionColumns, SectionId, Spacing, TabAlignment, TabStop,
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
        // 6. A hanging decimal list with a level tab stop: locks the marker span,
        //    the number-suffix-tab advance, and the body start column.
        (
            "decimal-list-hanging",
            numbered_list("%1.", NumberFormat::Decimal),
        ),
        // 7. A bullet list: the marker is the level glyph, same geometry surface.
        (
            "bullet-list",
            numbered_list("\u{2022}", NumberFormat::Bullet),
        ),
        // 8. A three-level nested list: each level indents further and carries its
        //    own marker, so the golden locks per-level indentation and the nested
        //    marker/body columns as items descend and ascend the levels.
        ("multilevel-list", multilevel_list()),
    ]
}

/// A three-level list (`1.` / `a.` / `i.`), each level at a deeper hanging indent
/// (720/1440/2160 twips), with items walking L0 → L1 → L1 → L2 → L0 so the golden
/// captures per-level indentation, nested markers, and the counter reset on the
/// return to L0.
fn multilevel_list() -> Document {
    let abs_id = AbstractNumberingId::new(node(920));
    let inst_id = NumberingInstanceId::new(node(921));
    let level = |index: u8, num_fmt: NumberFormat, text: &str| {
        let indent = 720 * i32::from(index + 1);
        NumberingLevel {
            level: index,
            start: 1,
            num_fmt: Some(num_fmt),
            lvl_text: Some(text.to_owned()),
            lvl_jc: None,
            suff: Some(LevelSuffix::Tab),
            is_lgl: false,
            paragraph_properties: Some(ParagraphProperties {
                indentation: Some(Indentation {
                    start_twips: Some(indent),
                    hanging_twips: Some(360),
                    ..Indentation::default()
                }),
                tabs: vec![TabStop {
                    position_twips: indent,
                    alignment: TabAlignment::Start,
                    leader: None,
                }],
                ..ParagraphProperties::default()
            }),
            run_properties: None,
            style_ref: None,
            lvl_restart: None,
        }
    };
    let mut definitions = Definitions::default();
    definitions.abstract_numbering.insert(
        abs_id,
        AbstractNumbering {
            levels: vec![
                level(0, NumberFormat::Decimal, "%1."),
                level(1, NumberFormat::LowerLetter, "%2."),
                level(2, NumberFormat::LowerRoman, "%3."),
            ],
            multi_level_type: None,
        },
    );
    definitions.numbering.insert(
        inst_id,
        NumberingInstance {
            abstract_ref: abs_id,
            overrides: Vec::new(),
        },
    );
    definitions.sections = vec![section(9, None)];
    let item = |id: u64, level: u8, text: &str| {
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties {
                numbering: Some(NumberingRef {
                    instance: inst_id,
                    level,
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run(id + 1, text)],
        })
    };
    Document::new(
        node(1),
        vec![
            item(700, 0, "One"),
            item(710, 1, "Nested a"),
            item(720, 1, "Nested b"),
            item(730, 2, "Deep i"),
            item(740, 0, "Two"),
        ],
        definitions,
    )
    .unwrap()
}

/// A two-item single-level list at a 720-twip hanging indent (marker protrudes
/// into the hanging space) with a level tab stop at 720 twips — the placement
/// surface for the marker and the number-suffix tab.
fn numbered_list(lvl_text: &str, num_fmt: NumberFormat) -> Document {
    let abs_id = AbstractNumberingId::new(node(900));
    let inst_id = NumberingInstanceId::new(node(901));
    let level_props = ParagraphProperties {
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
    };
    let mut definitions = Definitions::default();
    definitions.abstract_numbering.insert(
        abs_id,
        AbstractNumbering {
            levels: vec![NumberingLevel {
                level: 0,
                start: 1,
                num_fmt: Some(num_fmt),
                lvl_text: Some(lvl_text.to_owned()),
                lvl_jc: None,
                suff: Some(LevelSuffix::Tab),
                is_lgl: false,
                paragraph_properties: Some(level_props),
                run_properties: None,
                style_ref: None,
                lvl_restart: None,
            }],
            multi_level_type: None,
        },
    );
    definitions.numbering.insert(
        inst_id,
        NumberingInstance {
            abstract_ref: abs_id,
            overrides: Vec::new(),
        },
    );
    definitions.sections = vec![section(9, None)];
    let item = |id: u64, text: &str| {
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties {
                numbering: Some(NumberingRef {
                    instance: inst_id,
                    level: 0,
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run(id + 1, text)],
        })
    };
    Document::new(
        node(1),
        vec![item(600, "First"), item(610, "Second")],
        definitions,
    )
    .unwrap()
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
            // A list marker and the body's start column: the intra-line x that the
            // number-suffix-tab / hanging-indent logic places. Absolute page-local
            // x = placed.rect.origin.x + run.origin.x (the paint anchor).
            if let Some((marker_left, marker_right, body_left)) = marker_and_body_x(placed) {
                writeln!(
                    out,
                    "      marker=({}..{}) body={}",
                    pt(marker_left),
                    pt(marker_right),
                    pt(body_left),
                )
                .unwrap();
            }
        }
    }
}

/// The first line's marker span and body start column (absolute page-local x),
/// or `None` for a paragraph with no list marker. Locks the number-suffix-tab
/// and hanging-indent placement the geometry rect alone can't see.
fn marker_and_body_x(placed: &PlacedFragment) -> Option<(Twip, Twip, Twip)> {
    let BlockFragment::Paragraph {
        lines, box_metrics, ..
    } = &placed.fragment
    else {
        return None;
    };
    let line = lines.lines.first()?;
    // Line content is positioned relative to the content origin, which is the
    // fragment's left edge plus the paragraph start indent (matches the painter:
    // content_x = placed.rect.origin.x + box_metrics.indent_start).
    let origin_x = placed.rect.origin.x + box_metrics.indent_start;
    let marker = line.runs.iter().find(|run| run.is_marker)?;
    let marker_left = origin_x + marker.origin.x;
    let marker_right = marker
        .glyphs
        .iter()
        .fold(marker_left, |x, glyph| x + glyph.advance);
    let body_left = line
        .runs
        .iter()
        .find(|run| !run.is_marker)
        .map_or(marker_right, |run| origin_x + run.origin.x);
    Some((marker_left, marker_right, body_left))
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
