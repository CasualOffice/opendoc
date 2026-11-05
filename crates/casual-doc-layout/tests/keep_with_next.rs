//! Keep with next through the real driver (`109` FID-L-29).
//!
//! Word binds a `w:keepNext` paragraph to the START of the next paragraph
//! (ECMA-376 §17.3.1.15), not to all of it: a heading over a long paragraph stays
//! at the foot of a page with the paragraph's first lines, and the paragraph
//! splits. This engine moved the whole pair to the next page whenever it fitted a
//! page but not the space left, leaving a blank page foot above every such
//! heading (a 14-page Word contract came out at 15 pages, its page breaks diverging
//! from the ones Word recorded in the file).
//!
//! The guarantee asserted is the reader's: wherever the heading lands, if it
//! starts a page, the page before had no room for the heading plus the lines it
//! must keep. It is swept over every filler length, so it holds at every position
//! on the page, and it is asserted through both paginators: a document without
//! sections (`paginate.rs`) and one with a section (the column driver,
//! `columns.rs`), which carried a second copy of the same group rule.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
};

const HEADING: u64 = 9_000;
const BODY: u64 = 9_010;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn paragraph(id: u64, text: &str, properties: ParagraphProperties) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(id + 1),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
    })
}

fn section() -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(1)),
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

/// `fillers` one-line paragraphs, then a keepNext heading, then a widow-controlled
/// paragraph of about twenty lines.
fn document(fillers: u64, with_section: bool) -> Document {
    let mut body: Vec<BlockNode> = (0..fillers)
        .map(|i| paragraph(10 + i * 2, "Filler line.", ParagraphProperties::default()))
        .collect();
    body.push(paragraph(
        HEADING,
        "Invoicing and Payment",
        ParagraphProperties {
            keep_next: Some(true),
            ..ParagraphProperties::default()
        },
    ));
    body.push(paragraph(
        BODY,
        &"Contractor will submit invoices in accordance with the schedule. ".repeat(24),
        ParagraphProperties {
            widow_control: Some(true),
            ..ParagraphProperties::default()
        },
    ));
    let mut definitions = Definitions::default();
    if with_section {
        definitions.sections.push(section());
    }
    Document::new(node(2), body, definitions).expect("valid document")
}

/// (page index, fragment) of every placed fragment of `id`.
fn placements(layout: &PaginatedLayout, id: u64) -> Vec<(usize, &BlockFragment, i32)> {
    let mut out = Vec::new();
    for (index, page) in layout.pages.iter().enumerate() {
        for placed in &page.placed {
            if placed.fragment.node_id() == node(id) {
                out.push((index, &placed.fragment, placed.rect.origin.y.raw()));
            }
        }
    }
    out
}

fn line_heights(fragment: &BlockFragment) -> Vec<i32> {
    match fragment {
        BlockFragment::Paragraph { lines, .. } => {
            lines.lines.iter().map(|line| line.height.raw()).collect()
        }
        BlockFragment::TableRow { .. } => Vec::new(),
    }
}

fn check(with_section: bool) {
    let shaper = ParleyShaper::new();
    let mut heading_kept_at_a_page_foot = false;
    for fillers in 20..60 {
        let layout = paginate_document(&document(fillers, with_section), &shaper);
        let heading = placements(&layout, HEADING);
        let body = placements(&layout, BODY);
        assert_eq!(heading.len(), 1, "{fillers}: one heading");
        let (heading_page, heading_fragment, heading_top) = heading[0];
        assert_eq!(
            body[0].0, heading_page,
            "{fillers}: the heading is on the page where its paragraph starts"
        );
        let heading_height = heading_fragment.height().raw();
        let body_lines = line_heights(body[0].1);
        if heading_page > 0 && heading_top == layout.pages[heading_page].content_area.origin.y.raw()
        {
            // The heading starts a page: the page before must have had no room for
            // it and the two lines widow control keeps with it.
            let previous = &layout.pages[heading_page - 1];
            let used = previous
                .placed
                .iter()
                .map(|placed| placed.rect.origin.y.raw() + placed.rect.size.height.raw())
                .max()
                .unwrap_or(0);
            let room = previous.content_area.origin.y.raw()
                + previous.content_area.size.height.raw()
                - used;
            let needed = heading_height + body_lines.iter().take(2).sum::<i32>();
            assert!(
                room < needed,
                "{fillers} fillers (section: {with_section}): the heading moved to page {} \
                 although page {} had {room} twips left and it needed only {needed}",
                heading_page + 1,
                heading_page
            );
        }
        if body.len() > 1 && body[0].0 == heading_page {
            heading_kept_at_a_page_foot = true;
        }
    }
    assert!(
        heading_kept_at_a_page_foot,
        "precondition: some filler length puts the heading near a page foot, \
         with its paragraph split onto the next page"
    );
}

#[test]
fn keep_with_next_keeps_the_heading_with_the_first_lines_of_its_paragraph() {
    check(false);
}

#[test]
fn keep_with_next_keeps_the_heading_with_the_first_lines_in_a_sectioned_document() {
    check(true);
}
