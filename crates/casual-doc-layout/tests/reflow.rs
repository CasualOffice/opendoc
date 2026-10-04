//! Reflow (pageless) layout — ADR-046, `docs/151`.
//!
//! Reflow lays the body out at a width the *caller* chooses and cuts the result
//! into tiles, instead of laying it out on the document's paper. A phone cannot
//! show a 6.5in text column at 390px and stay readable, and the alternatives are
//! a 31% zoom (refused) or a sideways pan (today's defect), so this is the third
//! answer and the only one that keeps the text legible.
//!
//! Every guard here was driven **red** by mutating the driver before it was
//! trusted; the mutation for each is named on the test. Four properties matter,
//! and the ones that matter most are asserted at the **paint tier** — over the
//! [`DisplayList`](casual_doc_layout::display::DisplayList) a renderer actually
//! executes — because a model-level assertion has passed through every real
//! defect in this engine:
//!
//! 1. **Inertness.** `LayoutView::Paged` is the default and produces byte-for-byte
//!    what the driver produced before the parameter existed. This one is only half
//!    a guard on its own and the test says so: the other half is
//!    `geometry_snapshot.golden`, a committed artifact that cannot move with the
//!    code.
//! 2. **The width is honoured.** Nothing is painted past the requested column.
//! 3. **A tile is exactly as tall as its content.** Asserted the strong way: the
//!    painted column is *identical* whichever tile height the caller picks, which
//!    can only hold if no tile carries slack below its content.
//! 4. **The document stays editable, and is never edited.** Every model position
//!    stays reachable through hit-testing, and the document's own bytes and
//!    section geometry are untouched.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{DisplayList, PaintItem};
use casual_doc_layout::document_layout::{
    DEFAULT_TILE_HEIGHT, LayoutView, ReflowRefused, paginate_document,
    paginate_document_after_edit_in, paginate_document_in, paginate_document_view,
};
use casual_doc_layout::flow::ReviewView;
use casual_doc_layout::hittest::LayoutSnapshot;
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::model::ModelPos;
use casual_doc_layout::page::{Page, PaginatedLayout};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, DefinitionMap, Definitions, Document, GridColumn, HeaderFooter, HeaderFooterId,
    HeaderFooterKind, HeaderFooterRef, InlineNode, PageBorderDisplay, PageBorderOffset,
    PageBorders, PageMargins, PageSize, PageVerticalAlignment, Paragraph, ParagraphProperties, Run,
    RunProperties, SectionBoundary, SectionColumns, SectionId, SectionType, Table, TableCell,
    TableCellProperties, TableProperties, TableRow, TableRowProperties,
};
// Separate `use` lines (the repo's anti-conflict convention for new v1 imports).
use casual_doc_model::v1::BorderEdge;
use casual_doc_model::v1::Drawing;
use casual_doc_model::v1::Extent;
use casual_doc_model::v1::Field;
use casual_doc_model::v1::FieldKind;
use casual_doc_model::v1::LineNumbering;
use casual_doc_model::v1::MediaId;
use casual_doc_model::v1::MediaReference;
use casual_doc_model::v1::RgbColor;
use casual_doc_model::v1::Rgba;
use casual_doc_model::v1::TableWidth;
use casual_doc_model::v1::Watermark;
use casual_doc_model::v1::WatermarkContent;
use casual_doc_model::v1::WatermarkLayout;
use casual_doc_model::v1::WatermarkText;

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------

/// The reflow geometry every guard here uses unless it says otherwise: a 3.75in
/// column, a deliberately SHORT 2in tile (so a short fixture still produces
/// several cuts to assert about), and a 0.25in gutter.
const COLUMN: Twip = Twip(5_400);
const TILE: Twip = Twip(2_880);
const GUTTER: Twip = Twip(360);

/// How far a shaped run's descent may reach below the line box that holds it —
/// **one twip**, measured, and stated rather than tuned.
///
/// A tile is trimmed to its *layout* extent (the lowest placed rect), and a line
/// box's height is a rounded sum of the faces' metrics, so the face's own descent
/// can end up a rounding unit below it. One twip is 1/1440 inch: 0.067 CSS px, or
/// a quarter of a device pixel on a 4x phone. It is the measured maximum across
/// every fixture in this file, not a value picked to make an assertion pass — the
/// mutation this guard exists for (dropping the trim) misses by 270 twips — more
/// than two orders of magnitude clear of it.
const DESCENT_ROUNDING: Twip = Twip(1);

fn reflow() -> LayoutView {
    LayoutView::reflow(COLUMN, TILE, GUTTER).expect("a 3.75in column in a 2in tile is a valid view")
}

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

fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default().into(),
        inlines,
    })
}

fn paragraph_with(id: u64, properties: ParagraphProperties, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines: vec![run(id + 1_000_000, text)],
    })
}

const LINE: &str = "The quick brown fox jumps over the lazy dog and keeps on running. ";

/// A US-Letter section with 1in margins — the paper a phone cannot show.
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

/// A grey semi-transparent "DRAFT" stamp — page furniture reflow suppresses.
fn draft_watermark() -> Watermark {
    Watermark {
        content: WatermarkContent::Text(WatermarkText {
            text: "DRAFT".to_owned(),
            font: None,
            size_half_points: None,
            color: Rgba {
                r: 192,
                g: 192,
                b: 192,
                a: 255,
            },
            bold: false,
            italic: false,
        }),
        layout: WatermarkLayout::Diagonal,
        semi_transparent: true,
    }
}

fn document(body: Vec<BlockNode>, definitions: Definitions) -> Document {
    Document::new(node(9_000_000), body, definitions).expect("a well-formed test document")
}

/// `paragraphs` wrapping prose paragraphs on one Letter section — the plain
/// reading document reflow exists for.
fn prose(paragraphs: usize) -> Document {
    document(
        (0..paragraphs)
            .map(|i| {
                paragraph(
                    i as u64 + 1_000_000,
                    vec![run(i as u64 + 2_000_000, &LINE.repeat(3))],
                )
            })
            .collect(),
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    )
}

// --------------------------------------------------------------------------
// The paint tier: what a renderer actually executes
// --------------------------------------------------------------------------

