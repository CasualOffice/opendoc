//! Watermarks end to end through the real driver (`109` OO-006).
//!
//! The row this closes said no watermark exists, and it was right in the way that
//! matters: of sixteen mentions of the word across the crates, none was code. So
//! these assertions are made on what a reader of the page sees — that the stamp
//! reaches the paint list, that it is behind everything else, that it turns as one
//! object, and that it appears on every page rather than only the first.
//!
//! Asserting the pass's internals would miss the whole defect class here. A
//! watermark that resolves correctly and composes into nothing is invisible, and
//! invisible is exactly what this feature was.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::PlacedWatermarkContent;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Rgba, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
    Watermark, WatermarkContent, WatermarkLayout, WatermarkText,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn paragraph(id: u64, text: &str, break_before: bool) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            page_break_before: break_before,
            ..ParagraphProperties::default()
        }
        .into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(id + 1),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
    })
}

/// A US-Letter section with 1-inch margins carrying `watermark`.
fn section(id: u64, watermark: Option<Watermark>) -> SectionBoundary {
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
        watermark,
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

fn draft(layout: WatermarkLayout, semi_transparent: bool) -> Watermark {
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
        layout,
        semi_transparent,
    }
}

/// Three pages in one section, so "every page" is a real claim.
fn document(watermark: Option<Watermark>) -> Document {
    let mut definitions = Definitions::default();
    definitions.sections.push(section(900, watermark));
    Document::new(
        node(1000),
        vec![
            paragraph(1, "page one", false),
            paragraph(3, "page two", true),
            paragraph(5, "page three", true),
        ],
        definitions,
    )
    .expect("valid document")
}

fn paginate(document: &Document) -> casual_doc_layout::page::PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

#[test]
fn a_document_without_a_watermark_has_no_stamp_on_any_page() {
    let doc = document(None);
    let layout = paginate(&doc);
    assert_eq!(layout.pages.len(), 3);
    for page in &layout.pages {
        assert!(page.watermark.is_none());
    }
}

#[test]
fn the_watermark_is_stamped_on_every_page_of_the_section() {
    let doc = document(Some(draft(WatermarkLayout::Diagonal, true)));
    let layout = paginate(&doc);
    assert_eq!(layout.pages.len(), 3, "three pages to stamp");
    for (index, page) in layout.pages.iter().enumerate() {
        let stamp = page
            .watermark
            .as_ref()
            .unwrap_or_else(|| panic!("page {} carries no watermark", index + 1));
        let PlacedWatermarkContent::Text { runs } = &stamp.content else {
            panic!("a text watermark resolves to text");
        };
        assert!(
            runs.iter().any(|run| !run.glyphs.is_empty()),
            "page {} stamped no glyphs",
            index + 1
        );
    }
}

#[test]
fn the_stamp_reaches_the_paint_list_behind_everything_else() {
    let doc = document(Some(draft(WatermarkLayout::Diagonal, true)));
    let layout = paginate(&doc);
    let list = compose_page(&layout.pages[0]);

    // Behind EVERYTHING: the bracket opens at index 0. A watermark that painted
    // after the body would be a highlighter over the text, which is the one thing
    // it must never be.
    assert!(
        matches!(list.items.first(), Some(PaintItem::PushTransform(_))),
        "the diagonal stamp opens the page's paint list, got {:?}",
        list.items.first()
    );
    let close = list
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::PopTransform))
        .expect("the bracket closes");
    assert!(
        list.items[1..close]
            .iter()
            .any(|item| matches!(item, PaintItem::Glyphs { .. })),
        "the stamp's glyphs are inside its own transform bracket"
    );

    // And the body is outside it, so the rotation applies to the stamp alone.
    assert!(
        list.items[close..]
            .iter()
            .any(|item| matches!(item, PaintItem::Glyphs { .. })),
        "the body text paints after the bracket closes"
    );
}

#[test]
fn a_horizontal_watermark_carries_no_rotation() {
    let doc = document(Some(draft(WatermarkLayout::Horizontal, true)));
    let layout = paginate(&doc);
    assert!(
        layout.pages[0]
            .watermark
            .as_ref()
            .expect("a stamp")
            .transform
            .is_none(),
        "level means no transform at all, not a zero-degree one"
    );
    let list = compose_page(&layout.pages[0]);
    assert!(
        !list
            .items
            .iter()
            .any(|item| matches!(item, PaintItem::PushTransform(_))),
        "and so it emits no bracket"
    );
    // It still paints: the stamp's glyphs lead the list.
    assert!(matches!(list.items.first(), Some(PaintItem::Glyphs { .. })));
}

