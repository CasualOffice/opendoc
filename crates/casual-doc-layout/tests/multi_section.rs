//! Multi-section pagination (P1F-19), end to end.
//!
//! These drive the real pipeline — split a two-section body with
//! [`build_sections`] (each section shaped at its own content width), reserve each
//! section's header band, [`paginate_sections`], place per-section running content,
//! resolve fields — and assert the Word-grade behaviors:
//!
//! - each section paginates under its **own geometry** (a portrait section 1 and a
//!   landscape section 2 with different margins);
//! - the second section begins on a **new page** (`nextPage`);
//! - the two sections show **different headers** across the boundary; and
//! - `repaginate_sections == paginate_sections` holds with headers + fields, for an
//!   edit in section 1 that changes its page count (section 2 reused) — the
//!   incremental golden, extended to multi-section.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::flow::{build_sections, flow_header_footer};
use casual_doc_layout::paginate::resolve_fields;
use casual_doc_layout::running::{HeaderFooter, RunningContent, place_running_content_sections};
use casual_doc_layout::section::{Section, paginate_sections, repaginate_sections};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::LineShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
    SectionType,
};

const MARGIN: i32 = 1_440;
const PORTRAIT_W: i32 = 12_240;
const PORTRAIT_H: i32 = 15_840;
// Landscape section: width/height swapped and a wider start/end margin, so its
// content area is genuinely different from the portrait section's.
const LAND_W: i32 = 15_840;
const LAND_H: i32 = 12_240;
const LAND_MARGIN: i32 = 2_160;

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

/// A plain paragraph.
fn paragraph(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default(),
        inlines: vec![run(id + 1, text)],
    })
}

/// A paragraph that forces a page break before it (so per-section page counts are
/// exact, independent of shaped heights).
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

/// A paragraph that ends a section (carries its `w:sectPr` via `section_break`).
fn section_end(id: u64, text: &str, section: SectionId, page_break_before: bool) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            page_break_before,
            section_break: Some(section),
            ..ParagraphProperties::default()
        },
        inlines: vec![run(id + 1, text)],
    })
}

fn portrait_section(id: u64) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(id)),
        page_size: PageSize {
            width_twips: PORTRAIT_W,
            height_twips: PORTRAIT_H,
        },
        page_margins: margins(MARGIN),
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
        },
        headers: Vec::new(),
        footers: Vec::new(),
        section_type: Some(SectionType::NextPage),
        title_page: None,
        vertical_alignment: None,
        page_numbering: Default::default(),
        doc_grid: Default::default(),
    }
}

fn landscape_section(id: u64) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(id)),
        page_size: PageSize {
            width_twips: LAND_W,
            height_twips: LAND_H,
        },
        page_margins: margins(LAND_MARGIN),
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
        },
        headers: Vec::new(),
        footers: Vec::new(),
        section_type: Some(SectionType::NextPage),
        title_page: None,
        vertical_alignment: None,
        page_numbering: Default::default(),
        doc_grid: Default::default(),
    }
}

fn margins(v: i32) -> PageMargins {
    PageMargins {
        top_twips: v,
        bottom_twips: v,
        start_twips: v,
        end_twips: v,
    }
}

/// A two-section document: section 1 (portrait) has `s1_pages` pages via forced
/// breaks and ends with a `w:sectPr`; section 2 (landscape) is one page.
fn two_section_doc(s1_extra_pages: usize) -> Document {
    let s1 = SectionId::new(node(10));
    let mut body = vec![paragraph(100, "section one, page one")];
    let mut next = 200;
    for _ in 0..s1_extra_pages {
        body.push(page_break(next, "section one, later page"));
        next += 10;
    }
    // The section-1-ending paragraph carries the sectPr; it stays on section 1's
    // last page (no extra break), so section 1 has exactly `1 + s1_extra_pages`
    // pages.
    body.push(section_end(300, "end of section one", s1, false));
    // Section 2 (body-level section = the trailing `sections` entry).
    body.push(paragraph(400, "section two, page one"));

    let definitions = Definitions {
        sections: vec![portrait_section(10), landscape_section(20)],
        ..Definitions::default()
    };
    Document::new(node(1), body, definitions).unwrap()
}

/// Flows a one-paragraph header galley at the given section band width.
fn header_galley(
    doc: &Document,
    shaper: &dyn LineShaper,
    id: u64,
    text: &str,
    page_width: i32,
    margin: i32,
) -> Vec<BlockFragment> {
    flow_header_footer(
        doc,
        &[paragraph(id, text)],
        shaper,
        casual_doc_layout::units::Twip(page_width - 2 * margin),
    )
}

