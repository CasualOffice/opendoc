// SPDX-License-Identifier: Apache-2.0

//! Slide text: `a:txBody` folded through the placeholder cascade, shaped by the
//! **document** engine's shaper, and placed in the shape's rectangle.
//!
//! # The seam, and why there is no second one
//!
//! A slide paragraph and a DOCX text box's paragraph are shaped by the same
//! [`LineShaper`] into the same [`Line`]s, carried
//! by the same [`BlockFragment::Paragraph`] and painted by the same
//! [`AnchorContent::TextBox`] arm of
//! [`compose_anchors`](casual_doc_layout::compose::compose_anchors). Nothing here
//! paints a glyph, measures a face or breaks a line; this module's whole job is
//! the **translation** from DrawingML's property vocabulary into the shaper's.
//! That is deliberate: a second shaping path would diverge from the document's at
//! the first property either side gained, and `casual-doc-layout`'s
//! `finish_text_box` already proves the box-model arithmetic.
//!
//! What a slide genuinely does differently is named at each site: the vertical
//! anchor is `a:bodyPr@anchor` rather than `wps:bodyPr@anchor`, the insets are in
//! EMU rather than in `wps` EMU-as-`i32`, and the effective properties arrive
//! already folded by [`TextCascade`](casual_pres_model::TextCascade) instead of by the
//! Word style chain. Those are
//! inputs to one mechanism, not a branch inside it.
//!
//! # Units, which are the trap
//!
//! `a:rPr@sz` and `a:rPr@spc` are **hundredths of a point**, so twips are
//! `value / 5` (20 twips to a point). `a:spcPct` is thousandths of a percent;
//! `a:spcPts` is hundredths of a point. Every conversion below names its source
//! unit, because a size carried across unconverted is wrong by a factor of fifty
//! and still looks like a plausible font size.
//!
//! # What cannot be resolved here, and is reported rather than guessed
//!
//! Two properties reach layout unresolvable, and both are **carried into
//! [`UnresolvedTextProperty`]** rather than silently defaulted:
//!
//! * a `+mj-lt` / `+mn-lt` typeface. Resolving one needs `ppt/theme/theme1.xml`'s
//!   `a:fontScheme`, which no part of this engine reads yet. The run is shaped
//!   with the shaper's own bundled default — the alternative is painting nothing —
//!   and the authored name is reported so a caller can say the deck is not in its
//!   own font.
//! * `StyleColor::Placeholder` (`a:phClr`). It is a formal parameter standing for
//!   the referencing shape's colour, and nothing supplies that argument on a text
//!   run.
//!
//! **A theme colour (`a:schemeClr val="accent2"`) cannot be reported from here at
//! all**, and that is an upstream gap rather than a decision: `casual-pres-import`
//! resolves a scheme colour to `None` (its `ColorRead::style_color` keeps only
//! `a:phClr` and a concrete `a:srgbClr`), so by the time the model reaches layout
//! a theme colour is indistinguishable from a run that stated no colour. The
//! importer reports it as degraded; layout cannot.
//!
//! # The last-resort defaults, stated here because this is the draw site
//!
//! [`TextCascade`](casual_pres_model::TextCascade) deliberately supplies none — a chain that states no size
//! resolves to `None`, not to 18 points — because "inherited nothing" and
//! "inherited 18pt" must stay distinguishable. The default therefore belongs
//! here, where the shaper and its faces are known: [`LAST_RESORT_SIZE`] and
//! [`LAST_RESORT_COLOR`]. An unstated size is additionally reported, so a caller
//! never has to guess whether a size came from the file.
//!
//! # Deliberately out of scope for this slice
//!
//! Each of these is modelled, reaches here, and is **not** acted on. Stated so
//! the omission is a recorded decision rather than an ambiguity:
//!
//! * **Bullets and autonumbering.** `a:buChar`, `a:buAutoNum`, `a:buBlip`,
//!   `a:buClr`, `a:buSzPct` and `a:buFont` produce no marker glyph. The hanging
//!   indent they are authored with (`@marL` with a negative `@indent`) *is*
//!   honoured, so an unbulleted first line protrudes into the space the marker
//!   would have occupied — which is what the file says, and visibly incomplete.
//! * **Tab stops.** `a:tabLst` and `a:pPr@defTabSz` are ignored; a tab character
//!   inside an `a:t` advances by whatever the shaper gives it.
//! * **`a:normAutofit` re-solving.** The producer's recorded `@fontScale` **is**
//!   applied, because the model states both values are authoritative on load and
//!   honouring them is what makes an untouched file render as authored. Its
//!   `@lnSpcReduction` is **not**: reducing a line box needs a percentage
//!   baseline, and a paragraph that states no `a:lnSpc` has only the face's
//!   natural height, which is not a percentage. `a:spAutoFit` does not grow the
//!   shape.
//! * **Vertical and rotated text.** `a:bodyPr@vert` and `@rot` are not applied;
//!   text in a `vert` body paints upright. The carrier exists
//!   (`AnchorContent::TextBox::text_transform`) and the transposed-box arithmetic
//!   does not.
//! * **`a:bodyPr@anchorCtr`, `@numCol`, `@spcCol`.** No horizontal block centring
//!   and no text columns inside one shape.
//! * **`a:rPr@cap`.** `all`/`small` need a case rewrite of the run text, which is
//!   a transform on the characters rather than a property on the run.
//! * **`a:spcBef`/`a:spcAft` as `a:spcPct`.** The absolute `a:spcPts` form is
//!   applied exactly; the percentage form is not, because its base ("the size of
//!   the text") is per-line rather than per-paragraph and guessing it would move
//!   every paragraph after it.
//! * **An `a:br`'s own `a:rPr`.** A break opens a new line; the blank line's
//!   height comes from the following runs rather than from the break's own size.
//! * **Horizontal overflow.** `a:bodyPr@wrap="none"` is honoured in the shaping
//!   — the line is not wrapped — but the shared composition clips a text box to
//!   its own width whatever the content layout asks for, so the part of the line
//!   past the shape's edge is not painted. The divergence is `casual-doc-layout`'s
//!   `text_box_clip` and is recorded at the call site.
//! * **An empty `a:p`.** PowerPoint writes one for a blank line and gives it the
//!   height of its `a:endParaRPr`. Here it contributes no line and therefore no
//!   height, so a blank line between two paragraphs opens no gap. Shaping a
//!   zero-glyph line needs a face's metrics at a size, which the shaper exposes
//!   only by shaping something.
//!
//! # `a:bodyPr` does NOT inherit, and that is a model limit rather than a choice
//!
//! PowerPoint resolves `a:bodyPr` through the same placeholder chain the text
//! properties take, so a slide title that writes a bare `<a:bodyPr/>` still gets
//! its layout's `anchor="b"`. This slice reads the **shape's own** `a:bodyPr` and
//! nothing above it, because the chain is not expressible: every field of
//! `TextBodyProperties` is a plain value with DrawingML's default filled in at
//! import, so "the shape stated no `@anchor`" and "the shape stated `anchor="t"`"
//! are the same model. Inheriting would mean treating the schema default as
//! "unset" and letting a tier above override a shape that really did state `t`.
//!
//! Making it work needs `TextBodyProperties`' fields to become `Option`, which is
//! an upstream change in `casual-pres-model`. Until then a shape's own
//! `a:bodyPr` — which every non-placeholder shape writes in full — is honoured,
//! and a placeholder that delegates takes DrawingML's defaults.

