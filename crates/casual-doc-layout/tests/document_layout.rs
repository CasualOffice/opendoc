//! The one-call layout driver (`paginate_document`), end to end.
//!
//! These drive the real driver against documents carrying their own section
//! geometry, header/footer definitions, and drawings, and assert the Word-grade
//! behaviors the viewer and the fidelity harness depend on:
//!
//! - a document with a distinct page size paginates at *that* size, not the old
//!   hard-coded US-Letter;
//! - a section header/footer produces non-empty `Page.header`/`Page.footer`;
//! - a `titlePg` first-page header differs on page 1;
//! - the driver output equals the hand-wired pipeline (a regression anchor that
//!   keeps the driver from drifting from the pieces it composes);
//! - header content flows the full galley pipeline — a header holding an image
//!   renders (the uniform-flow-pipeline invariant).

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::{
    document_page_config, paginate_document, paginate_document_view,
};
use casual_doc_layout::flow::ReviewView;
use casual_doc_layout::flow::{build_galley, flow_header_footer};
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::paginate::{paginate, resolve_fields};
use casual_doc_layout::running::{HeaderFooter, RunningContent, place_running_content};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, AnchoredDrawing, BlockNode, DefinitionMap, Definitions,
    Document, DocumentSettings, Drawing, DrawingAnchor, Extent, HeaderFooter as ModelHeaderFooter,
    HeaderFooterId, HeaderFooterKind, HeaderFooterRef, HorizontalAlign, HorizontalAnchor,
    HorizontalPosition, InlineNode, MediaId, MediaReference, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
    SectionType, VerticalAlign, VerticalAnchor, VerticalPosition, WrapMode,
};
use casual_doc_model::v1::{
    BorderEdge, PageBorderDisplay, PageBorderOffset, PageBorders, PageVerticalAlignment, RgbColor,
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

fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default(),
        inlines,
    })
}

/// A paragraph that forces a page break before it (deterministic page counts,
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

fn aligned_float(
    id: u64,
    media: MediaId,
    horizontal: HorizontalAnchor,
    vertical: VerticalAnchor,
    descr: &str,
) -> InlineNode {
    InlineNode::AnchoredDrawing(AnchoredDrawing {
        id: node(id),
        media,
        extent: Extent {
            width_emu: 635_000,
            height_emu: 635_000,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: horizontal,
                position: HorizontalPosition::Align(HorizontalAlign::Right),
            },
            vertical: AnchorVertical {
                relative_from: vertical,
                position: VerticalPosition::Align(VerticalAlign::Bottom),
            },
            wrap: WrapMode::None,
            wrap_distances: Default::default(),
            behind_doc: false,
        },
        descr: Some(descr.to_owned()),
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    })
}

fn offset_float(
    id: u64,
    media: MediaId,
    horizontal: HorizontalAnchor,
    x_emu: i64,
    descr: &str,
) -> InlineNode {
    InlineNode::AnchoredDrawing(AnchoredDrawing {
        id: node(id),
        media,
        extent: Extent {
            width_emu: 635_000,
            height_emu: 635_000,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: horizontal,
                position: HorizontalPosition::Offset(x_emu),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Paragraph,
                position: VerticalPosition::Offset(0),
            },
            wrap: WrapMode::None,
            wrap_distances: Default::default(),
            behind_doc: false,
        },
        descr: Some(descr.to_owned()),
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    })
}

/// A single-column section boundary with the given geometry and header/footer
/// references.
#[allow(clippy::too_many_arguments)]
fn section(
    id: u64,
    size: (i32, i32),
    margin: i32,
    headers: Vec<HeaderFooterRef>,
    footers: Vec<HeaderFooterRef>,
    title_page: bool,
) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(id)),
        page_size: PageSize {
            width_twips: size.0,
            height_twips: size.1,
        },
        page_margins: PageMargins {
            top_twips: margin,
            bottom_twips: margin,
            start_twips: margin,
            end_twips: margin,
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
        footers,
        section_type: None,
        title_page: title_page.then_some(true),
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
    }
}

fn href(kind: HeaderFooterKind, id: u64) -> HeaderFooterRef {
    HeaderFooterRef {
        kind,
        reference: HeaderFooterId::new(node(id)),
    }
}

