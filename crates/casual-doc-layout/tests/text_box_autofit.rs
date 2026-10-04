//! The `a:normAutofit` re-solve: `docs/156` §6 Tier 0 row 0.7, `109` FID-L-04.
//!
//! Two behaviours that pull in opposite directions, which is why they are
//! guarded together in one file:
//!
//! 1. **On load the persisted `fontScale`/`lnSpcReduction` are authoritative.**
//!    Word and PowerPoint computed them with their own font metrics; reproducing
//!    the producer's own numbers is what makes a file render the way its author
//!    saw it. Re-solving at load would change what an unedited document looks
//!    like, which is a fidelity *regression* (`docs/156` §4.4 rank 4). So the
//!    first test asserts the rendered scale is the persisted one **and** that the
//!    solver's answer for the same box is a different number — i.e. it pins the
//!    gap between the two, so a solver that leaked into the load path cannot pass.
//! 2. **On edit the scale must be re-solved**, in both directions, because the
//!    authored scale now describes text that no longer exists.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::flow::{
    AUTOFIT_FONT_SCALE_STEP, AUTOFIT_LADDER_RUNGS, AUTOFIT_LINE_SPACING_REDUCTION_PER_RUNG,
    AUTOFIT_MAX_FONT_SCALE, AUTOFIT_MIN_FONT_SCALE, AUTOFIT_SOLVER_PASSES, build_galley,
    solve_text_box_normal_autofit,
};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Size;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, Extent, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties, Spacing, TextBox, TextBoxAutoFit, TextBoxBodyProperties,
};

/// EMU per twip (914400 EMU/inch ÷ 1440 twip/inch).
const EMU_PER_TWIP: i64 = 635;
/// The guarded box: 2 inches wide, 1 inch tall.
const BOX_WIDTH: i32 = 2_880;
const BOX_HEIGHT: i32 = 1_440;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// One text-box paragraph carrying **explicit single `auto` line spacing**, so
/// that the solved `lnSpcReduction` has something to reduce: the engine applies a
/// reduction only to percentage line spacing (`docs/52` layout rule 4), exactly
/// as DrawingML requires.
fn para(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties {
            spacing: Some(Spacing {
                line_percent: Some(100),
                ..Spacing::default()
            }),
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

/// `count` short paragraphs — the "text the user just typed" dial.
fn body(count: usize) -> Vec<BlockNode> {
    (0..count)
        .map(|i| para(1_000 + (i as u64) * 2, "autofit"))
        .collect()
}

fn outer() -> Size {
    Size::new(Twip(BOX_WIDTH), Twip(BOX_HEIGHT))
}

fn properties(auto_fit: TextBoxAutoFit) -> TextBoxBodyProperties {
    TextBoxBodyProperties {
        auto_fit,
        ..TextBoxBodyProperties::default()
    }
}

/// A document whose body is one paragraph holding one authored-size text box.
fn hosting(inner: Vec<BlockNode>, auto_fit: TextBoxAutoFit) -> Document {
    let text_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(20),
        anchor: None,
        relative_height: None,
        extent: Some(Extent {
            width_emu: i64::from(BOX_WIDTH) * EMU_PER_TWIP,
            height_emu: i64::from(BOX_HEIGHT) * EMU_PER_TWIP,
        }),
        fill: None,
        border: None,
        body_properties: properties(auto_fit),
        blocks: inner,
    }));
    Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![text_box],
        })],
        Definitions::default(),
    )
    .expect("a one-paragraph host document")
}