#[test]
fn diagonal_rotates_about_the_page_centre_by_words_own_angle() {
    let doc = document(Some(draft(WatermarkLayout::Diagonal, false)));
    let layout = paginate(&doc);
    let transform = layout.pages[0]
        .watermark
        .as_ref()
        .expect("a stamp")
        .transform
        .expect("a diagonal stamp rotates");
    // Word writes `rotation:315` on the shape; 60,000ths of a degree here.
    assert_eq!(transform.rotation, 315 * 60_000);
    assert!(!transform.flip_h && !transform.flip_v);
    // The centre is the PAGE's centre, not the content area's: a stamp that moved
    // with a binding gutter would not read as a stamp.
    assert_eq!(transform.center.x.raw(), 12_240 / 2);
    assert_eq!(transform.center.y.raw(), 15_840 / 2);
}

#[test]
fn semitransparent_halves_the_ink_and_opaque_leaves_it_alone() {
    let faint = paginate(&document(Some(draft(WatermarkLayout::Diagonal, true))));
    let solid = paginate(&document(Some(draft(WatermarkLayout::Diagonal, false))));

    let alpha = |layout: &casual_doc_layout::page::PaginatedLayout| {
        let PlacedWatermarkContent::Text { runs } =
            &layout.pages[0].watermark.as_ref().expect("a stamp").content
        else {
            panic!("text");
        };
        runs.first().expect("a run").color[3]
    };
    assert_eq!(alpha(&solid), 255, "an opaque stamp keeps its own alpha");
    assert_eq!(alpha(&faint), 128, "Word's `<v:fill opacity=\".5\"/>`");
}

#[test]
fn an_auto_sized_stamp_fills_the_page_without_leaving_it() {
    let doc = document(Some(draft(WatermarkLayout::Diagonal, true)));
    let layout = paginate(&doc);
    let PlacedWatermarkContent::Text { runs } =
        &layout.pages[0].watermark.as_ref().expect("a stamp").content
    else {
        panic!("text");
    };
    let advance: i32 = runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| glyph.advance.raw())
        .sum();

    // Auto fits 85% of the span the stamp runs along — the page diagonal here.
    // Asserted as a band rather than an exact number because the advance depends
    // on the bundled face's metrics, but the band is tight enough to fail if the
    // fit were against the page WIDTH (12,240) or not applied at all.
    let diagonal = ((12_240f32).hypot(15_840f32)) as i32;
    let target = (diagonal as f32 * 0.85) as i32;
    assert!(
        (advance - target).abs() < target / 20,
        "auto-fit advance {advance} is not within 5% of {target} (page diagonal {diagonal})"
    );
    assert!(
        advance < diagonal,
        "an auto-sized stamp must not be longer than the diagonal it lies on"
    );
}

#[test]
fn an_explicitly_sized_stamp_is_not_rescaled() {
    let mut watermark = draft(WatermarkLayout::Horizontal, false);
    let WatermarkContent::Text(text) = &mut watermark.content else {
        panic!("text");
    };
    // 12pt — far too small to fill the page, which is the point: an explicit size
    // is the author's instruction and auto-fit must not overrule it.
    text.size_half_points = Some(24);

    let layout = paginate(&document(Some(watermark)));
    let PlacedWatermarkContent::Text { runs } =
        &layout.pages[0].watermark.as_ref().expect("a stamp").content
    else {
        panic!("text");
    };
    let run = runs.first().expect("a run");
    assert_eq!(run.size.raw(), 240, "12pt in twips, as authored");
    let advance: i32 = runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .map(|g| g.advance.raw())
        .sum();
    assert!(
        advance < 12_240 / 2,
        "a 12pt stamp stays small ({advance} twips) rather than being fitted to the page"
    );
}

#[test]
fn the_pass_is_idempotent_so_repagination_matches_pagination() {
    // The property that keeps `repaginate == paginate` for every other
    // post-pagination pass, asserted here too: the stamp is a pure function of the
    // page list and the document.
    let doc = document(Some(draft(WatermarkLayout::Diagonal, true)));
    let first = paginate(&doc);
    let second = paginate(&doc);
    assert_eq!(first.pages[0].watermark, second.pages[0].watermark);
}