/// The bounding box of everything one [`PaintItem`] paints, in page-local twips,
/// or `None` for an item that paints nothing of its own (a clip/layer bracket).
///
/// A glyph run's box is its **inked** pen extent — the origin plus the advances up
/// to and including the last non-whitespace glyph — spanning the baseline ± the
/// face's ascent/descent. That is the same "text region" the oracle geometry gate
/// compares, rather than the line box (a layout decision) and rather than the raw
/// pen extent, because a line's TRAILING WHITESPACE is allowed to hang past the
/// measure: every line broken at a space carries that space's advance, and
/// counting it would report a 43-twip overhang on a perfectly justified line.
///
/// Complexity: `O(glyphs in the item)`.
fn painted_bounds(item: &PaintItem) -> Option<(Twip, Twip, Twip, Twip)> {
    let of_rect = |rect: &casual_doc_layout::units::Rect| {
        Some((rect.origin.x, rect.origin.y, rect.right(), rect.bottom()))
    };
    match item {
        PaintItem::Glyphs { run } => {
            let inked = run
                .glyphs
                .iter()
                .rposition(|glyph| !glyph.is_whitespace)
                .map_or(0, |last| {
                    run.glyphs[..=last]
                        .iter()
                        .map(|glyph| glyph.advance.raw())
                        .sum()
                });
            Some((
                run.origin.x,
                run.origin.y - run.ascent,
                run.origin.x + Twip(inked),
                run.origin.y + run.descent,
            ))
        }
        PaintItem::Rect { rect, .. }
        | PaintItem::Ellipse { rect, .. }
        | PaintItem::RoundedRect { rect, .. }
        | PaintItem::Image { rect, .. } => of_rect(rect),
        PaintItem::Line { from, to, .. } => Some((
            from.x.min(to.x),
            from.y.min(to.y),
            from.x.max(to.x),
            from.y.max(to.y),
        )),
        PaintItem::Polygon { points, .. } => {
            let mut bounds: Option<(Twip, Twip, Twip, Twip)> = None;
            for point in points {
                bounds = Some(match bounds {
                    None => (point.x, point.y, point.x, point.y),
                    Some((l, t, r, b)) => (
                        l.min(point.x),
                        t.min(point.y),
                        r.max(point.x),
                        b.max(point.y),
                    ),
                });
            }
            bounds
        }
        // A `Shape`'s geometry is a private enum from here; every fixture in this
        // file paints its floats as images or glyphs, and a shape that appeared
        // would be reported by `nothing_is_painted_outside_a_trimmed_tile`'s
        // companion count rather than silently skipped.
        _ => None,
    }
}

/// The union of every painted box on `page`, as `(left, top, right, bottom)`.
fn painted_extent(page: &Page) -> Option<(Twip, Twip, Twip, Twip)> {
    let list: DisplayList = compose_page(page);
    let mut union: Option<(Twip, Twip, Twip, Twip)> = None;
    for item in &list.items {
        let Some((l, t, r, b)) = painted_bounds(item) else {
            continue;
        };
        union = Some(match union {
            None => (l, t, r, b),
            Some((ul, ut, ur, ub)) => (ul.min(l), ut.min(t), ur.max(r), ub.max(b)),
        });
    }
    union
}

/// Every painted glyph's absolute position in the **continuous column** the tiles
/// make when they are stacked edge to edge, with its advance so a moved line break
/// shows up as well as a moved line.
///
/// This is the function the tile-trim guard turns on: stacking trimmed tiles must
/// reproduce one column, so this sequence must not depend on where the cuts fell.
fn stacked_glyph_column(layout: &PaginatedLayout) -> Vec<(i32, i32, i32, u32)> {
    let mut out = Vec::new();
    let mut offset = 0i32;
    for page in &layout.pages {
        for item in &compose_page(page).items {
            if let PaintItem::Glyphs { run } = item {
                let advance: i32 = run.glyphs.iter().map(|glyph| glyph.advance.raw()).sum();
                out.push((
                    run.origin.x.raw(),
                    run.origin.y.raw() + offset,
                    advance,
                    run.glyphs.len() as u32,
                ));
            }
        }
        offset += page.page_size.height.raw();
    }
    out
}

// --------------------------------------------------------------------------
// 1. Inertness — the parameter is invisible when it is not used
// --------------------------------------------------------------------------

/// Every fixture this file can build, so inertness is asserted over the shapes
/// that have a page-geometry opinion rather than over prose alone.
fn inertness_corpus() -> Vec<(&'static str, Document)> {
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        HeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "The running header")])],
        },
    );
    let mut footers = DefinitionMap::default();
    footers.insert(
        HeaderFooterId::new(node(400)),
        HeaderFooter {
            blocks: vec![paragraph(410, vec![run(411, "The running footer")])],
        },
    );

    let mut banded = letter_section(7_000_001);
    banded.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(300)),
    }];
    banded.footers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(400)),
    }];

    let mut two_columns = letter_section(7_000_001);
    two_columns.columns.count = 2;

    let mut bordered = letter_section(7_000_001);
    bordered.page_borders = PageBorders {
        top: Some(BorderEdge {
            style: "single".to_owned(),
            size_eighth_points: Some(8),
            color: Some(RgbColor { r: 0, g: 0, b: 0 }),
            space_points: None,
            theme_color: None,
        }),
        bottom: None,
        start: None,
        end: None,
        display: Some(PageBorderDisplay::AllPages),
        offset_from: Some(PageBorderOffset::Page),
    };

    let mut numbered = letter_section(7_000_001);
    numbered.line_numbering = LineNumbering {
        count_by: Some(1),
        start: Some(1),
        distance: None,
        restart: None,
    };

    let mut stamped = letter_section(7_000_001);
    stamped.watermark = Some(draft_watermark());

    let mut centred = letter_section(7_000_001);
    centred.vertical_alignment = Some(PageVerticalAlignment::Center);

    let mut first = letter_section(7_000_001);
    let mut second = letter_section(7_000_002);
    second.section_type = Some(SectionType::NextPage);
    first.section_type = Some(SectionType::NextPage);

    vec![
        ("prose", prose(12)),
        (
            "header and footer",
            document(
                vec![
                    paragraph(100, vec![run(101, &LINE.repeat(4))]),
                    paragraph(110, vec![run(111, &LINE.repeat(4))]),
                ],
                Definitions {
                    sections: vec![banded],
                    headers,
                    footers,
                    ..Definitions::default()
                },
            ),
        ),
        (
            "two newspaper columns",
            document(
                (0..8)
                    .map(|i| {
                        paragraph(
                            i as u64 + 1_000_000,
                            vec![run(i as u64 + 2_000_000, &LINE.repeat(3))],
                        )
                    })
                    .collect(),
                Definitions {
                    sections: vec![two_columns],
                    ..Definitions::default()
                },
            ),
        ),
        (
            "a page border",
            document(
                vec![paragraph(100, vec![run(101, &LINE.repeat(4))])],
                Definitions {
                    sections: vec![bordered],
                    ..Definitions::default()
                },
            ),
        ),
        (
            "margin line numbers",
            document(
                vec![paragraph(100, vec![run(101, &LINE.repeat(4))])],
                Definitions {
                    sections: vec![numbered],
                    ..Definitions::default()
                },
            ),
        ),
        (
            "a watermark",
            document(
                vec![paragraph(100, vec![run(101, &LINE.repeat(4))])],
                Definitions {
                    sections: vec![stamped],
                    ..Definitions::default()
                },
            ),
        ),
        (
            "a vertically centred section",
            document(
                vec![paragraph(100, vec![run(101, "One short line")])],
                Definitions {
                    sections: vec![centred],
                    ..Definitions::default()
                },
            ),
        ),
        (
            "two sections",
            document(
                vec![
                    paragraph_with(
                        100,
                        ParagraphProperties {
                            section_break: Some(SectionId::new(node(7_000_001))),
                            ..ParagraphProperties::default()
                        },
                        &LINE.repeat(3),
                    ),
                    paragraph(200, vec![run(201, &LINE.repeat(3))]),
                ],
                Definitions {
                    sections: vec![first, second],
                    ..Definitions::default()
                },
            ),
        ),
        ("a forced page break", page_break_document()),
        ("a table in the flow", table_document()),
        ("a NUMPAGES field", field_document()),
    ]
}

