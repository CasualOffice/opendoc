//! Non-printing characters — Word's ¶ button, ONLYOFFICE's `mniHiddenChars`.
//!
//! The pilcrow at a paragraph mark, the arrow inside a tab's advance, the dot at
//! a space's centre, the return arrow at a line break, and the rule at a page or
//! column break. They are how anyone reads a layout they did not author, and
//! they are the first thing a Word user reaches for.
//!
//! # They are PAINT, and nothing else
//!
//! The one invariant this module exists to keep: **a mark cannot move anything.**
//! Concretely, and each of these is a property of the design rather than a
//! promise to be careful:
//!
//! - Marks never enter the document model, so they cannot export.
//! - Marks are produced during **composition** ([`crate::compose`]) — the
//!   display-list build — not during shaping, flow or pagination. Turning them on
//!   re-composes a page; it cannot repaginate one, because the flag is not an
//!   input to any function that measures. The galley, the page list and the
//!   geometry golden are the same objects either way.
//! - Marks are appended as a **suffix** of the page's display list, after every
//!   content item. So `compose_page_with(page, marks_on)` is literally
//!   `compose_page(page)` followed by extra items: the content prefix is
//!   byte-identical, which is what
//!   `crates/casual-doc-layout/tests/formatting_marks.rs` asserts.
//! - Marks are never caret positions, never selectable, and never hit-testable:
//!   [`crate::hittest`] reads the galley, which has no idea they exist.
//!
//! This is the **overlay / adornment layer** pattern — the same shape a code
//! editor's whitespace renderer or a browser's selection layer has: a decoration
//! derived from finished geometry, composited over it, owning no state of its own.
//!
//! ## Why a suffix rather than a nested layer
//!
//! The display list does have a grouping bracket
//! ([`PaintItem::PushLayer`]), and a
//! nested layer was the obvious answer. A suffix was chosen instead because the
//! list is painted back-to-front with no z-index, so *appending* already means
//! "on top", and a suffix gives the guard something a bracket cannot: the
//! content items are a **prefix**, comparable byte-for-byte against the
//! marks-off list with no filtering, no bracket matching and no way for the
//! guard to be fooled by an item the filter did not recognise. A `PushLayer`
//! would also have cost a composited surface per page for a decoration that
//! needs neither a transform nor a blend.
//!
//! The cost, stated: marks are outside the per-line clip that
//! `w:spacing@lineRule="exact"` installs, so on a line whose authored exact
//! height is smaller than its text, the mark is drawn where the text is clipped
//! away. That is the deliberate choice — a diagnostic overlay that is itself
//! hidden is useless, and the clipped text is already the visible defect.
//!
//! # Drawn as geometry, not as glyphs
//!
//! Word and ONLYOFFICE both draw these as characters: ONLYOFFICE emits U+00B6 /
//! U+00B7 from the run's own font and has to **bundle a private symbol font**
//! (`ASCW3`, ten glyphs, embedded as base64) for the tab and line-break arrows,
//! because ordinary text faces do not carry U+2192 or U+21B5.
//!
//! This module draws every mark from the display list's existing geometry
//! primitives instead. Three reasons, in order:
//!
//! 1. **Composition has no shaper, and should not get one.** `compose_page`
//!    takes a finished [`Page`](crate::page::Page); the display list is
//!    deliberately font- and backend-neutral. Handing the paint seam a text
//!    stack so it could shape five symbols would be a layering regression at the
//!    one place the engine keeps clean.
//! 2. **Determinism.** A glyph mark looks different per resolved face and turns
//!    into tofu on a face that lacks the codepoint — which is exactly why
//!    ONLYOFFICE had to bundle a font. Geometry is identical on every build,
//!    every platform and every document font.
//! 3. **Precedent.** [`crate::compose`]'s emphasis marks (`w:em`) already made
//!    this call for this reason.
//!
//! The cost, stated plainly: these are *approximations* of the glyph forms, not
//! the glyphs. The pilcrow is a filled lobe with two stems rather than a typeset
//! ¶, and the page-break rule carries no `Page Break` label because a label is
//! text. Both are recorded in `docs/153` rather than left ambiguous.
//!
//! # Deliberate differences from Word / ONLYOFFICE
//!
//! - **Every whitespace gets the same dot.** Word shows a raised circle for a
//!   non-breaking space and a different mark for the fixed-width spaces;
//!   ONLYOFFICE maps U+00A0/U+2002/U+2003 to `°` and U+2005 to `|`. Telling them
//!   apart needs the source character at paint time, and a
//!   [`GlyphRun`](crate::text::GlyphRun) deliberately carries a node anchor
//!   rather than its text (a copy per run would be paid by every document). The
//!   shaper's [`Glyph::is_whitespace`](crate::text::Glyph::is_whitespace) is what
//!   is available, so one dot is what is drawn. Adding a whitespace *kind* to the
//!   glyph is the follow-up; it costs no memory (the field packs into existing
//!   padding) and is not guessed at here.
//! - **No end-of-cell mark.** Word and ONLYOFFICE draw ¤ instead of ¶ on the last
//!   paragraph of a table cell. We draw ¶ there.
//! - **No section-break rule.** A section end is not visible from one page:
//!   composition is handed a single [`Page`](crate::page::Page), and a continuous
//!   section break falls in the middle of one with nothing on the line to say so.
//!   It needs a section-end flag on the fragment, which is a layout change, not a
//!   paint one. Page and column break rules, which a line *does* carry, are
//!   drawn.
//! - **Marks follow the text colour.** ONLYOFFICE hard-codes black for the tab
//!   and break marks while the pilcrow and space dot follow the run colour — so
//!   their tab arrows vanish on a dark page. One rule here: every mark takes the
//!   line's own ink unless the host overrides it.
//! - **An empty centred paragraph's pilcrow sits at the leading edge**, not at
//!   the centre where Word puts it. The line has no runs to measure a centre
//!   from.