use casual_doc_layout::block::{BlockFragment, BoxMetrics, BreakControl, ParagraphDecor};
use casual_doc_layout::page::AnchorContent;
use casual_doc_layout::text::{
    Decoration, Line, LineConstraints, LineLayout, LineShaper, StyledRun, TextAlignment,
    TextBoxContentLayout,
};
use casual_doc_layout::units::{Point, Rect, Twip, emu_to_twip_extent, emu_to_twip_offset};
use casual_doc_model::NodeId;
use casual_doc_model::v1::UnderlineStyle;
use casual_pres_model::{
    Presentation, ResolvedText, Slide, SlideNode, TextAlign, TextAnchor, TextAutoFit, TextBody,
    TextBodyProperties, TextCharacterProperties, TextRun, TextSpacing, TextStrike, TextUnderline,
    TextWrap,
};

/// The size a run is shaped at when **no tier of the cascade stated one**.
///
/// This is the engine's last resort and it lives at the draw site on purpose:
/// [`TextCascade`](casual_pres_model::TextCascade) resolves an unstated size to
/// `None` so that "the file said nothing" stays distinguishable from "the file
/// said 18pt", and only here are the shaper and its faces known. 18 points is
/// PowerPoint's own default for text outside any `a:defRPr`, and it matches the
/// `p:defaultTextStyle` that real decks write. Every use is reported as
/// [`UnresolvedProperty::UnstatedSize`], so a caller is never left to infer
/// whether the number came from the deck.
pub const LAST_RESORT_SIZE: Twip = Twip(360);

