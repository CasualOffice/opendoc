//! Step 4 of `docs/113` §6: the flow engine emits the measure tier **directly**,
//! so the shaped galley never becomes resident.
//!
//! Step 2 split the measure tier out of a finished galley
//! ([`measure_galley`](casual_doc_layout::measure::measure_galley)). That made
//! the tier's *resident* cost small but left its *peak* cost unchanged: the
//! engine still shaped the whole document and then threw the glyphs away.
//! [`build_measures_for_blocks`] closes that by handing the engine a
//! [`MeasureSink`] instead of a `Vec<BlockFragment>`.
//!
//! The property, and the only thing standing between this and a document that
//! paginates differently depending on how it was opened:
//!
//! > **Streaming the measure tier out of the flow engine produces exactly the
//! > tier a full galley projects to, fragment for fragment.**
//!
//! Two further guards back the memory claim itself, because the equality above
//! would still hold if the sink secretly kept everything:
//!
//! - `the_engine_never_reaches_further_back_than_the_sink_contract_allows`
//!   watches the *engine* through a spying sink and fails if it ever mutates a
//!   fragment older than the last two. That contract is what lets the measure
//!   sink drop glyphs at all.
//! - `the_measure_sink_holds_only_the_lookback_in_shaped_form` watches the
//!   *sink* and fails if it retains more than that.
//!
//! The corpus below deliberately covers every [`BlockNode`] variant and every
//! branch of the flow loop that touches the sink — the contextual-spacing
//! collapse (the one retroactive mutation), a table (many fragments from one
//! block), a nested content control (a recursive flow sharing one sink), a
//! drop-cap pair (two blocks consumed in one iteration), and an `altChunk`.
//! `docs/113` §6.1 records why that matters: four of step 2's guards first
//! passed under mutation because the corpus did not exercise what they
//! asserted.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::document_page_config;
use casual_doc_layout::flow::build_galley_for_blocks;
use casual_doc_layout::flow::build_measures_for_blocks;
use casual_doc_layout::measure::FragmentMeasure;
use casual_doc_layout::measure::GalleySink;
use casual_doc_layout::measure::MeasureSink;
use casual_doc_layout::measure::measure_galley;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, AltChunk, AltChunkProperties, BlockNode, BlockSdt,
    Definitions, Document, DropCapFrame, DropCapMode, EmbeddedPart, FrameWrap, GridColumn,
    Indentation, InlineNode, LevelSuffix, LineRule, NumberFormat, NumberingInstance,
    NumberingInstanceId, NumberingLevel, NumberingRef, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunProperties, SdtProperties, SectionBoundary, SectionColumns,
    SectionId, Spacing, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// Node ids are minted in disjoint bands so a fixture can nest paragraphs
/// inside cells inside tables without two of them colliding (the model rejects
/// a duplicate id, which is how this scheme was chosen rather than guessed).
const RUN_BAND: u64 = 5_000_000;
/// Cell paragraphs sit one band above their cell.
const CELL_BODY_BAND: u64 = 1_000_000;

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn paragraph(id: u64, properties: ParagraphProperties, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: properties.into(),
        inlines: vec![run(id + RUN_BAND, text)],
    })
}

fn spacing(before: i32, after: i32) -> Spacing {
    Spacing {
        before_twips: Some(before),
        after_twips: Some(after),
        ..Spacing::default()
    }
}

/// A US-Letter, 1-inch-margin single section.
fn section() -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(9)),
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

fn document(body: Vec<BlockNode>) -> Document {
    Document::new(
        node(1),
        body,
        Definitions {
            sections: vec![section()],
            ..Definitions::default()
        },
    )
    .expect("the fixture body is valid")
}

