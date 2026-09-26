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

use casual_doc_layout::document_layout::{paginate_document_view, paginate_document_view_cached};
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
    let _ = paginate_document_view_cached(&document, &shaper, &mut cache, &DirtySet::new(), view);
    let edited = build(&mut document);
    let incremental = paginate_document_view_cached(
        &document,
        &shaper,
        &mut cache,
        &DirtySet::complete([edited]),
        view,
    );
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

fn assert_incremental_matches_fresh(document: Document, edit_at: usize, view: ReviewView) {
    let (cache, equal) =
        incremental_and_fresh(|doc| type_into(doc, edit_at, 'z'), document, view);
    assert!(
        equal,
        "an incrementally rebuilt layout must equal a freshly built one"
    );
    assert!(
        cache.reused_retained_galley(),
        "the incremental rebuild must actually have reused the retained galley, \
         or this case proves nothing"
    );
    let _ = cache.rebuilt_last_build();
}

#[test]
fn incremental_matches_the_fresh_path_for_prose() {
    for at in [0usize, 7, 39] {
        assert_incremental_matches_fresh(
            document(prose_body(40), Definitions::default()),
            at,
            ReviewView::Editing,
        );
    }
}

#[test]
fn incremental_matches_the_fresh_path_in_the_markup_view() {
    assert_incremental_matches_fresh(
        document(prose_body(40), Definitions::default()),
        5,
        ReviewView::Markup,
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
    assert_incremental_matches_fresh(document(body, definitions), 1, ReviewView::Editing);
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
    paragraph.inlines.push(InlineNode::NoteReference(NoteReference {
        id: node(802),
        kind: NoteKind::Footnote,
        note,
    }));
    assert_incremental_matches_fresh(document(body, definitions), 2, ReviewView::Editing);
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

/// The complexity guard. Doubling the document must not increase the blocks a
/// keystroke re-derives — the assertion is that the count is **flat**, because
/// the behaviour being replaced re-derived every block and a doubling bound would
/// accept it.
#[test]
fn a_keystroke_re_derives_a_bounded_number_of_blocks() {
    let rebuilt = |paragraphs: usize| {
        let shaper = ParleyShaper::new();
        let mut cache = GalleyCache::new();
        let mut document = document(prose_body(paragraphs), Definitions::default());
        let _ = paginate_document_view_cached(
            &document,
            &shaper,
            &mut cache,
            &DirtySet::new(),
            ReviewView::Editing,
        );
        // Two keystrokes: the first establishes the retained galley through the
        // incremental path, the second is the steady state being measured.
        let mut rebuilt = 0;
        for _ in 0..2 {
            let edited = type_into(&mut document, 0, 'z');
            let _ = paginate_document_view_cached(
                &document,
                &shaper,
                &mut cache,
                &DirtySet::complete([edited]),
                ReviewView::Editing,
            );
            rebuilt = cache.rebuilt_last_build();
        }
        rebuilt
    };
    let small = rebuilt(200);
    let large = rebuilt(400);
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