/// The colour a run is painted in when no tier stated one, or when the one it
/// stated is an unresolvable `a:phClr`.
///
/// Also a draw-site last resort, for the same reason as [`LAST_RESORT_SIZE`].
/// Opaque black, which is what the default Office theme's `tx1` resolves to; it
/// is **not** a resolution of the theme and must not be read as one. Painting
/// nothing instead would hide the text, which is a worse failure than painting it
/// in the wrong colour and saying so.
pub const LAST_RESORT_COLOR: [u8; 4] = [0, 0, 0, 255];

/// The inline width a body with `a:bodyPr@wrap="none"` is shaped against — wide
/// enough that nothing wraps, finite so the shaper's arithmetic stays in `i32`.
/// The same order of magnitude `casual-doc-layout`'s own unwrapped measurement
/// constraints use.
const NO_WRAP_WIDTH: Twip = Twip(1_000_000);

/// A text property a slide states and this engine cannot resolve.
///
/// Carried out of layout rather than logged, because the caller is the only layer
/// that can decide what to do with it: a viewer may warn, an exporter must not,
/// and a fidelity report counts it. The shape and paragraph are named so a report
/// can point at the offending text rather than at the deck.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedTextProperty {
    /// The shape whose `a:txBody` carries it.
    pub shape: NodeId,
    /// The paragraph within that body.
    pub paragraph: NodeId,
    /// The run being shaped when the property could not be resolved.
    ///
    /// `Option` rather than a bare id because the value may have been INHERITED
    /// rather than stated on that run — a `+mj-lt` on a master's `a:defRPr` is
    /// reported once per run that inherits it — and a later tier-level report
    /// would have no run to name. Every report this slice emits names one.
    pub run: Option<NodeId>,
    /// What could not be resolved.
    pub property: UnresolvedProperty,
}

/// What [`UnresolvedTextProperty`] reports.
///
/// Three variants and not four: a theme **colour** is absent deliberately, since
/// the importer resolves one to "no colour stated" before layout sees it — see the
/// module documentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnresolvedProperty {
    /// A `+mj-lt` / `+mn-lt` typeface, carried verbatim. The run was shaped with
    /// the shaper's bundled default instead.
    ThemeTypeface {
        /// The authored `@typeface`, e.g. `+mj-lt`.
        name: String,
    },
    /// An `a:phClr` glyph colour: a formal parameter with no argument on a text
    /// run. The run was painted in [`LAST_RESORT_COLOR`].
    PlaceholderColor,
    /// No tier of the cascade stated a font size. The run was shaped at
    /// [`LAST_RESORT_SIZE`].
    UnstatedSize,
}

/// One shape's text with the cascade already folded: the per-paragraph effective
/// properties and the per-run effective character properties.
///
/// Built once per shape, before the shape's rectangle is known, because the fold
/// is independent of geometry and the slot lookups behind it scan a layout's and a
/// master's children — doing them per paragraph would be `O(paragraphs x shapes)`,
/// which is the shape of the `O(n^2)` that shipped on the outline path.
pub(crate) struct PreparedText<'a> {
    /// The deck, for the one thing shaping needs from it that the cascade does not
    /// supply: `Presentation::resolve_typeface`, which turns a `+mj-lt` reference
    /// into the family the theme names. Carried here rather than threaded through
    /// three call sites, because `prepare` already holds it.
    presentation: &'a Presentation,
    /// The shape this text belongs to, for the unresolved report.
    shape: NodeId,
    /// `a:bodyPr`: the insets, the anchor, the wrap and the autofit scale.
    body: &'a TextBodyProperties,
    /// The paragraphs, in order.
    paragraphs: Vec<PreparedParagraph<'a>>,
}

/// One paragraph's effective properties and its runs, split at every `a:br`.
struct PreparedParagraph<'a> {
    /// The paragraph node, which is what the glyphs' cluster offsets index.
    id: NodeId,
    /// The cascade resolved at this paragraph's level, with its own `a:pPr`
    /// overlaid.
    effective: ResolvedText,
    /// The runs between hard breaks. Always at least one entry, which may be
    /// empty for a paragraph with no runs at all.
    segments: Vec<PreparedSegment<'a>>,
}

