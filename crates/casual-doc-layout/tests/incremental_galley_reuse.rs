//! The editor's per-keystroke layout must cost what the EDIT costs, not what the
//! document costs (`docs/107` §4 B1, `109` HF-182) — and it must produce exactly
//! the layout a fresh pagination would.
//!
//! Two things are asserted here, and they are the two halves of the same claim:
//!
//! 1. **Equality.** An incrementally rebuilt layout is `==` a freshly built one,
//!    over the block shapes the reuse path has to survive: plain prose, a table
//!    in the flow, a numbered list, a note reference, and an edit that changes
//!    how many lines a paragraph wraps to.
//! 2. **Complexity.** The blocks the rebuild re-derives do not grow with the
//!    document. The assertion is `flat`, not a ratio and not a millisecond
//!    threshold: a threshold cannot tell a slow constant from a linear walk, and
//!    a doubling bound would pass for the linear behaviour this replaces.

use casual_doc_layout::document_layout::{
    paginate_document_view, paginate_document_view_after_edit, paginate_document_view_cached,
};
use casual_doc_layout::flow::ReviewView;
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, BlockNode, Definitions, Document, GridColumn,
    InlineNode, Note, NoteId, NoteKind, NoteReference, NumberFormat, NumberingInstance,
    NumberingInstanceId, NumberingLevel, NumberingRef, Paragraph, ParagraphProperties, Run,
    RunProperties, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).expect("a non-zero benchmark node id")
}

/// One prose paragraph long enough to wrap, so a keystroke can move a line break.
fn prose(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default().into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(id + 1_000_000),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
    })
}

const LINE: &str = "The quick brown fox jumps over the lazy dog and keeps on running. ";

fn prose_body(paragraphs: usize) -> Vec<BlockNode> {
    (0..paragraphs)
        .map(|i| prose(i as u64 + 1, &LINE.repeat(3)))
        .collect()
}