/// Renders the hosting document and reports what the box's text actually came
/// out as: the first shaped run's font size, and the stacked height of the box's
/// flowed content. These are the only two numbers a reader of an unedited
/// document can see change.
fn rendered(inner: Vec<BlockNode>, auto_fit: TextBoxAutoFit) -> (Twip, Twip) {
    let shaper = ParleyShaper::new();
    let document = hosting(inner, auto_fit);
    let galley = build_galley(&document, &shaper, Twip(9_360));
    let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
        panic!("expected a paragraph fragment");
    };
    let text_box = lines
        .lines
        .iter()
        .flat_map(|line| &line.text_boxes)
        .next()
        .expect("the text box was placed inline");
    let content_height = text_box
        .blocks
        .iter()
        .map(BlockFragment::height)
        .fold(Twip::ZERO, |a, h| a + h);
    let BlockFragment::Paragraph { lines, .. } = &text_box.blocks[0] else {
        panic!("expected the box's first block to be a paragraph");
    };
    let size = lines
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .next()
        .expect("the box's first paragraph shaped a run")
        .size;
    (size, content_height)
}

/// The solved autofit for `count` paragraphs in the guarded box, starting from
/// `persisted`.
fn solve(count: usize, persisted: TextBoxAutoFit) -> (u32, u32, u32) {
    let shaper = ParleyShaper::new();
    let inner = body(count);
    let document = hosting(inner.clone(), persisted);
    let solution =
        solve_text_box_normal_autofit(&document, &inner, &shaper, outer(), &properties(persisted));
    let TextBoxAutoFit::Normal {
        font_scale,
        line_spacing_reduction,
    } = solution.auto_fit
    else {
        panic!("a normAutofit box must solve to a normAutofit value");
    };
    (font_scale, line_spacing_reduction, solution.passes)
}

/// Guard 1 — the fidelity guard. An unedited document renders the producer's
/// numbers, not ours.
#[test]
fn an_unedited_text_box_renders_its_persisted_font_scale_and_is_never_re_solved() {
    let (unscaled, _) = rendered(body(1), TextBoxAutoFit::None);
    assert_eq!(
        unscaled,
        Twip(220),
        "the engine's default run size (11 pt), pinned so the halving below means something"
    );

    let persisted = TextBoxAutoFit::Normal {
        font_scale: 50_000,
        line_spacing_reduction: 20_000,
    };
    let (as_rendered, _) = rendered(body(1), persisted);
    assert_eq!(
        as_rendered,
        Twip(110),
        "load applies the authored 50% verbatim — it does not recompute it"
    );

    // The sharp half: the solver, asked about this very box, answers something
    // else. One short paragraph fits a 1-inch box unscaled, so a re-solve would
    // render the text at full size and the assertion above would read 220. The
    // two numbers being different is what makes the load path's silence
    // observable rather than assumed.
    let (font_scale, line_spacing_reduction, _) = solve(1, persisted);
    assert_eq!(
        (font_scale, line_spacing_reduction),
        (AUTOFIT_MAX_FONT_SCALE, 0),
        "the solver would clear the shrink for text that fits"
    );
}

/// Guard 2 — editing text in so it no longer fits shrinks the scale.
#[test]
fn editing_text_in_until_it_overflows_re_solves_the_scale_down() {
    let (fits, _, _) = solve(
        3,
        TextBoxAutoFit::Normal {
            font_scale: AUTOFIT_MAX_FONT_SCALE,
            line_spacing_reduction: 0,
        },
    );
    assert_eq!(
        fits, AUTOFIT_MAX_FONT_SCALE,
        "three short lines fit a 1-inch box unscaled"
    );

    let (overflowing, reduction, _) = solve(
        9,
        TextBoxAutoFit::Normal {
            font_scale: AUTOFIT_MAX_FONT_SCALE,
            line_spacing_reduction: 0,
        },
    );
    assert!(
        overflowing < AUTOFIT_MAX_FONT_SCALE,
        "nine lines cannot fit a 1-inch box unscaled; the solver must shrink \
         (got {overflowing})"
    );
    assert!(
        overflowing > AUTOFIT_MIN_FONT_SCALE,
        "nine lines do not need the floor either, so this exercises a middle \
         rung rather than the clamp (got {overflowing})"
    );
    assert!(
        reduction > 0,
        "a shrunk box also carries the coupled line-spacing reduction"
    );

    // And the floor is reachable: far more text than the box can ever hold stops
    // at the bottom rung instead of chasing the schema minimum.
    let (floored, _, _) = solve(
        60,
        TextBoxAutoFit::Normal {
            font_scale: AUTOFIT_MAX_FONT_SCALE,
            line_spacing_reduction: 0,
        },
    );
    assert_eq!(
        floored, AUTOFIT_MIN_FONT_SCALE,
        "text that overflows even at 25% is left at 25%"
    );
}