/// Asserts that naming `LayoutView::Paged` changes nothing, over every fixture.
///
/// **What this can and cannot catch, stated rather than assumed.** Both sides of
/// the comparison run the same driver, so a mutation that made the driver reflow
/// *unconditionally* would move both and leave this green. That half of inertness
/// belongs to `tests/geometry_snapshot.golden`, which is a committed artifact and
/// therefore cannot move with the code — and it is the one that fires:
///
/// MUTATION PROOF: making `reflow_page_config` ignore its `view` and always
/// synthesise a tile leaves this test green and turns
/// `geometry_of_fixtures_matches_the_golden_snapshot` red, with the first fixture
/// going from `page 1 size=(612.00x792.00) content=(72.00,72.00 468.00x648.00)` to
/// `page 1 size=(306.00x792.00) content=(18.00,0.00 270.00x792.00)`. The two
/// guards are a pair; neither is sufficient alone, and that is why the golden is
/// named here.
///
/// What THIS one catches is the other shape: an entry point that forgets to
/// delegate with `Paged`, or a `Paged` arm that diverges from the unnamed path.
#[test]
fn paged_is_what_the_driver_produced_before_the_view_parameter_existed() {
    let shaper = ParleyShaper::new();
    for (name, doc) in inertness_corpus() {
        let before = paginate_document(&doc, &shaper);
        let after = paginate_document_in(&doc, &shaper, ReviewView::Editing, LayoutView::Paged);
        assert_eq!(
            before.pages.len(),
            after.pages.len(),
            "{name}: naming Paged changed the page count"
        );
        for (index, (a, b)) in before.pages.iter().zip(after.pages.iter()).enumerate() {
            assert_eq!(a, b, "{name}: naming Paged changed page {index}");
        }
        // The default is Paged, so the markup view has to be inert too.
        assert_eq!(
            paginate_document_view(&doc, &shaper, ReviewView::Markup),
            paginate_document_in(&doc, &shaper, ReviewView::Markup, LayoutView::Paged),
            "{name}: naming Paged changed the markup layout"
        );
    }
}

// --------------------------------------------------------------------------
// 2. The width is honoured — asserted at the paint tier
// --------------------------------------------------------------------------

/// Nothing a reflow layout paints reaches past the column the caller asked for,
/// and the tile is exactly the column plus its two gutters wide.
///
/// This is the guard against the whole design failing silently: if the driver kept
/// deriving the width from the section, the document would lay out at 9,360 twips
/// and a phone would pan again — with every model-level assertion still green.
///
/// MUTATION PROOF, twice, because the width has two halves:
///
/// - making `reflow_page_config` return `None` (so the section's own geometry
///   stands) fails the tile-box assertion with `left: Twip(12240), right:
///   Twip(6120)`;
/// - keeping the tile but flowing the galley at
///   `section_page_config(boundary).content_area().size.width` — the driver's
///   behaviour before this work — fails the paint-tier assertion with `tile 1
///   paints out to 9652 past its own 6120 width`.
#[test]
fn nothing_is_painted_past_the_requested_reflow_column() {
    let shaper = ParleyShaper::new();
    let doc = prose(14);
    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());

    // The per-tile checks come BEFORE the "more than one tile" precondition, so
    // that a driver which ignored the view fails on the width it got wrong rather
    // than on a page count that is merely a symptom of it.
    for page in &layout.pages {
        assert_eq!(
            page.page_size.width,
            COLUMN + GUTTER + GUTTER,
            "tile {} is the column plus both gutters wide",
            page.number
        );
        let (left, _, right, _) =
            painted_extent(page).expect("every tile of a prose body paints something");
        assert!(
            right <= page.page_size.width,
            "tile {} paints out to {} past its own {} width",
            page.number,
            right.raw(),
            page.page_size.width.raw()
        );
        assert!(
            right <= GUTTER + COLUMN,
            "tile {} paints out to {}, past the {} twip column it was asked for",
            page.number,
            right.raw(),
            (GUTTER + COLUMN).raw()
        );
        assert!(
            left >= Twip::ZERO,
            "tile {} paints at {} — off the left of the raster",
            page.number,
            left.raw()
        );
    }
    assert!(
        layout.pages.len() > 1,
        "a 2in tile cuts a 14-paragraph body into more than one tile"
    );
}

// --------------------------------------------------------------------------
// 3. A tile is exactly as tall as its content — asserted at the paint tier
// --------------------------------------------------------------------------

/// The painted column does not depend on where the tile cuts fall.
///
/// This is the strong form of "no reflow tile is taller than its content", and it
/// is the guard worth having. A tile that keeps the slack the paginator left below
/// its last chunk pushes everything on the following tile down by that slack; cut
/// the same document at a different tile height and the slack lands in different
/// places, so the stacked column moves. Only a trim to the exact content extent
/// makes the painted result cut-independent — which is also precisely what the
/// reader means by "a continuous column".
///
/// MUTATION PROOF: deleting the `trim_reflow_tiles(layout)` call at the end of
/// `post_pagination_passes` fails this at the first glyph after the first cut, and
/// the recorded failure was:
///
/// ```text
/// glyph run 9 moved between a 2880-twip tiling and a 4320-twip tiling:
///   (360, 3111, 5104, 52) vs (360, 2841, 5104, 52)
/// ```
///
/// — a 270-twip shift, which is the line the 2in tile could not fit.
#[test]
fn the_painted_column_is_the_same_wherever_the_tile_cuts_fall() {
    let shaper = ParleyShaper::new();
    let doc = prose(16);

    let short = LayoutView::reflow(COLUMN, Twip(2_880), GUTTER).expect("a 2in tile");
    let tall = LayoutView::reflow(COLUMN, Twip(4_320), GUTTER).expect("a 3in tile");
    let taller = LayoutView::reflow(COLUMN, DEFAULT_TILE_HEIGHT, GUTTER).expect("an 11in tile");

    let a = paginate_document_in(&doc, &shaper, ReviewView::Editing, short);
    let b = paginate_document_in(&doc, &shaper, ReviewView::Editing, tall);
    let c = paginate_document_in(&doc, &shaper, ReviewView::Editing, taller);
    assert!(
        a.pages.len() > b.pages.len() && b.pages.len() > c.pages.len(),
        "the three tile heights must actually cut the document differently: {} / {} / {}",
        a.pages.len(),
        b.pages.len(),
        c.pages.len()
    );

    let column_a = stacked_glyph_column(&a);
    assert!(!column_a.is_empty(), "the fixture paints glyphs");
    for (name, other) in [("4320-twip", &b), ("15840-twip", &c)] {
        let column = stacked_glyph_column(other);
        assert_eq!(
            column_a.len(),
            column.len(),
            "a {name} tiling painted {} glyph runs against the 2880-twip tiling's {}",
            column.len(),
            column_a.len()
        );
        for (index, (first, second)) in column_a.iter().zip(column.iter()).enumerate() {
            assert_eq!(
                first, second,
                "glyph run {index} moved between a 2880-twip tiling and a {name} tiling: \
                 {first:?} vs {second:?}"
            );
        }
    }
}

