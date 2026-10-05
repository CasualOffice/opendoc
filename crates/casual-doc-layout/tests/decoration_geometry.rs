//! Where a mark that hangs off the text's baseline is allowed to sit.
//!
//! The owner's report: "the spelling zigzag is appear way below the content
//! line". The squiggle was placed in a `selectionRects` box — the LINE box,
//! `ascent + descent + leading` — and drawn at its bottom edge, so it walked
//! away from the text as the paragraph's line spacing grew. Measured through
//! this same pipeline before the fix, in px below the painted baseline at
//! 96 dpi on an 11pt line: **single 3.9, 1.15x 6.9, 1.5x 13.6, double 23.3**.
//! An 11pt misspelling sharing a line with 28pt text was marked 10.1 px down
//! instead of 3.9, because `Line::descent` is the maximum over the line's runs.
//!
//! # These guards assert a RELATIONSHIP, not a measured number
//!
//! A squiggle has ink wherever it is drawn, so "the squiggle is painted" passes
//! while it is 20 px too low — the same shape as the font lane's finding that a
//! tofu box carries 512 ink pixels. What is asserted here is that the mark's box
//! brackets the baseline THE GLYPHS WERE PAINTED ON, read from the composed
//! display list rather than recomputed, and that its offset from that baseline
//! does not change when the line box does. A guard pinned to "+59 twips" would
//! redden on a font change that removes nothing; a guard pinned to the
//! relationship cannot.
//!
//! # The competitive standard
//!
//! ONLYOFFICE draws the spelling line at the character underline's y, from the
//! run's own metrics: `sdkjs/word/Editor/Paragraph.js` computes
//! `UnderlineOffset = lineMetrics.TextDescent * 0.4`, and
//! `sdkjs/word/Editor/Paragraph/draw/line-draw-state.js`'s
//! `updateStrikeoutUnderlinePos` sets `underlineY = Baseline - yOffset +
//! UnderlineOffset` and feeds that same y to `Underline`, `DUnderline` and
//! `Spelling`. Their newer per-handler path agrees:
//! `CGraphics.prototype.drawCustomRange` in `sdkjs/word/Drawing/Graphics.js`
//! takes `baseLine` as a parameter and uses `0.1 * (baseLine - y0) + baseLine`
//! for a spelling range, `0.2 * …` for a grammar one. Neither reads the line
//! box. `TextDescent` is also not the line's `TextDescent + TextAscent +
//! LineGap`, which is what the same file uses for a review RECT — the leading is
//! deliberately excluded from the mark and included in the region fill.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::hittest::LayoutSnapshot;
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, LineRule, Paragraph, ParagraphProperties, Run,
    RunProperties, Spacing,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// One run, optionally at a non-default size (half-points, as `w:sz` carries it).
fn sized_run(id: u64, text: &str, half_points: Option<u32>) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties {
            size_half_points: half_points,
            ..RunProperties::default()
        }
        .into(),
        text: text.to_owned(),
    })
}

fn paragraph(id: u64, spacing: Option<Spacing>, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            spacing,
            ..ParagraphProperties::default()
        }
        .into(),
        inlines,
    })
}

/// `w:spacing@line="<percent>"` with the implicit `auto` rule — Word's
/// "Multiple" line spacing, which is what 1.15x, 1.5x and double are.
fn auto_multiple(percent: u16) -> Spacing {
    Spacing {
        line_percent: Some(percent),
        ..Spacing::default()
    }
}

/// What the harness measures for one paragraph.
struct Measured {
    /// The y the first glyph run was actually painted on, from the display list.
    painted_baseline: i32,
    /// The decoration box for the marked span.
    decoration: (i32, i32),
    /// The selection box for the same span.
    selection: (i32, i32),
    /// The decoration box's horizontal span.
    decoration_x: (i32, i32),
    /// The selection box's horizontal span.
    selection_x: (i32, i32),
    /// The caret box at the span's start.
    caret: (i32, i32),
    /// The line's own metrics, for the paired assertions.
    line: (i32, i32, i32),
}

