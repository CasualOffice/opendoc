//! A paragraph containing a field flows like a paragraph (`docs/105` OO-005).
//!
//! # The defect these guard
//!
//! Any `FlowItem::Field` used to route the whole paragraph to
//! `shape_fielded_paragraph`, which lays out **one line per hard break** — it was
//! written for headers and footers, which are single-line in Word. That was
//! invisible while fields lived only in running content. A caption is
//! `"Figure "` + a `SEQ` field + the author's sentence, so the moment captions
//! exist the same paragraph runs off the page instead of wrapping, and a table of
//! contents row (`title` + a dot-leader tab + a `PAGEREF`) loses its leader dots,
//! because the fielded path reads `stop.alignment` and `stop.position` and never
//! `stop.leader`.
//!
//! The fix is not a second wrapping implementation: a field the post-pagination
//! field pass does **not** recompute has a cached result that is already final and
//! that *is* the paragraph's model text, so it flows as ordinary inline content
//! through the path that already wraps and already draws leaders. `PAGE` and
//! `NUMPAGES` keep their markers, because their value is recomputed per page and
//! is not model text.
//!
//! Each test below asserts the **guarantee** (this paragraph wraps; these dots are
//! drawn; that number is still recomputable), not the mechanism, so a future
//! rewrite of the field path passes them unchanged.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::flow::build_galley;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::FieldKind;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, Field, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties, Symbol, Tab, TabAlignment, TabLeader, TabStop,
};

/// The body measure of an 8.5 in page with 1 in margins — the shape every real
/// caption wraps inside.
const MEASURE: Twip = Twip(9_360);

/// Long enough that no single line of a default-size font can hold it. The
/// assertions are all "more than one line", never a pinned line count, so a font
/// or shaping change cannot redden them.
const LONG_CAPTION: &str = ": a long descriptive caption that certainly does not fit \
on one single line of an eight and a half inch page with one inch margins on both sides \
of the text column, not even close to fitting";

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).expect("a fresh node id")
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

/// A field carrying `instruction` and one cached-result run.
fn field(id: u64, instruction: &str, cached: &str) -> InlineNode {
    InlineNode::Field(Box::new(Field {
        id: node(id),
        instruction: instruction.to_owned(),
        kind: casual_doc_model::v1::FieldKind::parse(instruction),
        inlines: vec![run(id + 1, cached)],
        form: None,
        update: Default::default(),
    }))
}

fn paragraph(id: u64, properties: ParagraphProperties, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines,
    })
}

fn document(body: Vec<BlockNode>) -> Document {
    Document::new(node(1), body, Definitions::default()).expect("a valid document")
}

/// Every line of every paragraph fragment in the flowed galley.
fn lines(document: &Document) -> Vec<casual_doc_layout::text::Line> {
    let shaper = ParleyShaper::new();
    let galley = build_galley(document, &shaper, MEASURE);
    let mut out = Vec::new();
    for fragment in &galley {
        if let BlockFragment::Paragraph { lines, .. } = fragment {
            out.extend(lines.lines.iter().cloned());
        }
    }
    out
}

#[test]
fn a_long_caption_with_a_sequence_field_wraps_instead_of_running_off_the_page() {
    let captioned = document(vec![paragraph(
        100,
        ParagraphProperties::default(),
        vec![
            run(101, "Figure "),
            field(200, " SEQ Figure \\* ARABIC ", "1"),
            run(103, LONG_CAPTION),
        ],
    )]);
    let fielded = lines(&captioned).len();
    assert!(
        fielded > 1,
        "a caption longer than the measure must wrap; it produced {fielded} line(s)"
    );
    // The guarantee, stated so a font change cannot redden it: a field does not
    // change how a paragraph wraps. The control is the same paragraph with the
    // field's cached result written as literal text — which is exactly what the
    // paragraph's model text already is, so the two must lay out identically.
    let control = document(vec![paragraph(
        100,
        ParagraphProperties::default(),
        vec![run(101, &format!("Figure 1{LONG_CAPTION}"))],
    )]);
    assert_eq!(
        fielded,
        lines(&control).len(),
        "a paragraph with a SEQ field must wrap exactly like the same text without one"
    );
}

#[test]
fn a_cross_reference_in_a_long_sentence_wraps_too() {
    // The other half of OO-005: a `REF` field inside ordinary body prose. Same
    // guarantee, different field keyword, because the routing decision is made
    // per field kind and a per-keyword mistake would pass the caption test.
    let document = document(vec![paragraph(
        100,
        ParagraphProperties::default(),
        vec![
            run(101, "As set out in "),
            field(200, " REF _Ref12345 \\h ", "Figure 1"),
            run(103, LONG_CAPTION),
        ],
    )]);
    assert!(
        lines(&document).len() > 1,
        "a paragraph containing a REF field must wrap like any other"
    );
}