use casual_doc_model::v1::DashStyle;

use crate::display::{Color, PaintItem, ShapeGeometry, ShapeOutline};
use crate::text::{Line, LineBreak, TabExtent};
use crate::units::{Point, Rect, Size, Twip};

/// One device pixel at 96 DPI, the floor for any mark dimension so a mark stays
/// visible at a small font size instead of rounding away.
const MIN_INK: Twip = Twip(15);

/// The em size assumed for a line that carries no measurable text run (an empty
/// paragraph still has a paragraph mark). 10pt.
const FALLBACK_EM: Twip = Twip(200);

/// Which non-printing characters to paint.
///
/// Word's ¶ button turns all of them on at once ([`FormattingMarks::ALL`]); its
/// *Display* options then expose them individually, which is why they are
/// individual flags here rather than one boolean. The default is all-off, so a
/// caller that does not ask gets exactly the display list it always got.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormattingMarks {
    /// The pilcrow at each paragraph mark.
    pub paragraph: bool,
    /// The arrow inside each tab's advance.
    pub tab: bool,
    /// The middle dot at each space.
    pub space: bool,
    /// The return arrow at each hard line break (`w:br`).
    pub line_break: bool,
    /// The rule at each page or column break (`w:br` type `page`/`column`).
    pub page_break: bool,
    /// The ink for every mark, or `None` to take the line's own text colour —
    /// which is the default, and what keeps marks visible on a dark page.
    pub color: Option<Color>,
}

impl FormattingMarks {
    /// Every mark — what the ¶ button does.
    ///
    /// O(1).
    pub const ALL: Self = Self {
        paragraph: true,
        tab: true,
        space: true,
        line_break: true,
        page_break: true,
        color: None,
    };

    /// Whether nothing at all is painted, in which case composition skips the
    /// whole pass.
    ///
    /// O(1).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.paragraph && !self.tab && !self.space && !self.line_break && !self.page_break
    }
}

