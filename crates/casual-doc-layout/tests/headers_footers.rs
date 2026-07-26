//! Headers / footers + page-number fields (P1F-17), end to end.
//!
//! These drive the real pipeline — flow the body and the header/footer variants,
//! reserve the bands in the [`PageConfig`], paginate, run the running-content pass
//! and the field pass, then compose — and assert the Word-grade behaviors:
//!
//! - a running header/footer paints in its band on every page, and the body area
//!   shrinks by the bands;
//! - `titlePg` gives page 1 the first-page header;
//! - `evenAndOddHeaders` gives even pages the even header;
//! - a `Page X of Y` footer shows the right values per page after the field pass;
//! - `repaginate == paginate` still holds with headers/footers + resolved fields,
//!   and an edit that changes the page count updates `NUMPAGES` on every page,
//!   including the ones the incremental paginator reused.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::flow::{build_galley, flow_header_footer};
use casual_doc_layout::paginate::{PageConfig, paginate, repaginate, resolve_fields};
use casual_doc_layout::running::{HeaderFooter, RunningContent, place_running_content};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{FieldKind, LineShaper};
use casual_doc_layout::units::{Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, DefinitionMap, Definitions, Document, Drawing, Extent, Field, GridColumn,
    InlineNode, MediaId, MediaReference, Paragraph, ParagraphProperties, Run, RunProperties,
    SectionId, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties,
};

const PAGE_W: i32 = 12_240;
const PAGE_H: i32 = 15_840;
const MARGIN: i32 = 1_440;
/// Body content width = page width − start − end margins (band-independent).
const WIDTH: Twip = Twip(PAGE_W - 2 * MARGIN);

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

fn field(id: u64, instruction: &str, cached: &str) -> InlineNode {
    InlineNode::Field(Field {
        id: node(id),
        instruction: instruction.to_owned(),
        inlines: vec![run(id + 1, cached)],
        form: None,
    })
}

fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default(),
        inlines,
    })
}

/// A paragraph that forces a page break before it (so page counts are exact,
/// independent of shaped heights).
fn page_break(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            page_break_before: true,
            ..ParagraphProperties::default()
        },
        inlines: vec![run(id + 1, text)],
    })
}

fn document(body: Vec<BlockNode>) -> Document {
    Document::new(node(1), body, Definitions::default()).unwrap()
}

fn base_config(header_height: Twip, footer_height: Twip) -> PageConfig {
    PageConfig {
        section: SectionId::new(node(9)),
        page_size: Size::new(Twip(PAGE_W), Twip(PAGE_H)),
        margin_top: Twip(MARGIN),
        margin_bottom: Twip(MARGIN),
        margin_start: Twip(MARGIN),
        margin_end: Twip(MARGIN),
        header_height,
        footer_height,
    }
}

/// Flows a one-paragraph header/footer body into a galley at the band width.
fn flow_one(doc: &Document, shaper: &dyn LineShaper, block: BlockNode) -> Vec<BlockFragment> {
    flow_header_footer(doc, &[block], shaper, WIDTH)
}

/// The `(page, numpages)` values a placed footer/header fragment resolves to, read
/// off its first line's field markers.
fn field_values(
    placed: &[casual_doc_layout::page::PlacedFragment],
) -> (Option<String>, Option<String>) {
    let mut page = None;
    let mut numpages = None;
    for pf in placed {
        if let BlockFragment::Paragraph { lines, .. } = &pf.fragment {
            for line in &lines.lines {
                for f in &line.fields {
                    match f.kind {
                        FieldKind::Page => page = Some(f.value.clone()),
                        FieldKind::NumPages => numpages = Some(f.value.clone()),
                        FieldKind::Passthrough => {}
                    }
                }
            }
        }
    }
    (page, numpages)
}

