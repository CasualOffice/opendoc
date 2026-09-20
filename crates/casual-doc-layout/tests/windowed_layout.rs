//! Windowed layout, steps 2 and 3 of `docs/113` §6: the measure tier and
//! resumable pagination.
//!
//! Both steps rest on one property, and it is the only thing standing between
//! this and a document that paginates differently depending on where the user
//! scrolled (`docs/113` §5):
//!
//! > **A windowed or resumed page must equal the same page of a full
//! > `paginate_document`, field for field.**
//!
//! That is the shape the incremental paginator already asserts with its
//! `incremental_equals_full_*` goldens, so these are written the same way:
//! build a galley, paginate it fully, and require the cheap path to reproduce
//! the expensive one exactly rather than merely "closely".
//!
//! - `the_measure_tier_*` cover step 2. The measure tier carries no glyphs, so
//!   its pages are [`PageOutline`]s; the comparison is against
//!   [`PageOutline::of`] every page of a full [`paginate`], which is every
//!   field of a [`Page`](casual_doc_layout::page::Page) except the painted
//!   content.
//! - `paginate_from_*` cover step 3, including the case the checkpoint's
//!   `current_table`/`table_headers` exist for: a table whose rows straddle the
//!   checkpoint, whose repeated `w:tblHeader` row has to reappear on the first
//!   resumed page.
//!
//! The corpus-level version of the step-2 property — the measure tier over the
//! real shaped fixtures rather than synthetic heights — lives in
//! `geometry_snapshot.rs`, next to the corpus it uses.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::block::BoxMetrics;
use casual_doc_layout::block::BreakControl;
use casual_doc_layout::block::CellContentMargins;
use casual_doc_layout::block::CellFragment;
use casual_doc_layout::block::ParagraphDecor;
use casual_doc_layout::measure::FragmentMeasure;
use casual_doc_layout::measure::PageOutline;
use casual_doc_layout::measure::Paginable;
use casual_doc_layout::measure::PaginableCell;
use casual_doc_layout::measure::RowMeasure;
use casual_doc_layout::measure::measure_galley;
use casual_doc_layout::model::ModelPos;
use casual_doc_layout::model::ModelRange;
use casual_doc_layout::paginate::Checkpoint;
use casual_doc_layout::paginate::PageConfig;
use casual_doc_layout::paginate::paginate;
use casual_doc_layout::paginate::paginate_from;
use casual_doc_layout::paginate::paginate_measures;
use casual_doc_layout::paginate::paginate_with_checkpoints;
use casual_doc_layout::text::Line;
use casual_doc_layout::text::LineBreak;
use casual_doc_layout::text::LineLayout;
use casual_doc_layout::units::Size;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// A US-Letter page (12240×15840 twips) with 1-inch margins → a 9360×12960
/// content area.
fn letter_config() -> PageConfig {
    PageConfig {
        section: SectionId::new(node(9)),
        page_size: Size::new(Twip(12_240), Twip(15_840)),
        margin_top: Twip(1_440),
        margin_bottom: Twip(1_440),
        margin_start: Twip(1_440),
        margin_end: Twip(1_440),
        header_distance: Twip(720),
        footer_distance: Twip(720),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

fn line(owner: NodeId, height: Twip, page_break_after: bool) -> Line {
    Line {
        runs: Vec::new(),
        ascent: height,
        descent: Twip::ZERO,
        height,
        clip: false,
        range: ModelRange::new(ModelPos::new(owner, 0), ModelPos::new(owner, 0)),
        line_break: LineBreak::ParagraphEnd,
        page_break_after,
        bars: Vec::new(),
        images: Vec::new(),
        fields: Vec::new(),
        notes: Vec::new(),
        text_boxes: Vec::new(),
        rules: Vec::new(),
    }
}

/// A paragraph fragment of `count` lines, each `line_h` tall.
fn para(id: u64, count: usize, line_h: Twip, break_control: BreakControl) -> BlockFragment {
    para_spaced(id, count, line_h, break_control, Twip::ZERO, Twip::ZERO)
}

/// A paragraph fragment with explicit `w:spacing` above and below it. Some
/// corpus cases carry non-zero box space deliberately: with `space_before` and
/// `space_after` both zero, a measure tier that dropped them would still
/// paginate identically, and the guards would pass while being wrong.
fn para_spaced(
    id: u64,
    count: usize,
    line_h: Twip,
    break_control: BreakControl,
    space_before: Twip,
    space_after: Twip,
) -> BlockFragment {
    let owner = node(id);
    BlockFragment::Paragraph {
        id: owner,
        lines: LineLayout {
            lines: (0..count).map(|_| line(owner, line_h, false)).collect(),
        },
        box_metrics: BoxMetrics {
            space_before,
            space_after,
            indent_start: Twip::ZERO,
            indent_end: Twip::ZERO,
        },
        break_control,
        decor: ParagraphDecor::default(),
    }
}

/// A paragraph whose line `break_after` carries a forced page break.
fn para_forced(id: u64, count: usize, line_h: Twip, break_after: usize) -> BlockFragment {
    let owner = node(id);
    BlockFragment::Paragraph {
        id: owner,
        lines: LineLayout {
            lines: (0..count)
                .map(|i| line(owner, line_h, i == break_after))
                .collect(),
        },
        box_metrics: BoxMetrics::default(),
        break_control: BreakControl::default(),
        decor: ParagraphDecor::default(),
    }
}

fn cell_of(id: u64, blocks: Vec<BlockFragment>) -> CellFragment {
    // Non-zero `w:tcMar` on purpose: the measure tier folds a cell's four
    // margin edges into one vertical total, and with zero margins a tier that
    // lost them would still cut rows in the same place.
    CellFragment {
        id: node(id),
        grid_span: 1,
        x: Twip::ZERO,
        width: Twip(3_000),
        cell_spacing: Default::default(),
        blocks,
        margins: CellContentMargins {
            top: Twip(115),
            start: Twip(108),
            bottom: Twip(95),
            end: Twip(108),
        },
        vertical_alignment: Default::default(),
        vertical_merge: Default::default(),
        borders: Default::default(),
        table_borders: Default::default(),
        shading: None,
    }
}

fn table_row(
    id: u64,
    table: u64,
    cells: Vec<CellFragment>,
    can_split: bool,
    header: bool,
) -> BlockFragment {
    let height = BlockFragment::cells_content_height(&cells);
    BlockFragment::TableRow {
        id: node(id),
        table: node(table),
        cells,
        height,
        can_split,
        header,
        merge_keep_next: false,
        clip: false,
    }
}

/// The named galleys every property below is checked against. Between them they
/// exercise each branch the paginator can take: plain fill, keep-with-next
/// chains, forced `pageBreakBefore`, a forced `w:br` mid-paragraph, line
/// splitting with widow/orphan control, a `cantSplit` row, a table with a
/// repeated header row, and a row split across a page boundary.
fn corpus() -> Vec<(&'static str, Vec<BlockFragment>)> {
    let content_h = letter_config().content_area().size.height.raw();
    let widow = BreakControl {
        widow_control: true,
        ..BreakControl::default()
    };
    let keep_next = BreakControl {
        keep_next: true,
        ..BreakControl::default()
    };
    let page_break = BreakControl {
        page_break_before: true,
        ..BreakControl::default()
    };
    let keep_lines = BreakControl {
        keep_lines: true,
        ..BreakControl::default()
    };

    let mut cases: Vec<(&'static str, Vec<BlockFragment>)> = Vec::new();

    cases.push((
        "plain prose",
        (1..=300)
            .map(|i| para(i, 1, Twip(400), BreakControl::default()))
            .collect(),
    ));

    cases.push((
        "multi-line paragraphs that split across pages",
        (1..=120).map(|i| para(i, 7, Twip(240), widow)).collect(),
    ));

    cases.push((
        "keep-with-next headings above their bodies",
        (1..=240)
            .map(|i| {
                let bc = if i % 8 == 1 { keep_next } else { widow };
                para(i, if i % 8 == 1 { 1 } else { 5 }, Twip(300), bc)
            })
            .collect(),
    ));

    cases.push((
        "forced page breaks every twelfth paragraph",
        (1..=180)
            .map(|i| {
                let bc = if i > 1 && i % 12 == 1 {
                    page_break
                } else {
                    BreakControl::default()
                };
                para(i, 2, Twip(420), bc)
            })
            .collect(),
    ));

    cases.push((
        "a forced line break inside a paragraph",
        (1..=90)
            .map(|i| {
                if i % 9 == 0 {
                    para_forced(i, 6, Twip(300), 2)
                } else {
                    para(i, 3, Twip(300), widow)
                }
            })
            .collect(),
    ));

    // A keep-with-next chain taller than a whole page. The paginator gives up
    // on keeping it together (`allow_split` turns back on for an oversized
    // group), so page boundaries land *inside* the chain — the only shape that
    // produces a boundary whose predecessor keeps with next. Resuming there
    // would re-form the group from the remainder, which fits a page, and the
    // remainder would then be moved whole instead of split. That is exactly
    // what `is_resumable_boundary`'s keep-group condition refuses.
    cases.push((
        "oversized keep-with-next chains",
        (1..=160u64)
            .map(|i| {
                let last_of_chain = i % 16 == 0;
                let bc = if last_of_chain {
                    widow
                } else {
                    BreakControl {
                        keep_next: true,
                        widow_control: true,
                        ..BreakControl::default()
                    }
                };
                para(i, 6, Twip(300), bc)
            })
            .collect(),
    ));

    // A forced `pageBreakBefore` part-way down a keep-with-next chain. Word
    // lets the break win, so the group is cut short and the next group starts
    // at a fragment whose predecessor still keeps with next.
    cases.push((
        "a forced break inside a keep-with-next chain",
        (1..=180u64)
            .map(|i| {
                let position = i % 9;
                let mut bc = BreakControl {
                    keep_next: position != 0,
                    widow_control: true,
                    ..BreakControl::default()
                };
                if position == 5 {
                    bc.page_break_before = true;
                }
                para(i, 3, Twip(320), bc)
            })
            .collect(),
    ));

    cases.push((
        "paragraphs with space before and after",
        (1..=150)
            .map(|i| {
                para_spaced(
                    i,
                    3,
                    Twip(280),
                    widow,
                    Twip(160 + (i as i32 % 3) * 40),
                    Twip(200 + (i as i32 % 5) * 20),
                )
            })
            .collect(),
    ));

    cases.push((
        "keep-lines paragraphs that move whole",
        (1..=120)
            .map(|i| {
                let bc = if i % 5 == 0 { keep_lines } else { widow };
                para(i, 9, Twip(260), bc)
            })
            .collect(),
    ));

    // A table with one repeated header row and enough body rows to run over
    // several pages, with prose before and after so the table does not start or
    // end the document.
    let mut table = vec![
        para(1, 3, Twip(400), BreakControl::default()),
        table_row(
            10,
            9_000,
            vec![cell_of(
                11,
                vec![para(12, 2, Twip(300), BreakControl::default())],
            )],
            false,
            true,
        ),
    ];
    for i in 0..60u64 {
        table.push(table_row(
            100 + i * 10,
            9_000,
            vec![cell_of(
                101 + i * 10,
                vec![para(102 + i * 10, 4, Twip(300), BreakControl::default())],
            )],
            i % 4 != 3,
            false,
        ));
    }
    table.push(para(2, 3, Twip(400), BreakControl::default()));
    cases.push(("a table with a repeated header row", table));

    // A single row taller than a whole page, so the row splitter runs.
    cases.push((
        "a row taller than a page",
        vec![
            para(1, 2, Twip(400), BreakControl::default()),
            table_row(
                20,
                9_100,
                vec![cell_of(
                    21,
                    vec![para(22, 40, Twip(content_h / 12), BreakControl::default())],
                )],
                true,
                false,
            ),
            para(3, 2, Twip(400), BreakControl::default()),
        ],
    ));

    cases
}

/// Every page of a full paginate, reduced to the fields the measure tier can
/// answer for.
fn outlines_of_full(galley: &[BlockFragment], config: &PageConfig) -> Vec<PageOutline> {
    paginate(galley, config)
        .pages
        .iter()
        .map(PageOutline::of)
        .collect()
}

// --- Step 2: the measure tier --------------------------------------------

#[test]
fn the_measure_tier_reproduces_every_page_boundary() {
    let config = letter_config();
    for (name, galley) in corpus() {
        let measures = measure_galley(&galley);
        let measured = paginate_measures(&measures, &config, 0);
        let expected = outlines_of_full(&galley, &config);
        assert!(
            expected.len() > 1,
            "{name}: the case must produce several pages to be worth comparing"
        );
        assert_eq!(
            measured.pages.len(),
            expected.len(),
            "{name}: the measure tier must agree on the page count"
        );
        assert_eq!(
            measured.pages, expected,
            "{name}: the measure tier must reproduce every page boundary, field for field"
        );
    }
}

/// Asserts a measure fragment answers every paginator question the same way
/// the paint fragment it was projected from does, recursing into table cells.
fn assert_projects_faithfully(where_: &str, paint: &BlockFragment, measure: &FragmentMeasure) {
    assert_eq!(paint.height(), measure.height(), "{where_}: height");
    assert_eq!(
        paint.space_before(),
        measure.space_before(),
        "{where_}: space before"
    );
    assert_eq!(
        paint.space_after(),
        measure.space_after(),
        "{where_}: space after"
    );
    assert_eq!(
        Paginable::break_control(paint),
        measure.break_control(),
        "{where_}: break control"
    );
    assert_eq!(
        Paginable::node_id(paint),
        measure.node_id(),
        "{where_}: node id"
    );
    assert_eq!(
        Paginable::is_vertical_merge_row(paint),
        measure.is_vertical_merge_row(),
        "{where_}: vertical merge"
    );
    assert_eq!(paint.row_info(), measure.row_info(), "{where_}: row info");
    assert_eq!(
        paint.line_count(),
        measure.line_count(),
        "{where_}: line count"
    );
    for i in 0..paint.line_count() {
        assert_eq!(
            paint.line_height(i),
            measure.line_height(i),
            "{where_}: height of line {i}"
        );
        assert_eq!(
            paint.line_page_break_after(i),
            measure.line_page_break_after(i),
            "{where_}: forced break after line {i}"
        );
    }
    let paint_cells = Paginable::cells(paint);
    let measure_cells = measure.cells();
    assert_eq!(
        paint_cells.len(),
        measure_cells.len(),
        "{where_}: cell count"
    );
    for (index, (pc, mc)) in paint_cells.iter().zip(measure_cells).enumerate() {
        assert_eq!(
            PaginableCell::vertical_margins(pc),
            mc.vertical_margins(),
            "{where_}: cell {index} vertical margins"
        );
        assert_eq!(
            PaginableCell::occupied_height(pc),
            mc.occupied_height(),
            "{where_}: cell {index} occupied height"
        );
        let pb = PaginableCell::blocks(pc);
        let mb = mc.blocks();
        assert_eq!(pb.len(), mb.len(), "{where_}: cell {index} block count");
        for (j, (p, m)) in pb.iter().zip(mb).enumerate() {
            assert_projects_faithfully(&format!("{where_}/cell {index}/block {j}"), p, m);
        }
    }
}

#[test]
fn the_measure_tier_answers_every_question_the_paint_tier_does() {
    // Page boundaries are the property that matters, but they are a lossy
    // detector: a height the projection drops only moves a boundary when the
    // dropped amount happens to straddle one. This compares the projection
    // directly, question by question, so any lost field fails on the first
    // fragment rather than on whichever document is unlucky.
    let mut checked = 0;
    for (name, galley) in corpus() {
        let measures = measure_galley(&galley);
        assert_eq!(
            measures.len(),
            galley.len(),
            "{name}: one measure per fragment"
        );
        for (index, (paint, measure)) in galley.iter().zip(&measures).enumerate() {
            assert_projects_faithfully(&format!("{name}/fragment {index}"), paint, measure);
            checked += 1;
        }
    }
    assert!(
        checked > 1_000,
        "the corpus must be substantial ({checked})"
    );
}

#[test]
fn the_measure_tier_costs_less_than_a_single_shaped_line() {
    // The whole point of the tier split (`docs/113` §3.2): a paragraph's entire
    // measure — node id, per-line heights, break opportunities, box space — must
    // be cheaper than *one* of the paint-tier lines it replaces. Written as a
    // relationship rather than a constant so it cannot be "fixed" by raising a
    // number alongside the regression it was meant to catch (`docs/111` §4).
    assert!(
        size_of::<FragmentMeasure>() <= size_of::<Line>(),
        "a whole measure fragment ({} B) must cost no more than one paint-tier line ({} B)",
        size_of::<FragmentMeasure>(),
        size_of::<Line>()
    );
    assert!(
        size_of::<FragmentMeasure>() <= size_of::<BlockFragment>(),
        "a measure fragment ({} B) must not be larger than the paint fragment it replaces ({} B)",
        size_of::<FragmentMeasure>(),
        size_of::<BlockFragment>()
    );
    // A `Vec` pays for its enum's largest variant on every element, so the
    // rare table-row variant must stay boxed out of the enum or every
    // paragraph in the document is charged for it — the lesson `docs/111` §4
    // stage 1b paid for on `BlockNode`. Stated as "the enum is smaller than
    // its row payload" rather than as a byte count, so the only way to satisfy
    // it is to keep the row out of line.
    assert!(
        size_of::<FragmentMeasure>() < size_of::<RowMeasure>(),
        "the row variant must be boxed: enum {} B vs row payload {} B",
        size_of::<FragmentMeasure>(),
        size_of::<RowMeasure>()
    );
    // Eight bytes per line: a height and a forced-break flag, nothing else.
    assert_eq!(size_of::<casual_doc_layout::measure::LineMeasure>(), 8);
}

// --- Step 3: checkpoints and resumable pagination -------------------------

#[test]
fn both_tiers_record_the_same_checkpoints() {
    // The production flow produces checkpoints from heights alone and consumes
    // them on the paint tier, so the two must be interchangeable.
    let config = letter_config();
    for (name, galley) in corpus() {
        for interval in [1usize, 2, 64] {
            let painted = paginate_with_checkpoints(&galley, &config, interval).1;
            let measured = paginate_measures(&measure_galley(&galley), &config, interval);
            assert_eq!(
                painted, measured.checkpoints,
                "{name} @ every {interval} pages: both tiers must record the same checkpoints"
            );
        }
    }
}

#[test]
fn recording_checkpoints_does_not_change_the_layout() {
    let config = letter_config();
    for (name, galley) in corpus() {
        let plain = paginate(&galley, &config);
        let (checkpointed, _) = paginate_with_checkpoints(&galley, &config, 1);
        assert_pages_equal(
            &format!("{name}: observing the paginator must not steer it"),
            &checkpointed.pages,
            &plain.pages,
            1,
        );
    }
}

#[test]
fn a_checkpoint_is_only_placed_where_pagination_can_restart() {
    // A checkpoint must sit at a whole-fragment boundary that starts a
    // keep-group, or `run` cannot reproduce the forward fill from it: it always
    // begins a fragment at line 0, and it re-forms keep-groups from their head.
    let config = letter_config();
    let mut saw_a_skip = false;
    for (name, galley) in corpus() {
        let layout = paginate(&galley, &config);
        let checkpoints = paginate_with_checkpoints(&galley, &config, 1).1;
        // Every page boundary but the last is a candidate; the ones rejected are
        // the mid-paragraph and mid-keep-group boundaries.
        if checkpoints.len() + 1 < layout.pages.len() {
            saw_a_skip = true;
        }
        for checkpoint in &checkpoints {
            assert_eq!(
                checkpoint.at.line, 0,
                "{name}: a checkpoint must not sit inside a split paragraph"
            );
            let index = checkpoint.at.fragment as usize;
            assert!(
                index < galley.len(),
                "{name}: a checkpoint must name real downstream content"
            );
            assert!(
                index == 0 || !galley[index - 1].break_control().keep_next,
                "{name}: a checkpoint must start a keep-group, not sit inside one"
            );
        }
    }
    assert!(
        saw_a_skip,
        "the corpus must contain boundaries the guard rejects, or it proves nothing"
    );
}

/// A page reduced to a one-line shape, so a mismatch reports where the layouts
/// diverged instead of dumping two whole display lists.
fn summary(page: &casual_doc_layout::page::Page) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "p{} flow[{}.{}..{}.{}] area {:?}",
        page.number,
        page.flow.start.fragment,
        page.flow.start.line,
        page.flow.end.fragment,
        page.flow.end.line,
        page.content_area,
    );
    for placed in &page.placed {
        let _ = write!(
            out,
            " | {:?}@{}+{}",
            placed.fragment.node_id(),
            placed.rect.origin.y.raw(),
            placed.rect.size.height.raw()
        );
    }
    out
}

/// Compares two page runs page by page, reporting the first divergence as a
/// one-line summary rather than two whole display lists.
fn assert_pages_equal(
    context: &str,
    got: &[casual_doc_layout::page::Page],
    want: &[casual_doc_layout::page::Page],
    first_number: usize,
) {
    assert_eq!(got.len(), want.len(), "{context}: page count");
    for (offset, (a, b)) in got.iter().zip(want).enumerate() {
        assert_eq!(
            summary(a),
            summary(b),
            "{context}: page {} differs",
            first_number + offset
        );
        assert!(
            a == b,
            "{context}: page {} differs beyond its summary",
            first_number + offset
        );
    }
}

/// The property of step 3, for one galley and one checkpoint.
fn assert_resumes_identically(name: &str, galley: &[BlockFragment], checkpoint: &Checkpoint) {
    let config = letter_config();
    let full = paginate(galley, &config);
    let resumed = paginate_from(galley, &config, checkpoint);
    let from = checkpoint.page_index as usize;
    assert_eq!(
        resumed.pages.len(),
        full.pages.len() - from,
        "{name}: resuming at page {from} must produce the remaining pages"
    );
    for (offset, (got, want)) in resumed.pages.iter().zip(&full.pages[from..]).enumerate() {
        assert_eq!(
            summary(got),
            summary(want),
            "{name}: resumed page {} differs from the same page of a full paginate \
             (resumed at page {from}, checkpoint {checkpoint:?})",
            from + offset + 1
        );
        assert!(
            got == want,
            "{name}: resumed page {} differs beyond its summary",
            from + offset + 1
        );
    }
}

#[test]
fn paginate_from_equals_the_tail_of_a_full_paginate() {
    let config = letter_config();
    let mut total = 0;
    for (name, galley) in corpus() {
        let checkpoints = paginate_measures(&measure_galley(&galley), &config, 1).checkpoints;
        total += checkpoints.len();
        for checkpoint in &checkpoints {
            assert_resumes_identically(name, &galley, checkpoint);
        }
    }
    assert!(
        total > 20,
        "the corpus must exercise many resume points, not a handful ({total})"
    );
}

#[test]
fn paginate_from_equals_a_full_paginate_across_a_table_that_spans_the_checkpoint() {
    // The case `current_table` / `table_headers` exist for. A table with a
    // repeated `w:tblHeader` row runs over several pages; a checkpoint lands
    // between two of its body rows, so the resumed run has to know both that it
    // is inside that table and which row is its header — otherwise the first
    // resumed page silently loses the repeated header, and every later page
    // shifts.
    let config = letter_config();
    let (name, galley) = corpus()
        .into_iter()
        .find(|(name, _)| *name == "a table with a repeated header row")
        .expect("the corpus carries the table case");

    let checkpoints = paginate_measures(&measure_galley(&galley), &config, 1).checkpoints;
    let inside_table: Vec<&Checkpoint> = checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.current_table == Some(node(9_000)) && !checkpoint.table_headers.is_empty()
        })
        .collect();
    assert!(
        inside_table.len() > 1,
        "the table must straddle several checkpoints, else this proves nothing (found {})",
        inside_table.len()
    );

    // The header row really is repeated on the resumed page, so the assertion
    // below is charged with reproducing it rather than reproducing its absence.
    let full = paginate(&galley, &config);
    let header_id = node(10);
    for checkpoint in &inside_table {
        let page = &full.pages[checkpoint.page_index as usize];
        assert!(
            page.placed
                .iter()
                .any(|placed| placed.fragment.node_id() == header_id),
            "page {} must repeat the table header for this case to be the one we mean",
            checkpoint.page_index + 1
        );
        assert_resumes_identically(name, &galley, checkpoint);
    }
}

#[test]
fn paginate_from_the_document_start_equals_a_full_paginate() {
    // The degenerate checkpoint: resuming at page 0 is a full pagination, which
    // pins the page-number base and the empty table context.
    let config = letter_config();
    for (name, galley) in corpus() {
        let start = Checkpoint {
            page_index: 0,
            at: casual_doc_layout::page::FlowPos::at(0),
            current_table: None,
            table_headers: Vec::new(),
        };
        let full = paginate(&galley, &config);
        let resumed = paginate_from(&galley, &config, &start);
        assert_pages_equal(
            &format!("{name}: resuming at page 0 is a full paginate"),
            &resumed.pages,
            &full.pages,
            1,
        );
    }
}
