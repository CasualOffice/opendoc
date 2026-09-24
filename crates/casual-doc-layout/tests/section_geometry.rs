//! Two-sided and parity section geometry, end to end through the real driver
//! (`docs/105` FID-L-03 and FID-L-16).
//!
//! Both rows are "the model is complete and layout never reads it" gaps, so the
//! assertions are deliberately made against user-visible page geometry — how
//! many pages come out, which section owns each one, which running content each
//! one paints, and where the text band sits horizontally — rather than against
//! the internal plumbing that carries the values.
//!
//! - **FID-L-03**: a `w:type="evenPage"`/`"oddPage"` section must open on a page
//!   of that parity; when the next page would have the wrong one, Word inserts a
//!   blank page and still paints the *preceding* section's header/footer on it.
//! - **FID-L-16**: `w:pgMar/@w:gutter` is the binding allowance added to the
//!   inside margin, and `w:mirrorMargins` swaps the inside and outside margins on
//!   verso (even) pages, moving the body and the running bands with them.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::{document_page_config, paginate_document};
use casual_doc_layout::page::PlacedFragment;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, DefinitionMap, Definitions, Document, DocumentSettings,
    HeaderFooter as ModelHeaderFooter, HeaderFooterId, HeaderFooterKind, HeaderFooterRef,
    InlineNode, PageMargins, PageSize, Paragraph, ParagraphProperties, Run, RunProperties,
    SectionBoundary, SectionColumns, SectionId, SectionType,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn paragraph_with(id: u64, text: &str, properties: ParagraphProperties) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines: vec![run(id + 1, text)],
    })
}

fn paragraph(id: u64, text: &str) -> BlockNode {
    paragraph_with(id, text, ParagraphProperties::default())
}

/// A paragraph that ends section `section` (it carries that section's
/// `w:sectPr`), which is how a DOCX body marks a section break.
fn section_break(id: u64, text: &str, section: SectionId) -> BlockNode {
    paragraph_with(
        id,
        text,
        ParagraphProperties {
            section_break: Some(section),
            ..ParagraphProperties::default()
        },
    )
}

/// A section-ending paragraph that also starts a new page, so the section it
/// closes spans an exact, shape-independent number of pages.
fn section_break_on_new_page(id: u64, text: &str, section: SectionId) -> BlockNode {
    paragraph_with(
        id,
        text,
        ParagraphProperties {
            section_break: Some(section),
            page_break_before: true,
            ..ParagraphProperties::default()
        },
    )
}

/// A paragraph that forces a page break before it, so page counts are exact and
/// independent of shaped line heights.
fn page_break(id: u64, text: &str) -> BlockNode {
    paragraph_with(
        id,
        text,
        ParagraphProperties {
            page_break_before: true,
            ..ParagraphProperties::default()
        },
    )
}

/// A US-Letter section with the given margins, start type, and header reference.
fn section(
    id: u64,
    margins: PageMargins,
    section_type: Option<SectionType>,
    headers: Vec<HeaderFooterRef>,
) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(id)),
        page_size: PageSize {
            width_twips: 12_240,
            height_twips: 15_840,
        },
        page_margins: margins,
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        },
        headers,
        footers: Vec::new(),
        section_type,
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

fn margins(start: i32, end: i32, gutter: Option<i32>) -> PageMargins {
    PageMargins {
        top_twips: 1_440,
        bottom_twips: 1_440,
        start_twips: start,
        end_twips: end,
        header_twips: None,
        footer_twips: None,
        gutter_twips: gutter,
    }
}

fn href(id: u64) -> HeaderFooterRef {
    HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: HeaderFooterId::new(node(id)),
    }
}

/// A one-paragraph header part; its paragraph node id identifies it on a page.
fn header_part(id: u64) -> (HeaderFooterId, ModelHeaderFooter) {
    (
        HeaderFooterId::new(node(id)),
        ModelHeaderFooter {
            blocks: vec![paragraph(id + 1, "Running head")],
        },
    )
}

/// The node id of a band's first placed fragment (`None` for an empty band).
fn first_node(band: &[PlacedFragment]) -> Option<NodeId> {
    band.first().map(|placed| placed.fragment.node_id())
}

// ---------------------------------------------------------------------------
// FID-L-03 — `evenPage` / `oddPage` parity breaks
// ---------------------------------------------------------------------------

