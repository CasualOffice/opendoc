//! Margin line numbering (`w:lnNumType`) end to end through the real driver
//! (`docs/105` FID-L-09).
//!
//! FID-L-09 is a "modeled but never consumed" row (FID-P-04): `LineNumbering` has
//! been on `SectionBoundary` since P1F-36 and every layout reference to it was
//! `Default::default()` in test scaffolding. So these assertions are made on what
//! a reader of the page sees — which numbers appear, where they sit, and that
//! they reach the paint list — not on the pass's internals.
//!
//! Legal pleadings are the driving case: many jurisdictions require numbered
//! lines, `countBy` selects the step, `restart` decides whether the count runs
//! through the filing or starts over each page, and `w:suppressLineNumbers` takes
//! the caption block out of the count so the numbering below it does not shift.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::{Page, PaginatedLayout};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, LineNumberRestart, LineNumbering, PageMargins,
    PageSize, Paragraph, ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns,
    SectionId, Style, StyleId, StyleKind,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn paragraph_with(id: u64, text: &str, properties: ParagraphProperties) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties,
        inlines: vec![InlineNode::Run(Run {
            id: node(id + 1),
            properties: RunProperties::default(),
            text: text.to_owned(),
        })],
    })
}

fn paragraph(id: u64, text: &str) -> BlockNode {
    paragraph_with(id, text, ParagraphProperties::default())
}

/// A paragraph that forces a page break, so page boundaries are exact and
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