/// The marks collected while a page is composed, to be appended to its display
/// list once the content is complete.
///
/// Held by [`crate::compose`] for the duration of one page and threaded through
/// the fragment recursion, so a mark inside a table cell, a header, a footnote or
/// a text box is collected by the same code that collects a body one — the
/// uniform-flow rule, with no second implementation to diverge.
///
/// Complexity: [`Self::line`] is O(runs + glyphs + tabs) **on that line**, so a
/// page costs O(visible glyphs) when space dots are on and O(visible lines)
/// otherwise. Nothing here is a function of document size and nothing looks
/// anything up by id.
#[derive(Debug)]
pub(crate) struct MarkLayer {
    marks: FormattingMarks,
    items: Vec<PaintItem>,
}

impl MarkLayer {
    /// A collector for `marks`. Cheap when `marks.is_empty()`: every
    /// [`Self::line`] call then returns immediately and no allocation happens.
    ///
    /// O(1).
    pub(crate) fn new(marks: FormattingMarks) -> Self {
        Self {
            marks,
            items: Vec::new(),
        }
    }

    /// Whether anything will be collected.
    ///
    /// O(1).
    pub(crate) const fn is_on(&self) -> bool {
        !self.marks.is_empty()
    }

    /// The collected paint items, in collection order.
    ///
    /// O(1) (moves the buffer).
    pub(crate) fn into_items(self) -> Vec<PaintItem> {
        self.items
    }

    /// Collects the marks for one shaped line.
    ///
    /// `origin` is the paragraph's content top-left in page coordinates,
    /// `line_top` the line's top relative to that, and `measure` the paragraph's
    /// content width (what a break rule spans). Run origins inside `line` are
    /// paragraph-relative, exactly as [`crate::compose::compose_paragraph`] reads
    /// them — so this derives every position from geometry the engine already
    /// settled and measures no text.
    ///
    /// Complexity: O(runs + glyphs + tab extents) on this line; O(runs + tabs)
    /// when space dots are off.
    pub(crate) fn line(&mut self, line: &Line, origin: Point, line_top: Twip, measure: Twip) {
        if !self.is_on() {
            return;
        }
        let style = LineInk::of(line, self.marks.color);
        let baseline = origin.y
            + line
                .runs
                .iter()
                .find(|run| !run.is_marker && !run.is_leader)
                .map_or(line_top + line.ascent, |run| run.origin.y);

        if self.marks.space {
            self.spaces(line, origin.x, baseline, style);
        }
        if self.marks.tab {
            for extent in &line.tab_extents {
                self.tab(*extent, origin.x, baseline, style);
            }
        }
        match line.line_break {
            LineBreak::ParagraphEnd if self.marks.paragraph => {
                let x = origin.x + content_end(line) + style.em_frac(1, 8);
                self.pilcrow(x, baseline, style);
            }
            LineBreak::Hard if self.marks.line_break => {
                let x = origin.x + content_end(line) + style.em_frac(1, 8);
                self.return_arrow(x, baseline, style);
            }
            LineBreak::Page | LineBreak::Column if self.marks.page_break => {
                self.break_rule(origin, line_top, line, measure, style);
            }
            _ => {}
        }
    }

    /// A filled middle dot centred on each whitespace cluster's own advance.
    ///
    /// The pen walks the run's glyphs once, which is the only way to know where a
    /// given glyph sits: a [`GlyphRun`](crate::text::GlyphRun) stores one origin
    /// and per-glyph advances, not per-glyph positions.
    fn spaces(&mut self, line: &Line, origin_x: Twip, baseline: Twip, style: LineInk) {
        let diameter = style.em_frac(1, 8).max(MIN_INK);
        let centre_y = baseline - style.em_frac(27, 100);
        for run in &line.runs {
            if run.is_marker || run.is_leader {
                // A bullet glyph and a tab-leader fill are rendering artifacts,
                // not model text; their spaces are not the document's spaces.
                continue;
            }
            let mut pen = origin_x + run.origin.x;
            for glyph in &run.glyphs {
                let advance = glyph.advance;
                if glyph.is_whitespace && advance.raw() > 0 {
                    let centre_x = Twip(pen.raw() + advance.raw() / 2);
                    self.items.push(PaintItem::Ellipse {
                        rect: centred_square(centre_x, centre_y, diameter),
                        fill: Some(style.ink),
                        stroke: None,
                    });
                }
                pen = pen + advance;
            }
        }
    }