/// US-Letter content width (page 12240 − 2×1440 margins) for a sanity contrast.
const LETTER_CONTENT_W: i32 = 12_240 - 2 * 1_440;

#[test]
fn a_distinct_page_size_paginates_at_that_size_not_us_letter() {
    let shaper = ParleyShaper::new();
    // A5-ish portrait page (5.83in × 8.27in) with half-inch margins — nothing like
    // US-Letter.
    let (w, h, margin) = (8_390, 11_907, 720);
    let sections = vec![section(9, (w, h), margin, vec![], vec![], false)];
    let doc = Document::new(
        node(1),
        vec![
            paragraph(100, vec![run(101, "One")]),
            page_break(110, "Two"),
            page_break(120, "Three"),
        ],
        Definitions {
            sections,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);

    // Three forced-break paragraphs => three pages, each at the section geometry.
    assert_eq!(layout.page_count(), 3);
    let expected_w = Twip(w - 2 * margin);
    for page in &layout.pages {
        assert_eq!(
            page.page_size,
            Size::new(Twip(w), Twip(h)),
            "each immutable page carries its section-resolved physical box"
        );
        assert_eq!(
            page.content_area.size.width, expected_w,
            "the content width comes from the section, not US-Letter"
        );
        assert_ne!(
            page.content_area.size.width,
            Twip(LETTER_CONTENT_W),
            "it is emphatically not the old hard-coded Letter width"
        );
    }
    // The derived config agrees with the page geometry.
    let config = document_page_config(&doc);
    assert_eq!(config.page_size, Size::new(Twip(w), Twip(h)));
}

#[test]
fn a_short_page_forces_more_breaks_than_letter_would() {
    let shaper = ParleyShaper::new();
    // A very short page (7in wide, ~2in tall) with no forced breaks: natural flow
    // must overflow onto multiple pages where a Letter page would hold everything.
    let sections = vec![section(9, (10_080, 2_880), 720, vec![], vec![], false)];
    let body: Vec<BlockNode> = (0..24)
        .map(|i| paragraph(100 + i, vec![run(500 + i, "A line of body text.")]))
        .collect();
    let doc = Document::new(
        node(1),
        body,
        Definitions {
            sections,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert!(
        layout.page_count() >= 2,
        "a short page overflows to multiple pages (got {})",
        layout.page_count()
    );
}

/// A document whose first section references a default header and footer, with
/// those definitions in the store. Returns the document.
fn doc_with_header_footer() -> Document {
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "The running header")])],
        },
    );
    let mut footers = DefinitionMap::default();
    footers.insert(
        HeaderFooterId::new(node(400)),
        ModelHeaderFooter {
            blocks: vec![paragraph(410, vec![run(411, "The running footer")])],
        },
    );
    let sections = vec![section(
        9,
        (12_240, 15_840),
        1_440,
        vec![href(HeaderFooterKind::Default, 300)],
        vec![href(HeaderFooterKind::Default, 400)],
        false,
    )];
    Document::new(
        node(1),
        vec![
            paragraph(100, vec![run(101, "Body one")]),
            page_break(110, "Body two"),
        ],
        Definitions {
            sections,
            headers,
            footers,
            ..Definitions::default()
        },
    )
    .unwrap()
}

#[test]
fn a_section_header_and_footer_produce_non_empty_page_bands() {
    let shaper = ParleyShaper::new();
    let doc = doc_with_header_footer();
    let layout = paginate_document(&doc, &shaper);

    assert_eq!(layout.page_count(), 2);
    for page in &layout.pages {
        assert!(
            !page.header.is_empty(),
            "page {} has a running header",
            page.number
        );
        assert!(
            !page.footer.is_empty(),
            "page {} has a running footer",
            page.number
        );
        // The header sits above the body content, the footer below it.
        let body_top = page.content_area.origin.y.raw();
        let body_bottom = page.content_area.bottom().raw();
        assert!(page.header[0].rect.origin.y.raw() < body_top);
        assert!(page.footer[0].rect.origin.y.raw() >= body_bottom);
    }
}