/// A decimal list definition, so the numbering corpus advances a real counter
/// across blocks (cross-block flow state the sink must not disturb).
fn numbered_document(body: Vec<BlockNode>) -> Document {
    let abstract_id = AbstractNumberingId::new(node(7_000_000));
    let instance_id = NumberingInstanceId::new(node(7_000_001));
    let mut definitions = Definitions {
        sections: vec![section()],
        ..Definitions::default()
    };
    definitions.abstract_numbering.insert(
        abstract_id,
        AbstractNumbering {
            levels: vec![NumberingLevel {
                level: 0,
                start: 1,
                num_fmt: Some(NumberFormat::Decimal),
                lvl_text: Some("%1.".to_owned()),
                lvl_jc: None,
                suff: Some(LevelSuffix::Tab),
                is_lgl: false,
                paragraph_properties: Some(ParagraphProperties {
                    indentation: Some(Indentation {
                        start_twips: Some(720),
                        hanging_twips: Some(360),
                        ..Indentation::default()
                    }),
                    ..ParagraphProperties::default()
                }),
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
    definitions.numbering.insert(
        instance_id,
        NumberingInstance {
            abstract_ref: abstract_id,
            overrides: Vec::new(),
        },
    );
    Document::new(node(1), body, definitions).expect("the numbered fixture body is valid")
}

fn numbered_paragraph(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            numbering: Some(NumberingRef {
                instance: NumberingInstanceId::new(node(7_000_001)),
                level: 0,
            }),
            ..ParagraphProperties::default()
        }
        .into(),
        inlines: vec![run(id + RUN_BAND, text)],
    })
}

fn cell(id: u64, text: &str) -> TableCell {
    TableCell {
        id: node(id),
        properties: TableCellProperties::default(),
        blocks: vec![paragraph(
            id + CELL_BODY_BAND,
            ParagraphProperties::default(),
            text,
        )],
    }
}

fn table(id: u64, rows: usize) -> BlockNode {
    BlockNode::Table(Box::new(Table {
        id: node(id),
        grid: vec![
            GridColumn {
                width_twips: Some(3_000),
            },
            GridColumn {
                width_twips: Some(3_000),
            },
        ],
        grid_change: None,
        properties: TableProperties::default(),
        rows: (0..rows)
            .map(|r| {
                let base = id + 10 + (r as u64) * 10;
                TableRow {
                    id: node(base),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        cell(base + 1, "left cell text"),
                        cell(base + 2, "right cell text"),
                    ],
                }
            })
            .collect(),
    }))
}

/// A drop-cap paragraph and the body paragraph it is coupled to — the one
/// branch of the flow loop that consumes two blocks in a single iteration.
fn drop_cap_pair(id: u64) -> Vec<BlockNode> {
    vec![
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties {
                keep_next: true,
                spacing: Some(Spacing {
                    line_rule: Some(LineRule::Exact),
                    line_twips: Some(700),
                    ..Spacing::default()
                }),
                drop_cap_frame: Some(DropCapFrame {
                    mode: DropCapMode::Drop,
                    lines: 3,
                    wrap: Some(FrameWrap::Around),
                    horizontal_anchor: None,
                    vertical_anchor: None,
                    horizontal_alignment: None,
                    vertical_alignment: None,
                    horizontal_position_twips: None,
                    vertical_position_twips: None,
                    horizontal_space_twips: Some(80),
                    vertical_space_twips: None,
                }),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: vec![InlineNode::Run(Run {
                id: node(id + RUN_BAND),
                properties: RunProperties {
                    size_half_points: Some(117),
                    ..RunProperties::default()
                }
                .into(),
                text: "D".to_owned(),
            })],
        }),
        paragraph(
            id + 2,
            ParagraphProperties::default(),
            &"rop cap body text wraps beside the initial ".repeat(20),
        ),
    ]
}

fn alt_chunk(id: u64) -> BlockNode {
    BlockNode::AltChunk(Box::new(AltChunk {
        id: node(id),
        part: EmbeddedPart {
            relationship_id: "rId9".to_owned(),
            relationship_type: "http://schemas.openxmlformats.org/officeDocument/2006/\
                 relationships/aFChunk"
                .to_owned(),
            part_name: "word/afchunk.htm".to_owned(),
        },
        properties: AltChunkProperties::default(),
    }))
}