    /// The tab arrow: a shaft plus a head, centred in the tab's advance when
    /// there is room for the whole arrow and otherwise shortened from the tail so
    /// the **head** stays visible.
    ///
    /// That is ONLYOFFICE's rule (they centre the glyph and crop its tail when
    /// the tab is narrower than the arrow), and it is the right one: the head is
    /// what identifies the mark and says which way the tab ran.
    fn tab(&mut self, extent: TabExtent, origin_x: Twip, baseline: Twip, style: LineInk) {
        let width = extent.width();
        let thickness = style.em_frac(1, 18).max(MIN_INK);
        // Below a readable minimum there is no arrow to draw, only a smear.
        if width.raw() <= thickness.raw() * 2 {
            return;
        }
        let nominal = style.em_frac(3, 4);
        let drawn = Twip(width.raw().min(nominal.raw()));
        let slack = width.raw() - drawn.raw();
        let left = Twip(origin_x.raw() + extent.start.raw() + slack / 2);
        let right = Twip(left.raw() + drawn.raw());
        let shaft_y = baseline - style.em_frac(22, 100);
        let head = Twip(style.em_frac(22, 100).raw().min(drawn.raw()));
        let half_height = style.em_frac(15, 100).max(thickness);
        // An RTL line's tab ran the other way, so the arrow points the other way.
        let (apex_x, base_x, shaft_from, shaft_to) = if style.rtl {
            (left, Twip(left.raw() + head.raw()), left, right)
        } else {
            (right, Twip(right.raw() - head.raw()), left, right)
        };
        self.items.push(PaintItem::Rect {
            rect: Rect::new(
                Point::new(shaft_from, Twip(shaft_y.raw() - thickness.raw() / 2)),
                Size::new(Twip(shaft_to.raw() - shaft_from.raw()), thickness),
            ),
            fill: Some(style.ink),
            stroke: None,
        });
        self.items.push(PaintItem::Polygon {
            points: vec![
                Point::new(apex_x, shaft_y),
                Point::new(base_x, Twip(shaft_y.raw() - half_height.raw())),
                Point::new(base_x, Twip(shaft_y.raw() + half_height.raw())),
            ],
            fill: Some(style.ink),
            stroke: None,
        });
    }

    /// The pilcrow: a filled lobe with two stems dropping from it to the
    /// baseline. An approximation of ¶ (module header), not the glyph.
    fn pilcrow(&mut self, x: Twip, baseline: Twip, style: LineInk) {
        let height = style.em_frac(66, 100);
        let bowl = style.em_frac(42, 100);
        let stem = style.em_frac(1, 14).max(MIN_INK);
        let gap = style.em_frac(16, 100);
        let top = baseline - height;
        self.items.push(PaintItem::Ellipse {
            rect: Rect::new(Point::new(x, top), Size::new(bowl, bowl)),
            fill: Some(style.ink),
            stroke: None,
        });
        for stem_x in [
            Twip(x.raw() + bowl.raw() - stem.raw()),
            Twip(x.raw() + bowl.raw() + gap.raw()),
        ] {
            self.items.push(PaintItem::Rect {
                rect: Rect::new(Point::new(stem_x, top), Size::new(stem, height)),
                fill: Some(style.ink),
                stroke: None,
            });
        }
    }