#[test]
fn a_title_page_header_differs_on_page_one() {
    let shaper = ParleyShaper::new();
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "Default header")])],
        },
    );
    headers.insert(
        HeaderFooterId::new(node(320)),
        ModelHeaderFooter {
            blocks: vec![paragraph(330, vec![run(331, "First page header")])],
        },
    );
    let sections = vec![section(
        9,
        (12_240, 15_840),
        1_440,
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::First, 320),
        ],
        vec![],
        true, // titlePg
    )];
    let doc = Document::new(
        node(1),
        vec![
            paragraph(100, vec![run(101, "One")]),
            page_break(110, "Two"),
        ],
        Definitions {
            sections,
            headers,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    // Page 1 uses the first-page header (node 330); page 2 the default (310).
    assert_eq!(layout.pages[0].header[0].fragment.node_id(), node(330));
    assert_eq!(layout.pages[1].header[0].fragment.node_id(), node(310));
}

#[test]
fn later_sections_use_their_own_geometry_running_content_and_first_page_variant() {
    let shaper = ParleyShaper::new();
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "First-section header")])],
        },
    );
    headers.insert(
        HeaderFooterId::new(node(600)),
        ModelHeaderFooter {
            blocks: vec![paragraph(610, vec![run(611, "Landscape default")])],
        },
    );
    headers.insert(
        HeaderFooterId::new(node(620)),
        ModelHeaderFooter {
            blocks: vec![paragraph(630, vec![run(631, "Landscape first")])],
        },
    );
    let mut footers = DefinitionMap::default();
    footers.insert(
        HeaderFooterId::new(node(400)),
        ModelHeaderFooter {
            blocks: vec![paragraph(410, vec![run(411, "First-section footer")])],
        },
    );
    footers.insert(
        HeaderFooterId::new(node(700)),
        ModelHeaderFooter {
            blocks: vec![paragraph(710, vec![run(711, "Landscape default footer")])],
        },
    );
    footers.insert(
        HeaderFooterId::new(node(720)),
        ModelHeaderFooter {
            blocks: vec![paragraph(730, vec![run(731, "Landscape first footer")])],
        },
    );

    let first = section(
        9,
        (12_240, 15_840),
        1_440,
        vec![href(HeaderFooterKind::Default, 300)],
        vec![href(HeaderFooterKind::Default, 400)],
        false,
    );
    let first_id = first.id;
    let mut second = section(
        19,
        (15_840, 12_240),
        720,
        vec![
            href(HeaderFooterKind::Default, 600),
            href(HeaderFooterKind::First, 620),
        ],
        vec![
            href(HeaderFooterKind::Default, 700),
            href(HeaderFooterKind::First, 720),
        ],
        true,
    );
    second.section_type = Some(SectionType::NextPage);
    let second_id = second.id;
    let doc = Document::new(
        node(1),
        vec![
            BlockNode::Paragraph(Paragraph {
                id: node(100),
                properties: ParagraphProperties {
                    section_break: Some(first_id),
                    ..ParagraphProperties::default()
                },
                inlines: vec![run(101, "Portrait")],
            }),
            paragraph(110, vec![run(111, "Landscape first page")]),
            page_break(120, "Landscape second page"),
        ],
        Definitions {
            sections: vec![first, second],
            headers,
            footers,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(layout.page_count(), 3);
    assert_eq!(
        layout.pages[0].page_size,
        Size::new(Twip(12_240), Twip(15_840))
    );
    assert_eq!(
        layout.pages[1].page_size,
        Size::new(Twip(15_840), Twip(12_240))
    );
    assert_eq!(
        layout.pages[2].page_size,
        Size::new(Twip(15_840), Twip(12_240))
    );
    assert_eq!(layout.pages[0].section, first_id);
    assert_eq!(layout.pages[1].section, second_id);
    assert_eq!(layout.pages[2].section, second_id);

    assert_eq!(layout.pages[0].header[0].fragment.node_id(), node(310));
    assert_eq!(layout.pages[0].footer[0].fragment.node_id(), node(410));
    // `titlePg` is section-local: document page 2 is the first landscape page.
    assert_eq!(layout.pages[1].header[0].fragment.node_id(), node(630));
    assert_eq!(layout.pages[1].footer[0].fragment.node_id(), node(730));
    assert_eq!(layout.pages[2].header[0].fragment.node_id(), node(610));
    assert_eq!(layout.pages[2].footer[0].fragment.node_id(), node(710));
}

#[test]
fn driver_equals_manual_wiring() {
    let shaper = ParleyShaper::new();
    let doc = doc_with_header_footer();

    // The driver's output.
    let driven = paginate_document(&doc, &shaper);

    // The exact hand-wired pipeline a caller used to write by hand.
    let mut config = document_page_config(&doc);
    let content_width = config.content_area().size.width;
    let header = flow_header_footer(
        &doc,
        &[paragraph(310, vec![run(311, "The running header")])],
        &shaper,
        content_width,
    );
    let footer = flow_header_footer(
        &doc,
        &[paragraph(410, vec![run(411, "The running footer")])],
        &shaper,
        content_width,
    );
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
    config.header_height = hh;
    config.footer_height = fh;
    let galley = build_galley(&doc, &shaper, content_width);
    let mut manual = paginate(&galley, &config);
    place_running_content(&mut manual, &running, &config);
    resolve_fields(&mut manual, &shaper);
    // (No anchored drawings in this document; the driver's anchor pass is a no-op.)

    assert_eq!(
        driven, manual,
        "the driver produces exactly the hand-wired layout"
    );
}

#[test]
fn even_and_odd_headers_setting_is_honored() {
    let shaper = ParleyShaper::new();
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![paragraph(310, vec![run(311, "Odd header")])],
        },
    );
    headers.insert(
        HeaderFooterId::new(node(340)),
        ModelHeaderFooter {
            blocks: vec![paragraph(350, vec![run(351, "Even header")])],
        },
    );
    let sections = vec![section(
        9,
        (12_240, 15_840),
        1_440,
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::Even, 340),
        ],
        vec![],
        false,
    )];
    let doc = Document::new(
        node(1),
        vec![
            paragraph(100, vec![run(101, "One")]),
            page_break(110, "Two"),
        ],
        Definitions {
            sections,
            headers,
            settings: DocumentSettings {
                even_and_odd_headers: true,
                ..DocumentSettings::default()
            },
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    // Odd page 1 -> default/odd (310); even page 2 -> even (350).
    assert_eq!(layout.pages[0].header[0].fragment.node_id(), node(310));
    assert_eq!(layout.pages[1].header[0].fragment.node_id(), node(350));
}

#[test]
fn a_header_image_renders_through_the_full_pipeline() {
    let shaper = ParleyShaper::new();
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
    // The header holds a drawing (300×200 twips from its EMU extent).
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: node(310),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Drawing(Drawing {
                    id: node(311),
                    media: media_id,
                    extent: Some(Extent {
                        width_emu: 190_500,
                        height_emu: 127_000,
                    }),
                    descr: None,
                    crop: None,
                    border: None,
                    flip_h: false,
                    flip_v: false,
                    rotation: None,
                })],
            })],
        },
    );
    let sections = vec![section(
        9,
        (12_240, 15_840),
        1_440,
        vec![href(HeaderFooterKind::Default, 300)],
        vec![],
        false,
    )];
    let doc = Document::new(
        node(1),
        vec![paragraph(100, vec![run(101, "Body")])],
        Definitions {
            sections,
            headers,
            media,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    let page = &layout.pages[0];
    let body_top = page.content_area.origin.y.raw();
    // The image paints in the header band via the same compose machinery as the body.
    let list = compose_page(page);
    let img = list
        .items
        .iter()
        .find_map(|i| match i {
            PaintItem::Image { media, rect, .. } if media == "word/media/logo.png" => Some(*rect),
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
fn later_section_floats_use_that_sections_page_and_margin_geometry() {
    let shaper = ParleyShaper::new();
    let media_id = MediaId::new(node(70));
    let mut media = DefinitionMap::default();
    media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId7".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/section.png".to_owned(),
        },
    );

    let first = section(9, (12_240, 15_840), 1_440, vec![], vec![], false);
    let first_id = first.id;
    let mut second = section(19, (20_000, 10_000), 0, vec![], vec![], false);
    second.page_margins = PageMargins {
        top_twips: 2_000,
        bottom_twips: 500,
        start_twips: 3_000,
        end_twips: 1_000,
        header_twips: None,
        footer_twips: None,
        gutter_twips: None,
    };
    second.section_type = Some(SectionType::NextPage);

    let body = vec![
        BlockNode::Paragraph(Paragraph {
            id: node(100),
            properties: ParagraphProperties {
                section_break: Some(first_id),
                ..ParagraphProperties::default()
            },
            inlines: vec![run(101, "First section")],
        }),
        BlockNode::Paragraph(Paragraph {
            id: node(110),
            properties: ParagraphProperties::default(),
            inlines: vec![
                run(111, "Second section"),
                aligned_float(
                    112,
                    media_id,
                    HorizontalAnchor::Page,
                    VerticalAnchor::Page,
                    "page frame",
                ),
                aligned_float(
                    113,
                    media_id,
                    HorizontalAnchor::Margin,
                    VerticalAnchor::Margin,
                    "margin frame",
                ),
            ],
        }),
    ];
    let doc = Document::new(
        node(1),
        body,
        Definitions {
            sections: vec![first, second],
            media,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(layout.page_count(), 2);
    let page = &layout.pages[1];
    let page_float = page
        .anchored
        .iter()
        .find(|anchor| anchor.descr.as_deref() == Some("page frame"))
        .expect("the page-relative second-section float");
    assert_eq!(
        page_float.rect.origin,
        Point::new(Twip(19_000), Twip(9_000))
    );
    let margin_float = page
        .anchored
        .iter()
        .find(|anchor| anchor.descr.as_deref() == Some("margin frame"))
        .expect("the margin-relative second-section float");
    assert_eq!(
        margin_float.rect.origin,
        Point::new(Twip(18_000), Twip(8_500))
    );
}

#[test]
fn continuous_sections_on_one_page_keep_distinct_anchor_margins() {
    let shaper = ParleyShaper::new();
    let media_id = MediaId::new(node(70));
    let mut media = DefinitionMap::default();
    media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId7".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/continuous.png".to_owned(),
        },
    );

    let mut first = section(9, (12_000, 16_000), 1_000, vec![], vec![], false);
    first.page_margins.end_twips = 2_000;
    let first_id = first.id;
    let mut second = section(19, (12_000, 16_000), 1_000, vec![], vec![], false);
    second.page_margins.start_twips = 3_000;
    second.page_margins.end_twips = 500;
    second.section_type = Some(SectionType::Continuous);

    let doc = Document::new(
        node(1),
        vec![
            BlockNode::Paragraph(Paragraph {
                id: node(100),
                properties: ParagraphProperties {
                    section_break: Some(first_id),
                    ..ParagraphProperties::default()
                },
                inlines: vec![
                    run(101, "First band"),
                    offset_float(102, media_id, HorizontalAnchor::Margin, 0, "first margin"),
                ],
            }),
            BlockNode::Paragraph(Paragraph {
                id: node(110),
                properties: ParagraphProperties::default(),
                inlines: vec![
                    run(111, "Second band"),
                    offset_float(112, media_id, HorizontalAnchor::Margin, 0, "second margin"),
                ],
            }),
        ],
        Definitions {
            sections: vec![first, second],
            media,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(
        layout.page_count(),
        1,
        "a continuous section remains on the same page"
    );
    let page = &layout.pages[0];
    let first_x = page
        .anchored
        .iter()
        .find(|anchor| anchor.descr.as_deref() == Some("first margin"))
        .expect("first-section float")
        .rect
        .origin
        .x;
    let second_x = page
        .anchored
        .iter()
        .find(|anchor| anchor.descr.as_deref() == Some("second margin"))
        .expect("second-section float")
        .rect
        .origin
        .x;
    assert_eq!(first_x, Twip(1_000));
    assert_eq!(second_x, Twip(3_000));
}

/// A positioned float in the header part (the SDS positions its title/version/date
/// text boxes there with page-relative offsets that reach past the top margin) must
/// push the body content down so it clears the header content, rather than starting
/// at `margin_top` and colliding with it — Word's `body_top = max(margin_top,
/// header_distance + header_extent)`. Regression for the SDS header/body overlap.
#[test]
fn a_positioned_header_float_reserves_band_so_the_body_clears_it() {
    let shaper = ParleyShaper::new();
    // Page-relative float at y = 1440 twips (914_400 EMU), 1440 twips tall, so its
    // bottom is 2880 twips from the page top — well past the 720-twip top margin.
    let header_float = || {
        InlineNode::AnchoredDrawing(AnchoredDrawing {
            id: node(320),
            media: MediaId::new(node(321)),
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
                wrap: WrapMode::None,
                wrap_distances: Default::default(),
                behind_doc: true,
            },
            descr: None,
            relative_height: None,
            crop: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    };
    let build = |header_blocks: Vec<InlineNode>| {
        let mut headers = DefinitionMap::default();
        headers.insert(
            HeaderFooterId::new(node(300)),
            ModelHeaderFooter {
                blocks: vec![paragraph(310, header_blocks)],
            },
        );
        let mut media = DefinitionMap::default();
        media.insert(
            MediaId::new(node(321)),
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/hdr.png".to_owned(),
            },
        );
        Document::new(
            node(1),
            vec![paragraph(100, vec![run(101, "Body")])],
            Definitions {
                sections: vec![section(
                    9,
                    (12_240, 15_840),
                    720,
                    vec![href(HeaderFooterKind::Default, 300)],
                    vec![],
                    false,
                )],
                headers,
                media,
                ..Definitions::default()
            },
        )
        .unwrap()
    };

    // With the positioned float, the body starts at its extent (2880), not the
    // 720-twip top margin.
    let doc = build(vec![header_float()]);
    let layout = paginate_document(&doc, &shaper);
    assert_eq!(
        layout.pages[0].content_area.origin.y,
        Twip(2_880),
        "the body clears the positioned header float (extent 2880 > margin 720)"
    );

    // A header with only ordinary inline text reserves nothing for the float path
    // (it only gets the normal flowed-band reservation), so the body sits far above
    // the float case — proof the extra space is the float reservation, not the band.
    let plain = build(vec![run(311, "Just header text")]);
    let plain_top = paginate_document(&plain, &shaper).pages[0]
        .content_area
        .origin
        .y;
    assert!(
        plain_top < Twip(2_880),
        "an ordinary header reserves only its flowed band ({plain_top:?}), far less \
         than the positioned float's 2880-twip extent"
    );
}

fn page_border_edge() -> BorderEdge {
    BorderEdge {
        style: "single".to_owned(),
        size_eighth_points: Some(24),
        color: Some(RgbColor {
            r: 20,
            g: 40,
            b: 120,
        }),
        space_points: Some(24),
    }
}

fn count_border_rects(list: &casual_doc_layout::display::DisplayList) -> usize {
    list.items
        .iter()
        .filter(|item| matches!(item, PaintItem::Rect { .. }))
        .count()
}

#[test]
fn page_borders_resolve_per_page_and_compose_paints_the_frame() {
    let shaper = ParleyShaper::new();
    let (w, h, margin) = (12_240, 15_840, 1_440);
    // A section whose page border shows on the first page only (title-page frame),
    // measured from the page edge.
    let mut sec = section(9, (w, h), margin, vec![], vec![], false);
    sec.page_borders = PageBorders {
        display: Some(PageBorderDisplay::FirstPage),
        offset_from: Some(PageBorderOffset::Page),
        top: Some(page_border_edge()),
        bottom: Some(page_border_edge()),
        start: Some(page_border_edge()),
        end: Some(page_border_edge()),
    };
    let doc = Document::new(
        node(1),
        vec![
            paragraph(100, vec![run(101, "Page one")]),
            page_break(110, "Page two"),
        ],
        Definitions {
            sections: vec![sec],
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(layout.page_count(), 2);

    // Page 1 carries the resolved frame, inset 24pt (480 twips) from the page edge.
    let first = layout.pages[0]
        .page_borders
        .expect("first page has a border");
    assert_eq!(first.rect.origin, Point::new(Twip(480), Twip(480)));
    assert_eq!(first.rect.right(), Twip(w - 480));
    assert_eq!(first.rect.bottom(), Twip(h - 480));
    assert!(first.top.is_some() && first.end.is_some());

    // Page 2 has none (display=firstPage), proving the per-page policy.
    assert!(
        layout.pages[1].page_borders.is_none(),
        "the firstPage policy suppresses the border on later pages"
    );

    // Compose paints the four edges on page 1 and nothing extra on page 2.
    let first_rects = count_border_rects(&compose_page(&layout.pages[0]));
    let second_rects = count_border_rects(&compose_page(&layout.pages[1]));
    assert!(
        first_rects >= second_rects + 4,
        "the framed page paints at least four more border rects ({first_rects} vs {second_rects})"
    );
}

/// (total glyphs, glyphs painted in struck runs) across a layout's body paragraphs.
fn glyph_and_struck_counts(layout: &PaginatedLayout) -> (usize, usize) {
    let mut total = 0;
    let mut struck = 0;
    for page in &layout.pages {
        for placed in &page.placed {
            if let BlockFragment::Paragraph { lines, .. } = &placed.fragment {
                for line in &lines.lines {
                    for run in &line.runs {
                        total += run.glyphs.len();
                        if run.decoration.strikethrough {
                            struck += run.glyphs.len();
                        }
                    }
                }
            }
        }
    }
    (total, struck)
}

#[test]
fn markup_view_shows_struck_deletions_the_editing_view_drops() {
    use casual_doc_model::v1::{Revision, RevisionKind};

    let deletion = InlineNode::Revision(Revision {
        id: node(20),
        kind: RevisionKind::Deletion,
        author: Some("Ada".to_owned()),
        date: None,
        revision_id: None,
        editor_group: None,
        inlines: vec![run(21, "GONE")],
    });
    let doc = Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(2),
            properties: ParagraphProperties::default(),
            inlines: vec![run(3, "keep"), deletion],
        })],
        Definitions {
            sections: vec![section(9, (12_240, 15_840), 1_440, vec![], vec![], false)],
            ..Definitions::default()
        },
    )
    .unwrap();
    let shaper = ParleyShaper::new();

    // The editing view drops the deletion: no struck glyphs at all.
    let (editing_total, editing_struck) =
        glyph_and_struck_counts(&paginate_document(&doc, &shaper));
    assert_eq!(editing_struck, 0, "the editing view shows no struck text");

    // The markup view shows the deleted "GONE" and strikes it — more glyphs, some struck.
    let (markup_total, markup_struck) =
        glyph_and_struck_counts(&paginate_document_view(&doc, &shaper, ReviewView::Markup));
    assert!(
        markup_struck > 0,
        "the markup view strikes the deleted text"
    );
    assert!(
        markup_total > editing_total,
        "the markup view shows the deleted text the editing view drops ({markup_total} vs {editing_total})"
    );
}

/// Section `w:vAlign` shifts the placed body content within the content area:
/// `Bottom` pushes a short block's bottom to the content-area bottom, `Center`
/// lands halfway, and the default (`Top`) leaves it at the content-area top.
#[test]
fn section_valign_positions_body_content_in_the_content_area() {
    let shaper = ParleyShaper::new();
    let make = |valign: Option<PageVerticalAlignment>| {
        let mut section = section(9, (12_240, 15_840), 1_440, vec![], vec![], false);
        section.vertical_alignment = valign;
        Document::new(
            node(1),
            vec![paragraph(100, vec![run(101, "Only line")])],
            Definitions {
                sections: vec![section],
                ..Definitions::default()
            },
        )
        .unwrap()
    };
    let placed_y = |doc: &Document| {
        let layout = paginate_document(doc, &shaper);
        let page = &layout.pages[0];
        let placed = &page.placed[0];
        (
            placed.rect.origin.y.raw(),
            (placed.rect.origin.y + placed.rect.size.height).raw(),
            page.content_area.origin.y.raw(),
            (page.content_area.origin.y + page.content_area.size.height).raw(),
        )
    };

    let (top_y, _, content_top, _) = placed_y(&make(None));
    assert_eq!(top_y, content_top, "the default keeps content at the top");

    let (bottom_y, bottom_bottom, _, content_bottom) =
        placed_y(&make(Some(PageVerticalAlignment::Bottom)));
    assert!(bottom_y > top_y, "bottom vAlign pushes the block down");
    assert_eq!(
        bottom_bottom, content_bottom,
        "the block's bottom aligns to the content-area bottom"
    );

    let (center_y, ..) = placed_y(&make(Some(PageVerticalAlignment::Center)));
    assert!(
        center_y > top_y && center_y < bottom_y,
        "center sits between top ({top_y}) and bottom ({bottom_y}), got {center_y}"
    );
}