/// The runs of one hard-break-delimited stretch of a paragraph.
struct PreparedSegment<'a> {
    /// The UTF-8 byte offset of this segment within the paragraph's plain text,
    /// so a glyph's cluster addresses the paragraph and not the segment.
    base: u32,
    /// The runs, in order.
    runs: Vec<PreparedRun<'a>>,
}

/// One run with its effective character properties.
struct PreparedRun<'a> {
    /// The run node, for the unresolved report.
    id: NodeId,
    /// The characters, borrowed from the model.
    text: &'a str,
    /// The cascade's character properties with this run's own `a:rPr` overlaid.
    character: TextCharacterProperties,
}

/// Folds one shape's text through the placeholder cascade.
///
/// `None` when the shape carries no `a:txBody` at all, which is most shapes.
///
/// # Complexity
///
/// O(layouts + masters + shapes-per-tree) for the cascade, then O(runs).
pub(crate) fn prepare<'a>(
    presentation: &'a Presentation,
    slide: &'a Slide,
    node: &'a SlideNode,
) -> Option<PreparedText<'a>> {
    let text: &TextBody = node.text.as_ref()?;
    let cascade = presentation.text_cascade(slide, node);
    Some(prepare_body(presentation, &cascade, node.id(), text))
}

/// Folds ONE `a:txBody` through an already-built cascade.
///
/// Split out of [`prepare`] for the table-cell caller, which has a text body and
/// a cascade but no `SlideNode` of its own: a cell is not a shape, so
/// `Presentation::text_cascade` has nothing to take. Splitting rather than
/// copying is the point — a cell's runs are folded, segmented at every `a:br`
/// and shaped by the identical code a shape's are, so the two cannot drift.
///
/// # Complexity
///
/// O(runs in this body).
pub(crate) fn prepare_body<'a>(
    presentation: &'a Presentation,
    cascade: &casual_pres_model::TextCascade<'a>,
    owner: NodeId,
    text: &'a TextBody,
) -> PreparedText<'a> {
    let mut paragraphs = Vec::with_capacity(text.paragraphs.len());
    for paragraph in &text.paragraphs {
        // `a:pPr@lvl` selects the tier level, so it is resolved BEFORE the
        // paragraph's own properties are overlaid — overlaying first would read
        // the level off a value the fold had already changed.
        let mut effective = cascade.resolve(paragraph.level());
        if let Some(own) = paragraph.properties.as_deref() {
            effective.overlay_paragraph(own);
        }
        let mut segments = vec![PreparedSegment {
            base: 0,
            runs: Vec::new(),
        }];
        let mut offset = 0_u32;
        for run in &paragraph.runs {
            let text = run.text();
            let length = u32::try_from(text.len()).unwrap_or(u32::MAX);
            match run {
                TextRun::LineBreak(_) => {
                    // A break ends the segment. The next one starts after the
                    // `\n` the model's plain text carries for it, so the cluster
                    // offsets of the runs after a break still address the
                    // paragraph's own byte layout.
                    offset = offset.saturating_add(length);
                    segments.push(PreparedSegment {
                        base: offset,
                        runs: Vec::new(),
                    });
                }
                TextRun::Run(_) | TextRun::Field(_) => {
                    if !text.is_empty() {
                        // Only the character layer varies per run, so the
                        // paragraph layer is left at its default here rather than
                        // cloned per run: `overlay_run` touches nothing else, and
                        // a real deck has more runs than paragraphs.
                        let mut character = ResolvedText {
                            paragraph: casual_pres_model::TextParagraphProperties::default(),
                            character: effective.character.clone(),
                        };
                        if let Some(own) = run.properties() {
                            character.overlay_run(own);
                        }
                        let segment = segments
                            .last_mut()
                            .expect("a segment list always holds its first entry");
                        segment.runs.push(PreparedRun {
                            id: run.id(),
                            text,
                            character: character.character,
                        });
                    }
                    offset = offset.saturating_add(length);
                }
            }
        }
        paragraphs.push(PreparedParagraph {
            id: paragraph.id,
            effective,
            segments,
        });
    }
    PreparedText {
        presentation,
        shape: owner,
        body: &text.body_properties,
        paragraphs,
    }
}