/// Lays out a one-paragraph document and measures the span `start..end` of it.
fn measure(block: BlockNode, start: u32, end: u32) -> Measured {
    let id = match &block {
        BlockNode::Paragraph(p) => p.id,
        _ => panic!("the fixture is a paragraph"),
    };
    let doc = Document::new(node(1), vec![block], Definitions::default()).expect("a document");
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&doc, &shaper);
    let page = &layout.pages[0];
    let snap = LayoutSnapshot::new(&layout);
    let range = ModelRange::new(ModelPos::new(id, start), ModelPos::new(id, end));

    // The baseline the TEXT was painted from — the same display list the raster
    // backend consumes, not a number recomputed here.
    let painted_baseline = compose_page(page)
        .items
        .iter()
        .find_map(|item| match item {
            PaintItem::Glyphs { run } => Some(run.origin.y.raw()),
            _ => None,
        })
        .expect("the paragraph painted a glyph run");

    let decoration = snap.decoration_rects(range);
    let selection = snap.selection_rects(range);
    assert_eq!(decoration.len(), 1, "the fixture is one line");
    assert_eq!(selection.len(), 1, "the fixture is one line");
    let caret = snap.caret_rect(ModelPos::new(id, start)).expect("a caret");

    let line = match &page.placed[0].fragment {
        BlockFragment::Paragraph { lines, .. } => {
            let first = &lines.lines[0];
            (first.ascent.raw(), first.descent.raw(), first.height.raw())
        }
        _ => panic!("the fixture is a paragraph"),
    };

    Measured {
        painted_baseline,
        decoration: (
            decoration[0].1.origin.y.raw(),
            decoration[0].1.bottom().raw(),
        ),
        selection: (selection[0].1.origin.y.raw(), selection[0].1.bottom().raw()),
        decoration_x: (
            decoration[0].1.origin.x.raw(),
            decoration[0].1.right().raw(),
        ),
        selection_x: (selection[0].1.origin.x.raw(), selection[0].1.right().raw()),
        caret: (caret.1.origin.y.raw(), caret.1.bottom().raw()),
        line,
    }
}

/// `mispelled` at offsets 0..9 of an 11pt paragraph with the given spacing.
fn eleven_point(id: u64, spacing: Option<Spacing>) -> Measured {
    measure(
        paragraph(
            id,
            spacing,
            vec![sized_run(id + 1, "mispelled word here", None)],
        ),
        0,
        9,
    )
}

#[test]
fn a_mark_sits_in_the_marked_runs_descender_space_not_at_the_line_box_bottom() {
    for (name, spacing) in [
        ("single", None),
        ("1.15x", Some(auto_multiple(115))),
        ("1.5x", Some(auto_multiple(150))),
        ("double", Some(auto_multiple(200))),
    ] {
        let m = eleven_point(100, spacing);
        let (ascent, descent, height) = m.line;
        let below = m.decoration.1 - m.painted_baseline;

        // The mark's box brackets the PAINTED baseline: ascent above it, descent
        // below it, and nothing else.
        assert_eq!(
            m.decoration.0,
            m.painted_baseline - ascent,
            "{name}: the mark's top must be one ascent above the painted baseline",
        );
        assert!(
            below > 0 && below <= descent,
            "{name}: the mark must end inside the run's descender space \
             (ended {below} twips below the baseline, descent is {descent})",
        );

        // And it must not be the line box, which on a leaded line is strictly
        // taller. This is the assertion the owner's defect fails.
        assert_eq!(
            m.selection.1 - m.selection.0,
            height,
            "{name}: a SELECTION rect is still the line box — a region fill \
             covers the leading, which is what Word and Docs do",
        );
        if height > ascent + descent {
            assert!(
                m.decoration.1 < m.selection.1,
                "{name}: the mark ({:?}) must sit strictly above the line box \
                 bottom ({:?}) on a leaded line — this is the reported defect",
                m.decoration,
                m.selection,
            );
        }

        // Horizontally the two must be identical: the fix moves the mark
        // vertically and must not have moved it sideways.
        assert_eq!(
            m.decoration_x, m.selection_x,
            "{name}: the mark's horizontal span must match the selection's",
        );

        // The caret is hung off the same baseline, so it straddles it too.
        assert!(
            m.caret.0 <= m.painted_baseline && m.caret.1 > m.painted_baseline,
            "{name}: the caret must straddle the painted baseline (got {:?}, baseline {})",
            m.caret,
            m.painted_baseline,
        );
    }
}

#[test]
fn the_marks_offset_below_the_baseline_does_not_change_with_line_spacing() {
    // THE defect, as one number. The line box grew by 290 twips from single to
    // double (290 -> 580) and the mark went with it (+59 -> +349 below the
    // baseline, 3.9 px -> 23.3 px at 96 dpi). The text did not move.
    let offsets: Vec<(&str, i32, i32)> = [
        ("single", None),
        ("1.15x", Some(auto_multiple(115))),
        ("1.5x", Some(auto_multiple(150))),
        ("double", Some(auto_multiple(200))),
    ]
    .into_iter()
    .map(|(name, spacing)| {
        let m = eleven_point(200, spacing);
        (
            name,
            m.decoration.1 - m.painted_baseline,
            m.selection.1 - m.painted_baseline,
        )
    })
    .collect();

    let (_, first_mark, _) = offsets[0];
    for (name, mark, region) in &offsets {
        assert_eq!(
            *mark, first_mark,
            "{name}: the mark's distance below the baseline must be the same at \
             every line spacing (got {mark}, single is {first_mark})",
        );
        // The paired positive case: the LINE box really does move, so the test
        // above is not passing because the fixture has no leading to speak of.
        // A single-spaced line's box is `ascent + descent` to within the twip
        // each was rounded to, so the two bottoms can differ by a twip in
        // either direction there; anything more is leading.
        assert!(
            *region >= *mark - 1,
            "{name}: the line box bottom cannot be above the text box bottom \
             (region {region}, mark {mark})",
        );
    }
    let (_, _, single_region) = offsets[0];
    let (_, _, double_region) = offsets[3];
    assert!(
        double_region > single_region + 200,
        "the fixture must actually gain leading at double spacing, or the \
         invariance above proves nothing (single {single_region}, double {double_region})",
    );
}