/// Guard 3 — the other direction. A one-way solver is the common half-fix: it
/// shrinks on the way in and leaves the box permanently small after a delete.
#[test]
fn editing_text_out_until_it_fits_again_re_solves_the_scale_back_up() {
    let stale = TextBoxAutoFit::Normal {
        font_scale: AUTOFIT_MIN_FONT_SCALE,
        line_spacing_reduction: AUTOFIT_LINE_SPACING_REDUCTION_PER_RUNG
            * (AUTOFIT_LADDER_RUNGS - 1),
    };
    let (restored, reduction, _) = solve(2, stale);
    assert_eq!(
        (restored, reduction),
        (AUTOFIT_MAX_FONT_SCALE, 0),
        "two short lines fit unscaled, so a stale 25% shrink must be cleared, \
         not kept"
    );

    // A partial delete lands on a middle rung, not just at the two extremes.
    let (middle, _, _) = solve(9, stale);
    assert!(
        middle > AUTOFIT_MIN_FONT_SCALE && middle < AUTOFIT_MAX_FONT_SCALE,
        "nine lines re-solve upward off the floor but not all the way \
         (got {middle})"
    );
}

/// Guard 4 — the solved values stay on the quantisation grid, and the
/// line-spacing reduction stays coupled to the font scale.
#[test]
fn every_solved_scale_lands_on_the_five_point_ladder_with_its_coupled_reduction() {
    assert_eq!(
        AUTOFIT_LADDER_RUNGS, 16,
        "100% down to 25% in 5-point steps is 16 rungs"
    );
    for count in 1..=24 {
        let (font_scale, reduction, _) = solve(
            count,
            TextBoxAutoFit::Normal {
                font_scale: AUTOFIT_MAX_FONT_SCALE,
                line_spacing_reduction: 0,
            },
        );
        assert!(
            (AUTOFIT_MIN_FONT_SCALE..=AUTOFIT_MAX_FONT_SCALE).contains(&font_scale),
            "{count} paragraphs solved off the ladder's ends: {font_scale}"
        );
        let deficit = AUTOFIT_MAX_FONT_SCALE - font_scale;
        assert_eq!(
            deficit % AUTOFIT_FONT_SCALE_STEP,
            0,
            "{count} paragraphs solved to an off-grid scale: {font_scale}"
        );
        assert_eq!(
            reduction,
            (deficit / AUTOFIT_FONT_SCALE_STEP) * AUTOFIT_LINE_SPACING_REDUCTION_PER_RUNG,
            "{count} paragraphs: lnSpcReduction is 20% of the fontScale deficit"
        );
    }
}

/// Guard 5 — monotone. More text never solves to a larger scale. This is the
/// predicate the bisection is bisecting; if it is not monotone, or the search's
/// comparison is inverted, the sequence stops descending.
#[test]
fn the_solved_scale_is_monotone_in_the_amount_of_text() {
    let mut previous = AUTOFIT_MAX_FONT_SCALE;
    for count in 1..=24 {
        let (font_scale, _, _) = solve(
            count,
            TextBoxAutoFit::Normal {
                font_scale: AUTOFIT_MAX_FONT_SCALE,
                line_spacing_reduction: 0,
            },
        );
        assert!(
            font_scale <= previous,
            "{count} paragraphs solved to {font_scale}, larger than the \
             {previous} solved for {} paragraphs",
            count - 1
        );
        previous = font_scale;
    }
    assert!(
        previous < AUTOFIT_MAX_FONT_SCALE,
        "the sweep must actually descend, or the monotonicity above is vacuous"
    );
}