/// Shapes one prepared body into the shape's rectangle.
///
/// `None` when the body produced no glyph at all — an empty placeholder, or a
/// body whose every run is whitespace the shaper drops. An anchor that paints
/// nothing is worse than no anchor: it would make "this shape has text" true for
/// every placeholder in the deck, which is exactly the vacuous assertion the
/// house rule warns about.
///
/// # Complexity
///
/// O(runs) plus the shaper's own cost per paragraph. Independent of the deck.
pub(crate) fn flow(
    prepared: &PreparedText<'_>,
    rect: Rect,
    shaper: &dyn LineShaper,
    report: &mut Vec<UnresolvedTextProperty>,
) -> Option<AnchorContent> {
    let body = prepared.body;
    let left = emu_to_twip_extent(body.inset_left_emu);
    let right = emu_to_twip_extent(body.inset_right_emu);
    let top = emu_to_twip_extent(body.inset_top_emu);
    let bottom = emu_to_twip_extent(body.inset_bottom_emu);
    let inner_width = Twip((rect.size.width.raw() - left.raw() - right.raw()).max(1));

    let (blocks, glyphs) = flow_blocks(prepared, inner_width, shaper, report);
    if glyphs == 0 {
        return None;
    }

    // The vertical anchor, which is `a:bodyPr@anchor` rather than the document's
    // `wps:bodyPr@anchor` — the same three positions, read from a different
    // attribute. The arithmetic is deliberately the same as
    // `casual-doc-layout`'s `finish_text_box`: free space inside the insets,
    // halved for a centre and spent whole for a bottom.
    let content_height = blocks
        .iter()
        .map(BlockFragment::height)
        .fold(Twip::ZERO, |sum, height| sum + height);
    let free = (i64::from(rect.size.height.raw())
        - i64::from(top.raw())
        - i64::from(bottom.raw())
        - i64::from(content_height.raw()))
    .max(0);
    let offset = match body.anchor {
        // `just` and `dist` distribute the paragraphs through the box rather than
        // placing the block, which needs per-paragraph spacing this slice does not
        // compute. Treated as `t`, which is where the block starts either way.
        TextAnchor::Top | TextAnchor::Justify | TextAnchor::Distribute => 0,
        TextAnchor::Center => free / 2,
        TextAnchor::Bottom => free,
    };

    Some(AnchorContent::TextBox {
        blocks,
        // The shape's own fill and outline were already painted by the anchor the
        // shared `GroupChild` walk emitted for it. Painting them again here would
        // double every slide's ink and would put the fill OVER the shape's
        // outline.
        fill: None,
        border: None,
        content_layout: TextBoxContentLayout {
            origin: Point::new(left, Twip(clamp(i64::from(top.raw()) + offset))),
            // DrawingML's default is that text OVERFLOWS its shape
            // (`a:noAutofit`), which is the opposite of a DOCX text box's
            // default, so neither axis asks to be clipped.
            //
            // The VERTICAL half of that is honoured. The horizontal half is NOT,
            // and not by this crate's choice: `compose_anchor`'s `text_box_clip`
            // takes the box's own width whatever `clip_horizontal` says, so a
            // line wider than its shape — which `wrap="none"` produces by
            // definition — is cut at the shape's edge. Inflating the anchor
            // rectangle to dodge it would move the shape, so the divergence is
            // recorded here instead: it belongs to the shared composition and is
            // a change for `casual-doc-layout`.
            clip_horizontal: false,
            clip_vertical: false,
        },
        backdrop: None,
        text_transform: None,
    })
}

/// Shapes every paragraph of a prepared body at `inner_width`, returning the
/// fragments and the total glyph count.
///
/// The glyph count is what distinguishes "this body laid out to nothing" from
/// "this body laid out", and both callers need the distinction: a shape with no
/// glyphs emits no anchor, and a table cell with no glyphs still owns its box but
/// contributes no content height.
///
/// # Complexity
///
/// O(runs) plus the shaper's cost per paragraph.
pub(crate) fn flow_blocks(
    prepared: &PreparedText<'_>,
    inner_width: Twip,
    shaper: &dyn LineShaper,
    report: &mut Vec<UnresolvedTextProperty>,
) -> (Vec<BlockFragment>, usize) {
    let body = prepared.body;
    let scale = font_scale(body.auto_fit);
    let mut blocks = Vec::with_capacity(prepared.paragraphs.len());
    let mut glyphs = 0_usize;
    for paragraph in &prepared.paragraphs {
        let fragment = flow_paragraph(
            prepared.presentation,
            prepared.shape,
            paragraph,
            body.wrap,
            inner_width,
            scale,
            shaper,
            report,
        );
        if let BlockFragment::Paragraph { lines, .. } = &fragment {
            glyphs += lines
                .lines
                .iter()
                .flat_map(|line| line.runs.iter())
                .map(|run| run.glyphs.len())
                .sum::<usize>();
        }
        blocks.push(fragment);
    }
    (blocks, glyphs)
}