#[test]
fn a_mark_is_charged_its_own_run_not_the_tallest_one_on_the_line() {
    // `Line::descent` is the maximum over the line's runs, so a line-box mark on
    // an 11pt word beside 28pt text sat 152 twips (10.1 px) below its baseline
    // instead of 59 (3.9 px) — two and a half times too far, on a line whose
    // spacing nobody touched.
    let mixed = measure(
        paragraph(
            300,
            None,
            vec![
                sized_run(301, "mispelled ", None),
                sized_run(302, "BIG", Some(56)),
            ],
        ),
        0,
        9,
    );
    let plain = eleven_point(310, None);

    assert!(
        mixed.line.1 > plain.line.1,
        "the fixture must really have a deeper LINE descent than the 11pt run \
         ({} vs {}), or this test proves nothing",
        mixed.line.1,
        plain.line.1,
    );
    assert_eq!(
        mixed.decoration.1 - mixed.painted_baseline,
        plain.decoration.1 - plain.painted_baseline,
        "the 11pt word must be marked at its own descent whether or not 28pt \
         text shares the line",
    );
    assert!(
        mixed.selection.1 - mixed.painted_baseline > mixed.decoration.1 - mixed.painted_baseline,
        "the line box on the mixed-size line really is lower than the mark",
    );
}

#[test]
fn an_at_least_line_anchors_the_mark_and_the_caret_on_the_painted_baseline() {
    // `apply_line_rule` puts a `w:lineRule="atLeast"` floor's extra space ABOVE
    // the baseline, so `top + line.ascent` is NOT the baseline there. Measured
    // before the fix: painted baseline 1861, `top + ascent` 1671 — the caret was
    // drawn 190 twips (12.7 px at 96 dpi) too high and ENDED ABOVE the baseline
    // it is supposed to straddle, and a mark hung from it would have been above
    // the text entirely.
    let m = measure(
        paragraph(
            400,
            Some(Spacing {
                line_rule: Some(LineRule::AtLeast),
                line_twips: Some(480),
                ..Spacing::default()
            }),
            vec![sized_run(401, "mispelled word here", None)],
        ),
        0,
        9,
    );
    let (ascent, descent, _) = m.line;

    // The precondition, stated: this line's baseline is NOT `top + ascent`, so
    // the assertions below cannot pass by accident on a line where it is.
    assert!(
        m.painted_baseline - ascent > m.selection.0,
        "the fixture must really have an atLeast floor above the baseline \
         (painted baseline {}, line top {}, ascent {ascent})",
        m.painted_baseline,
        m.selection.0,
    );

    assert_eq!(
        m.decoration.0,
        m.painted_baseline - ascent,
        "the mark's top is one ascent above the PAINTED baseline",
    );
    let below = m.decoration.1 - m.painted_baseline;
    assert!(
        below > 0 && below <= descent,
        "the mark ends inside the descender space ({below} twips below the baseline)",
    );
    assert!(
        m.caret.0 <= m.painted_baseline && m.caret.1 > m.painted_baseline,
        "the caret straddles the painted baseline (got {:?}, baseline {})",
        m.caret,
        m.painted_baseline,
    );
}

#[test]
fn an_empty_or_inverted_range_marks_nothing() {
    // Same contract as `selection_rects`, asserted so the two cannot drift.
    let block = paragraph(500, None, vec![sized_run(501, "mispelled word", None)]);
    let doc = Document::new(node(1), vec![block], Definitions::default()).expect("a document");
    let layout = paginate_document(&doc, &ParleyShaper::new());
    let snap = LayoutSnapshot::new(&layout);
    let at = |start, end| {
        ModelRange::new(
            ModelPos::new(node(500), start),
            ModelPos::new(node(500), end),
        )
    };
    assert!(snap.decoration_rects(at(3, 3)).is_empty(), "empty range");
    assert_eq!(
        snap.decoration_rects(at(7, 2)).len(),
        snap.selection_rects(at(7, 2)).len(),
        "an inverted range answers the same way both APIs do",
    );
    // The positive case, so none of the above passes by marking nothing ever.
    assert_eq!(snap.decoration_rects(at(0, 9)).len(), 1);
}