/// Builds the per-section running content (a distinct header per section) and sets
/// each section's reserved header band, returning the `(RunningContent, config)`
/// list the running-content pass consumes.
fn wire_running(
    doc: &Document,
    shaper: &dyn LineShaper,
    sections: &mut [Section],
) -> Vec<(RunningContent, casual_doc_layout::paginate::PageConfig)> {
    // Distinct header node ids per section so the placed header's provenance is
    // observable across the boundary.
    let headers = [
        (9001u64, "HEADER ONE", PORTRAIT_W, MARGIN),
        (9002u64, "HEADER TWO", LAND_W, LAND_MARGIN),
    ];
    let mut list = Vec::new();
    for (section, (hid, text, width, margin)) in sections.iter_mut().zip(headers) {
        let running = RunningContent {
            header: HeaderFooter {
                default: header_galley(doc, shaper, hid, text, width, margin),
                ..HeaderFooter::default()
            },
            ..RunningContent::default()
        };
        let (hh, fh) = running.band_heights();
        section.config.header_height = hh;
        section.config.footer_height = fh;
        list.push((running, section.config));
    }
    list
}

#[test]
fn each_section_paginates_under_its_own_geometry_with_its_own_header() {
    let shaper = ParleyShaper::new();
    let doc = two_section_doc(1); // section 1 = 2 pages, section 2 = 1 page
    let mut sections = build_sections(&doc, &shaper);
    assert_eq!(sections.len(), 2, "two sections built from the body");

    let running = wire_running(&doc, &shaper, &mut sections);
    let mut layout = paginate_sections(&sections);
    place_running_content_sections(&mut layout, &running);
    resolve_fields(&mut layout, &shaper);

    assert_eq!(
        layout.page_count(),
        3,
        "2 pages in section 1 + 1 in section 2"
    );
    let s1 = SectionId::new(node(10));
    let s2 = SectionId::new(node(20));
    assert_eq!(layout.pages[0].section, s1);
    assert_eq!(layout.pages[1].section, s1);
    assert_eq!(
        layout.pages[2].section, s2,
        "section 2 starts on a fresh page"
    );

    // Geometry: section 1 pages are portrait width; section 2 is landscape width.
    let portrait_w = PORTRAIT_W - 2 * MARGIN;
    let land_w = LAND_W - 2 * LAND_MARGIN;
    assert_eq!(layout.pages[0].content_area.size.width.raw(), portrait_w);
    assert_eq!(layout.pages[1].content_area.size.width.raw(), portrait_w);
    assert_eq!(layout.pages[2].content_area.size.width.raw(), land_w);
    assert_ne!(portrait_w, land_w, "the sections truly differ in geometry");

    // Headers differ across the boundary (distinct header node ids).
    let header_node = |page: &casual_doc_layout::page::Page| page.header[0].fragment.node_id();
    assert_eq!(header_node(&layout.pages[0]), node(9001));
    assert_eq!(header_node(&layout.pages[1]), node(9001));
    assert_eq!(
        header_node(&layout.pages[2]),
        node(9002),
        "section 2 shows its own header"
    );
}

#[test]
fn repaginate_sections_equals_full_across_a_section_one_page_count_change() {
    let shaper = ParleyShaper::new();
    // Prev: section 1 = 2 pages. New: section 1 = 3 pages (one more forced break).
    let prev_doc = two_section_doc(1);
    let new_doc = two_section_doc(2);

    let mut prev_sections = build_sections(&prev_doc, &shaper);
    let mut new_sections = build_sections(&new_doc, &shaper);
    let _prev_running = wire_running(&prev_doc, &shaper, &mut prev_sections);
    let new_running = wire_running(&new_doc, &shaper, &mut new_sections);

    // Bare (field-value-free) layouts drive the incremental path; the running +
    // field post-passes are applied identically afterward.
    let prev_bare = paginate_sections(&prev_sections);
    let mut inc = repaginate_sections(&prev_bare, &prev_sections, &new_sections);
    let mut full = paginate_sections(&new_sections);

    place_running_content_sections(&mut inc, &new_running);
    resolve_fields(&mut inc, &shaper);
    place_running_content_sections(&mut full, &new_running);
    resolve_fields(&mut full, &shaper);

    assert_eq!(
        inc, full,
        "incremental multi-section pagination equals a full paginate, with headers + fields"
    );
    assert_eq!(
        full.page_count(),
        4,
        "3 pages in section 1 + 1 in section 2"
    );
    // Section 2's single page was pushed from page 3 to page 4 but keeps its header.
    let s2_page = full.pages.last().unwrap();
    assert_eq!(s2_page.section, SectionId::new(node(20)));
    assert_eq!(s2_page.number, 4);
    assert_eq!(s2_page.header[0].fragment.node_id(), node(9002));
}