/// Two sections. Section 9 spans `front_pages` pages and owns header 300;
/// section 10 declares `start_type` and owns header 320, so the running content
/// on any inserted blank page says which section Word charged it to.
fn two_section_document(start_type: SectionType, front_pages: usize) -> Document {
    let mut headers = DefinitionMap::default();
    for id in [300, 320] {
        let (key, part) = header_part(id);
        headers.insert(key, part);
    }

    let first = SectionId::new(node(9));
    // One paragraph per page of section one; the last of them carries the
    // section's `w:sectPr`, which is how a DOCX marks where the section ends.
    let mut body = Vec::new();
    for page in 0..front_pages {
        let id = 100 + (page as u64) * 10;
        let text = "Front matter";
        body.push(match (page == 0, page + 1 == front_pages) {
            (true, true) => section_break(id, text, first),
            (true, false) => paragraph(id, text),
            (false, true) => section_break_on_new_page(id, text, first),
            (false, false) => page_break(id, text),
        });
    }
    body.push(paragraph(500, "Chapter body"));

    Document::new(
        node(1),
        body,
        Definitions {
            sections: vec![
                section(9, margins(1_440, 1_440, None), None, vec![href(300)]),
                section(
                    10,
                    margins(1_440, 1_440, None),
                    Some(start_type),
                    vec![href(320)],
                ),
            ],
            headers,
            ..Definitions::default()
        },
    )
    .unwrap()
}

#[test]
fn an_odd_page_section_break_inserts_the_parity_blank_page() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&two_section_document(SectionType::OddPage, 1), &shaper);

    // Section one fills page 1; an odd-page section cannot open on page 2, so a
    // blank page 2 is inserted and the chapter opens on page 3.
    assert_eq!(layout.pages.len(), 3, "the parity blank page is missing");
    assert_eq!(first_node(&layout.pages[0].placed), Some(node(100)));
    assert!(
        layout.pages[1].placed.is_empty(),
        "the inserted page must carry no body content"
    );
    assert_eq!(first_node(&layout.pages[2].placed), Some(node(500)));
    assert_eq!(
        layout.pages[2].number, 3,
        "the odd-page section must open on an odd page"
    );
}

#[test]
fn the_parity_blank_page_keeps_the_previous_sections_running_content() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&two_section_document(SectionType::OddPage, 1), &shaper);

    assert_eq!(
        layout.pages[1].section,
        SectionId::new(node(9)),
        "Word charges the blank page to the section it follows"
    );
    assert_eq!(
        first_node(&layout.pages[1].header),
        Some(node(301)),
        "Word still paints the previous section's header on the blank page"
    );
    assert_eq!(first_node(&layout.pages[2].header), Some(node(321)));
}

#[test]
fn the_parity_blank_page_composes_to_its_running_content_alone() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&two_section_document(SectionType::OddPage, 1), &shaper);

    // A page with no placed body fragments is a new shape for every downstream
    // consumer, so compose it: the display list must build and carry the
    // header's glyphs and nothing from the body.
    let blank = compose_page(&layout.pages[1]);
    let content = compose_page(&layout.pages[2]);
    assert!(
        !blank.items.is_empty(),
        "the blank page still paints its running content"
    );
    assert!(blank.items.len() < content.items.len());
}

#[test]
fn an_even_page_section_break_needs_no_blank_when_the_parity_already_matches() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&two_section_document(SectionType::EvenPage, 1), &shaper);

    // Page 2 is already even, so nothing is inserted — the pad must be
    // conditional on the parity, not on the break type.
    assert_eq!(layout.pages.len(), 2);
    assert_eq!(first_node(&layout.pages[1].placed), Some(node(500)));
}

#[test]
fn a_next_page_section_break_never_pads() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&two_section_document(SectionType::NextPage, 1), &shaper);

    assert_eq!(layout.pages.len(), 2);
    assert_eq!(first_node(&layout.pages[1].placed), Some(node(500)));
}

#[test]
fn the_parity_pad_follows_the_running_page_count_not_a_fixed_page() {
    let shaper = ParleyShaper::new();
    // Section one now spans two pages, so the natural next page is 3 (odd) and
    // it is the `evenPage` section that needs the blank — the mirror image of
    // the case above.
    let layout = paginate_document(&two_section_document(SectionType::EvenPage, 2), &shaper);

    assert_eq!(layout.pages.len(), 4, "the parity blank page is missing");
    assert!(layout.pages[2].placed.is_empty());
    assert_eq!(first_node(&layout.pages[3].placed), Some(node(500)));
    assert_eq!(layout.pages[3].number, 4);

    // The odd-page counterpart at the same length needs no pad at all.
    let odd = paginate_document(&two_section_document(SectionType::OddPage, 2), &shaper);
    assert_eq!(odd.pages.len(), 3);
    assert_eq!(first_node(&odd.pages[2].placed), Some(node(500)));
}

