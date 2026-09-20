//! The offset hit-testing returns is the offset the caret is painted at — over
//! tab-indented text, end to end (package → import → pagination → `hit_test`).
//!
//! This guards the worst shape a caret defect can take, and the one the owner
//! hit on the last page of a real loan agreement. The document's footer cell
//! holds `\t\t\tThe Voice of the Tax Agent community. Since 1992`. Clicking
//! between the `h` and the `e` of `the` drew the caret **exactly there** and
//! then inserted the typed character three bytes earlier, just after the `f` of
//! `of`. The editor showed one thing and did another.
//!
//! The mechanism is byte accounting, not geometry. A `w:tab` is one byte (`\t`)
//! of the paragraph's model text — `flow::node_plain_text` emits it and
//! `casual-doc-edit`'s `inline_text_len` counts it — but the tab layer threaded
//! its caret byte cursor across tabs as if they were zero-width. Every glyph
//! after a tab therefore carried a cluster one byte short per preceding tab.
//! Because `caret_rect` paints at the very stop `hit_test` resolved to, both
//! agreed on the wrong answer and the caret looked correct.
//!
//! So a geometric round trip alone cannot see this: `caret_rect(hit_test(p))`
//! lands back on `p` whether or not the offset means anything. The assertions
//! here are anchored to `node_plain_text` — the byte space the edit layer
//! actually inserts into — which is the only thing that can tell the two apart:
//!
//! > For every caret offset in the paragraph's model text, painting the caret
//! > at that offset and clicking a hair to its right resolves back to that same
//! > offset.
//!
//! plus the invariant underneath it: a paragraph's laid-out lines span exactly
//! its model text, no shorter.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::hittest::LayoutSnapshot;
use casual_doc_layout::model::ModelPos;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::Line;
use casual_doc_layout::units::Point;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::InlineNode;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;

const TAB_HIT_OFFSETS_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/tab-hit-offsets.docx");

fn imported() -> Document {
    let mut package = DocxPackage::open(TAB_HIT_OFFSETS_DOCX, PackageLimits::default())
        .expect("the tab-hit-offsets fixture opens");
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the tab-hit-offsets fixture imports")
    .document
}

/// Every paragraph in the document, paired with its model text — the byte space
/// `casual-doc-edit` addresses and the one a caret offset must name.
fn paragraphs(document: &Document) -> Vec<(NodeId, String)> {
    fn walk(blocks: &[BlockNode], out: &mut Vec<(NodeId, String)>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    out.push((paragraph.id, node_plain_text(&paragraph.inlines)));
                    for inline in &paragraph.inlines {
                        if let InlineNode::TextBox(text_box) = inline {
                            walk(&text_box.blocks, out);
                        }
                    }
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, out),
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(document.body(), &mut out);
    out
}