    /// The hard-line-break arrow: down, then left, with the head on the left —
    /// the shape of U+21B5, built from a stem, a bar and a triangle.
    fn return_arrow(&mut self, x: Twip, baseline: Twip, style: LineInk) {
        let thickness = style.em_frac(1, 14).max(MIN_INK);
        let width = style.em_frac(55, 100);
        let top = baseline - style.em_frac(60, 100);
        let bar_y = baseline - style.em_frac(14, 100);
        let head = style.em_frac(20, 100);
        let half_height = style.em_frac(14, 100).max(thickness);
        let right = Twip(x.raw() + width.raw());
        // The descending stem, at the right, from the top down to the bar.
        self.items.push(PaintItem::Rect {
            rect: Rect::new(
                Point::new(Twip(right.raw() - thickness.raw()), top),
                Size::new(thickness, Twip(bar_y.raw() - top.raw())),
            ),
            fill: Some(style.ink),
            stroke: None,
        });
        // The bar running back to the head.
        self.items.push(PaintItem::Rect {
            rect: Rect::new(
                Point::new(
                    Twip(x.raw() + head.raw()),
                    Twip(bar_y.raw() - thickness.raw() / 2),
                ),
                Size::new(Twip(right.raw() - x.raw() - head.raw()), thickness),
            ),
            fill: Some(style.ink),
            stroke: None,
        });
        self.items.push(PaintItem::Polygon {
            points: vec![
                Point::new(x, bar_y),
                Point::new(
                    Twip(x.raw() + head.raw()),
                    Twip(bar_y.raw() - half_height.raw()),
                ),
                Point::new(
                    Twip(x.raw() + head.raw()),
                    Twip(bar_y.raw() + half_height.raw()),
                ),
            ],
            fill: Some(style.ink),
            stroke: None,
        });
    }

    /// The page/column-break rule: a dashed line across the paragraph's content
    /// width at the break line's own vertical middle.
    ///
    /// Word centres the words `Page Break` in the rule. Painting a label needs a
    /// shaper composition does not have (module header), so the rule ships
    /// without it: the diagnostic — *the document breaks here, and it was
    /// authored, not reflowed* — is carried by the rule.
    fn break_rule(
        &mut self,
        origin: Point,
        line_top: Twip,
        line: &Line,
        measure: Twip,
        style: LineInk,
    ) {
        let y = Twip(origin.y.raw() + line_top.raw() + line.height.raw() / 2);
        let width = Twip(measure.raw().max(style.em_frac(2, 1).raw()));
        self.items.push(PaintItem::Shape {
            geometry: ShapeGeometry::Line {
                from: Point::new(origin.x, y),
                to: Point::new(Twip(origin.x.raw() + width.raw()), y),
            },
            fill: None,
            stroke: Some(ShapeOutline {
                color: style.ink,
                // Device pixels, like every other stroke in the display list: a
                // hairline is 1px at every zoom, which is what a rule wants.
                width: 1.0,
                dash: DashStyle::Dash,
                // A non-printing mark is this build's own chrome, not an `a:ln`, so
                // it states no cap, join, authored dash or compound form.
                cap: None,
                join: None,
                custom_dash: Vec::new(),
                compound: None,
            }),
            head_end: None,
            tail_end: None,
            transform: None,
        });
    }
}

/// The ink, em size and base direction a line's marks are drawn with, resolved
/// once per line from the line's own runs rather than per mark.
#[derive(Clone, Copy, Debug)]
struct LineInk {
    ink: Color,
    em: Twip,
    rtl: bool,
}