/// Every fixture the streaming path is asserted over. Each name says which
/// branch of the flow loop — and therefore which use of the sink — it covers.
fn corpus() -> Vec<(&'static str, Document)> {
    let contextual_props = || ParagraphProperties {
        spacing: Some(spacing(240, 240)),
        contextual_spacing: true,
        ..ParagraphProperties::default()
    };
    vec![
        (
            // The retroactive mutation: same-style adjacency zeroes the current
            // fragment's space-before AND the previous one's space-after.
            "contextual spacing collapse",
            document(vec![
                paragraph(100, contextual_props(), "First"),
                paragraph(110, contextual_props(), "Second"),
                paragraph(120, contextual_props(), "Third"),
            ]),
        ),
        (
            // The same shape without the flag, so a sink that collapsed nothing
            // and a sink that collapsed everything are told apart.
            "spacing kept",
            document(vec![
                paragraph(
                    200,
                    ParagraphProperties {
                        spacing: Some(spacing(240, 240)),
                        ..ParagraphProperties::default()
                    },
                    "First",
                ),
                paragraph(
                    210,
                    ParagraphProperties {
                        spacing: Some(spacing(240, 240)),
                        ..ParagraphProperties::default()
                    },
                    "Second",
                ),
            ]),
        ),
        (
            // One block, many fragments: the table pushes a row at a time.
            "table rows",
            document(vec![
                paragraph(300, ParagraphProperties::default(), "before the table"),
                table(310, 4),
                paragraph(400, ParagraphProperties::default(), "after the table"),
            ]),
        ),
        (
            // A recursive flow that shares the caller's sink, with a
            // contextual-spacing pair inside it so the collapse happens at a
            // nested depth too.
            "nested content control",
            document(vec![
                paragraph(500, contextual_props(), "outside"),
                BlockNode::Sdt(Box::new(BlockSdt {
                    id: node(510),
                    properties: SdtProperties::default(),
                    blocks: vec![
                        paragraph(520, contextual_props(), "inside one"),
                        paragraph(530, contextual_props(), "inside two"),
                        BlockNode::Sdt(Box::new(BlockSdt {
                            id: node(540),
                            properties: SdtProperties::default(),
                            blocks: vec![paragraph(
                                550,
                                ParagraphProperties::default(),
                                "inside deeper",
                            )],
                        })),
                        table(560, 2),
                    ],
                })),
                paragraph(600, contextual_props(), "after"),
            ]),
        ),
        (
            // Two blocks consumed in one loop iteration.
            "drop cap pair",
            document(
                drop_cap_pair(700)
                    .into_iter()
                    .chain(std::iter::once(paragraph(
                        710,
                        ParagraphProperties::default(),
                        "a following paragraph",
                    )))
                    .collect(),
            ),
        ),
        (
            "alt chunk",
            document(vec![
                paragraph(800, ParagraphProperties::default(), "before the chunk"),
                alt_chunk(810),
                paragraph(820, ParagraphProperties::default(), "after the chunk"),
            ]),
        ),
        (
            // Cross-block flow state: the list counter advances as blocks are
            // walked, so a sink that perturbed the walk would renumber.
            "numbered list",
            numbered_document(
                (0..6)
                    .map(|i| numbered_paragraph(900 + i * 10, "a numbered item"))
                    .collect(),
            ),
        ),
        (
            // Long enough that the sink projects and drops many times over,
            // rather than holding everything in its lookback by accident.
            "long prose",
            document(
                (0..600)
                    .map(|i| {
                        paragraph(
                            10_000 + i * 2,
                            ParagraphProperties {
                                spacing: Some(spacing(120, 180)),
                                ..ParagraphProperties::default()
                            },
                            "The quick brown fox jumps over the lazy dog while the editor \
                             reflows this paragraph and every other one on the page.",
                        )
                    })
                    .collect(),
            ),
        ),
    ]
}

fn content_width(document: &Document) -> Twip {
    document_page_config(document).content_area().size.width
}

/// The property: the streamed tier is the projected tier.
#[test]
fn streaming_the_measure_tier_equals_projecting_the_whole_galley() {
    let shaper = ParleyShaper::new();
    let mut collapses = 0usize;
    for (name, doc) in corpus() {
        let width = content_width(&doc);
        let galley = build_galley_for_blocks(&doc, &shaper, doc.body(), width);
        let projected = measure_galley(&galley);
        let (streamed, _marks) = build_measures_for_blocks(&doc, &shaper, doc.body(), width);
        assert_eq!(
            streamed.len(),
            projected.len(),
            "{name}: the streamed tier must have one measure per galley fragment"
        );
        assert_eq!(
            streamed, projected,
            "{name}: streaming the measure tier must equal projecting the whole galley"
        );
        collapses += collapsed_edges(&projected);
    }
    // Without this the equality above would still pass on a corpus where the
    // one retroactive mutation never fires, which is exactly how four of step
    // 2's guards first passed under mutation (`docs/113` §6.1).
    assert!(
        collapses >= 4,
        "the corpus must actually collapse contextual spacing, or the sink's \
         lookback is never exercised (saw {collapses} collapsed edges)"
    );
}