/// Every laid-out line belonging to `node`, in flow order.
fn lines_of(pages: &PaginatedLayout, node: NodeId) -> Vec<&Line> {
    fn collect<'a>(
        fragment: &'a casual_doc_layout::block::BlockFragment,
        node: NodeId,
        out: &mut Vec<&'a Line>,
    ) {
        match fragment {
            casual_doc_layout::block::BlockFragment::Paragraph { lines, .. } => {
                out.extend(
                    lines
                        .lines
                        .iter()
                        .filter(|line| line.range.start.node == node),
                );
            }
            casual_doc_layout::block::BlockFragment::TableRow { cells, .. } => {
                for cell in cells {
                    for block in &cell.blocks {
                        collect(block, node, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for page in &pages.pages {
        for placed in &page.placed {
            collect(&placed.fragment, node, &mut out);
        }
    }
    out
}

fn layout(document: &Document) -> PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

/// The paragraph whose model text contains `needle`, with that text.
fn paragraph_with<'a>(paragraphs: &'a [(NodeId, String)], needle: &str) -> (NodeId, &'a str) {
    let (node, text) = paragraphs
        .iter()
        .find(|(_, text)| text.contains(needle))
        .unwrap_or_else(|| panic!("the fixture has a paragraph containing {needle:?}"));
    (*node, text.as_str())
}

/// A paragraph's laid-out lines must cover exactly its model text: the first
/// line starts at byte 0 and the last ends at the text's length.
///
/// This is the invariant the tab defect broke. It is checked for EVERY paragraph
/// in the fixture rather than the one under test, because the failure mode is a
/// byte-accounting class — any inline kind the layout advances its cursor past
/// by the wrong number of bytes lands here.
#[test]
fn every_paragraphs_lines_span_exactly_its_model_text() {
    let document = imported();
    let pages = layout(&document);
    for (node, text) in paragraphs(&document) {
        let lines = lines_of(&pages, node);
        assert!(
            !lines.is_empty(),
            "paragraph {node:?} ({text:?}) contributes at least one line"
        );
        let start = lines
            .iter()
            .map(|line| line.range.start.offset)
            .min()
            .expect("a line exists");
        let end = lines
            .iter()
            .map(|line| line.range.end.offset)
            .max()
            .expect("a line exists");
        assert_eq!(
            start, 0,
            "paragraph {node:?} ({text:?}) starts at byte 0 of its model text"
        );
        assert_eq!(
            end,
            text.len() as u32,
            "paragraph {node:?} ({text:?}) ends at its model text length; a shorter \
             span means a caret offset drifts by the difference"
        );
    }
}

/// The property the owner's report names: the offset hit-testing returns is the
/// offset the caret is painted at.
///
/// For every caret offset over `range`, the caret is painted, and a click one
/// twip to the right of where it was painted must resolve back to that offset.
/// One twip is far inside any glyph (the fixture's 9pt text advances ~50-130
/// twips per glyph), so the nearest stop is unambiguous.
fn assert_offsets_round_trip(
    snapshot: &LayoutSnapshot<'_>,
    node: NodeId,
    text: &str,
    range: std::ops::Range<usize>,
) {
    let mut checked = 0usize;
    for offset in range.clone() {
        if !text.is_char_boundary(offset) {
            continue;
        }
        let pos = ModelPos::new(node, offset as u32);
        let (page, rect) = snapshot.caret_rect(pos).unwrap_or_else(|| {
            panic!(
                "offset {offset} ({:?} | {:?}) has a caret",
                &text[..offset],
                &text[offset..]
            )
        });
        let probe = Point::new(
            rect.origin.x + Twip(1),
            rect.origin.y + Twip(rect.size.height.raw() / 2),
        );
        let hit = snapshot
            .hit_test(page, probe)
            .expect("the caret's own page resolves a hit");
        assert_eq!(
            (hit.pos.node, hit.pos.offset),
            (node, offset as u32),
            "clicking where the caret for offset {offset} was painted ({:?} | {:?}) \
             must insert at offset {offset}, not at {} ({:?} | {:?})",
            &text[..offset],
            &text[offset..],
            hit.pos.offset,
            text.get(..hit.pos.offset as usize)
                .unwrap_or("<out of range>"),
            text.get(hit.pos.offset as usize..)
                .unwrap_or("<out of range>"),
        );
        checked += 1;
    }
    assert!(
        checked > 10,
        "the sweep covered {checked} offsets over {range:?} — too few to mean anything"
    );
}

/// Three leading tabs before ordinary body text: the owner's exact shape.
#[test]
fn a_click_after_three_leading_tabs_inserts_where_the_caret_is_drawn() {
    let document = imported();
    let pages = layout(&document);
    let snapshot = LayoutSnapshot::new(&pages);
    let all = paragraphs(&document);
    let (node, text) = paragraph_with(&all, "\t\t\tThe Voice of the Tax Agent community");

    // The caret slot between the `h` and the `e` of `the` — the one the owner
    // clicked. Before the fix this resolved three bytes earlier, just after the
    // `f` of `of`.
    let between_h_and_e = text.find("of the").expect("the phrase is present") + "of th".len();
    assert_eq!(&text[between_h_and_e..between_h_and_e + 1], "e");

    // ANCHOR, and the reason this test can fail at all.
    //
    // A geometric round trip — paint the caret for an offset, click there, get
    // the offset back — is satisfied by ANY self-consistent mapping, including
    // a uniformly shifted one. That is precisely what the defect was, and why
    // the caret looked right. So the sweep below is pinned to one position the
    // layout cannot derive from its own clusters: the paragraph's first text
    // byte is painted at the tab column the DOCUMENT authors, which is the
    // third default tab stop (3 x 720 twips) from the page's left margin (600
    // twips, `w:pgMar` in the fixture). With the tab bytes dropped, offset 3 is
    // the fourth painted glyph instead of the first and this moves right by the
    // width of `The`.
    const PAGE_LEFT_MARGIN: i32 = 600;
    const DEFAULT_TAB_STOP: i32 = 720;
    let leading_tabs = text.matches('\t').count() as i32;
    let tab_column = PAGE_LEFT_MARGIN + leading_tabs * DEFAULT_TAB_STOP;
    let first_text = text.rfind('\t').expect("the paragraph is tab indented") + 1;
    let (_, first_rect) = snapshot
        .caret_rect(ModelPos::new(node, first_text as u32))
        .expect("the first text byte has a caret");
    assert!(
        (first_rect.origin.x.raw() - tab_column).abs() <= 2,
        "the caret for the paragraph's first text byte ({first_text}) must be painted at \
         the third tab stop ({tab_column} twips), not at {} — an offset that is not \
         painted where its character is means the caret and the insertion point have \
         come apart",
        first_rect.origin.x.raw()
    );

    let pos = ModelPos::new(node, between_h_and_e as u32);
    let (page, rect) = snapshot
        .caret_rect(pos)
        .expect("the caret between `th` and `e` is painted");
    let hit = snapshot
        .hit_test(
            page,
            Point::new(
                rect.origin.x + Twip(1),
                rect.origin.y + Twip(rect.size.height.raw() / 2),
            ),
        )
        .expect("the click resolves");
    assert_eq!(
        (hit.pos.node, hit.pos.offset),
        (node, between_h_and_e as u32),
        "typing where the caret was drawn must land between the `h` and the `e`, \
         but it lands at {:?}|{:?}",
        text.get(..hit.pos.offset as usize)
            .unwrap_or("<out of range>"),
        text.get(hit.pos.offset as usize..)
            .unwrap_or("<out of range>"),
    );

    // And not just that one slot: every caret position in the text after the
    // tabs round-trips.
    assert_offsets_round_trip(&snapshot, node, text, first_text..text.len());
}

/// A tab between two runs: the drift starts in the middle of a line rather than
/// only at its start, so a fix that special-cased leading indentation would not
/// answer it.
#[test]
fn a_click_after_a_mid_line_tab_inserts_where_the_caret_is_drawn() {
    let document = imported();
    let pages = layout(&document);
    let snapshot = LayoutSnapshot::new(&pages);
    let all = paragraphs(&document);
    let (node, text) = paragraph_with(&all, "Name\tThe Voice of the Tax Agent");
    let after_tab = text.find('\t').expect("the paragraph has a tab") + 1;

    // The same cluster-independent anchor as the leading-tab case: `Name` is
    // shorter than one default tab stop, so the text after the tab starts at
    // the first stop (720 twips) from the page's left margin (600).
    let (_, after_rect) = snapshot
        .caret_rect(ModelPos::new(node, after_tab as u32))
        .expect("the byte after the tab has a caret");
    assert!(
        (after_rect.origin.x.raw() - (600 + 720)).abs() <= 2,
        "the caret for the first byte after the tab ({after_tab}) must be painted at the \
         first tab stop (1320 twips page-local), not at {}",
        after_rect.origin.x.raw()
    );

    assert_offsets_round_trip(&snapshot, node, text, after_tab..text.len());
}

/// A tab in a paragraph that also carries a `PAGE` field.
///
/// A fielded paragraph is assembled by an entirely separate routine
/// (`flow::layout_fielded_line`) with its **own** byte cursor across the tab
/// boundaries, and it carried the identical defect. A fixture without a field
/// never reaches it, so fixing only the tab layer would have left half the
/// documents in the corpus wrong — every one with a page number beside a tab.
#[test]
fn a_click_after_a_tab_in_a_fielded_paragraph_inserts_where_the_caret_is_drawn() {
    let document = imported();
    let pages = layout(&document);
    let snapshot = LayoutSnapshot::new(&pages);
    let all = paragraphs(&document);
    let (node, text) = paragraph_with(&all, "Ref\tThe Voice of the Tax Agent");
    let after_tab = text.find('\t').expect("the paragraph has a tab") + 1;

    // `Ref` is narrower than one default tab stop, so the text after the tab
    // starts at the first stop (720 twips) from the left margin (600).
    let (_, after_rect) = snapshot
        .caret_rect(ModelPos::new(node, after_tab as u32))
        .expect("the byte after the tab has a caret");
    assert!(
        (after_rect.origin.x.raw() - (600 + 720)).abs() <= 2,
        "the caret for the first byte after the tab ({after_tab}) must be painted at the \
         first tab stop (1320 twips page-local), not at {}",
        after_rect.origin.x.raw()
    );

    assert_offsets_round_trip(&snapshot, node, text, after_tab..text.len());
}

/// The same shape inside a narrow table cell, where the tab-indented text also
/// soft-wraps: the second visual line is reached through the wrap repair as
/// well as the tab cursor, and both have to agree on the byte space.
#[test]
fn a_click_on_a_wrapped_tab_indented_cell_line_inserts_where_the_caret_is_drawn() {
    let document = imported();
    let pages = layout(&document);
    let snapshot = LayoutSnapshot::new(&pages);
    let all = paragraphs(&document);
    let (node, text) = paragraph_with(&all, "and for many years after that");
    let lines = lines_of(&pages, node);
    assert!(
        lines.len() >= 2,
        "the cell paragraph soft-wraps (got {} line(s)); the fixture is meant to \
         exercise the wrap repair as well as the tab cursor",
        lines.len()
    );

    // ANCHOR, cluster-independent: a soft wrap happens at a SPACE, so every
    // visual line after the first must start immediately after whitespace in
    // the model text. (Every word in the fixture's cell is far narrower than
    // the cell, so no line can begin mid-word for want of a break opportunity.)
    // A line range shifted by the dropped tab bytes lands mid-word here, which
    // is what makes the sweep below able to fail — the sweep itself is
    // satisfied by any self-consistent mapping, shifted or not.
    for line in lines.iter().skip(1) {
        let start = line.range.start.offset as usize;
        assert!(
            text.is_char_boundary(start),
            "a line starts on a character boundary, got {start} in {text:?}"
        );
        assert!(
            text[..start].ends_with(char::is_whitespace),
            "the wrapped line starting at offset {start} must begin just after a space, \
             but the model text breaks it mid-word: {:?} | {:?}",
            &text[..start],
            &text[start..]
        );
    }

    // Sweep each visual line's interior. Two offsets are excluded, both for
    // stated reasons rather than to make the sweep pass:
    //
    //  * the offset that ENDS a soft-wrapped line also STARTS the next one, and
    //    the caret is deliberately painted at the start of the following line
    //    there — the one offset whose paint position is not on this line; and
    //  * the offsets held by the leading tabs themselves. `stops_for` builds
    //    caret slots from GLYPHS, and a tab has none, so a tab's own offset has
    //    no slot to be painted at. That is a real defect and it is recorded as
    //    one on `stops_for`; it is not the byte-accounting defect this file
    //    guards, and asserting it here would pin the broken behaviour.
    let first_text = text.rfind('\t').map_or(0, |index| index + 1);
    for line in &lines {
        let from = (line.range.start.offset as usize).max(first_text);
        let to = (line.range.end.offset as usize).min(text.len());
        let interior = from..to.saturating_sub(1).max(from);
        if interior.len() < 12 {
            continue;
        }
        assert_offsets_round_trip(&snapshot, node, text, interior);
    }
}