/// Every tile's height is its content's extent, and nothing is painted below it.
///
/// The companion to the cut-independence guard above: that one pins the *relative*
/// stacking, this one pins each tile's own box, so a trim that overshot (clipping
/// the last line) could not hide inside a consistent stacking.
///
/// MUTATION PROOF: deleting the `trim_reflow_tiles(layout)` call fails this on
/// tile 1 with `tile 1 is 2880 twips tall but its content ends at 2610`.
#[test]
fn no_reflow_tile_is_taller_than_its_content_and_nothing_falls_off_it() {
    let shaper = ParleyShaper::new();
    let doc = prose(16);
    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());

    for page in &layout.pages {
        let content = page
            .placed
            .iter()
            .chain(page.footnotes.iter())
            .map(|placed| placed.rect.bottom())
            .max()
            .expect("a tile of a prose body places content");
        assert_eq!(
            page.page_size.height,
            content,
            "tile {} is {} twips tall but its content ends at {}",
            page.number,
            page.page_size.height.raw(),
            content.raw()
        );
        assert_eq!(
            page.content_area.size.height,
            content - page.content_area.origin.y,
            "tile {}'s content area was not trimmed with its page box",
            page.number
        );
        let (_, top, _, bottom) = painted_extent(page).expect("a tile paints something");
        assert!(
            bottom <= page.page_size.height + DESCENT_ROUNDING,
            "tile {} paints down to {}, past its own {} height — the trim clipped content",
            page.number,
            bottom.raw(),
            page.page_size.height.raw()
        );
        assert!(
            top >= Twip::ZERO,
            "tile {} paints above its own top edge, at {}",
            page.number,
            top.raw()
        );
    }
}

/// Trimming is idempotent, which is what lets it live in the pass the incremental
/// resume path also runs. Re-running the whole driver over the same document must
/// produce the same tile boxes, not progressively shorter ones.
#[test]
fn trimming_a_trimmed_tile_changes_nothing() {
    let shaper = ParleyShaper::new();
    let doc = prose(10);
    let once = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    let twice = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    assert_eq!(once, twice);

    // And through the incremental entry, which reaches the same pass by another
    // route (`post_pagination_passes` after a resume).
    let mut cache = GalleyCache::new();
    let incremental = paginate_document_after_edit_in(
        &doc,
        &shaper,
        &mut cache,
        &DirtySet::new(),
        ReviewView::Editing,
        Some(once.clone()),
        reflow(),
    )
    .layout;
    assert_eq!(
        once.pages.iter().map(|p| p.page_size).collect::<Vec<_>>(),
        incremental
            .pages
            .iter()
            .map(|p| p.page_size)
            .collect::<Vec<_>>(),
        "an incremental pass re-trimmed tiles that were already trimmed"
    );
}

// --------------------------------------------------------------------------
// 4. Editable, and never edited
// --------------------------------------------------------------------------

/// Every model position the paged layout can place a caret at is still placeable,
/// and still hit-testable back to itself, in reflow.
///
/// This is what stops tiles becoming a read-only view by accident — the place
/// ONLYOFFICE's reader mode deliberately stops (`SelectEnabled = false`) and we
/// deliberately do not.
///
/// MUTATION PROOF: trimming a tile to `content - Twip(200)` (an overshoot by a
/// fraction of a line) fails this, because the last line of a tile is then
/// outside the tile and `hit_test` snaps the caret to the line above it.
#[test]
fn every_model_position_stays_reachable_in_reflow() {
    let shaper = ParleyShaper::new();
    let doc = prose(9);
    let paged = paginate_document(&doc, &shaper);
    let reflowed = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());

    let positions = |layout: &PaginatedLayout| -> Vec<ModelPos> {
        let mut out = Vec::new();
        for page in &layout.pages {
            for placed in &page.placed {
                if let BlockFragment::Paragraph { id, lines, .. } = &placed.fragment {
                    for line in &lines.lines {
                        for glyph_run in &line.runs {
                            for glyph in &glyph_run.glyphs {
                                out.push(ModelPos::new(*id, glyph.cluster));
                            }
                        }
                    }
                }
            }
        }
        out.sort_by_key(|pos| (pos.node, pos.offset));
        out.dedup();
        out
    };

    let paged_positions = positions(&paged);
    assert!(!paged_positions.is_empty(), "the fixture carries text");
    assert_eq!(
        paged_positions,
        positions(&reflowed),
        "reflow lost or invented model positions — the document is no longer the same document"
    );

    let snapshot = LayoutSnapshot::new(&reflowed);
    for pos in &paged_positions {
        let (page, rect) = snapshot
            .caret_rect(*pos)
            .unwrap_or_else(|| panic!("no caret rect in reflow for {pos:?}"));
        let midpoint = Point::new(
            rect.origin.x,
            rect.origin.y + Twip(rect.size.height.raw() / 2),
        );
        let hit = snapshot
            .hit_test(page, midpoint)
            .unwrap_or_else(|| panic!("hit-testing a reflow caret rect for {pos:?} found nothing"));
        assert_eq!(
            hit.pos, *pos,
            "a click on the caret of {pos:?} in reflow landed at {:?}",
            hit.pos
        );
    }
}

/// A reflow pass writes nothing to the document: not its bytes, not its section
/// geometry. ADR-046 §3.1 made into a test rather than a promise — `setPageSetup`
/// would produce a similar picture today by issuing a real, exportable section
/// mutation, and doing it that way would persist a 390px "page" into the user's
/// DOCX.
#[test]
fn a_reflow_pass_does_not_touch_the_document() {
    let shaper = ParleyShaper::new();
    let doc = prose(9);
    let before = serde_json::to_string(&doc).expect("the model serializes");

    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    assert!(!layout.pages.is_empty());

    assert_eq!(
        before,
        serde_json::to_string(&doc).expect("the model serializes"),
        "a reflow pass changed the document"
    );
    let section = &doc.definitions().sections[0];
    assert_eq!(
        (
            section.page_size.width_twips,
            section.page_margins.start_twips
        ),
        (12_240, 1_440),
        "the section still describes US-Letter with 1in margins"
    );
}

// --------------------------------------------------------------------------
// Constraints suspended, furniture suppressed
// --------------------------------------------------------------------------

fn page_break_document() -> Document {
    document(
        (0..4)
            .map(|i| {
                paragraph_with(
                    i as u64 * 10 + 1,
                    ParagraphProperties {
                        page_break_before: Some(true),
                        keep_next: Some(true),
                        keep_lines: Some(true),
                        widow_control: Some(true),
                        ..ParagraphProperties::default()
                    },
                    "One short line",
                )
            })
            .collect(),
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    )
}

/// `w:pageBreakBefore` (and its three companions) stop forcing a break under
/// reflow: four paragraphs that each demand their own page fit in one tile.
///
/// MUTATION PROOF: removing the `suspend_page_break_constraints(&mut galley)`
/// call from `push_section_run` fails this with 4 tiles instead of 1.
#[test]
fn the_page_shaped_break_constraints_are_suspended() {
    let shaper = ParleyShaper::new();
    let doc = page_break_document();

    assert_eq!(
        paginate_document(&doc, &shaper).page_count(),
        4,
        "on paper, four pageBreakBefore paragraphs are four pages"
    );
    let reflowed = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    assert_eq!(
        reflowed.page_count(),
        1,
        "in reflow, four short paragraphs are one tile — the break constraints are suspended"
    );
    for placed in &reflowed.pages[0].placed {
        if let BlockFragment::Paragraph { break_control, .. } = &placed.fragment {
            assert!(
                break_control.is_default(),
                "a reflow fragment still carries a page-shaped break constraint"
            );
        }
    }
}