/// Shapes one paragraph into a [`BlockFragment::Paragraph`].
#[allow(
    clippy::too_many_arguments,
    reason = "every argument is an independent input the fold does not carry"
)]
fn flow_paragraph(
    presentation: &Presentation,
    shape: NodeId,
    paragraph: &PreparedParagraph<'_>,
    wrap: TextWrap,
    inner_width: Twip,
    scale: u32,
    shaper: &dyn LineShaper,
    report: &mut Vec<UnresolvedTextProperty>,
) -> BlockFragment {
    let properties = &paragraph.effective.paragraph;
    let indent_start = emu_to_twip_extent(properties.margin_left_emu.unwrap_or(0));
    let indent_end = emu_to_twip_extent(properties.margin_right_emu.unwrap_or(0));
    // `@indent` is signed — every bulleted paragraph authors a negative one — so
    // it takes the offset conversion, not the extent one.
    let first_line_indent = emu_to_twip_offset(properties.indent_emu.unwrap_or(0));
    let column = Twip((inner_width.raw() - indent_start.raw() - indent_end.raw()).max(1));
    let (line_height_percent, line_exact) = line_spacing(properties.line_spacing);
    let constraints = LineConstraints {
        max_width: match wrap {
            TextWrap::Square => column,
            TextWrap::None => NO_WRAP_WIDTH,
        },
        // The full column before the paragraph's own margins come off it, which
        // is what the document engine calls the text margin. Only a positional
        // tab resolves against it, and slide tab stops are out of scope — but it
        // is the honest value rather than a copy of `max_width`.
        margin_width: inner_width,
        indent_start,
        rtl: properties.right_to_left.unwrap_or(false),
        alignment: alignment(properties.alignment),
        line_height_percent,
        line_at_least: None,
        line_exact,
        line_grid_pitch: None,
        first_line_indent,
    };

    let mut lines: Vec<Line> = Vec::new();
    let mut stacked = Twip::ZERO;
    for segment in &paragraph.segments {
        let styled: Vec<StyledRun<'_>> = segment
            .runs
            .iter()
            .map(|run| styled_run(presentation, shape, paragraph.id, run, scale, report))
            .collect();
        if styled.is_empty() {
            continue;
        }
        let end = segment.base.saturating_add(
            u32::try_from(segment.runs.iter().map(|run| run.text.len()).sum::<usize>())
                .unwrap_or(u32::MAX),
        );
        let range = casual_doc_layout::model::ModelRange::new(
            casual_doc_layout::model::ModelPos::new(paragraph.id, segment.base),
            casual_doc_layout::model::ModelPos::new(paragraph.id, end),
        );
        let layout = shaper.shape_paragraph(&styled, constraints, range);
        // A glyph run's origin is its baseline measured from the PARAGRAPH's
        // content top, not from its own line's top (that is what
        // `compose_paragraph_into` reads), so stacking a second segment under the
        // first means shifting its baselines by the height of every segment above
        // it — and by nothing else. The shaper has already spaced this segment's
        // own lines apart from each other.
        //
        // That distinction is the whole reason this is a call rather than a loop.
        // The loop that used to be here advanced the cursor by `line.height` once
        // per line *inside* the segment, so a segment's second line was shifted by
        // both the shaper's offset and this crate's copy of it and painted a whole
        // line box too low, over the paragraph below (`docs/109` HF-265). A
        // segment that held one line — which is every segment of a paragraph
        // written as `text <a:br/> text` — was unaffected, so the guards passed.
        // `stack_lines` takes the batch and advances the cursor once, after it.
        casual_doc_layout::text::stack_lines(&mut lines, layout.lines, &mut stacked);
    }

    BlockFragment::Paragraph {
        id: paragraph.id,
        lines: LineLayout { lines },
        box_metrics: BoxMetrics {
            space_before: absolute_spacing(properties.space_before),
            space_after: absolute_spacing(properties.space_after),
            indent_start,
            indent_end,
        },
        break_control: BreakControl::default(),
        // A slide paragraph has no `w:shd` and no `w:pBdr`: DrawingML authors a
        // background on the SHAPE, which the shape's own anchor already paints.
        decor: ParagraphDecor::default(),
    }
}