/// How many paragraph edges the contextual-spacing collapse zeroed, among
/// paragraphs that asked for space on that edge. A tier built without the
/// collapse has a strictly smaller count.
fn collapsed_edges(measures: &[FragmentMeasure]) -> usize {
    measures
        .windows(2)
        .filter(|pair| {
            let (
                FragmentMeasure::Paragraph {
                    space_after: prev_after,
                    ..
                },
                FragmentMeasure::Paragraph {
                    space_before: next_before,
                    ..
                },
            ) = (&pair[0], &pair[1])
            else {
                return false;
            };
            *prev_after == Twip::ZERO && *next_before == Twip::ZERO
        })
        .count()
}

/// The block marks are the map a window needs: block *b* of the body starts at
/// galley fragment `marks[b]`.
///
/// Derived independently here — from the node ids the paint galley carries —
/// rather than from the same counter the sink used.
#[test]
fn streaming_records_where_every_top_level_block_starts() {
    let shaper = ParleyShaper::new();
    for (name, doc) in corpus() {
        let width = content_width(&doc);
        let galley = build_galley_for_blocks(&doc, &shaper, doc.body(), width);
        let (_measures, marks) = build_measures_for_blocks(&doc, &shaper, doc.body(), width);

        assert_eq!(
            marks.len(),
            doc.body().len(),
            "{name}: one mark per top-level block"
        );
        for pair in marks.windows(2) {
            assert!(
                pair[0] <= pair[1],
                "{name}: block marks must not go backwards ({pair:?})"
            );
        }
        assert_eq!(
            marks.first().copied(),
            Some(0),
            "{name}: block 0 starts at 0"
        );
        assert!(
            marks.last().copied().unwrap_or(0) as usize <= galley.len(),
            "{name}: no mark may point past the galley"
        );

        // Independent derivation: the first fragment of block b must carry a
        // node id the block owns.
        for (index, block) in doc.body().iter().enumerate() {
            let at = marks[index] as usize;
            let Some(fragment) = galley.get(at) else {
                // A block that produced no fragment (an empty content control)
                // marks the position the next one starts at, which is allowed.
                continue;
            };
            assert!(
                block_owns(block, fragment.node_id()),
                "{name}: block {index} is marked at galley fragment {at}, whose node \
                 {:?} that block does not own",
                fragment.node_id()
            );
        }
    }
}

/// Whether `block` (or anything nested inside it) has node id `id`.
fn block_owns(block: &BlockNode, id: NodeId) -> bool {
    match block {
        BlockNode::Paragraph(paragraph) => paragraph.id == id,
        BlockNode::AltChunk(chunk) => chunk.id == id,
        BlockNode::Table(table) => {
            table.id == id
                || table.rows.iter().any(|row| {
                    row.id == id
                        || row
                            .cells
                            .iter()
                            .any(|cell| cell.blocks.iter().any(|b| block_owns(b, id)))
                })
        }
        BlockNode::Sdt(sdt) => sdt.id == id || sdt.blocks.iter().any(|b| block_owns(b, id)),
    }
}

/// A sink that records how far back the engine reaches when it mutates a
/// fragment it has already pushed.
///
/// This is the guard under [`MeasureSink`]'s whole reason for existing: it can
/// only drop a fragment's glyphs because the engine is claimed never to touch
/// that fragment again. Claiming it is not the same as watching it.
#[derive(Debug, Default)]
struct ReachSpy {
    galley: Vec<BlockFragment>,
    /// Largest `len() - index` seen on a retroactive mutation.
    deepest_reach: usize,
    mutations: usize,
}

impl GalleySink for ReachSpy {
    fn push(&mut self, fragment: BlockFragment) {
        self.galley.push(fragment);
    }

    fn len(&self) -> usize {
        self.galley.len()
    }

    fn stacked_height(&self) -> Twip {
        GalleySink::stacked_height(&self.galley)
    }

    fn zero_space_before(&mut self, index: usize) {
        self.record(index);
        self.galley.zero_space_before(index);
    }

    fn zero_space_after(&mut self, index: usize) {
        self.record(index);
        self.galley.zero_space_after(index);
    }
}

impl ReachSpy {
    fn record(&mut self, index: usize) {
        self.mutations += 1;
        self.deepest_reach = self.deepest_reach.max(self.galley.len() - index);
    }
}