/// Headers, footers, page borders, margin line numbers, the watermark and section
/// `w:vAlign` are all suppressed in reflow, and every one of them is *present* in
/// the same document's paged layout — so the assertion cannot pass by the document
/// not having them.
///
/// MUTATION PROOF: making `build_section_plans` build the real `RunningContent`
/// under reflow fails this with a non-empty header band on tile 1.
#[test]
fn reflow_suppresses_every_piece_of_page_furniture() {
    let shaper = ParleyShaper::new();
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        HeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "The running header")])],
        },
    );
    let mut footers = DefinitionMap::default();
    footers.insert(
        HeaderFooterId::new(node(400)),
        HeaderFooter {
            blocks: vec![paragraph(410, vec![run(411, "The running footer")])],
        },
    );
    let mut section = letter_section(7_000_001);
    section.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(300)),
    }];
    section.footers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(400)),
    }];
    section.page_borders = PageBorders {
        top: Some(BorderEdge {
            style: "single".to_owned(),
            size_eighth_points: Some(8),
            color: Some(RgbColor { r: 0, g: 0, b: 0 }),
            space_points: None,
            theme_color: None,
        }),
        bottom: None,
        start: None,
        end: None,
        display: Some(PageBorderDisplay::AllPages),
        offset_from: Some(PageBorderOffset::Page),
    };
    section.line_numbering = LineNumbering {
        count_by: Some(1),
        start: Some(1),
        distance: None,
        restart: None,
    };
    section.watermark = Some(draft_watermark());

    let doc = document(
        vec![
            paragraph(100, vec![run(101, &LINE.repeat(3))]),
            paragraph(110, vec![run(111, &LINE.repeat(3))]),
        ],
        Definitions {
            sections: vec![section],
            headers,
            footers,
            ..Definitions::default()
        },
    );

    // The precondition, stated explicitly: on paper this document really does
    // carry every one of them.
    let paged = paginate_document(&doc, &shaper);
    let first = &paged.pages[0];
    assert!(!first.header.is_empty(), "the paged page has a header");
    assert!(!first.footer.is_empty(), "the paged page has a footer");
    assert!(first.page_borders.is_some(), "the paged page has a border");
    assert!(
        !first.line_numbers.is_empty(),
        "the paged page has line numbers"
    );
    assert!(first.watermark.is_some(), "the paged page has a watermark");

    for page in &paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow()).pages {
        assert!(page.header.is_empty(), "tile {} has a header", page.number);
        assert!(page.footer.is_empty(), "tile {} has a footer", page.number);
        assert!(
            page.page_borders.is_none(),
            "tile {} has a page border",
            page.number
        );
        assert!(
            page.line_numbers.is_empty(),
            "tile {} has margin line numbers",
            page.number
        );
        assert!(
            page.watermark.is_none(),
            "tile {} has a watermark",
            page.number
        );
        assert!(
            page.separators.is_empty(),
            "tile {} has a column separator",
            page.number
        );
        assert_eq!(
            page.content_area.origin.y,
            Twip::ZERO,
            "tile {} reserved a top band that cannot exist",
            page.number
        );
    }
}

/// A two-column section reflows into one column: the content area is the whole
/// reflow column, not half of it.
///
/// MUTATION PROOF: dropping the `view.is_reflow()` arm from `push_section_run`'s
/// layout choice fails this — each tile then flows at 2,340 twips, half the column
/// minus the default 720-twip gap.
#[test]
fn a_multi_column_section_reflows_into_one_column() {
    let shaper = ParleyShaper::new();
    let mut section = letter_section(7_000_001);
    section.columns.count = 3;
    section.columns.separator = Some(true);
    let doc = document(
        (0..10)
            .map(|i| {
                paragraph(
                    i as u64 + 1_000_000,
                    vec![run(i as u64 + 2_000_000, &LINE.repeat(3))],
                )
            })
            .collect(),
        Definitions {
            sections: vec![section],
            ..Definitions::default()
        },
    );

    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    for page in &layout.pages {
        assert_eq!(
            page.content_area.size.width,
            COLUMN,
            "tile {} flows at {} rather than the whole {} column",
            page.number,
            page.content_area.size.width.raw(),
            COLUMN.raw()
        );
        assert!(
            page.separators.is_empty(),
            "tile {} drew a column separator",
            page.number
        );
    }
    // And the paged layout really is multi-column, so the assertion above is not
    // passing because the fixture forgot its `w:cols`.
    assert!(
        paginate_document(&doc, &shaper).pages[0]
            .placed
            .iter()
            .any(|placed| placed.rect.size.width < COLUMN),
        "the same document paginates into narrow columns on paper"
    );
}

/// A `nextPage` section break does not open a tile: it continues the column.
#[test]
fn a_section_break_does_not_cut_a_tile() {
    let shaper = ParleyShaper::new();
    let mut first = letter_section(7_000_001);
    first.section_type = Some(SectionType::NextPage);
    let mut second = letter_section(7_000_002);
    second.section_type = Some(SectionType::NextPage);
    let doc = document(
        vec![
            paragraph_with(
                100,
                ParagraphProperties {
                    section_break: Some(SectionId::new(node(7_000_001))),
                    ..ParagraphProperties::default()
                },
                "Section one",
            ),
            paragraph(200, vec![run(201, "Section two")]),
        ],
        Definitions {
            sections: vec![first, second],
            ..Definitions::default()
        },
    );

    assert_eq!(
        paginate_document(&doc, &shaper).page_count(),
        2,
        "on paper a nextPage break is a new page"
    );
    assert_eq!(
        paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow()).page_count(),
        1,
        "in reflow two short sections share one tile"
    );
}

// --------------------------------------------------------------------------
// Page numbers are refused, not invented
// --------------------------------------------------------------------------

fn field_document() -> Document {
    document(
        (0..40)
            .map(|i| {
                BlockNode::Paragraph(Paragraph {
                    id: node(i as u64 * 10 + 1),
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![
                        run(i as u64 * 10 + 2, &LINE.repeat(3)),
                        InlineNode::Field(Box::new(Field {
                            id: node(i as u64 * 10 + 3),
                            instruction: " NUMPAGES ".to_owned(),
                            kind: FieldKind::parse(" NUMPAGES "),
                            inlines: vec![run(i as u64 * 10 + 4, "1")],
                            form: None,
                            update: Default::default(),
                        })),
                    ],
                })
            })
            .collect(),
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    )
}