/// Translates one run's effective character properties into the shaper's
/// vocabulary, reporting whatever could not be resolved.
fn styled_run<'a>(
    presentation: &'a Presentation,
    shape: NodeId,
    paragraph: NodeId,
    run: &'a PreparedRun<'a>,
    scale: u32,
    report: &mut Vec<UnresolvedTextProperty>,
) -> StyledRun<'a> {
    let character = &run.character;
    let bold = character.bold.unwrap_or(false);
    let italic = character.italic.unwrap_or(false);

    let size = match character.size_hundredths_point {
        // Hundredths of a point to twips: 20 twips per point, so `/ 5`.
        Some(hundredths) => Twip(clamp(scaled(i64::from(hundredths) / 5, scale))),
        None => {
            report.push(UnresolvedTextProperty {
                shape,
                paragraph,
                run: Some(run.id),
                property: UnresolvedProperty::UnstatedSize,
            });
            Twip(clamp(scaled(i64::from(LAST_RESORT_SIZE.raw()), scale)))
        }
    };

    let color = match character.fill.as_ref() {
        // `a:phClr` with nothing to substitute. Reported, then painted in the
        // stated last resort rather than skipped.
        Some(fill) => match fill.resolve(None) {
            Some(rgba) => [rgba.r, rgba.g, rgba.b, rgba.a],
            None => {
                report.push(UnresolvedTextProperty {
                    shape,
                    paragraph,
                    run: Some(run.id),
                    property: UnresolvedProperty::PlaceholderColor,
                });
                LAST_RESORT_COLOR
            }
        },
        None => LAST_RESORT_COLOR,
    };

    // `a:latin` is the Latin-script face, which is the one the bundled faces can
    // answer. A `+mj-lt`/`+mn-lt` reference resolves through the deck's own
    // `a:fontScheme`, so the shaper is handed the family the THEME names — not the
    // token, which would make it search for a face literally called "+mj-lt", fail,
    // and fall back with the loss hidden.
    //
    // The run keeps storing the token; only the shaper's input is resolved.
    // Resolving it into the model would turn a theme reference into an authored
    // font: it would survive a reopen and stop following the theme.
    //
    // It is reported only when it resolves to NOTHING — a deck with no
    // `a:fontScheme`, or a scheme whose cell is empty. That is a real loss and
    // still nameable; a reference that resolved is not.
    //
    // One call, not a reference/concrete branch: `resolve_typeface` answers a
    // stated family with itself, so branching on `is_theme_reference` first was a
    // second path that could only ever agree with this one. Measured — removing the
    // branch changed no guard, which is what says it was redundant rather than
    // untested.
    let requested_family =
        character
            .latin
            .as_ref()
            .and_then(|latin| match presentation.resolve_typeface(latin) {
                Some(family) => Some(std::borrow::Cow::Borrowed(family)),
                None => {
                    report.push(UnresolvedTextProperty {
                        shape,
                        paragraph,
                        run: Some(run.id),
                        property: UnresolvedProperty::ThemeTypeface {
                            name: latin.name.clone(),
                        },
                    });
                    None
                }
            });

    StyledRun {
        text: std::borrow::Cow::Borrowed(run.text),
        requested_family,
        // The deck's font table is not read, so there is no declared generic
        // class to classify a missing face by.
        requested_family_kind: None,
        font: casual_doc_layout::fonts::face_id(bold, italic),
        size,
        character_scale_percent: 100,
        bold,
        italic,
        // `a:rPr@spc` is hundredths of a point, like `@sz`, and may be negative.
        letter_spacing: Twip(clamp(
            i64::from(character.spacing_hundredths_point.unwrap_or(0)) / 5,
        )),
        color,
        decoration: decoration(character),
        // `a:highlight` is not modelled, and DrawingML has no run shading.
        highlight: None,
        shading: None,
        // `a:rPr@baseline` is thousandths of a percent of the font size, positive
        // raised — the same sign convention `baseline_shift` uses.
        baseline_shift: Twip(clamp(
            i64::from(size.raw()) * i64::from(character.baseline_percent.unwrap_or(0)) / 100_000,
        )),
    }
}