/// How many shaped fragments the streaming path must keep alive at once.
///
/// Two: the fragment being pushed and the predecessor whose space-after the
/// contextual-spacing collapse may still zero. This is the number the memory
/// claim rests on, so it is stated once and asserted from both sides.
const LOOKBACK: usize = 2;

#[test]
fn the_engine_never_reaches_further_back_than_the_sink_contract_allows() {
    let shaper = ParleyShaper::new();
    let mut total_mutations = 0usize;
    for (name, doc) in corpus() {
        let width = content_width(&doc);
        let mut spy = ReachSpy::default();
        // Re-flow the body through the spy by the same public entry the paint
        // tier uses, so this watches the real engine and not a stand-in.
        casual_doc_layout::flow::flow_body_into_sink(&doc, &shaper, doc.body(), width, &mut spy);
        assert!(
            spy.deepest_reach <= LOOKBACK,
            "{name}: the flow engine mutated a fragment {} back, but a streaming sink \
             keeps only {LOOKBACK}; either the sink's lookback or this contract is wrong",
            spy.deepest_reach
        );
        // The spy's galley must also be the galley the paint path builds, or it
        // is watching something else.
        assert_eq!(
            spy.galley,
            build_galley_for_blocks(&doc, &shaper, doc.body(), width),
            "{name}: flowing into a custom sink must produce the same galley"
        );
        total_mutations += spy.mutations;
    }
    assert!(
        total_mutations > 0,
        "no retroactive mutation happened at all, so the reach bound is vacuous"
    );
}

#[test]
fn the_measure_sink_holds_only_the_lookback_in_shaped_form() {
    let shaper = ParleyShaper::new();
    let doc = document(
        (0..200)
            .map(|i| {
                paragraph(
                    20_000 + i * 2,
                    ParagraphProperties::default(),
                    "a paragraph whose shaped form must not survive the next one",
                )
            })
            .collect(),
    );
    let width = content_width(&doc);
    let galley = build_galley_for_blocks(&doc, &shaper, doc.body(), width);

    let mut sink = MeasureSink::new();
    let mut deepest = 0usize;
    for fragment in &galley {
        sink.push(fragment.clone());
        deepest = deepest.max(sink.shaped_len());
        assert!(
            sink.shaped_len() <= LOOKBACK,
            "the measure sink kept {} shaped fragments; it may keep only {LOOKBACK}",
            sink.shaped_len()
        );
    }
    assert_eq!(
        deepest, LOOKBACK,
        "the sink must actually fill its lookback, or the bound above is vacuous"
    );
    assert_eq!(sink.finish(), measure_galley(&galley));
}

/// The sink tracks the stacked height the flow engine measures float clearance
/// against, without holding the fragments it summed.
#[test]
fn the_measure_sink_tracks_the_same_stacked_height_as_the_galley() {
    let shaper = ParleyShaper::new();
    for (name, doc) in corpus() {
        let width = content_width(&doc);
        let galley = build_galley_for_blocks(&doc, &shaper, doc.body(), width);
        let mut sink = MeasureSink::new();
        let mut reference: Vec<BlockFragment> = Vec::new();
        for fragment in &galley {
            sink.push(fragment.clone());
            GalleySink::push(&mut reference, fragment.clone());
            assert_eq!(
                sink.stacked_height(),
                GalleySink::stacked_height(&reference),
                "{name}: the measure sink must report the galley's stacked height \
                 after {} fragments",
                reference.len()
            );
        }
    }
}

#[test]
#[should_panic(expected = "already projected and dropped")]
fn the_measure_sink_refuses_a_mutation_it_can_no_longer_honor() {
    let shaper = ParleyShaper::new();
    let doc = document(
        (0..8)
            .map(|i| paragraph(30_000 + i * 2, ParagraphProperties::default(), "text"))
            .collect(),
    );
    let width = content_width(&doc);
    let galley = build_galley_for_blocks(&doc, &shaper, doc.body(), width);
    let mut sink = MeasureSink::new();
    for fragment in &galley {
        sink.push(fragment.clone());
    }
    // Fragment 0 was projected and dropped long ago. Silently ignoring this
    // would corrupt a paragraph's spacing with no symptom until a page boundary
    // moved somewhere else in the document.
    sink.zero_space_after(0);
}