/// Guard 6 — the complexity guard, counted in layout passes rather than
/// milliseconds. Doubling the text box's size must not buy the solver more
/// iterations: bisection on a fixed ladder is `O(log rungs)` and `log rungs` is a
/// constant, so the pass count is the same for every input.
#[test]
fn the_solver_pass_count_does_not_grow_with_the_amount_of_text() {
    let from_scratch = TextBoxAutoFit::Normal {
        font_scale: AUTOFIT_MAX_FONT_SCALE,
        line_spacing_reduction: 0,
    };
    let mut counts = Vec::new();
    for count in [8usize, 16, 32, 64] {
        let (_, _, passes) = solve(count, from_scratch);
        counts.push((count, passes));
    }
    for (count, passes) in &counts {
        assert_eq!(
            *passes, AUTOFIT_SOLVER_PASSES,
            "{count} paragraphs cost {passes} fit evaluations; the ladder has \
             {AUTOFIT_LADDER_RUNGS} rungs so a bisection costs \
             {AUTOFIT_SOLVER_PASSES} whatever the content is. Observed: \
             {counts:?}"
        );
    }
}

/// Guard 7 — the solver does nothing to a box that is not `normAutofit`, and
/// nothing to a `normAutofit` box with no height to overflow.
#[test]
fn the_solver_leaves_alone_what_it_does_not_own() {
    let shaper = ParleyShaper::new();
    let inner = body(40);
    for untouched in [TextBoxAutoFit::None, TextBoxAutoFit::Shape] {
        let document = hosting(inner.clone(), untouched);
        let solution = solve_text_box_normal_autofit(
            &document,
            &inner,
            &shaper,
            outer(),
            &properties(untouched),
        );
        assert_eq!(
            solution.auto_fit, untouched,
            "{untouched:?} shrinks nothing: noAutofit keeps its size and \
             spAutoFit grows the shape"
        );
        assert_eq!(solution.passes, 0, "and costs no layout to decide");
    }

    let stale = TextBoxAutoFit::Normal {
        font_scale: AUTOFIT_MIN_FONT_SCALE,
        line_spacing_reduction: 15_000,
    };
    let document = hosting(inner.clone(), stale);
    let unbounded = solve_text_box_normal_autofit(
        &document,
        &inner,
        &shaper,
        Size::new(Twip(BOX_WIDTH), Twip::ZERO),
        &properties(stale),
    );
    assert_eq!(
        unbounded.auto_fit,
        TextBoxAutoFit::Normal {
            font_scale: AUTOFIT_MAX_FONT_SCALE,
            line_spacing_reduction: 0,
        },
        "with no authored height the box grows to its content, so there is \
         nothing to shrink into and the stale scale is cleared"
    );
    assert_eq!(unbounded.passes, 0, "and no fit evaluation was needed");
}

/// Guard 8 — the second axis is real. `lnSpcReduction` is not decoration: the
/// solved reduction reaches percentage line spacing and makes the content
/// shorter, which is why the solver is entitled to count it when it decides a
/// rung fits.
#[test]
fn the_coupled_line_spacing_reduction_actually_shortens_the_content() {
    let plain = TextBoxAutoFit::Normal {
        font_scale: 50_000,
        line_spacing_reduction: 0,
    };
    let reduced = TextBoxAutoFit::Normal {
        font_scale: 50_000,
        line_spacing_reduction: 10_000,
    };
    let (plain_size, plain_height) = rendered(body(6), plain);
    let (reduced_size, reduced_height) = rendered(body(6), reduced);
    assert_eq!(
        plain_size, reduced_size,
        "the reduction must not change the font size — that is fontScale's job"
    );
    assert!(
        reduced_height < plain_height,
        "a 10-point line-spacing reduction must shorten six auto-spaced lines \
         (got {reduced_height:?} vs {plain_height:?})"
    );
}