fn document(body: Vec<BlockNode>, definitions: Definitions) -> Document {
    Document::new(node(9_000_000), body, definitions).expect("a well-formed test document")
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

/// Builds `document` through the cached path twice — the second time with a
/// COMPLETE damage set naming only `edited`, which is what lets the cache move
/// the unchanged blocks' fragments across — and returns the incremental layout
/// beside a freshly built one.
fn incremental_and_fresh(
    build: impl Fn(&mut Document) -> NodeId,
    mut document: Document,
    view: ReviewView,
) -> (GalleyCache, bool) {
    let shaper = ParleyShaper::new();
    let mut cache = GalleyCache::new();
    let previous =
        paginate_document_view_cached(&document, &shaper, &mut cache, &DirtySet::new(), view);
    let edited = build(&mut document);
    let incremental = paginate_document_view_after_edit(
        &document,
        &shaper,
        &mut cache,
        &DirtySet::complete([edited]),
        view,
        Some(previous),
    )
    .layout;
    let fresh = paginate_document_view(&document, &shaper, view);
    let equal = incremental == fresh;
    if !equal {
        assert_eq!(
            incremental.pages.len(),
            fresh.pages.len(),
            "incremental and fresh disagree on the page count"
        );
        for (index, (a, b)) in incremental.pages.iter().zip(fresh.pages.iter()).enumerate() {
            assert_eq!(a, b, "incremental and fresh disagree on page {index}");
        }
    }
    (cache, equal)
}

/// `resumes` is whether this body is one the PAGE reuse is defined for. A body
/// carrying a footnote is not — footnotes paginate through their own paginator —
/// so that case asserts galley reuse only, and says so rather than quietly
/// asserting less.
fn assert_incremental_matches_fresh(
    document: Document,
    edit_at: usize,
    view: ReviewView,
    resumes: bool,
) {
    let (cache, equal) = incremental_and_fresh(|doc| type_into(doc, edit_at, 'z'), document, view);
    assert!(
        equal,
        "an incrementally rebuilt layout must equal a freshly built one"
    );
    assert!(
        cache.reused_retained_galley(),
        "the incremental rebuild must actually have reused the retained galley, \
         or this case proves nothing"
    );
    assert_eq!(
        cache.resumed_previous_layout(),
        resumes,
        "page reuse must be taken exactly where it is defined: expected \
         resumed={resumes}"
    );
}

#[test]
fn incremental_matches_the_fresh_path_for_prose() {
    for at in [0usize, 7, 39] {
        assert_incremental_matches_fresh(
            document(prose_body(40), Definitions::default()),
            at,
            ReviewView::Editing,
            true,
        );
    }
}

#[test]
fn incremental_matches_the_fresh_path_in_the_markup_view() {
    assert_incremental_matches_fresh(
        document(prose_body(40), Definitions::default()),
        5,
        ReviewView::Markup,
        true,
    );
}

#[test]
fn incremental_matches_the_fresh_path_across_a_table() {
    let cell = |id: u64, text: &str| TableCell {
        id: node(id),
        properties: TableCellProperties::default(),
        blocks: vec![prose(id + 1, text)],
    };
    let mut body = prose_body(20);
    body.insert(
        10,
        BlockNode::Table(Box::new(Table {
            id: node(500),
            grid: vec![
                GridColumn {
                    width_twips: Some(4_680),
                },
                GridColumn {
                    width_twips: Some(4_680),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: node(501),
                properties: TableRowProperties::default(),
                cells: vec![cell(510, "left"), cell(520, "right")],
            }],
        })),
    );
    // The edit lands AFTER the table, so the reuse path has to carry a
    // multi-fragment block across and still line the following blocks up.
    assert_incremental_matches_fresh(
        document(body, Definitions::default()),
        15,
        ReviewView::Editing,
        true,
    );
}

#[test]
fn incremental_matches_the_fresh_path_with_a_numbered_list() {
    let mut definitions = Definitions::default();
    let abstract_id = AbstractNumberingId::new(node(700));
    definitions.abstract_numbering.insert(
        abstract_id,
        AbstractNumbering {
            levels: vec![NumberingLevel {
                level: 0,
                start: 1,
                num_fmt: Some(NumberFormat::Decimal),
                lvl_text: Some("%1.".to_owned()),
                lvl_jc: None,
                suff: None,
                is_lgl: false,
                paragraph_properties: None,
                run_properties: None,
                style_ref: None,
                lvl_restart: None,
                pstyle: None,
                template_code: None,
                tentative: false,
            }],
            multi_level_type: None,
            num_style_link: None,
            style_link: None,
        },
    );
    let instance = NumberingInstanceId::new(node(701));
    definitions.numbering.insert(
        instance,
        NumberingInstance {
            abstract_ref: abstract_id,
            overrides: Vec::new(),
        },
    );
    let mut body = prose_body(20);
    for index in [4usize, 5, 6] {
        let BlockNode::Paragraph(paragraph) = &mut body[index] else {
            unreachable!("prose bodies are paragraphs");
        };
        let mut properties = paragraph.properties.get().clone();
        properties.numbering = Some(NumberingRef { instance, level: 0 });
        paragraph.properties = properties.into();
    }
    // The edit lands BEFORE the list, so a wrongly reused numbered paragraph
    // would show a stale marker.
    assert_incremental_matches_fresh(document(body, definitions), 1, ReviewView::Editing, true);
}

#[test]
fn incremental_matches_the_fresh_path_with_a_note_reference() {
    let mut definitions = Definitions::default();
    let note = NoteId::new(node(800));
    definitions.footnotes.insert(
        note,
        Note {
            blocks: vec![prose(801, "the note body")],
        },
    );
    let mut body = prose_body(20);
    let BlockNode::Paragraph(paragraph) = &mut body[8] else {
        unreachable!("prose bodies are paragraphs");
    };
    paragraph
        .inlines
        .push(InlineNode::NoteReference(NoteReference {
            id: node(802),
            kind: NoteKind::Footnote,
            note,
        }));
    // A body footnote paginates through `paginate_section_footnotes`, which is
    // outside the page-resume path — so this case proves the GALLEY reuse only.
    assert_incremental_matches_fresh(document(body, definitions), 2, ReviewView::Editing, false);
}

#[test]
fn incremental_matches_the_fresh_path_when_the_edit_adds_a_line() {
    // A paragraph that grows past its last line changes the page boundaries
    // below it, which is the case a reuse that forgot to re-paginate would pass.
    let (cache, equal) = incremental_and_fresh(
        |doc| {
            let mut body = doc.body().to_vec();
            let BlockNode::Paragraph(paragraph) = &mut body[3] else {
                unreachable!()
            };
            let id = paragraph.id;
            let Some(InlineNode::Run(run)) = paragraph.inlines.first_mut() else {
                unreachable!()
            };
            run.text.push_str(&LINE.repeat(8));
            *doc = Document::new(doc.id(), body, doc.definitions().clone())
                .expect("a longer run keeps the document well formed");
            id
        },
        document(prose_body(40), Definitions::default()),
        ReviewView::Editing,
    );
    assert!(equal, "a paragraph that grew must repaginate identically");
    assert!(cache.reused_retained_galley());
}

/// The work one keystroke costs, at `n` and at `2n` paragraphs: the blocks whose
/// layout was re-derived, and the pages that were re-flowed.
fn keystroke_work(paragraphs: usize, edit_at: usize) -> (usize, usize, usize) {
    let shaper = ParleyShaper::new();
    let mut cache = GalleyCache::new();
    let mut document = document(prose_body(paragraphs), Definitions::default());
    let mut layout = paginate_document_view_cached(
        &document,
        &shaper,
        &mut cache,
        &DirtySet::new(),
        ReviewView::Editing,
    );
    // Three keystrokes: the first establishes the retained galley and the
    // reusable layout, the rest are the steady state being measured.
    for _ in 0..3 {
        let edited = type_into(&mut document, edit_at, 'z');
        layout = paginate_document_view_after_edit(
            &document,
            &shaper,
            &mut cache,
            &DirtySet::complete([edited]),
            ReviewView::Editing,
            Some(layout),
        )
        .layout;
    }
    (
        cache.rebuilt_last_build(),
        cache.pages_reflowed_last_build(),
        layout.pages.len(),
    )
}

/// The complexity guard for the galley half. Doubling the document must not
/// increase the blocks a keystroke re-derives — the assertion is that the count is
/// **flat**, because the behaviour being replaced re-derived every block and a
/// doubling bound would accept it.
#[test]
fn a_keystroke_re_derives_a_bounded_number_of_blocks() {
    let (small, _, small_pages) = keystroke_work(200, 0);
    let (large, _, large_pages) = keystroke_work(400, 0);
    assert!(
        large_pages > small_pages,
        "the two documents must differ in size for this to measure anything: \
         {small_pages} pages and {large_pages}"
    );
    assert_eq!(
        large, small,
        "the blocks a keystroke re-derives must not grow with the document: \
         {small} at 200 paragraphs and {large} at 400"
    );
    assert_eq!(
        small, 1,
        "one keystroke touches one paragraph, so exactly one block is re-derived"
    );
}

/// The complexity guard for the pagination half. An edit at the TOP of the
/// document is the hard case: every page below it is a candidate for re-flow, and
/// re-flowing them is what made a keystroke cost `O(document)` even once the
/// galley stopped being re-derived.
#[test]
fn a_keystroke_re_flows_a_bounded_number_of_pages() {
    let (_, small, small_pages) = keystroke_work(200, 0);
    let (_, large, large_pages) = keystroke_work(400, 0);
    assert!(
        large_pages >= small_pages * 2 - 1,
        "doubling the paragraphs must roughly double the pages, or this guard \
         measures nothing: {small_pages} and {large_pages}"
    );
    assert_eq!(
        large, small,
        "the pages a keystroke re-flows must not grow with the document: \
         {small} of {small_pages} pages at 200 paragraphs and {large} of \
         {large_pages} at 400"
    );
    assert!(
        small <= 3,
        "a keystroke disturbs the page it lands on and its neighbours, not \
         {small} pages"
    );
}

/// The same for an edit at the END of the document, which must reuse the whole
/// prefix rather than re-flowing up to it.
#[test]
fn a_keystroke_at_the_end_re_flows_a_bounded_number_of_pages() {
    let (_, small, _) = keystroke_work(200, 199);
    let (_, large, _) = keystroke_work(400, 399);
    assert_eq!(
        large, small,
        "an edit at the end must cost the same in re-flowed pages whatever the \
         document length: {small} at 200 paragraphs and {large} at 400"
    );
}

/// `changed_pages` is what the host repaints, so a page left out of it must be
/// **byte-identical** to the page that was there before. Under-reporting here is
/// stale pixels on screen, which is worse than a slow repaint.
#[test]
fn the_reported_changed_pages_cover_every_page_that_actually_changed() {
    for (paragraphs, edit_at, grow) in [
        (200usize, 0usize, false),
        (200, 90, false),
        (200, 199, false),
        // A paragraph that grows by eight lines pushes content onto a new page,
        // which renumbers everything after it.
        (200, 5, true),
    ] {
        let shaper = ParleyShaper::new();
        let mut cache = GalleyCache::new();
        let mut document = document(prose_body(paragraphs), Definitions::default());
        let mut layout = paginate_document_view_cached(
            &document,
            &shaper,
            &mut cache,
            &DirtySet::new(),
            ReviewView::Editing,
        );
        // One incremental pass first, so the retained galley exists and the
        // measured pass is the steady state.
        let edited = type_into(&mut document, edit_at, 'z');
        layout = paginate_document_view_after_edit(
            &document,
            &shaper,
            &mut cache,
            &DirtySet::complete([edited]),
            ReviewView::Editing,
            Some(layout),
        )
        .layout;

        let before = layout.clone();
        let edited = if grow {
            let mut body = document.body().to_vec();
            let BlockNode::Paragraph(paragraph) = &mut body[edit_at] else {
                unreachable!()
            };
            let id = paragraph.id;
            let Some(InlineNode::Run(run)) = paragraph.inlines.first_mut() else {
                unreachable!()
            };
            run.text.push_str(&LINE.repeat(8));
            document = Document::new(document.id(), body, document.definitions().clone())
                .expect("a longer run keeps the document well formed");
            id
        } else {
            type_into(&mut document, edit_at, 'q')
        };
        let update = paginate_document_view_after_edit(
            &document,
            &shaper,
            &mut cache,
            &DirtySet::complete([edited]),
            ReviewView::Editing,
            Some(layout),
        );
        let changed = update
            .changed_pages
            .clone()
            .unwrap_or_else(|| panic!("the pass must have resumed for {paragraphs}/{edit_at}"));
        for (index, page) in update.layout.pages.iter().enumerate() {
            if changed.contains(&index) {
                continue;
            }
            assert_eq!(
                Some(page),
                before.pages.get(index),
                "page {index} was reported unchanged for a {paragraphs}-paragraph \
                 document edited at {edit_at} (grow={grow}), but it is not the page \
                 that was there before; reported changed range {changed:?}"
            );
        }
        assert_eq!(
            update.layout,
            paginate_document_view(&document, &shaper, ReviewView::Editing),
            "and the layout itself must still equal a fresh one"
        );
    }
}