impl LineInk {
    /// Resolves a line's mark style. `override_color` is the host's choice, or
    /// `None` to take the line's own text colour so a mark stays visible whatever
    /// the page and the text are coloured.
    ///
    /// The **last** text run supplies the colour and size, because that is the
    /// run a paragraph mark follows in Word (the mark carries the paragraph's
    /// end-run properties). A line with no text run at all — an empty paragraph —
    /// falls back to black at [`FALLBACK_EM`].
    ///
    /// O(runs) on this line.
    fn of(line: &Line, override_color: Option<Color>) -> Self {
        let source = line
            .runs
            .iter()
            .rfind(|run| !run.is_marker && !run.is_leader);
        let rtl = line
            .runs
            .iter()
            .find(|run| !run.is_marker && !run.is_leader)
            .is_some_and(|run| run.bidi_level % 2 == 1);
        let ink = override_color.unwrap_or_else(|| {
            source.map_or(Color::BLACK, |run| Color {
                r: run.color[0],
                g: run.color[1],
                b: run.color[2],
                a: run.color[3],
            })
        });
        let em = source
            .map(|run| run.size)
            .filter(|size| size.raw() > 0)
            .unwrap_or(FALLBACK_EM);
        Self { ink, em, rtl }
    }

    /// `em * numerator / denominator`, floored at zero.
    ///
    /// O(1).
    fn em_frac(self, numerator: i32, denominator: i32) -> Twip {
        Twip((self.em.raw().max(0) * numerator / denominator).max(0))
    }
}

/// The x of the right edge of a line's painted content, relative to the
/// paragraph's content origin — where a paragraph mark or a return arrow goes.
///
/// Zero for a line with no runs, which puts an empty paragraph's pilcrow at the
/// leading edge (module header records the divergence for a centred one).
///
/// O(runs + glyphs) on this line.
fn content_end(line: &Line) -> Twip {
    line.runs
        .iter()
        .filter(|run| !run.is_marker)
        .map(|run| {
            run.origin.x
                + run
                    .glyphs
                    .iter()
                    .fold(Twip::ZERO, |sum, glyph| sum + glyph.advance)
        })
        .max()
        .unwrap_or(Twip::ZERO)
}