/// A `NUMPAGES` field prints a tile count on paper and a refusal in reflow.
///
/// MUTATION PROOF: passing `layout.pages.len().to_string()` as the total under
/// reflow fails this with `a reflow NUMPAGES field printed "5" as though tiles
/// were pages` — a number the reader would read as a page count, for a document
/// that has no pages in this view.
#[test]
fn a_page_count_field_refuses_rather_than_printing_a_tile_count() {
    let shaper = ParleyShaper::new();
    let doc = field_document();

    let field_values = |layout: &PaginatedLayout| -> Vec<String> {
        let mut out = Vec::new();
        for page in &layout.pages {
            for placed in &page.placed {
                if let BlockFragment::Paragraph { lines, .. } = &placed.fragment {
                    for line in &lines.lines {
                        for field in &line.fields {
                            out.push(field.value.clone());
                        }
                    }
                }
            }
        }
        out
    };

    let paged = field_values(&paginate_document(&doc, &shaper));
    assert!(!paged.is_empty(), "the fixture carries NUMPAGES fields");
    assert!(
        paged.iter().all(|value| value.parse::<u32>().is_ok()),
        "on paper NUMPAGES prints a number: {paged:?}"
    );

    let reflow_layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    // The demonstration, not just the assertion: the tile count is a DIFFERENT
    // number from the page count, so a regression that printed it would be
    // printing a specific falsehood rather than coincidentally the right answer.
    assert_ne!(
        reflow_layout.pages.len().to_string(),
        paged[0],
        "the fixture must tile to a different count than it paginates to, or this guard proves \
         nothing"
    );
    let reflowed = field_values(&reflow_layout);
    assert_eq!(reflowed.len(), paged.len(), "reflow lost a field");
    for value in &reflowed {
        assert!(
            value.parse::<u32>().is_err(),
            "a reflow NUMPAGES field printed {value:?} as though tiles were pages"
        );
    }
}

// --------------------------------------------------------------------------
// The seam refuses what it cannot lay out, and reports what it approximates
// --------------------------------------------------------------------------

/// Every refusal is reachable, and each reason names the offending value and its
/// bound. A silently substituted bound would lay the document out at a measure the
/// caller never asked for and could not see.
#[test]
fn reflow_geometry_that_is_not_a_reading_column_is_refused_with_a_reason() {
    let cases: Vec<(LayoutView, Result<LayoutView, ReflowRefused>, &str)> = Vec::new();
    drop(cases);

    // An unconverted CSS pixel width — the mistake the lower bound exists for.
    let too_narrow =
        LayoutView::reflow(Twip(390), TILE, GUTTER).expect_err("390 twips is a quarter inch");
    assert_eq!(too_narrow, ReflowRefused::ColumnTooNarrow(Twip(390)));
    assert!(
        too_narrow.reason().contains("390"),
        "{}",
        too_narrow.reason()
    );
    assert!(
        too_narrow.reason().contains("1440"),
        "{}",
        too_narrow.reason()
    );

    // Arguments in the wrong order: a tile height handed in as a column.
    let too_wide =
        LayoutView::reflow(Twip(50_000), TILE, GUTTER).expect_err("50,000 twips is 34in");
    assert_eq!(too_wide, ReflowRefused::ColumnTooWide(Twip(50_000)));
    assert!(too_wide.reason().contains("50000"), "{}", too_wide.reason());

    let too_short = LayoutView::reflow(COLUMN, Twip(1_000), GUTTER).expect_err("a 0.7in tile");
    assert_eq!(too_short, ReflowRefused::TileTooShort(Twip(1_000)));
    assert!(
        too_short.reason().contains("2880"),
        "{}",
        too_short.reason()
    );

    let too_tall = LayoutView::reflow(COLUMN, Twip(60_000), GUTTER).expect_err("a 41in tile");
    assert_eq!(too_tall, ReflowRefused::TileTooTall(Twip(60_000)));
    assert!(
        too_tall.reason().contains("canvas"),
        "{}",
        too_tall.reason()
    );

    let negative = LayoutView::reflow(COLUMN, TILE, Twip(-1)).expect_err("a negative gutter");
    assert_eq!(negative, ReflowRefused::GutterOutOfRange(Twip(-1)));
    let swallowing = LayoutView::reflow(COLUMN, TILE, COLUMN + Twip(1))
        .expect_err("a gutter wider than the column it pads");
    assert_eq!(
        swallowing,
        ReflowRefused::GutterOutOfRange(COLUMN + Twip(1))
    );

    // The bounds are inclusive at both ends, so the extremes are usable rather
    // than a hidden off-by-one.
    assert!(LayoutView::reflow(Twip(1_440), Twip(2_880), Twip::ZERO).is_ok());
    assert!(LayoutView::reflow(Twip(31_680), Twip(47_520), Twip::ZERO).is_ok());
}

/// What reflow approximates is reported **about this document**, not recited.
///
/// The guarantee is that every sentence the host can show is true of the document
/// it is shown for. The old list was a `vec![]` of three literals keyed on nothing
/// but `is_reflow()`, so a document with no footnotes was told where its footnotes
/// go and a document with no `PAGE` field was told that its page numbers refuse —
/// and the one approximation a reader can actually see, content fitted to the
/// measure, was not in the list at all (`docs/151` §8 item 8 and §6.3).
///
/// MUTATION PROOF: restoring the constant list (returning all four sentences
/// whenever `is_reflow()`) fails on the first assertion with
/// `plain prose approximates nothing, but 4 sentences were reported: ["A table or
/// an image wider than the reading column has been fitted…", "A drawing anchored
/// to the page…", "A footnote is placed…", "A PAGE or NUMPAGES field…"]`.
#[test]
fn reflow_reports_what_this_document_approximates_and_paged_reports_none() {
    let view = reflow();
    assert!(view.is_reflow());
    assert!(!LayoutView::Paged.is_reflow());

    // Paper approximates nothing, whatever the document.
    for (name, doc) in inertness_corpus() {
        assert!(
            LayoutView::Paged.approximations(&doc).is_empty(),
            "paged reported approximations for {name}"
        );
    }

    // Plain prose: no notes, no fields, no page-anchored art, nothing over-wide.
    let plain = view.approximations(&prose(4));
    assert!(
        plain.is_empty(),
        "plain prose approximates nothing, but {} sentences were reported: {plain:?}",
        plain.len()
    );

    // A document with a NUMPAGES field and nothing else: exactly that sentence.
    let fielded = view.approximations(&field_document());
    assert_eq!(
        fielded.len(),
        1,
        "a document whose only reflow casualty is a NUMPAGES field: {fielded:?}"
    );
    assert!(
        fielded[0].contains("PAGE or NUMPAGES"),
        "the reported sentence is not the field one: {fielded:?}"
    );

    // A document whose table is wider than the measure: the fitting sentence,
    // which is the one the constant list omitted.
    let wide = view.approximations(&table_document());
    assert_eq!(
        wide.len(),
        1,
        "the table fixture's only reflow approximation is the fit: {wide:?}"
    );
    assert!(
        wide[0].contains("wider than the reading column"),
        "the over-wide case is still not reported: {wide:?}"
    );

    // And the measure is part of the question: the same table in a column wide
    // enough to hold it approximates nothing.
    let roomy = LayoutView::reflow(Twip(13_000), TILE, GUTTER).expect("a 9in reading column");
    assert!(
        roomy.approximations(&table_document()).is_empty(),
        "a table that fits was still reported as fitted"
    );
}

// --------------------------------------------------------------------------
// A table still flows, and the incremental path agrees with the fresh one
// --------------------------------------------------------------------------

/// The number of columns and rows in [`table_document`], so the guards can name
/// every cell without restating the fixture.
const TABLE_COLS: u64 = 4;
const TABLE_ROWS: u64 = 6;

/// The node id of the paragraph in cell `(row, col)` of [`table_document`].
fn table_cell_paragraph(row: u64, col: u64) -> NodeId {
    node(310 + row * 40 + col * 6 + 1)
}