/// The run's underline and strike, mapped onto the shared decoration record.
fn decoration(character: &TextCharacterProperties) -> Decoration {
    let underline = character.underline.unwrap_or(TextUnderline::None);
    let strike = character.strike.unwrap_or(TextStrike::None);
    Decoration {
        underline: !matches!(underline, TextUnderline::None),
        underline_style: match underline {
            // `heavy` has no shared variant; `Thick` is the same line.
            TextUnderline::Heavy => UnderlineStyle::Thick,
            TextUnderline::Double => UnderlineStyle::Double,
            TextUnderline::Dotted => UnderlineStyle::Dotted,
            TextUnderline::Dash => UnderlineStyle::Dashed,
            TextUnderline::Wavy => UnderlineStyle::Wavy,
            TextUnderline::Words => UnderlineStyle::Words,
            TextUnderline::None | TextUnderline::Single => UnderlineStyle::Single,
        },
        underline_color: None,
        strikethrough: !matches!(strike, TextStrike::None),
        double_strike: matches!(strike, TextStrike::Double),
        emphasis: None,
        border: None,
    }
}

/// `a:pPr@algn` onto the shaper's alignment.
///
/// Four of DrawingML's seven alignments collapse onto [`TextAlignment::Justify`],
/// and that is a stated loss rather than an oversight: `justLow` is Kashida
/// justification, and `dist`/`thaiDist` justify the last line too. The shaper's
/// vocabulary cannot express either difference, and left-aligning them instead
/// would be further from the file than justifying them is.
fn alignment(align: Option<TextAlign>) -> TextAlignment {
    match align.unwrap_or(TextAlign::Left) {
        TextAlign::Left => TextAlignment::Start,
        TextAlign::Center => TextAlignment::Center,
        TextAlign::Right => TextAlignment::End,
        TextAlign::Justify
        | TextAlign::JustifyLow
        | TextAlign::Distribute
        | TextAlign::ThaiDistribute => TextAlignment::Justify,
    }
}

/// `a:lnSpc` onto the shaper's two line-height channels.
///
/// The two forms are mutually exclusive in the schema and they map onto different
/// fields, which is why the model made them an enum: a percentage scales the
/// face's natural height (`line_height_percent`), an absolute measure replaces it
/// (`line_exact`).
fn line_spacing(spacing: Option<TextSpacing>) -> (Option<u16>, Option<Twip>) {
    match spacing {
        // Thousandths of a percent to percent.
        Some(TextSpacing::Percent { thousandths }) => (
            Some(u16::try_from(thousandths / 1_000).unwrap_or(u16::MAX)),
            None,
        ),
        // Hundredths of a point to twips.
        Some(TextSpacing::Points { hundredths }) => {
            (None, Some(Twip(clamp(i64::from(hundredths) / 5))))
        }
        None => (None, None),
    }
}

/// `a:spcBef`/`a:spcAft` in its absolute form only.
///
/// The percentage form yields zero: its base is the size of the text on the
/// adjoining line, which is a per-line quantity this slice does not carry, and
/// inventing a base would move every paragraph below it. Recorded in the module
/// header as out of scope.
fn absolute_spacing(spacing: Option<TextSpacing>) -> Twip {
    match spacing {
        Some(TextSpacing::Points { hundredths }) => Twip(clamp(i64::from(hundredths) / 5)),
        Some(TextSpacing::Percent { .. }) | None => Twip::ZERO,
    }
}

/// The producer's recorded `a:normAutofit@fontScale`, in thousandths of a
/// percent; `100_000` (100%) for every other autofit mode.
///
/// Honoured rather than re-solved. The producer solved the fit and wrote the
/// answer down, so applying its number is what makes an untouched file render as
/// authored; re-solving at open would replace PowerPoint's arithmetic with ours
/// and change the appearance of a file nobody edited.
fn font_scale(auto_fit: TextAutoFit) -> u32 {
    match auto_fit {
        TextAutoFit::Normal {
            font_scale: Some(scale),
            ..
        } => scale,
        TextAutoFit::Normal {
            font_scale: None, ..
        }
        | TextAutoFit::None
        | TextAutoFit::Shape => 100_000,
    }
}

/// Applies a thousandths-of-a-percent scale to a twip measure.
fn scaled(value: i64, scale: u32) -> i64 {
    value.saturating_mul(i64::from(scale)) / 100_000
}

/// Narrows to the twip domain without wrapping, the same clamp
/// `casual-doc-layout`'s own box-model arithmetic uses.
fn clamp(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
