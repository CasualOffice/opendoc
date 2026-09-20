//! Footnote and endnote options end to end through the real driver: `w:numFmt`,
//! `w:numStart`, `w:numRestart` and `w:pos` (`docs/105` FID-L-05).
//!
//! FID-L-05 was a "modeled but never consumed" row: `NoteProperties` carried all
//! four fields and `crates/casual-doc-layout/src/notes.rs` read none of them, so
//! every note was decimal, continuous, and page-bottom whatever the document said.
//!
//! The assertions are therefore made on what a reader of the page sees — the
//! *string* printed at the reference and at the head of the note body, where the
//! footnote band starts, and which page the endnote bodies land on — not on the
//! resolution helpers' internals.
//!
//! Reading the printed label needs the shaper: a [`GlyphRun`] carries advances, not
//! characters, so [`Recorder`] captures the text the flow layer hands the shaper,
//! tagged with the paragraph node it belongs to. `eachPage` numbering runs the
//! layout more than once (its numbers depend on the pages they number), so the
//! assertions read the **last** shaping recorded for a node — the one that produced
//! the returned layout.
//!
//! [`GlyphRun`]: casual_doc_layout::text::GlyphRun

use std::cell::RefCell;

use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::model::ModelRange;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{LineConstraints, LineLayout, LineShaper, StyledRun};
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Note, NoteId, NoteKind, NoteNumberMark,
    NoteNumberRestart, NotePosition, NoteProperties, NoteReference, NumberFormat, PageMargins,
    PageSize, Paragraph, ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns,
    SectionId,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn paragraph_with(id: u64, inlines: Vec<InlineNode>, properties: ParagraphProperties) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines,
    })
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn paragraph(id: u64, text: &str) -> BlockNode {
    paragraph_with(id, vec![run(id + 1, text)], ParagraphProperties::default())
}

/// A body paragraph whose last inline references `note`.
fn reference(id: u64, note: NoteId, kind: NoteKind) -> BlockNode {
    paragraph_with(
        id,
        vec![
            run(id + 1, "body"),
            InlineNode::NoteReference(NoteReference {
                id: node(id + 2),
                kind,
                note,
            }),
        ],
        ParagraphProperties::default(),
    )
}

/// A body paragraph referencing `note` that also starts a new page, so page
/// boundaries are exact and independent of shaped line heights.
fn reference_on_a_new_page(id: u64, note: NoteId, kind: NoteKind) -> BlockNode {
    paragraph_with(
        id,
        vec![
            run(id + 1, "body"),
            InlineNode::NoteReference(NoteReference {
                id: node(id + 2),
                kind,
                note,
            }),
        ],
        ParagraphProperties {
            page_break_before: true,
            ..ParagraphProperties::default()
        },
    )
}

/// A note body carrying its own auto-number mark (`w:footnoteRef`/`w:endnoteRef`)
/// ahead of its text, which is what Word's note styles write.
fn note_body(id: u64, kind: NoteKind, text: &str) -> Note {
    Note {
        blocks: vec![paragraph_with(
            id,
            vec![
                InlineNode::NoteNumberMark(NoteNumberMark {
                    id: node(id + 1),
                    kind,
                    properties: RunProperties::default().into(),
                }),
                run(id + 2, text),
            ],
            ParagraphProperties::default(),
        )],
    }
}

/// A paragraph ending section `section`, breaking to a new page.
fn section_break(id: u64, text: &str, section: SectionId) -> BlockNode {
    paragraph_with(
        id,
        vec![run(id + 1, text)],
        ParagraphProperties {
            section_break: Some(section),
            ..ParagraphProperties::default()
        },
    )
}

/// A US-Letter section with 1-inch margins and no note overrides.
fn section(id: u64) -> SectionBoundary {
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
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

fn document(body: Vec<BlockNode>, definitions: Definitions) -> Document {
    Document::new(node(1), body, definitions).unwrap()
}

/// A [`LineShaper`] that records the concatenated run text of every shaped
/// paragraph against the paragraph's node id, delegating real layout to
/// [`ParleyShaper`]. A glyph run carries no characters, so this is how a test reads
/// the *string* a note marker printed.
struct Recorder {
    inner: ParleyShaper,
    shaped: RefCell<Vec<(NodeId, String)>>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            inner: ParleyShaper::new(),
            shaped: RefCell::new(Vec::new()),
        }
    }

    /// The last text shaped for `id` — the pass that produced the returned layout.
    fn last(&self, id: u64) -> String {
        self.shaped
            .borrow()
            .iter()
            .rfind(|(shaped, _)| *shaped == node(id))
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| panic!("paragraph {id} was never shaped"))
    }
}

