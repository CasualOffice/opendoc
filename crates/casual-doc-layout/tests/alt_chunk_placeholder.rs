//! `w:altChunk` placeholder layout footprint (`P1F-28`, bounded slice).
//!
//! The semantic model carries only an opaque part reference for `w:altChunk`
//! (the referenced HTML/RTF/nested-WordprocessingML content is never parsed
//! into blocks), so the flow engine cannot lay out the chunk's actual content.
//! Before this slice, `BlockNode::AltChunk` contributed exactly zero layout
//! space at all three `casual-doc-layout` flow sites (body flow, table-cell
//! flow, intrinsic-width measurement) — silently vanishing from the page.
//!
//! This test drives the real pipeline (`build_galley` + `paginate`) and proves
//! the chunk now reserves a deterministic, non-zero placeholder box instead:
//! a page sized to hold exactly one ordinary paragraph plus one altChunk
//! placeholder overflows a trailing third paragraph onto a second page. If the
//! altChunk still contributed zero height, all three blocks would fit on one
//! page. This is **not** a test of altChunk's real embedded content rendering
//! (out of scope here, tracked separately) — only that its placeholder now
//! claims real space and is visually distinguishable (a bordered box).

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::flow::build_galley;
use casual_doc_layout::paginate::{PageConfig, paginate};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AltChunk, AltChunkProperties, BlockNode, Definitions, Document, EmbeddedPart, GridColumn,
    InlineNode, Paragraph, ParagraphProperties, Run, RunProperties, SectionId, Table, TableCell,
    TableCellProperties, TableProperties, TableRow, TableRowProperties,
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

fn paragraph(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default(),
        inlines: vec![run(id + 1, text)],
    })
}

fn alt_chunk_part() -> EmbeddedPart {
    EmbeddedPart {
        relationship_id: "rId9".to_owned(),
        relationship_type:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/aFChunk".to_owned(),
        part_name: "word/afchunk.mht".to_owned(),
    }
}

fn alt_chunk(id: u64) -> BlockNode {
    BlockNode::AltChunk(AltChunk {
        id: node(id),
        part: alt_chunk_part(),
        properties: AltChunkProperties::default(),
    })
}

fn document(body: Vec<BlockNode>) -> Document {
    Document::new(node(1), body, Definitions::default()).unwrap()
}