// ---------------------------------------------------------------------------
// FID-L-16 — `w:gutter` and `w:mirrorMargins`
// ---------------------------------------------------------------------------

fn bound_document(gutter: Option<i32>, mirror_margins: bool) -> Document {
    let mut headers = DefinitionMap::default();
    let (key, part) = header_part(300);
    headers.insert(key, part);
    Document::new(
        node(1),
        vec![paragraph(100, "Recto"), page_break(110, "Verso")],
        Definitions {
            sections: vec![section(
                9,
                // An asymmetric pair so a swap is unmistakable: inside 1440,
                // outside 2880.
                margins(1_440, 2_880, gutter),
                None,
                vec![href(300)],
            )],
            headers,
            settings: DocumentSettings {
                mirror_margins,
                ..DocumentSettings::default()
            },
            ..Definitions::default()
        },
    )
    .unwrap()
}

#[test]
fn the_binding_gutter_widens_the_inside_margin() {
    let shaper = ParleyShaper::new();
    let plain = paginate_document(&bound_document(None, false), &shaper);
    let bound = paginate_document(&bound_document(Some(720), false), &shaper);

    let plain_area = plain.pages[0].content_area;
    let bound_area = bound.pages[0].content_area;
    assert_eq!(plain_area.origin.x.raw(), 1_440);
    assert_eq!(plain_area.size.width.raw(), 12_240 - 1_440 - 2_880);

    assert_eq!(
        bound_area.origin.x.raw(),
        2_160,
        "the gutter is added to the inside margin"
    );
    assert_eq!(
        bound_area.size.width.raw(),
        12_240 - 1_440 - 720 - 2_880,
        "the gutter comes out of the text width"
    );
    // The text band and the header band both sit at the gutter-inset edge.
    assert_eq!(bound.pages[0].placed[0].rect.origin.x.raw(), 2_160);
    assert_eq!(bound.pages[0].header[0].rect.origin.x.raw(), 2_160);
    // The page-geometry accessor that callers size render surfaces with agrees.
    assert_eq!(
        document_page_config(&bound_document(Some(720), false))
            .content_area()
            .origin
            .x
            .raw(),
        2_160
    );
}

#[test]
fn mirrored_margins_swap_the_inside_and_outside_edges_on_verso_pages() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&bound_document(Some(360), true), &shaper);
    assert_eq!(layout.pages.len(), 2);

    let recto = layout.pages[0].content_area;
    let verso = layout.pages[1].content_area;
    // Recto (page 1): inside margin 1440 + 360 gutter on the left.
    assert_eq!(recto.origin.x.raw(), 1_800);
    // Verso (page 2): the outside margin is on the left, the inside margin (with
    // the gutter) moves to the right-hand edge.
    assert_eq!(
        verso.origin.x.raw(),
        2_880,
        "the verso page must start at the outside margin"
    );
    assert_eq!(
        verso.size.width.raw(),
        recto.size.width.raw(),
        "mirroring moves the band, it does not resize it"
    );
    assert_eq!(
        (verso.origin.x + verso.size.width).raw(),
        12_240 - 1_800,
        "the inside margin plus gutter is reserved at the right edge"
    );

    // Body text and running content both move with the band.
    assert_eq!(layout.pages[0].placed[0].rect.origin.x.raw(), 1_800);
    assert_eq!(layout.pages[1].placed[0].rect.origin.x.raw(), 2_880);
    assert_eq!(layout.pages[0].header[0].rect.origin.x.raw(), 1_800);
    assert_eq!(
        layout.pages[1].header[0].rect.origin.x.raw(),
        2_880,
        "the running band mirrors with the text it aligns to"
    );
}

#[test]
fn an_unmirrored_document_keeps_every_page_on_the_same_edge() {
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&bound_document(Some(360), false), &shaper);

    assert_eq!(layout.pages[0].content_area.origin.x.raw(), 1_800);
    assert_eq!(
        layout.pages[1].content_area.origin.x.raw(),
        1_800,
        "without w:mirrorMargins both sides use the same geometry"
    );
}