/// A square of `size` centred on `(cx, cy)`.
///
/// O(1).
fn centred_square(cx: Twip, cy: Twip, size: Twip) -> Rect {
    Rect::new(
        Point::new(
            Twip(cx.raw() - size.raw() / 2),
            Twip(cy.raw() - size.raw() / 2),
        ),
        Size::new(size, size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelPos, ModelRange};
    use crate::text::{Decoration, FontId, Glyph, GlyphRun};
    use casual_doc_model::NodeId;

    fn node() -> NodeId {
        NodeId::from_parts(1, 1).expect("1/1 is a valid node id")
    }

    fn run(x: i32, text_advances: &[(i32, bool)]) -> GlyphRun {
        GlyphRun {
            font: FontId(0),
            size: Twip(240),
            ascent: Twip(200),
            descent: Twip(40),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip(x), Twip(200)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs: text_advances
                .iter()
                .enumerate()
                .map(|(i, (advance, whitespace))| Glyph {
                    id: 1,
                    advance: Twip(*advance),
                    cluster: i as u32,
                    is_whitespace: *whitespace,
                })
                .collect(),
            is_marker: false,
            node: Some(node()),
            is_leader: false,
        }
    }

    fn line(runs: Vec<GlyphRun>, line_break: LineBreak) -> Line {
        Line {
            runs,
            ascent: Twip(200),
            descent: Twip(40),
            height: Twip(240),
            clip: false,
            range: ModelRange::new(ModelPos::new(node(), 0), ModelPos::new(node(), 0)),
            line_break,
            page_break_after: matches!(line_break, LineBreak::Page | LineBreak::Column),
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
            tab_extents: Vec::new(),
            charts: Vec::new(),
        }
    }

    fn collect(marks: FormattingMarks, line: &Line, measure: Twip) -> Vec<PaintItem> {
        let mut layer = MarkLayer::new(marks);
        layer.line(
            line,
            Point::new(Twip(1_440), Twip(1_440)),
            Twip::ZERO,
            measure,
        );
        layer.into_items()
    }

    #[test]
    fn an_empty_mark_set_collects_nothing() {
        let l = line(
            vec![run(0, &[(120, false), (60, true)])],
            LineBreak::ParagraphEnd,
        );
        assert!(collect(FormattingMarks::default(), &l, Twip(9_360)).is_empty());
        assert!(FormattingMarks::default().is_empty());
        assert!(!FormattingMarks::ALL.is_empty());
    }

    #[test]
    fn one_dot_per_whitespace_cluster_at_its_own_centre() {
        // "a b c": three text glyphs and two spaces, so two dots and no more.
        let l = line(
            vec![run(
                0,
                &[
                    (120, false),
                    (60, true),
                    (120, false),
                    (60, true),
                    (120, false),
                ],
            )],
            LineBreak::Wrap,
        );
        let items = collect(
            FormattingMarks {
                space: true,
                ..FormattingMarks::default()
            },
            &l,
            Twip(9_360),
        );
        let centres: Vec<i32> = items
            .iter()
            .map(|item| match item {
                PaintItem::Ellipse { rect, .. } => rect.origin.x.raw() + rect.size.width.raw() / 2,
                other => panic!("a space dot is an ellipse, got {other:?}"),
            })
            .collect();
        // Pen: 0,120,180,300,360. The spaces are at 120..180 and 300..360, so the
        // dots sit at 150 and 330, offset by the paragraph origin (1440).
        assert_eq!(centres, vec![1_440 + 150, 1_440 + 330]);
    }

    #[test]
    fn a_marker_or_leader_run_contributes_no_dots() {
        let mut marker = run(0, &[(60, true)]);
        marker.is_marker = true;
        let mut leader = run(200, &[(60, true), (60, true)]);
        leader.is_leader = true;
        let l = line(vec![marker, leader], LineBreak::Wrap);
        assert!(
            collect(
                FormattingMarks {
                    space: true,
                    ..FormattingMarks::default()
                },
                &l,
                Twip(9_360)
            )
            .is_empty(),
            "a bullet glyph and a tab-leader fill are not the document's spaces"
        );
    }

    #[test]
    fn the_tab_arrow_stays_inside_the_tabs_advance_and_keeps_its_head() {
        let mut wide = line(vec![run(0, &[(120, false)])], LineBreak::Wrap);
        wide.tab_extents.push(TabExtent {
            start: Twip(120),
            end: Twip(1_000),
        });
        let items = collect(
            FormattingMarks {
                tab: true,
                ..FormattingMarks::default()
            },
            &wide,
            Twip(9_360),
        );
        let (mut min_x, mut max_x) = (i32::MAX, i32::MIN);
        for item in &items {
            match item {
                PaintItem::Rect { rect, .. } => {
                    min_x = min_x.min(rect.origin.x.raw());
                    max_x = max_x.max(rect.right().raw());
                }
                PaintItem::Polygon { points, .. } => {
                    for point in points {
                        min_x = min_x.min(point.x.raw());
                        max_x = max_x.max(point.x.raw());
                    }
                }
                other => panic!("unexpected tab mark item {other:?}"),
            }
        }
        assert!(
            min_x >= 1_440 + 120 && max_x <= 1_440 + 1_000,
            "the arrow ({min_x}..{max_x}) must stay inside the tab's advance \
             ({}..{})",
            1_440 + 120,
            1_440 + 1_000
        );
        // A tab narrower than the nominal arrow is shortened from the TAIL: the
        // head still lands on the tab's right edge.
        let mut narrow = line(vec![run(0, &[(120, false)])], LineBreak::Wrap);
        narrow.tab_extents.push(TabExtent {
            start: Twip(120),
            end: Twip(220),
        });
        let narrow_items = collect(
            FormattingMarks {
                tab: true,
                ..FormattingMarks::default()
            },
            &narrow,
            Twip(9_360),
        );
        let apex = narrow_items
            .iter()
            .find_map(|item| match item {
                PaintItem::Polygon { points, .. } => Some(points[0].x.raw()),
                _ => None,
            })
            .expect("the arrow has a head");
        assert_eq!(apex, 1_440 + 220, "the head sits on the tab's right edge");
    }

    #[test]
    fn a_tab_that_advanced_nothing_paints_nothing() {
        let mut l = line(vec![run(0, &[(120, false)])], LineBreak::Wrap);
        l.tab_extents.push(TabExtent {
            start: Twip(120),
            end: Twip(120),
        });
        assert!(
            collect(
                FormattingMarks {
                    tab: true,
                    ..FormattingMarks::default()
                },
                &l,
                Twip(9_360)
            )
            .is_empty()
        );
    }

    #[test]
    fn each_line_ending_draws_its_own_mark_and_only_its_own() {
        let text = vec![run(0, &[(120, false)])];
        for (ending, expect_items) in [
            (LineBreak::ParagraphEnd, 3), // bowl + two stems
            (LineBreak::Hard, 3),         // stem + bar + head
            (LineBreak::Page, 1),         // one dashed rule
            (LineBreak::Column, 1),
            (LineBreak::Wrap, 0), // a soft wrap is not a mark
        ] {
            let l = line(text.clone(), ending);
            let items = collect(FormattingMarks::ALL, &l, Twip(9_360));
            assert_eq!(items.len(), expect_items, "{ending:?} produced {items:?}");
        }
    }

    #[test]
    fn a_mark_takes_the_lines_own_ink_unless_the_host_overrides_it() {
        let mut coloured = run(0, &[(120, false)]);
        coloured.color = [255, 0, 0, 255];
        let l = line(vec![coloured], LineBreak::ParagraphEnd);
        let default_ink = collect(FormattingMarks::ALL, &l, Twip(9_360));
        let fill = default_ink.iter().find_map(|item| match item {
            PaintItem::Ellipse { fill, .. } => *fill,
            _ => None,
        });
        assert_eq!(
            fill,
            Some(Color::rgb(255, 0, 0)),
            "white text keeps its marks"
        );
        let forced = collect(
            FormattingMarks {
                color: Some(Color::rgb(0, 0, 255)),
                ..FormattingMarks::ALL
            },
            &l,
            Twip(9_360),
        );
        let fill = forced.iter().find_map(|item| match item {
            PaintItem::Ellipse { fill, .. } => *fill,
            _ => None,
        });
        assert_eq!(fill, Some(Color::rgb(0, 0, 255)));
    }

    #[test]
    fn an_rtl_line_points_its_tab_arrow_the_other_way() {
        let mut rtl = run(0, &[(120, false)]);
        rtl.bidi_level = 1;
        let mut l = line(vec![rtl], LineBreak::Wrap);
        l.tab_extents.push(TabExtent {
            start: Twip(120),
            end: Twip(1_000),
        });
        let items = collect(
            FormattingMarks {
                tab: true,
                ..FormattingMarks::default()
            },
            &l,
            Twip(9_360),
        );
        let apex = items
            .iter()
            .find_map(|item| match item {
                PaintItem::Polygon { points, .. } => Some(points[0].x.raw()),
                _ => None,
            })
            .expect("the arrow has a head");
        let base = items
            .iter()
            .find_map(|item| match item {
                PaintItem::Polygon { points, .. } => Some(points[1].x.raw()),
                _ => None,
            })
            .expect("the arrow has a head");
        assert!(
            apex < base,
            "an RTL tab arrow points left ({apex} < {base})"
        );
    }

    #[test]
    fn an_empty_paragraph_still_gets_a_pilcrow_at_a_readable_size() {
        // The empty-line case: no runs at all, so the em and the ink both come
        // from the fallback rather than from a run that is not there.
        let l = line(Vec::new(), LineBreak::ParagraphEnd);
        let items = collect(FormattingMarks::ALL, &l, Twip(9_360));
        assert_eq!(items.len(), 3, "bowl and two stems");
        let heights: Vec<i32> = items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect { rect, .. } => Some(rect.size.height.raw()),
                _ => None,
            })
            .collect();
        assert!(
            heights.iter().all(|h| *h >= MIN_INK.raw()),
            "a fallback-size pilcrow is still visible: {heights:?}"
        );
    }
}