#[test]
fn a_running_header_and_footer_paint_on_every_page_and_shrink_the_body() {
    let shaper = ParleyShaper::new();
    // Three pages via forced breaks.
    let doc = document(vec![
        paragraph(100, vec![run(101, "Body one")]),
        page_break(110, "Body two"),
        page_break(120, "Body three"),
    ]);

    let header = flow_one(&doc, &shaper, paragraph(200, vec![run(201, "The header")]));
    let footer = flow_one(&doc, &shaper, paragraph(210, vec![run(211, "The footer")]));
    let running = RunningContent {
        header: HeaderFooter {
            default: header,
            ..HeaderFooter::default()
        },
        footer: HeaderFooter {
            default: footer,
            ..HeaderFooter::default()
        },
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    assert!(hh.raw() > 0 && fh.raw() > 0, "the bands have real height");
    let config = base_config(hh, fh);

    // The body content area shrinks by both bands relative to a no-band config.
    let plain = base_config(Twip::ZERO, Twip::ZERO);
    let reduced = config.content_area();
    let full = plain.content_area();
    assert_eq!(
        reduced.origin.y,
        full.origin.y + hh,
        "body starts below the header"
    );
    assert_eq!(
        reduced.size.height,
        full.size.height - hh - fh,
        "body height is reduced by both bands"
    );

    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    assert_eq!(layout.page_count(), 3);
    let header_band = config.header_band();
    let footer_band = config.footer_band();
    for page in &layout.pages {
        assert!(!page.header.is_empty(), "page {} has a header", page.number);
        assert!(!page.footer.is_empty(), "page {} has a footer", page.number);
        // The header sits in the header band and the footer in the footer band.
        assert_eq!(page.header[0].rect.origin.y, header_band.origin.y);
        assert_eq!(page.footer[0].rect.origin.y, footer_band.origin.y);
        // The body content is below the header band, above the footer band.
        for placed in &page.placed {
            assert!(placed.rect.origin.y >= config.content_area().origin.y);
        }
        // Composition paints header, body, and footer glyphs.
        let list = compose_page(page);
        let glyph_ys: Vec<i32> = list
            .items
            .iter()
            .filter_map(|i| match i {
                PaintItem::Glyphs { run } => Some(run.origin.y.raw()),
                _ => None,
            })
            .collect();
        assert!(
            glyph_ys
                .iter()
                .any(|y| *y < config.content_area().origin.y.raw()),
            "some header glyph paints above the body"
        );
        assert!(
            glyph_ys.iter().any(|y| *y >= footer_band.origin.y.raw()),
            "some footer glyph paints in the footer band"
        );
    }
}

#[test]
fn title_page_uses_the_first_page_header() {
    let shaper = ParleyShaper::new();
    let doc = document(vec![
        paragraph(100, vec![run(101, "One")]),
        page_break(110, "Two"),
    ]);
    let default = flow_one(
        &doc,
        &shaper,
        paragraph(200, vec![run(201, "Default header")]),
    );
    let first = flow_one(
        &doc,
        &shaper,
        paragraph(210, vec![run(211, "First page header")]),
    );
    let running = RunningContent {
        header: HeaderFooter {
            default,
            first,
            ..HeaderFooter::default()
        },
        footer: HeaderFooter::default(),
        title_page: true,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    // The header node id identifies which variant was placed: page 1 uses the
    // first-page header (node 210); page 2 uses the default (200).
    assert_eq!(layout.pages[0].header[0].fragment.node_id(), node(210));
    assert_eq!(layout.pages[1].header[0].fragment.node_id(), node(200));
}

#[test]
fn even_and_odd_headers_alternate() {
    let shaper = ParleyShaper::new();
    let doc = document(vec![
        paragraph(100, vec![run(101, "One")]),
        page_break(110, "Two"),
        page_break(120, "Three"),
        page_break(130, "Four"),
    ]);
    let default = flow_one(&doc, &shaper, paragraph(200, vec![run(201, "Odd header")]));
    let even = flow_one(&doc, &shaper, paragraph(210, vec![run(211, "Even header")]));
    let running = RunningContent {
        header: HeaderFooter {
            default,
            even,
            ..HeaderFooter::default()
        },
        footer: HeaderFooter::default(),
        title_page: false,
        even_and_odd: true,
    };
    let (hh, fh) = running.band_heights();
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    assert_eq!(layout.page_count(), 4);
    // Odd pages (1, 3) -> default/odd header (node 200); even pages (2, 4) -> even (210).
    assert_eq!(layout.pages[0].header[0].fragment.node_id(), node(200));
    assert_eq!(layout.pages[1].header[0].fragment.node_id(), node(210));
    assert_eq!(layout.pages[2].header[0].fragment.node_id(), node(200));
    assert_eq!(layout.pages[3].header[0].fragment.node_id(), node(210));
}

/// Builds a `Page X of Y` footer galley.
fn page_of_y_footer(doc: &Document, shaper: &dyn LineShaper) -> Vec<BlockFragment> {
    let para = paragraph(
        300,
        vec![
            run(301, "Page "),
            field(310, "PAGE", "1"),
            run(320, " of "),
            field(330, "NUMPAGES", "1"),
        ],
    );
    flow_one(doc, shaper, para)
}

#[test]
fn page_x_of_y_footer_resolves_per_page() {
    let shaper = ParleyShaper::new();
    let doc = document(vec![
        paragraph(100, vec![run(101, "One")]),
        page_break(110, "Two"),
        page_break(120, "Three"),
    ]);
    let footer = page_of_y_footer(&doc, &shaper);
    let running = RunningContent {
        header: HeaderFooter::default(),
        footer: HeaderFooter {
            default: footer,
            ..HeaderFooter::default()
        },
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    assert!(fh.raw() > 0, "the footer band has height");
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    let total = layout.page_count();
    assert_eq!(total, 3);
    for page in &layout.pages {
        let (p, n) = field_values(&page.footer);
        assert_eq!(p.as_deref(), Some(page.number.to_string().as_str()));
        assert_eq!(
            n.as_deref(),
            Some("3"),
            "NUMPAGES equals the total page count"
        );
    }
}

/// The full pipeline as a driver applies it: paginate, place running content,
/// resolve fields.
fn full_layout(
    galley: &[BlockFragment],
    running: &RunningContent,
    config: &PageConfig,
    shaper: &dyn LineShaper,
) -> casual_doc_layout::page::PaginatedLayout {
    let mut layout = paginate(galley, config);
    place_running_content(&mut layout, running, config);
    resolve_fields(&mut layout, shaper);
    layout
}

#[test]
fn repaginate_equals_paginate_with_headers_footers_and_fields() {
    let shaper = ParleyShaper::new();

    // A five-page document (forced breaks), with a `Page X of Y` footer.
    let body = |extra: bool| {
        let mut b = vec![paragraph(100, vec![run(101, "One")])];
        b.push(page_break(110, "Two"));
        b.push(page_break(120, "Three"));
        b.push(page_break(130, "Four"));
        b.push(page_break(140, "Five"));
        if extra {
            b.push(page_break(150, "Six")); // the edit: one more page
        }
        b
    };
    let doc_prev = document(body(false));
    let doc_new = document(body(true));

    let footer = page_of_y_footer(&doc_prev, &shaper);
    let running = RunningContent {
        header: HeaderFooter::default(),
        footer: HeaderFooter {
            default: footer,
            ..HeaderFooter::default()
        },
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    let config = base_config(hh, fh);

    let prev_galley = build_galley(&doc_prev, &shaper, WIDTH);
    let new_galley = build_galley(&doc_new, &shaper, WIDTH);

    // Previous layout, fully resolved (as a caller would hold it).
    let prev_layout = full_layout(&prev_galley, &running, &config, &shaper);
    assert_eq!(prev_layout.page_count(), 5);

    // Incremental re-pagination, then the same post-passes.
    let mut inc = repaginate(&prev_layout, &prev_galley, &new_galley, &config);
    place_running_content(&mut inc, &running, &config);
    resolve_fields(&mut inc, &shaper);

    // A full re-layout of the new galley.
    let full = full_layout(&new_galley, &running, &config, &shaper);

    // The golden invariant survives headers/footers + resolved fields.
    assert_eq!(
        inc, full,
        "repaginate == paginate with running content + fields"
    );
    assert_eq!(inc.page_count(), 6);

    // NUMPAGES is now 6 on EVERY page — including the reused first page.
    for page in &inc.pages {
        let (p, n) = field_values(&page.footer);
        assert_eq!(p.as_deref(), Some(page.number.to_string().as_str()));
        assert_eq!(n.as_deref(), Some("6"), "reused pages get the new NUMPAGES");
    }
}

#[test]
fn multi_digit_page_numbers_reflow_the_trailing_text() {
    let shaper = ParleyShaper::new();
    // Twelve pages, so PAGE reaches two digits and NUMPAGES is "12".
    let mut body = vec![paragraph(100, vec![run(101, "p1")])];
    for i in 1..12u64 {
        body.push(page_break(200 + i * 2, "p"));
    }
    let doc = document(body);
    let footer = page_of_y_footer(&doc, &shaper);
    let running = RunningContent {
        header: HeaderFooter::default(),
        footer: HeaderFooter {
            default: footer,
            ..HeaderFooter::default()
        },
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    assert_eq!(layout.page_count(), 12);
    for page in &layout.pages {
        let (p, n) = field_values(&page.footer);
        assert_eq!(p.as_deref(), Some(page.number.to_string().as_str()));
        assert_eq!(n.as_deref(), Some("12"));
        // The " of " run begins exactly where the PAGE field's glyphs end — the
        // trailing text stayed contiguous after a two-digit value widened the field.
        let BlockFragment::Paragraph { lines, .. } = &page.footer[0].fragment else {
            panic!("footer paragraph");
        };
        let line = &lines.lines[0];
        let page_field = line
            .fields
            .iter()
            .find(|f| f.kind == FieldKind::Page)
            .unwrap();
        let field_run = &line.runs[page_field.run as usize];
        let field_end = field_run.origin.x.raw()
            + field_run
                .glyphs
                .iter()
                .map(|g| g.advance.raw())
                .sum::<i32>();
        let next_run = &line.runs[page_field.run as usize + 1];
        assert_eq!(
            next_run.origin.x.raw(),
            field_end,
            "the run after PAGE is contiguous on page {}",
            page.number
        );
    }
}

#[test]
fn a_body_page_field_resolves() {
    let shaper = ParleyShaper::new();
    // A PAGE field in body content (not just running content) resolves too.
    let doc = document(vec![
        paragraph(100, vec![run(101, "Section "), field(102, "PAGE", "1")]),
        page_break(110, "next"),
    ]);
    let config = base_config(Twip::ZERO, Twip::ZERO);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    // No running content, but the body field still resolves.
    resolve_fields(&mut layout, &shaper);
    let (p, _) = field_values(&layout.pages[0].placed);
    assert_eq!(p.as_deref(), Some("1"));
}

#[test]
fn a_header_can_contain_an_inline_image() {
    let shaper = ParleyShaper::new();
    // A document whose media table backs an inline drawing used *in the header* —
    // the header flows through the identical galley pipeline as the body, so images
    // work with no separate path.
    let media_id = MediaId::new(node(70));
    let mut media = DefinitionMap::default();
    media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId7".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/logo.png".to_owned(),
        },
    );
    let definitions = Definitions {
        media,
        ..Definitions::default()
    };
    let doc = Document::new(
        node(1),
        vec![paragraph(100, vec![run(101, "Body")])],
        definitions,
    )
    .unwrap();

    // The header holds a drawing (300×200 twips from its EMU extent).
    let header_block = BlockNode::Paragraph(Paragraph {
        id: node(200),
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::Drawing(Drawing {
            id: node(201),
            media: media_id,
            extent: Some(Extent {
                width_emu: 190_500,
                height_emu: 127_000,
            }),
        })],
    });
    let header = flow_header_footer(&doc, &[header_block], &shaper, WIDTH);
    let running = RunningContent {
        header: HeaderFooter {
            default: header,
            ..HeaderFooter::default()
        },
        footer: HeaderFooter::default(),
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    assert!(hh.raw() >= 200, "the header band fits the image: {hh:?}");
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    // The image paints in the header band (same `compose_page` machinery as the body).
    let page = &layout.pages[0];
    let list = compose_page(page);
    let body_top = config.content_area().origin.y.raw();
    let img = list
        .items
        .iter()
        .find_map(|i| match i {
            PaintItem::Image { media, rect } if media == "word/media/logo.png" => Some(*rect),
            _ => None,
        })
        .expect("the header image paints");
    assert_eq!(img.size, Size::new(Twip(300), Twip(200)));
    assert!(
        img.origin.y.raw() < body_top,
        "the header image paints above the body"
    );
}

#[test]
fn a_header_can_contain_a_nested_table() {
    let shaper = ParleyShaper::new();
    let doc = document(vec![paragraph(100, vec![run(101, "Body")])]);

    // A header table whose only cell holds a *nested* table — the shared flow
    // recurses (flow_table -> flow_blocks -> flow_table) exactly as in the body.
    let inner = Table {
        id: node(230),
        grid: vec![GridColumn {
            width_twips: Some(2000),
        }],
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(231),
            properties: TableRowProperties::default(),
            cells: vec![TableCell {
                id: node(232),
                properties: TableCellProperties::default(),
                blocks: vec![paragraph(233, vec![run(234, "inner")])],
            }],
        }],
    };
    let outer = BlockNode::Table(Table {
        id: node(220),
        grid: vec![GridColumn {
            width_twips: Some(4000),
        }],
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(221),
            properties: TableRowProperties::default(),
            cells: vec![TableCell {
                id: node(222),
                properties: TableCellProperties::default(),
                blocks: vec![BlockNode::Table(inner)],
            }],
        }],
    });
    let header = flow_header_footer(&doc, &[outer], &shaper, WIDTH);

    // The header flowed to a table row whose cell carries the nested table's row.
    let BlockFragment::TableRow { cells, .. } = &header[0] else {
        panic!("the header table flowed to a row fragment");
    };
    assert!(
        cells[0]
            .blocks
            .iter()
            .any(|b| matches!(b, BlockFragment::TableRow { .. })),
        "the outer cell holds the nested table's row"
    );

    let running = RunningContent {
        header: HeaderFooter {
            default: header,
            ..HeaderFooter::default()
        },
        footer: HeaderFooter::default(),
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut layout = paginate(&galley, &config);
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, &shaper);

    // The nested table's cell grid lines paint in the header band.
    let page = &layout.pages[0];
    let band_top = config.header_band().origin.y.raw();
    let body_top = config.content_area().origin.y.raw();
    let list = compose_page(page);
    let rect_in_header = list.items.iter().any(|i| match i {
        PaintItem::Rect { rect, .. } => {
            rect.origin.y.raw() >= band_top && rect.origin.y.raw() < body_top
        }
        _ => false,
    });
    assert!(
        rect_in_header,
        "the header table's grid lines paint in the band"
    );
}

#[test]
fn resolve_fields_is_idempotent() {
    let shaper = ParleyShaper::new();
    let doc = document(vec![
        paragraph(100, vec![run(101, "One")]),
        page_break(110, "Two"),
    ]);
    let footer = page_of_y_footer(&doc, &shaper);
    let running = RunningContent {
        header: HeaderFooter::default(),
        footer: HeaderFooter {
            default: footer,
            ..HeaderFooter::default()
        },
        title_page: false,
        even_and_odd: false,
    };
    let (hh, fh) = running.band_heights();
    let config = base_config(hh, fh);
    let galley = build_galley(&doc, &shaper, WIDTH);
    let mut once = paginate(&galley, &config);
    place_running_content(&mut once, &running, &config);
    resolve_fields(&mut once, &shaper);
    let mut twice = once.clone();
    resolve_fields(&mut twice, &shaper);
    assert_eq!(once, twice, "running the field pass again changes nothing");
}