impl LineShaper for Recorder {
    fn shape_paragraph(
        &self,
        runs: &[StyledRun<'_>],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        self.shaped.borrow_mut().push((
            range.start.node,
            runs.iter().map(|run| run.text.as_ref()).collect(),
        ));
        self.inner.shape_paragraph(runs, constraints, range)
    }
}

fn paginate_recording(document: &Document) -> (PaginatedLayout, Recorder) {
    let recorder = Recorder::new();
    let layout = paginate_document(document, &recorder);
    (layout, recorder)
}

/// A one-section document with `footnote_props` and one footnote reference.
fn footnote_document(props: NoteProperties) -> Document {
    let note = NoteId::new(node(500));
    let mut definitions = Definitions::default();
    definitions
        .footnotes
        .insert(note, note_body(600, NoteKind::Footnote, "note text"));
    let mut boundary = section(400);
    boundary.footnote_props = props;
    definitions.sections.push(boundary);
    document(vec![reference(100, note, NoteKind::Footnote)], definitions)
}

#[test]
fn num_fmt_formats_the_reference_marker_and_the_note_body_number() {
    let doc = footnote_document(NoteProperties {
        number_format: Some(NumberFormat::UpperRoman),
        number_start: Some(4),
        ..NoteProperties::default()
    });

    let (layout, recorder) = paginate_recording(&doc);

    assert_eq!(
        recorder.last(100),
        "bodyIV",
        "the in-body reference marker prints the section's w:numFmt at its w:numStart"
    );
    assert_eq!(
        recorder.last(600),
        "IVnote text",
        "the note body's own auto-number mark prints the same label as the reference"
    );
    assert!(
        !layout.pages[0].footnotes.is_empty(),
        "the footnote band was placed, so the label above is what a reader sees"
    );
}

#[test]
fn the_document_default_note_format_applies_when_the_section_is_silent() {
    let note = NoteId::new(node(510));
    let mut definitions = Definitions::default();
    definitions
        .footnotes
        .insert(note, note_body(610, NoteKind::Footnote, "note text"));
    definitions.sections.push(section(410));
    definitions.settings.footnote_props = NoteProperties {
        number_format: Some(NumberFormat::LowerLetter),
        ..NoteProperties::default()
    };
    let doc = document(vec![reference(110, note, NoteKind::Footnote)], definitions);

    let (_, recorder) = paginate_recording(&doc);

    assert_eq!(
        recorder.last(110),
        "bodya",
        "w:settings/w:footnotePr is the document default a silent section inherits"
    );
}

#[test]
fn chicago_num_fmt_prints_the_traditional_reference_marks() {
    let first = NoteId::new(node(520));
    let second = NoteId::new(node(521));
    let mut definitions = Definitions::default();
    definitions
        .footnotes
        .insert(first, note_body(620, NoteKind::Footnote, "first"));
    definitions
        .footnotes
        .insert(second, note_body(630, NoteKind::Footnote, "second"));
    let mut boundary = section(420);
    boundary.footnote_props = NoteProperties {
        number_format: Some(NumberFormat::Other("chicago".to_owned())),
        ..NoteProperties::default()
    };
    definitions.sections.push(boundary);
    let doc = document(
        vec![
            reference(120, first, NoteKind::Footnote),
            reference(130, second, NoteKind::Footnote),
        ],
        definitions,
    );

    let (_, recorder) = paginate_recording(&doc);

    assert_eq!(recorder.last(120), "body*");
    assert_eq!(recorder.last(130), "body†");
}

#[test]
fn num_restart_each_section_restarts_the_count_at_the_section_boundary() {
    let first = NoteId::new(node(530));
    let second = NoteId::new(node(531));
    let mut definitions = Definitions::default();
    definitions
        .footnotes
        .insert(first, note_body(640, NoteKind::Footnote, "first"));
    definitions
        .footnotes
        .insert(second, note_body(650, NoteKind::Footnote, "second"));
    let first_section = SectionId::new(node(430));
    let mut opening = section(430);
    opening.footnote_props = NoteProperties {
        number_restart: Some(NoteNumberRestart::EachSection),
        ..NoteProperties::default()
    };
    let mut trailing = section(431);
    trailing.footnote_props = NoteProperties {
        number_restart: Some(NoteNumberRestart::EachSection),
        ..NoteProperties::default()
    };
    definitions.sections.push(opening);
    definitions.sections.push(trailing);
    let doc = document(
        vec![
            reference(140, first, NoteKind::Footnote),
            section_break(150, "end of section one", first_section),
            reference(160, second, NoteKind::Footnote),
        ],
        definitions,
    );

    let (_, recorder) = paginate_recording(&doc);

    assert_eq!(recorder.last(140), "body1");
    assert_eq!(
        recorder.last(160),
        "body1",
        "eachSect restarts the note count in the second section instead of continuing to 2"
    );
}