/// The table fixture: **9 inches of declared table in a 3.75-inch column.**
///
/// The width is the point. This fixture used to declare a 2,600 + 2,600 = 5,200
/// twip grid against a `COLUMN` of 5,400 and no `w:tblW` at all — so it was an
/// `Auto`/`Autofit` table, which `solve_column_widths` has always clamped to the
/// available width, and it **fitted**. Every assertion below was therefore
/// satisfied by arithmetic and could not have failed however badly an over-wide
/// table behaved, which is exactly the shape `SKILL.md` §4 forbids and is why
/// the gap survived two design documents (`docs/151` §6.3a).
///
/// So it now declares `w:tblW` in `dxa` at 12,960 twips over a four-column grid
/// of 3,240 each: the one input the solver consulted `available` for neither
/// before nor after, and the input that produced the loss.
fn table_document() -> Document {
    let cell = |id: u64, text: &str| TableCell {
        id: node(id),
        properties: TableCellProperties::default(),
        blocks: vec![paragraph(id + 1, vec![run(id + 2, text)])],
    };
    document(
        vec![
            paragraph(100, vec![run(101, &LINE.repeat(2))]),
            BlockNode::Table(Box::new(Table {
                id: node(200),
                properties: TableProperties {
                    width: Some(TableWidth::dxa(12_960)),
                    ..TableProperties::default()
                },
                grid_change: None,
                grid: (0..TABLE_COLS)
                    .map(|_| GridColumn {
                        width_twips: Some(3_240),
                    })
                    .collect(),
                rows: (0..TABLE_ROWS)
                    .map(|r| TableRow {
                        id: node(300 + r * 40),
                        properties: TableRowProperties::default(),
                        cells: (0..TABLE_COLS)
                            .map(|c| cell(310 + r * 40 + c * 6, "cell text"))
                            .collect(),
                    })
                    .collect(),
            })),
            paragraph(900, vec![run(901, &LINE.repeat(2))]),
        ],
        Definitions {
            sections: vec![letter_section(7_000_001)],
            ..Definitions::default()
        },
    )
}

/// A table reflows through the same pipeline, its auto widths resolving against
/// the reflow column, and the tiles it is cut into still trim to their content.
#[test]
fn a_table_reflows_into_the_column_and_its_tiles_still_trim() {
    let shaper = ParleyShaper::new();
    let doc = table_document();
    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    assert!(layout.pages.len() > 1, "a 2in tile cuts this fixture");

    for page in &layout.pages {
        let (_, _, right, bottom) = painted_extent(page).expect("a tile paints something");
        assert!(
            right <= GUTTER + COLUMN,
            "tile {} paints a table out to {}, past the {} column",
            page.number,
            right.raw(),
            (GUTTER + COLUMN).raw()
        );
        assert!(
            bottom <= page.page_size.height + DESCENT_ROUNDING,
            "tile {} paints down to {} below its own trimmed {} height",
            page.number,
            bottom.raw(),
            page.page_size.height.raw()
        );
    }
}

/// The incremental entry produces the layout a fresh pass produces, in reflow as
/// in paged. Reuse changes what a pass costs, never what it produces.
#[test]
fn an_incremental_reflow_pass_equals_a_fresh_one() {
    let shaper = ParleyShaper::new();
    let mut doc = prose(12);
    let mut cache = GalleyCache::new();
    let first = paginate_document_after_edit_in(
        &doc,
        &shaper,
        &mut cache,
        &DirtySet::new(),
        ReviewView::Editing,
        None,
        reflow(),
    )
    .layout;

    let edited = type_into(&mut doc, 3, 'z');
    let incremental = paginate_document_after_edit_in(
        &doc,
        &shaper,
        &mut cache,
        &DirtySet::complete([edited]),
        ReviewView::Editing,
        Some(first),
        reflow(),
    )
    .layout;
    let fresh = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());

    assert_eq!(
        incremental.pages.len(),
        fresh.pages.len(),
        "incremental and fresh reflow disagree on the tile count"
    );
    for (index, (a, b)) in incremental.pages.iter().zip(fresh.pages.iter()).enumerate() {
        assert_eq!(
            a, b,
            "incremental and fresh reflow disagree on tile {index}"
        );
    }
}

/// Types one character into the paragraph at `index`, in place.
fn type_into(document: &mut Document, index: usize, ch: char) -> NodeId {
    let mut body = document.body().to_vec();
    let BlockNode::Paragraph(paragraph) = &mut body[index] else {
        panic!("block {index} is not a paragraph");
    };
    let id = paragraph.id;
    let Some(InlineNode::Run(run)) = paragraph.inlines.first_mut() else {
        panic!("paragraph {index} has no leading run");
    };
    run.text.push(ch);
    *document = Document::new(document.id(), body, document.definitions().clone())
        .expect("growing a run keeps the document well formed");
    id
}

// --------------------------------------------------------------------------
// Complexity, guarded by doubling
// --------------------------------------------------------------------------

/// The shaped-paragraph count for a first reflow build and for one steady-state
/// keystroke after it, at `paragraphs` paragraphs.
fn reflow_work(paragraphs: usize) -> (usize, usize) {
    let shaper = ParleyShaper::new();
    let mut cache = GalleyCache::new();
    let mut doc = prose(paragraphs);
    let mut layout = paginate_document_after_edit_in(
        &doc,
        &shaper,
        &mut cache,
        &DirtySet::new(),
        ReviewView::Editing,
        None,
        reflow(),
    )
    .layout;
    let entering = cache.shaped_last_build();
    // Three keystrokes: the first establishes the retained galley and a reusable
    // layout, the rest are the steady state being measured.
    for _ in 0..3 {
        let edited = type_into(&mut doc, paragraphs / 2, 'z');
        layout = paginate_document_after_edit_in(
            &doc,
            &shaper,
            &mut cache,
            &DirtySet::complete([edited]),
            ReviewView::Editing,
            Some(layout),
            reflow(),
        )
        .layout;
    }
    (entering, cache.shaped_last_build())
}

/// Entering reflow is `O(document)` — by design, it is a full re-shape — and a
/// keystroke *in* reflow is `O(edit)`, which is the owner's constraint (`docs/107`
/// §4 B1) and the reason reflow can be an editable view at all.
///
/// Guarded by doubling `n`, not by a millisecond threshold: a threshold cannot
/// tell a slow constant from a linear walk, and the keystroke half is asserted
/// **flat** rather than merely sub-linear, because the behaviour a regression
/// would reintroduce re-shapes every paragraph and a doubling bound would accept
/// it.
#[test]
fn entering_reflow_costs_the_document_and_a_keystroke_in_it_costs_the_edit() {
    let (enter_n, keystroke_n) = reflow_work(120);
    let (enter_2n, keystroke_2n) = reflow_work(240);

    assert!(
        enter_n > 0 && enter_2n > 0,
        "entering reflow shapes the document: {enter_n} / {enter_2n}"
    );
    let ratio = enter_2n as f64 / enter_n as f64;
    assert!(
        (1.6..=2.4).contains(&ratio),
        "entering reflow should cost about twice as much at twice the size, not {ratio:.2}x \
         ({enter_n} -> {enter_2n} paragraphs shaped)"
    );
    assert_eq!(
        keystroke_n, keystroke_2n,
        "a keystroke in reflow re-shaped {keystroke_n} paragraphs at n and {keystroke_2n} at 2n — \
         the cost must not grow with the document"
    );
    assert!(
        keystroke_n <= 4,
        "a keystroke in reflow re-shaped {keystroke_n} paragraphs; it should re-shape the edited \
         one and its immediate neighbours, not more"
    );
}