/// A page wide enough not to wrap the short test paragraphs, and tall enough to
/// hold exactly `lines` line-heights of content (no header/footer bands).
fn page_config(section: SectionId, content_height: Twip) -> PageConfig {
    PageConfig {
        section,
        page_size: Size::new(Twip::from_points(2000), content_height),
        margin_top: Twip::ZERO,
        margin_bottom: Twip::ZERO,
        margin_start: Twip::ZERO,
        margin_end: Twip::ZERO,
        header_distance: Twip::ZERO,
        footer_distance: Twip::ZERO,
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

const CONTENT_WIDTH: Twip = Twip::from_points(1000);

#[test]
fn alt_chunk_reserves_real_height_in_body_flow() {
    let shaper = ParleyShaper::new();

    // A lone ordinary paragraph's natural height is the yardstick: the
    // placeholder is sized off document defaults exactly like a plain
    // paragraph, so it should occupy about the same height.
    let one_paragraph = document(vec![paragraph(10, "one line")]);
    let galley = build_galley(&one_paragraph, &shaper, CONTENT_WIDTH);
    assert_eq!(galley.len(), 1);
    let line_height = galley[0].height();
    assert!(
        line_height.raw() > 0,
        "a plain paragraph must have positive height"
    );

    // The altChunk placeholder fragment itself: non-zero height, and the
    // dashed placeholder border this slice adds (visually distinguishing it
    // from real authored content).
    let chunk_only = document(vec![alt_chunk(20)]);
    let chunk_galley = build_galley(&chunk_only, &shaper, CONTENT_WIDTH);
    assert_eq!(chunk_galley.len(), 1, "the altChunk flows to one fragment");
    let BlockFragment::Paragraph { lines, decor, .. } = &chunk_galley[0] else {
        panic!("expected the altChunk placeholder to be a Paragraph-shaped fragment");
    };
    assert!(
        !lines.lines.is_empty() && lines.height().raw() > 0,
        "the altChunk placeholder must reserve positive height, not vanish (P1F-28)"
    );
    assert!(
        decor.borders.top.is_some()
            && decor.borders.bottom.is_some()
            && decor.borders.start.is_some()
            && decor.borders.end.is_some(),
        "the placeholder must be visibly bordered, distinguishing it from real content"
    );

    // Pagination proof: a page tall enough for exactly the first paragraph plus
    // the altChunk placeholder (using their *actual* measured heights — the
    // placeholder's glyph can resolve to a different fallback face than plain
    // ASCII text, so its line metrics are not assumed equal to a plain
    // paragraph's) but not the trailing paragraph too. With the fixed bug
    // (altChunk contributing zero height), all three blocks would fit on one
    // page; with a real placeholder height, the third paragraph must spill to
    // a second page.
    let section = SectionId::new(node(900));
    let three_blocks = document(vec![
        paragraph(30, "first paragraph"),
        alt_chunk(40),
        paragraph(50, "third paragraph"),
    ]);
    let galley = build_galley(&three_blocks, &shaper, CONTENT_WIDTH);
    assert_eq!(galley.len(), 3);
    assert!(
        galley[1].height().raw() > 0,
        "the altChunk fragment in a 3-block flow must also reserve positive height"
    );
    let fits_two = galley[0].height().raw() + galley[1].height().raw();
    let half_third = galley[2].height().raw() / 2;
    let config = page_config(section, Twip(fits_two + half_third));
    let layout = paginate(&galley, &config);
    assert_eq!(
        layout.page_count(),
        2,
        "the altChunk's reserved height must push the trailing paragraph to page 2"
    );
    assert_eq!(
        layout.pages[0].placed.len(),
        2,
        "page 1 holds para + altChunk"
    );
    assert_eq!(
        layout.pages[1].placed.len(),
        1,
        "page 2 holds the spilled paragraph"
    );
}

#[test]
fn alt_chunk_reserves_real_height_in_table_cell_flow() {
    let shaper = ParleyShaper::new();
    let cell = TableCell {
        id: node(61),
        properties: TableCellProperties::default(),
        blocks: vec![alt_chunk(62)],
    };
    let table = BlockNode::Table(Table {
        id: node(60),
        grid: vec![GridColumn {
            width_twips: Some(3000),
        }],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(63),
            properties: TableRowProperties::default(),
            cells: vec![cell],
        }],
    });
    let galley = build_galley(&document(vec![table]), &shaper, CONTENT_WIDTH);
    assert_eq!(galley.len(), 1, "the table flows to one row fragment");
    let BlockFragment::TableRow { cells, height, .. } = &galley[0] else {
        panic!("expected a table row fragment");
    };
    assert_eq!(cells.len(), 1);
    assert_eq!(
        cells[0].blocks.len(),
        1,
        "the cell's altChunk flows to its own placeholder fragment"
    );
    assert!(
        cells[0].blocks[0].height().raw() > 0,
        "the altChunk placeholder inside a table cell must reserve positive height"
    );
    assert!(
        height.raw() > 0,
        "the row height must reflect the cell's altChunk placeholder, not collapse to zero"
    );
}

#[test]
fn alt_chunk_contributes_real_intrinsic_width_for_autofit_columns() {
    // An autofit column (no declared grid width) sizes itself from its cells'
    // intrinsic content width. A cell holding only an altChunk previously
    // contributed no width at all (the third flow.rs site under fix); prove the
    // resolved column now reflects the placeholder label's measured width
    // instead of collapsing to (near-)zero.
    let shaper = ParleyShaper::new();
    let cell = TableCell {
        id: node(71),
        properties: TableCellProperties::default(),
        blocks: vec![alt_chunk(72)],
    };
    let table = BlockNode::Table(Table {
        id: node(70),
        grid: vec![GridColumn { width_twips: None }],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(73),
            properties: TableRowProperties::default(),
            cells: vec![cell],
        }],
    });
    let galley = build_galley(&document(vec![table]), &shaper, CONTENT_WIDTH);
    let BlockFragment::TableRow { cells, .. } = &galley[0] else {
        panic!("expected a table row fragment");
    };
    // The autofit solver floors every column at 1 twip regardless, so a bare
    // `> 0` would pass even under the old zero-contribution bug; require a
    // width that could only come from actually measuring the placeholder
    // label's shaped glyphs (a multi-word string is unavoidably hundreds of
    // twips wide in any real font).
    assert!(
        cells[0].width.raw() > 500,
        "an autofit column holding only an altChunk must size from the \
         placeholder's measured intrinsic width ({}), not the solver's \
         1-twip floor for a still-zero-contributing block",
        cells[0].width.raw()
    );
}