#[test]
fn num_restart_continuous_counts_across_a_section_boundary() {
    let first = NoteId::new(node(540));
    let second = NoteId::new(node(541));
    let mut definitions = Definitions::default();
    definitions
        .footnotes
        .insert(first, note_body(660, NoteKind::Footnote, "first"));
    definitions
        .footnotes
        .insert(second, note_body(670, NoteKind::Footnote, "second"));
    let first_section = SectionId::new(node(440));
    definitions.sections.push(section(440));
    definitions.sections.push(section(441));
    let doc = document(
        vec![
            reference(170, first, NoteKind::Footnote),
            section_break(180, "end of section one", first_section),
            reference(190, second, NoteKind::Footnote),
        ],
        definitions,
    );

    let (_, recorder) = paginate_recording(&doc);

    assert_eq!(recorder.last(170), "body1");
    assert_eq!(
        recorder.last(190),
        "body2",
        "with no w:numRestart the count is continuous — the eachSect guard above must \
         not be passing for the default reason"
    );
}

#[test]
fn num_restart_each_page_restarts_the_count_on_the_next_page() {
    let first = NoteId::new(node(550));
    let second = NoteId::new(node(551));
    let third = NoteId::new(node(552));
    let mut definitions = Definitions::default();
    definitions
        .footnotes
        .insert(first, note_body(680, NoteKind::Footnote, "first"));
    definitions
        .footnotes
        .insert(second, note_body(690, NoteKind::Footnote, "second"));
    definitions
        .footnotes
        .insert(third, note_body(700, NoteKind::Footnote, "third"));
    let mut boundary = section(450);
    boundary.footnote_props = NoteProperties {
        number_restart: Some(NoteNumberRestart::EachPage),
        ..NoteProperties::default()
    };
    definitions.sections.push(boundary);
    let doc = document(
        vec![
            reference(200, first, NoteKind::Footnote),
            reference(210, second, NoteKind::Footnote),
            reference_on_a_new_page(220, third, NoteKind::Footnote),
        ],
        definitions,
    );

    let (layout, recorder) = paginate_recording(&doc);

    assert_eq!(layout.pages.len(), 2, "the third reference is on page two");
    assert_eq!(recorder.last(200), "body1");
    assert_eq!(recorder.last(210), "body2");
    assert_eq!(
        recorder.last(220),
        "body1",
        "eachPage restarts the count on the page the reference landed on, so the \
         third note is 1 and not 3"
    );
    assert_eq!(
        recorder.last(700),
        "1third",
        "the continued note body prints the page-restarted label too"
    );
}

#[test]
fn beneath_text_hangs_the_footnote_band_off_the_last_body_line() {
    let at_page_bottom = footnote_document(NoteProperties::default());
    let beneath_text = footnote_document(NoteProperties {
        position: Some(NotePosition::BeneathText),
        ..NoteProperties::default()
    });
    let shaper = ParleyShaper::new();

    let bottom = paginate_document(&at_page_bottom, &shaper);
    let beneath = paginate_document(&beneath_text, &shaper);

    let bottom_page = &bottom.pages[0];
    let beneath_page = &beneath.pages[0];
    assert_eq!(
        bottom_page.footnotes[0].rect.origin.y,
        bottom_page.content_area.bottom(),
        "pageBottom (the default) keeps the band under the reserved body area"
    );
    let last_line_bottom = beneath_page
        .placed
        .iter()
        .map(|placed| placed.rect.bottom())
        .max()
        .expect("the reference paragraph is placed");
    assert_eq!(
        beneath_page.footnotes[0].rect.origin.y, last_line_bottom,
        "beneathText starts the band at the bottom of the last body fragment"
    );
    assert!(
        beneath_page.footnotes[0].rect.origin.y < bottom_page.footnotes[0].rect.origin.y,
        "on an under-full page beneathText rides up with the text \
         (ours {} vs pageBottom {})",
        beneath_page.footnotes[0].rect.origin.y.raw(),
        bottom_page.footnotes[0].rect.origin.y.raw(),
    );
    assert!(
        beneath_page.footnotes[0].rect.origin.y > Twip::ZERO,
        "the band still starts inside the page"
    );
}