// --------------------------------------------------------------------------
// Nothing authored is unreachable — `docs/151` §6.3
// --------------------------------------------------------------------------

/// **No authored content is unreachable in reflow.** Asserted over a table nine
/// inches wide in a three-and-three-quarter-inch column, through the only route a
/// reader has: the caret.
///
/// This is the guarantee rather than the circumstance. It does not say the raster
/// is N twips wide, nor what width the solver chose, nor how many lines a cell
/// wrapped to — all of which move for reasons that lose nothing. It says that for
/// every one of the fixture's twenty-four cells there is a caret position inside
/// the tile that was rasterised, and that clicking it comes back to that cell. A
/// cell whose caret lies outside the raster is a cell the reader cannot see, put
/// the caret in, select, search to, or read with a screen reader, and the only
/// way to reach it is to leave the view.
///
/// WHY THE CARET AND NOT THE PAINT. A display list that places a glyph at
/// x = 10,080 in a 6,120-twip tile is not evidence on its own: a renderer could
/// in principle be made to scroll it. A caret rect outside the raster is evidence,
/// because the host paints exactly the raster and guarantees no horizontal scroll
/// (`reflow_view.mjs`'s quantised bucket never exceeds `clientWidth`), so there is
/// nothing to scroll with.
///
/// MUTATION PROOF: with `MeasureFit::Fit` removed from `solve_column_widths` — the
/// single line `MeasureFit::Fit => target.min(available)` reverted to `target` —
/// this fails on the first cell of the third column:
///
/// ```text
/// cell (0,2)'s caret sits at 6948..6948 twips, outside the 0..6120 raster of
/// tile 2 — that part of the table is painted nowhere a reader can reach it
/// ```
#[test]
fn no_cell_of_a_table_wider_than_the_reading_column_is_unreachable() {
    let shaper = ParleyShaper::new();
    let doc = table_document();
    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());
    let snapshot = LayoutSnapshot::new(&layout);

    for row in 0..TABLE_ROWS {
        for col in 0..TABLE_COLS {
            let pos = ModelPos::new(table_cell_paragraph(row, col), 0);
            let (page, rect) = snapshot
                .caret_rect(pos)
                .unwrap_or_else(|| panic!("cell ({row},{col}) has no caret rect in reflow at all"));
            let raster = layout.pages[page as usize].page_size.width;
            assert!(
                rect.origin.x >= Twip::ZERO && rect.origin.x + rect.size.width <= raster,
                "cell ({row},{col})'s caret sits at {}..{} twips, outside the 0..{} raster of \
                 tile {} — that part of the table is painted nowhere a reader can reach it",
                rect.origin.x.raw(),
                (rect.origin.x + rect.size.width).raw(),
                raster.raw(),
                page + 1
            );
            let midpoint = Point::new(
                rect.origin.x,
                rect.origin.y + Twip(rect.size.height.raw() / 2),
            );
            let hit = snapshot.hit_test(page, midpoint).unwrap_or_else(|| {
                panic!("clicking cell ({row},{col})'s own caret in reflow found nothing")
            });
            assert_eq!(
                hit.pos, pos,
                "clicking cell ({row},{col})'s caret landed in {:?} instead",
                hit.pos
            );
        }
    }
}

/// An inline image wider than the reading column is **scaled into it with its
/// proportions kept**, not laid out past the raster and cut off.
///
/// Google document exactly this behaviour for pageless — *"images will adjust to
/// your screen size"* ([answer/11528737], quoted in `docs/151` §6.3b) — and
/// `hr_item`, the function immediately below `image_item` in `flow.rs`, already
/// resolved its width against the measure. Only the image did not.
///
/// The guarantee asserted is "the whole picture is inside the raster, and it is
/// still the same picture": the painted box fits, and its aspect ratio is the
/// declared one. The exact scaled height is NOT asserted as a number — it is
/// derived from the fixture's own ratio, so a fixture change cannot make this
/// pass for the wrong reason.
///
/// MUTATION PROOF: reverting `image_item` to ignore its `measure` — the
/// `fit_box_to_measure` call replaced by the bare `extent_to_size` it wrapped —
/// fails with
/// `the image is painted 10080 twips wide in a 6120-twip tile: 4320 twips of it
/// are outside the raster`. Squashing instead of scaling — the height kept while
/// the width is clamped — fails the ratio assertion with `the image was squashed
/// rather than scaled: 5400x5040 twips is not the declared 6400800:3200400
/// ratio`.
#[test]
fn an_image_wider_than_the_reading_column_is_scaled_into_it() {
    let shaper = ParleyShaper::new();
    // 7in x 3.5in — wider than the 3.75in reading column, and a 2:1 ratio so a
    // scale that forgot the height would be visible rather than plausible.
    const WIDTH_EMU: i64 = 6_400_800;
    const HEIGHT_EMU: i64 = 3_200_400;
    let doc = image_document(WIDTH_EMU, HEIGHT_EMU);
    let layout = paginate_document_in(&doc, &shaper, ReviewView::Editing, reflow());

    let mut seen = 0;
    for page in &layout.pages {
        for item in &compose_page(page).items {
            let PaintItem::Image { rect, .. } = item else {
                continue;
            };
            seen += 1;
            let overshoot = (rect.origin.x + rect.size.width).raw() - page.page_size.width.raw();
            assert!(
                overshoot <= 0,
                "the image is painted {} twips wide in a {}-twip tile: {overshoot} twips of it \
                 are outside the raster",
                rect.size.width.raw(),
                page.page_size.width.raw()
            );
            // Same picture, not a crop of it: the painted box keeps the declared
            // ratio to within the twip the integer scale can cost.
            let expected_height = (i64::from(rect.size.width.raw()) * HEIGHT_EMU) / WIDTH_EMU;
            assert!(
                (i64::from(rect.size.height.raw()) - expected_height).abs() <= 1,
                "the image was squashed rather than scaled: {}x{} twips is not the declared \
                 {WIDTH_EMU}:{HEIGHT_EMU} ratio",
                rect.size.width.raw(),
                rect.size.height.raw()
            );
        }
    }
    assert_eq!(seen, 1, "the fixture paints exactly one image");
}

/// A paragraph holding one inline drawing of the given EMU extent, between two
/// paragraphs of prose so the image is a real in-flow box rather than the whole
/// document.
fn image_document(width_emu: i64, height_emu: i64) -> Document {
    let media_id = MediaId::new(node(7_100_001));
    let mut media = DefinitionMap::default();
    media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId9".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/wide.png".to_owned(),
        },
    );
    document(
        vec![
            paragraph(100, vec![run(101, LINE)]),
            paragraph(
                200,
                vec![InlineNode::Drawing(Box::new(Drawing {
                    hyperlink: None,
                    opacity: None,
                    id: node(201),
                    media: media_id,
                    extent: Some(Extent {
                        width_emu,
                        height_emu,
                    }),
                    descr: None,
                    crop: None,
                    rotation: None,
                    border: None,
                    flip_h: false,
                    flip_v: false,
                }))],
            ),
            paragraph(300, vec![run(301, LINE)]),
        ],
        Definitions {
            sections: vec![letter_section(7_000_002)],
            media,
            ..Definitions::default()
        },
    )
}