#[test]
fn a_page_number_field_is_still_a_marker_the_field_pass_can_recompute() {
    // The counterpart guarantee: the fix must not flatten the two fields whose
    // value is NOT model text. If it did, every header would paint the placeholder
    // `"1"` on every page — a silent, plausible-looking wrong answer.
    let document = document(vec![paragraph(
        100,
        ParagraphProperties::default(),
        vec![run(101, "Page "), field(200, " PAGE ", "1")],
    )]);
    let markers: Vec<FieldKind> = lines(&document)
        .iter()
        .flat_map(|line| line.fields.iter().map(|field| field.kind))
        .collect();
    assert_eq!(
        markers,
        vec![FieldKind::Page],
        "a PAGE field must still reach pagination as a marker"
    );
}

#[test]
fn a_cached_result_that_is_not_plain_text_is_no_longer_dropped() {
    // The marker's value came from a helper that understood only runs and tabs, so
    // a field whose cached result carried a symbol lost it silently — "no silent
    // data loss" (AGENTS.md) in the one place a reader cannot tell.
    let document = document(vec![paragraph(
        100,
        ParagraphProperties::default(),
        vec![InlineNode::Field(Box::new(Field {
            id: node(200),
            instruction: " REF _Ref1 ".to_owned(),
            kind: casual_doc_model::v1::FieldKind::parse(" REF _Ref1 "),
            inlines: vec![InlineNode::Symbol(Box::new(Symbol {
                id: node(201),
                font: "Symbol".to_owned(),
                char: 0x00A7,
                properties: RunProperties::default().into(),
            }))],
            form: None,
            update: Default::default(),
        }))],
    )]);
    let glyphs: usize = lines(&document)
        .iter()
        .flat_map(|line| line.runs.iter())
        .map(|run| run.glyphs.len())
        .sum();
    assert!(
        glyphs > 0,
        "a field whose cached result is a symbol must still paint it"
    );
}

#[test]
fn a_contents_row_with_a_page_reference_draws_its_dot_leader() {
    // The shape of every table-of-contents row Word writes: the title, a tab to a
    // right-aligned stop with a dot leader, then a PAGEREF field. The fielded path
    // never read `stop.leader`, so the dots were simply absent.
    let properties = ParagraphProperties {
        tabs: vec![TabStop {
            position_twips: MEASURE.raw(),
            alignment: TabAlignment::End,
            leader: Some(TabLeader::Dot),
        }],
        ..ParagraphProperties::default()
    };
    let document = document(vec![paragraph(
        100,
        properties,
        vec![
            run(101, "Typography and character formatting"),
            InlineNode::Tab(Tab { id: node(102) }),
            field(200, " PAGEREF _Toc1 \\h ", "7"),
        ],
    )]);
    let leaders = lines(&document)
        .iter()
        .flat_map(|line| line.runs.iter())
        .filter(|run| run.is_leader)
        .count();
    assert!(
        leaders > 0,
        "a contents row with a dot-leader tab stop must paint leader glyphs"
    );
}

#[test]
fn a_recomputed_field_run_carries_the_faces_metrics_not_zero() {
    // A `PAGE` field's value is synthesized after pagination rather than taken
    // from model text, and its glyph run used to be built with `ascent`/`descent`
    // of zero, meaning "use the line's". The line's are the metrics of the
    // TALLEST run sharing it, which is the exact thing per-run metrics exist to
    // stop: `GlyphRun::ascent` documents that a caret must be as tall as the text
    // at the insertion point, not as tall as its neighbour. It also left the run
    // unmeasurable to anything that reduces a page by run metrics — the oracle
    // geometry comparison dropped every footer carrying a page number for it.
    //
    // The guarantee, not the mechanism: the field's run reports a real vertical
    // extent, and it is the SMALL face's, not the 28pt neighbour's.
    let small = RunProperties {
        size_half_points: Some(16), // 8pt
        ..RunProperties::default()
    };
    let large = RunProperties {
        size_half_points: Some(56), // 28pt
        ..RunProperties::default()
    };
    let document = document(vec![paragraph(
        100,
        ParagraphProperties::default(),
        vec![
            InlineNode::Run(Run {
                id: node(101),
                properties: large.into(),
                text: "Tall ".to_owned(),
            }),
            InlineNode::Field(Box::new(Field {
                id: node(200),
                instruction: " PAGE ".to_owned(),
                kind: casual_doc_model::v1::FieldKind::parse(" PAGE "),
                inlines: vec![InlineNode::Run(Run {
                    id: node(201),
                    properties: small.into(),
                    text: "1".to_owned(),
                })],
                form: None,
                update: Default::default(),
            })),
        ],
    )]);

    let lines = lines(&document);
    let line = lines.first().expect("the paragraph lays out one line");
    let field_run = line
        .runs
        .iter()
        .find(|run| run.size == Twip::from_points(8))
        .expect("the 8pt field value is a run of its own");
    assert!(
        field_run.ascent > Twip::ZERO && field_run.descent > Twip::ZERO,
        "a recomputed field's run must carry its own face metrics, got \
         ascent {:?} descent {:?}",
        field_run.ascent,
        field_run.descent
    );
    assert!(
        field_run.ascent < line.ascent,
        "the 8pt field must report ITS ascent ({:?}), not the 28pt line's ({:?})",
        field_run.ascent,
        line.ascent
    );
}