/// The two endnote `w:pos` values, exercised on the same two-section body: the
/// endnote referenced in section one either travels to the document end (`docEnd`,
/// Word's default) or is laid out at the end of its own section (`sectEnd`).
fn two_section_endnote_document(position: Option<NotePosition>) -> Document {
    let endnote = NoteId::new(node(560));
    let mut definitions = Definitions::default();
    definitions
        .endnotes
        .insert(endnote, note_body(710, NoteKind::Endnote, "endnote text"));
    let first_section = SectionId::new(node(460));
    let mut opening = section(460);
    opening.endnote_props = NoteProperties {
        position,
        ..NoteProperties::default()
    };
    definitions.sections.push(opening);
    definitions.sections.push(section(461));
    document(
        vec![
            reference(230, endnote, NoteKind::Endnote),
            section_break(240, "end of section one", first_section),
            paragraph(250, "second section body"),
        ],
        definitions,
    )
}

#[test]
fn endnote_pos_sect_end_lays_the_note_out_at_the_end_of_its_own_section() {
    let shaper = ParleyShaper::new();
    let doc_end = paginate_document(&two_section_endnote_document(None), &shaper);
    let sect_end = paginate_document(
        &two_section_endnote_document(Some(NotePosition::SectionEnd)),
        &shaper,
    );

    let order = |layout: &PaginatedLayout| -> Vec<NodeId> {
        layout
            .pages
            .iter()
            .flat_map(|page| page.placed.iter())
            .map(|placed| placed.fragment.node_id())
            .collect()
    };
    let position_of = |layout: &PaginatedLayout, id: u64| {
        order(layout)
            .iter()
            .position(|placed| *placed == node(id))
            .unwrap_or_else(|| panic!("block {id} is not placed"))
    };

    assert!(
        position_of(&doc_end, 710) > position_of(&doc_end, 250),
        "docEnd (the default) places the endnote body after the final section's body"
    );
    assert!(
        position_of(&sect_end, 710) < position_of(&sect_end, 250),
        "sectEnd places the endnote body at the end of section one, before the \
         second section's body"
    );
}

/// `w:pos` is two vocabularies under one name: `beneathText` belongs to
/// `ST_FtnPos`, and an endnote container carrying it must keep its own default.
///
/// Two things have to hold for that, and this asserts the pair: the cross-kind
/// token is rejected at resolution *and* the placement decision compares against
/// `sectEnd` exactly rather than "anything but `docEnd`". Breaking either one alone
/// leaves the layout unchanged (each masks the other), so this guard goes red only
/// under the combined mutation — which is exactly the failure mode worth catching,
/// and the unit test in `note_numbering` pins the resolution half on its own.
#[test]
fn a_footnote_pos_token_does_not_move_endnotes() {
    let shaper = ParleyShaper::new();
    let stray = paginate_document(
        &two_section_endnote_document(Some(NotePosition::BeneathText)),
        &shaper,
    );
    let default = paginate_document(&two_section_endnote_document(None), &shaper);

    let ids = |layout: &PaginatedLayout| -> Vec<NodeId> {
        layout
            .pages
            .iter()
            .flat_map(|page| page.placed.iter())
            .map(|placed| placed.fragment.node_id())
            .collect()
    };
    assert_eq!(
        ids(&stray),
        ids(&default),
        "an ST_FtnPos token on w:endnotePr is ignored, not treated as sectEnd"
    );
}

#[test]
fn endnote_num_fmt_applies_to_the_reference_marker() {
    let endnote = NoteId::new(node(570));
    let mut definitions = Definitions::default();
    definitions
        .endnotes
        .insert(endnote, note_body(720, NoteKind::Endnote, "endnote text"));
    let mut boundary = section(470);
    boundary.endnote_props = NoteProperties {
        number_format: Some(NumberFormat::LowerRoman),
        number_start: Some(3),
        ..NoteProperties::default()
    };
    definitions.sections.push(boundary);
    let doc = document(
        vec![reference(260, endnote, NoteKind::Endnote)],
        definitions,
    );

    let (_, recorder) = paginate_recording(&doc);

    assert_eq!(
        recorder.last(260),
        "bodyiii",
        "w:endnotePr's w:numFmt and w:numStart reach the endnote reference marker"
    );
}