/// A paragraph ending section `section` (it carries that section's `w:sectPr`).
fn section_break(id: u64, text: &str, section: SectionId) -> BlockNode {
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

/// A US-Letter section with 1-inch margins carrying `line_numbering`.
fn section(id: u64, line_numbering: LineNumbering) -> SectionBoundary {
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
        line_numbering,
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

/// Numbers every line, starting at 1, restarting per `restart`.
fn every_line(restart: LineNumberRestart) -> LineNumbering {
    LineNumbering {
        count_by: Some(1),
        start: Some(1),
        distance: None,
        restart: Some(restart),
    }
}

fn document(body: Vec<BlockNode>, sections: Vec<SectionBoundary>) -> Document {
    Document::new(
        node(1),
        body,
        Definitions {
            sections,
            ..Definitions::default()
        },
    )
    .unwrap()
}

fn paginate(document: &Document) -> PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

/// The numbers stamped on a page, in flow order.
fn numbers(page: &Page) -> Vec<u32> {
    page.line_numbers.iter().map(|stamp| stamp.number).collect()
}

// ---------------------------------------------------------------------------
// The construct exists at all
// ---------------------------------------------------------------------------

#[test]
fn a_section_that_declares_line_numbering_stamps_every_line_in_the_margin() {
    let layout = paginate(&document(
        vec![paragraph(100, "First"), paragraph(200, "Second")],
        vec![section(9, every_line(LineNumberRestart::Continuous))],
    ));
    let page = &layout.pages[0];
    assert_eq!(
        numbers(page),
        vec![1, 2],
        "two one-line paragraphs are lines 1 and 2"
    );

    // Each number sits to the LEFT of the text column (in the margin), on its
    // line's baseline, and is right-aligned to the 0.25-inch auto gap — so its
    // right edge is at the column edge minus 360 twips.
    let column_left = page.placed[0].rect.origin.x;
    for (stamp, placed) in page.line_numbers.iter().zip(&page.placed) {
        let advance: i32 = stamp.run.glyphs.iter().map(|g| g.advance.raw()).sum();
        assert!(
            stamp.run.origin.x.raw() < column_left.raw(),
            "line {} is painted inside the text column, not in the margin",
            stamp.number
        );
        assert_eq!(
            stamp.run.origin.x.raw() + advance,
            column_left.raw() - 360,
            "numbers are right-aligned to the auto (0.25in) gap"
        );
        assert!(
            !stamp.run.glyphs.is_empty(),
            "line {} shaped to no glyphs — an invisible number is not a number",
            stamp.number
        );
        // The baseline is inside the numbered paragraph's own band.
        let baseline = stamp.run.origin.y.raw();
        assert!(
            baseline >= placed.rect.origin.y.raw()
                && baseline <= placed.rect.origin.y.raw() + placed.rect.size.height.raw(),
            "line {} is stamped outside its paragraph's band",
            stamp.number
        );
    }
}

#[test]
fn a_document_without_line_numbering_stamps_nothing() {
    let layout = paginate(&document(
        vec![paragraph(100, "First"), paragraph(200, "Second")],
        vec![section(9, LineNumbering::default())],
    ));
    assert!(
        layout.pages.iter().all(|page| page.line_numbers.is_empty()),
        "line numbering must be inert unless the section declares it"
    );
}

#[test]
fn the_stamped_numbers_reach_the_paint_list() {
    // The pass and the paint arm are separate failures: numbers that exist on the
    // page but are never composed are still invisible to the user.
    let layout = paginate(&document(
        vec![paragraph(100, "First")],
        vec![section(9, every_line(LineNumberRestart::Continuous))],
    ));
    let page = &layout.pages[0];
    let stamped = page.line_numbers[0].run.origin;
    let composed = compose_page(page).items.into_iter().any(|item| match item {
        PaintItem::Glyphs { run } => run.origin == stamped && !run.glyphs.is_empty(),
        _ => false,
    });
    assert!(composed, "the line number is not in the display list");
}

// ---------------------------------------------------------------------------
// countBy / start
// ---------------------------------------------------------------------------

#[test]
fn count_by_shows_only_every_nth_line_while_still_counting_the_others() {
    let body = (0..7)
        .map(|i| paragraph(100 + i * 10, "Line"))
        .collect::<Vec<_>>();
    let layout = paginate(&document(
        body,
        vec![section(
            9,
            LineNumbering {
                count_by: Some(3),
                start: Some(1),
                distance: None,
                restart: Some(LineNumberRestart::Continuous),
            },
        )],
    ));
    assert_eq!(
        numbers(&layout.pages[0]),
        vec![3, 6],
        "`countBy=3` labels the 3rd and 6th of seven lines and no others"
    );
}

#[test]
fn start_moves_the_first_lines_number() {
    let layout = paginate(&document(
        vec![paragraph(100, "First"), paragraph(200, "Second")],
        vec![section(
            9,
            LineNumbering {
                count_by: Some(1),
                start: Some(17),
                distance: None,
                restart: Some(LineNumberRestart::Continuous),
            },
        )],
    ));
    assert_eq!(numbers(&layout.pages[0]), vec![17, 18]);
}

#[test]
fn an_explicit_distance_overrides_the_auto_gap() {
    let layout = paginate(&document(
        vec![paragraph(100, "First")],
        vec![section(
            9,
            LineNumbering {
                count_by: Some(1),
                start: Some(1),
                distance: Some(720),
                restart: Some(LineNumberRestart::Continuous),
            },
        )],
    ));
    let page = &layout.pages[0];
    let stamp = &page.line_numbers[0];
    let advance: i32 = stamp.run.glyphs.iter().map(|g| g.advance.raw()).sum();
    assert_eq!(
        stamp.run.origin.x.raw() + advance,
        page.placed[0].rect.origin.x.raw() - 720,
        "a declared `w:distance` must be honoured, not the 0.25in auto default"
    );
}

// ---------------------------------------------------------------------------
// restart
// ---------------------------------------------------------------------------

/// Three pages in one section, one line each.
fn three_page_document(rule: LineNumbering) -> Document {
    document(
        vec![
            paragraph(100, "One"),
            page_break(200, "Two"),
            page_break(300, "Three"),
        ],
        vec![section(9, rule)],
    )
}

#[test]
fn restart_new_page_starts_over_on_every_page() {
    let layout = paginate(&three_page_document(every_line(LineNumberRestart::NewPage)));
    assert_eq!(layout.pages.len(), 3);
    for page in &layout.pages {
        assert_eq!(
            numbers(page),
            vec![1],
            "page {} must restart at the section's start value",
            page.number
        );
    }
}

#[test]
fn restart_continuous_runs_through_the_document() {
    let layout = paginate(&three_page_document(every_line(
        LineNumberRestart::Continuous,
    )));
    assert_eq!(
        layout
            .pages
            .iter()
            .map(numbers)
            .collect::<Vec<_>>()
            .concat(),
        vec![1, 2, 3],
        "a continuous count never restarts"
    );
}

#[test]
fn restart_new_section_starts_over_at_a_section_boundary_but_not_at_a_page_boundary() {
    // Section 9 spans pages 1-2, section 10 opens on page 3.
    let layout = paginate(&document(
        vec![
            paragraph(100, "One"),
            page_break(200, "Two"),
            section_break(300, "Three", SectionId::new(node(9))),
            page_break(400, "Four"),
        ],
        vec![
            section(9, every_line(LineNumberRestart::NewSection)),
            section(10, every_line(LineNumberRestart::NewSection)),
        ],
    ));
    let per_page: Vec<Vec<u32>> = layout.pages.iter().map(numbers).collect();
    assert_eq!(
        per_page.concat(),
        vec![1, 2, 3, 1],
        "the count crosses page boundaries inside a section and restarts at the \
         section break, got {per_page:?}"
    );
}

// ---------------------------------------------------------------------------
// w:suppressLineNumbers
// ---------------------------------------------------------------------------

#[test]
fn a_suppressed_paragraph_is_neither_numbered_nor_counted() {
    let suppressed = ParagraphProperties {
        suppress_line_numbers: true,
        ..ParagraphProperties::default()
    };
    let layout = paginate(&document(
        vec![
            paragraph(100, "Numbered one"),
            paragraph_with(200, "Caption", suppressed),
            paragraph(300, "Numbered two"),
        ],
        vec![section(9, every_line(LineNumberRestart::Continuous))],
    ));
    assert_eq!(
        numbers(&layout.pages[0]),
        vec![1, 2],
        "the suppressed paragraph must drop OUT of the count, not just hide its \
         own number — otherwise every line below it is numbered one too high"
    );
    // And the two numbers belong to the two numbered paragraphs, not to the
    // caption: the second number sits on the third paragraph's band.
    let third = layout.pages[0].placed[2].rect;
    let second_number = layout.pages[0].line_numbers[1].run.origin.y.raw();
    assert!(
        second_number >= third.origin.y.raw()
            && second_number <= third.origin.y.raw() + third.size.height.raw(),
        "line 2 must be stamped beside the third paragraph"
    );
}

#[test]
fn suppress_line_numbers_is_honoured_when_it_comes_from_a_style() {
    // A pleading template puts the flag on its caption style, never on every
    // caption paragraph. The cascade dropped the flag entirely before FID-L-09,
    // so a style-suppressed paragraph was numbered like any other.
    let style_id = StyleId::new(node(50));
    let mut styles = casual_doc_model::v1::DefinitionMap::default();
    styles.insert(
        style_id,
        Style {
            kind: StyleKind::Paragraph,
            is_default: false,
            name: Some("Caption".to_owned()),
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: Some(ParagraphProperties {
                suppress_line_numbers: true,
                ..ParagraphProperties::default()
            }),
            run: None,
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
        },
    );
    let styled = ParagraphProperties {
        style_ref: Some(style_id),
        ..ParagraphProperties::default()
    };
    let document = Document::new(
        node(1),
        vec![
            paragraph(100, "Numbered one"),
            paragraph_with(200, "Caption", styled),
            paragraph(300, "Numbered two"),
        ],
        Definitions {
            sections: vec![section(9, every_line(LineNumberRestart::Continuous))],
            styles,
            ..Definitions::default()
        },
    )
    .unwrap();
    assert_eq!(
        numbers(&paginate(&document).pages[0]),
        vec![1, 2],
        "a style-level `w:suppressLineNumbers` must reach layout through the cascade"
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn the_line_number_pass_is_idempotent_across_repeated_paginations() {
    // The pass runs inside the driver, and the driver may paginate several times
    // (the float fixed point). Two runs over the same document must agree, or the
    // numbers would depend on how many passes the float loop happened to take.
    let build = || paginate(&three_page_document(every_line(LineNumberRestart::NewPage)));
    assert_eq!(build(), build());
}
