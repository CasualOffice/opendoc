// SPDX-License-Identifier: Apache-2.0

//! Reduces a laid-out [`Page`] to the **text region** a PDF text extractor
//! reports for the same page, so our geometry can be compared, edge for edge,
//! against a renderer we do not control.
//!
//! # Why this quantity, and why it lives here
//!
//! Every fidelity judgement in this project used to be made by eye: render two
//! PNGs and look at them. That cannot tell a regression from a taste difference
//! and it cannot say *by how much*. The measurable alternative is the quantity
//! `pdftotext -bbox` reports per **word**:
//!
//! ```text
//! x: [pen start of the first non-space glyph, pen end of the last non-space glyph]
//! y: [baseline − face ascent,                 baseline + face descent]
//! ```
//!
//! That is *not* ink and it is *not* the paragraph box. Poppler derives it from
//! the PDF text state: `yMin`/`yMax` come from the font descriptor's
//! ascent/descent scaled to the run's size, `xMin`/`xMax` are pen positions.
//! Our shaped runs carry the same two numbers ([`GlyphRun::ascent`] /
//! [`GlyphRun::descent`], per run, from the same faces), so the oracle's quantity
//! is reproducible here **exactly** — no ink-vs-layout fudge factor is needed and
//! none is applied.
//!
//! This module exists because there were about to be two implementations of that
//! reduction: one inside the `casual-doc-render` oracle gate, one inside the
//! `opendoc-fidelity` comparison harness. Two implementations of one rule
//! diverge, and the divergence would be invisible — the gate would keep passing
//! while the harness reported different numbers for the same page. One mechanism,
//! consumed twice.
//!
//! # Font-parity scoping
//!
//! Comparing advances against another renderer is only sound for code points the
//! **pinned metric-compatible faces** (Liberation Sans/Serif/Mono, Carlito,
//! Caladea) cover on both sides. CJK, Arabic and emoji are substituted by
//! whatever each side happens to find, with different advances — and a wider
//! substitute also shifts every word after it on the same line. So text boxes are
//! grouped into lines by vertical overlap and a line holding any unpinned run is
//! **excluded** from the region; the vertical extent the excluded lines occupy is
//! reported separately ([`PageTextRegion::excluded_extent`]) so the filter cannot
//! silently swallow content the other side did measure.
//!
//! The extent rather than the line *count* is the comparable quantity: line
//! grouping is per-renderer (a tall list-marker box can merge two lines that the
//! oracle grouped as two), whereas the extent moves by a whole line height the
//! moment an exclusion eats real content.
//!
//! # Complexity
//!
//! O(g log g) for a page carrying `g` glyph runs — one sort to group runs into
//! lines, and a linear pass over each run's glyphs. Nothing here runs per
//! keystroke; it is a diagnostic reduction over an already-composed page.

use crate::compose::compose_page;
use crate::display::PaintItem;
use crate::font_registry::FontRegistry;
use crate::fonts::{
    CALADEA, CARLITO, LIBERATION_MONO, LIBERATION_SANS, LIBERATION_SERIF, family_name,
};
use crate::page::Page;
use crate::text::{FontId, GlyphRun};

/// One piece of painted text reduced to the box `pdftotext -bbox` reports per
/// word: pen extents horizontally, face ascent/descent about the baseline
/// vertically, in page-local twips.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBox {
    /// Left pen edge of the first non-space glyph.
    pub x0: i32,
    /// Baseline − the face's ascent.
    pub y0: i32,
    /// Right pen edge of the last non-space glyph.
    pub x1: i32,
    /// Baseline + the face's descent.
    pub y1: i32,
    /// The baseline itself, kept because a baseline divergence is the single
    /// most diagnostic vertical measurement — a whole-line slip and a leading
    /// difference look identical in `y0`/`y1` alone.
    pub baseline: i32,
    /// Whether this text shaped with a face the oracle also has, so its advances
    /// are pinned to the same metrics on both sides.
    pub pinned: bool,
    /// Whether the run carried its own ascent/descent. A tab leader deliberately
    /// does not (`tabs.rs` sets both to zero, meaning "use the line's"), so its
    /// box is horizontally real and vertically degenerate and must not be allowed
    /// to shrink the line it sits on.
    pub has_metrics: bool,
    /// Whitespace-delimited words this box contributes, counted the way a text
    /// extractor counts them (see [`PageTextRegion`]).
    pub words: usize,
    /// The font size, so a mixed-size line can be read back.
    pub size: i32,
    /// The resolved face, for reporting which font actually shaped the text.
    pub font: FontId,
}

/// A group of [`TextBox`]es that share a line, by vertical overlap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextLine {
    /// Left edge of the leftmost box.
    pub x0: i32,
    /// Top edge of the topmost box.
    pub y0: i32,
    /// Right edge of the rightmost box.
    pub x1: i32,
    /// Bottom edge of the bottommost box.
    pub y1: i32,
    /// The lowest baseline on the line (the dominant one for a mixed-size line).
    pub baseline: i32,
    /// Whether every box on the line shaped with a pinned face. A line that is
    /// not pinned is excluded from [`PageTextRegion::content_bbox`].
    pub pinned: bool,
    /// Words on the line, counted the way a PDF text extractor counts them: a
    /// word split across formatting runs (bold mid-word) counts **once**, while
    /// two runs separated by a space, a tab or a cell boundary count as two. The
    /// discriminator is the horizontal gap between the boxes — see the private
    /// `count_words` in this module for why that is exact rather than tuned.
    pub words: usize,
    /// The distinct resolved faces on the line, in first-seen order.
    pub fonts: Vec<FontId>,
}

/// A laid-out page reduced to its oracle-comparable text region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageTextRegion {
    /// The physical page size in twips, `[width, height]`.
    pub size: [i32; 2],
    /// The union of the pinned lines' boxes, `[x0, y0, x1, y1]`, or `None` when
    /// the page carries no comparable text.
    pub content_bbox: Option<[i32; 4]>,
    /// The summed vertical extent of the lines excluded for font parity.
    pub excluded_extent: i32,
    /// How many lines were excluded. **Review context only** — grouping is
    /// per-renderer, so this is not a comparable quantity.
    pub excluded_lines: usize,
    /// Every line on the page, excluded ones included, in top-to-bottom order.
    pub lines: Vec<TextLine>,
}

impl PageTextRegion {
    /// The lines that contribute to [`content_bbox`](Self::content_bbox).
    pub fn pinned_lines(&self) -> impl Iterator<Item = &TextLine> {
        self.lines.iter().filter(|line| line.pinned)
    }

    /// The distinct faces the page's text resolved to, as human-readable family
    /// names in first-seen order. A face interned at runtime (an embedded or
    /// host-supplied font) has no static family name and is reported as
    /// `dynamic(#id)` so it is visibly *not* one of the bundled families.
    #[must_use]
    pub fn font_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for font in self.lines.iter().flat_map(|line| &line.fonts) {
            let name = font_label(*font);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}

/// A human-readable label for a resolved face.
#[must_use]
pub fn font_label(font: FontId) -> String {
    family_name(font).map_or_else(
        || {
            if FontRegistry::is_dynamic(font) {
                format!("dynamic(#{})", font.0)
            } else {
                format!("unknown(#{})", font.0)
            }
        },
        ToOwned::to_owned,
    )
}

/// Whether `font` is one of the bundled **metric-compatible** faces an oracle
/// environment also installs (Liberation Sans/Serif/Mono, Carlito, Caladea).
///
/// Roboto is bundled but deliberately excluded: it is our native *default*, not a
/// substitute for anything another renderer would pick, so a run that fell back
/// to it has no font parity either. A dynamic (interned fallback / host / system)
/// face is outside every bundled block and so is excluded by construction.
#[must_use]
pub fn is_pinned_face(font: FontId) -> bool {
    [
        &CALADEA,
        &CARLITO,
        &LIBERATION_SANS,
        &LIBERATION_SERIF,
        &LIBERATION_MONO,
    ]
    .iter()
    .any(|family| family.contains(font))
}

/// Reduces one composed glyph run to its comparable box, or `None` when it
/// contributes no word to a PDF text layer — a run of pure whitespace, since a
/// text extractor emits *words* and a word carries no leading or trailing spaces.
///
/// A run with no face metrics cannot be measured vertically, so it is kept but
/// marked unpinned: that excludes its line (and shows in the excluded extent)
/// instead of silently vanishing from a region the oracle did measure.
///
/// **A tab leader is the one exception**, and it is less a special case than the
/// case the metrics test was guessing at. `tabs.rs` builds a leader with
/// `ascent`/`descent` of zero and says why: *"No shaped face here, so no run
/// metrics: zero means 'use the line's'."* Its face is known and real. Treating
/// it like an unmeasurable run cost the comparison every line carrying one —
/// measured on a real document, that was all fourteen lines of a table of
/// contents, so the harness went blind on a whole page while reporting nothing
/// wrong. A leader is therefore pinned by its face like any other run, and
/// contributes horizontally only.
fn run_text_box(run: &GlyphRun) -> Option<TextBox> {
    let has_metrics = run.ascent.raw() != 0 || run.descent.raw() != 0;
    let mut pen = run.origin.x.raw();
    let mut x0 = None;
    let mut x1 = pen;
    for glyph in &run.glyphs {
        if !glyph.is_whitespace {
            x0.get_or_insert(pen);
            x1 = pen + glyph.advance.raw();
        }
        pen += glyph.advance.raw();
    }
    Some(TextBox {
        x0: x0?,
        y0: run.origin.y.raw() - run.ascent.raw(),
        x1,
        y1: run.origin.y.raw() + run.descent.raw(),
        baseline: run.origin.y.raw(),
        pinned: (has_metrics || run.is_leader) && is_pinned_face(run.font),
        has_metrics,
        // Counted per line, not per run, so a word split across runs counts once.
        words: 0,
        size: run.size.raw(),
        font: run.font,
    })
}

/// Groups text boxes into lines by vertical overlap: sort by top edge, then start
/// a new line whenever a box begins at or below the running bottom of the current
/// one.
///
/// Mixed font sizes on one line still overlap vertically, so they group together.
/// Side-by-side content (table cells, columns) can merge into one band — that is
/// deliberately conservative: it can only *widen* what a single unshapeable run
/// excludes, never narrow it.
fn group_into_lines(mut boxes: Vec<(TextBox, Vec<bool>)>) -> Vec<Vec<(TextBox, Vec<bool>)>> {
    boxes.sort_by_key(|(b, _)| (b.y0, b.x0));
    let mut lines: Vec<Vec<(TextBox, Vec<bool>)>> = Vec::new();
    let mut bottom = i32::MIN;
    for entry in boxes {
        match lines.last_mut() {
            Some(line) if entry.0.y0 < bottom => {
                bottom = bottom.max(entry.0.y1);
                line.push(entry);
            }
            _ => {
                bottom = entry.0.y1;
                lines.push(vec![entry]);
            }
        }
    }
    lines
}

/// Counts the words on a line the way a PDF text extractor does: one word per
/// transition into non-whitespace, over the line's glyphs in visual order.
///
/// `boxes` are the line's boxes **sorted by `x0`**, each with its glyphs'
/// whitespace flags.
///
/// # Why a gap between boxes is a separator
///
/// A box's `x0`/`x1` are the pen positions of its first and last *non-space*
/// glyphs, so the whitespace at either end of a run is not in the flags that
/// survive here. Concatenating two boxes' flags directly therefore fuses the last
/// word of one to the first word of the next, and it does so on every line with
/// more than one run: on the corpus footnote fixture that was every single line,
/// each reporting exactly one word fewer than the oracle — a finding that fires
/// constantly is noise, and noise is how a gate gets ignored.
///
/// The discriminator is exact rather than tuned. Whitespace (or a tab, or a cell
/// boundary) between two runs advances the pen past the first run's `x1`, so the
/// next box starts strictly to its right. A run split for *formatting* mid-word —
/// bold in the middle of a word — leaves the next box starting exactly at `x1`,
/// and must stay one word. So: a strictly positive gap separates, an abutment
/// does not.
fn count_words(boxes: &[(TextBox, Vec<bool>)]) -> usize {
    let mut words = 0;
    let mut in_word = false;
    let mut previous_end: Option<i32> = None;
    for (text_box, flags) in boxes {
        if previous_end.is_some_and(|end| text_box.x0 > end) {
            in_word = false;
        }
        for &is_whitespace in flags {
            if is_whitespace {
                in_word = false;
            } else if !in_word {
                in_word = true;
                words += 1;
            }
        }
        previous_end = Some(text_box.x1);
    }
    words
}

/// Reduces one laid-out page to its comparable text region.
///
/// The text is taken from [`compose_page`] — the same display list the renderer
/// paints — so body text, running headers/footers, footnote bodies, table cells,
/// inline and floating text boxes are all already flattened into absolute
/// page-local coordinates by the one implementation that owns those transforms.
#[must_use]
pub fn page_text_region(page: &Page) -> PageTextRegion {
    let display = compose_page(page);
    let boxes: Vec<(TextBox, Vec<bool>)> = display
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => run_text_box(run).map(|text_box| {
                (
                    text_box,
                    run.glyphs.iter().map(|g| g.is_whitespace).collect(),
                )
            }),
            _ => None,
        })
        .collect();

    let mut content_bbox: Option<[i32; 4]> = None;
    let mut excluded_extent = 0;
    let mut excluded_lines = 0;
    let mut lines = Vec::new();

    for mut group in group_into_lines(boxes) {
        group.sort_by_key(|(b, _)| b.x0);
        let pinned = group.iter().all(|(b, _)| b.pinned);
        let x0 = group.iter().map(|(b, _)| b.x0).min().unwrap_or(0);
        let x1 = group.iter().map(|(b, _)| b.x1).max().unwrap_or(0);
        // Vertically, a box with no metrics of its own means "use the line's", so
        // it must not narrow the band. Only when NOTHING on the line carried
        // metrics is the degenerate extent all there is to report.
        let measured: Vec<&TextBox> = group
            .iter()
            .map(|(b, _)| b)
            .filter(|b| b.has_metrics)
            .collect();
        let vertical: Vec<&TextBox> = if measured.is_empty() {
            group.iter().map(|(b, _)| b).collect()
        } else {
            measured
        };
        let y0 = vertical.iter().map(|b| b.y0).min().unwrap_or(0);
        let y1 = vertical.iter().map(|b| b.y1).max().unwrap_or(0);
        let baseline = vertical.iter().map(|b| b.baseline).max().unwrap_or(0);
        let words = count_words(&group);
        let mut fonts: Vec<FontId> = Vec::new();
        for (text_box, _) in &group {
            if !fonts.contains(&text_box.font) {
                fonts.push(text_box.font);
            }
        }
        if pinned {
            content_bbox = Some(match content_bbox {
                None => [x0, y0, x1, y1],
                Some([bx0, by0, bx1, by1]) => [bx0.min(x0), by0.min(y0), bx1.max(x1), by1.max(y1)],
            });
        } else {
            excluded_lines += 1;
            excluded_extent += y1 - y0;
        }
        lines.push(TextLine {
            x0,
            y0,
            x1,
            y1,
            baseline,
            pinned,
            words,
            fonts,
        });
    }

    PageTextRegion {
        size: [page.page_size.width.raw(), page.page_size.height.raw()],
        content_bbox,
        excluded_extent,
        excluded_lines,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{LIBERATION_SERIF, ROBOTO};
    use crate::text::{Decoration, Glyph};
    use crate::units::{Point, Twip};

    fn glyph(advance: i32, is_whitespace: bool) -> Glyph {
        Glyph {
            id: 1,
            advance: Twip(advance),
            cluster: 0,
            is_whitespace,
        }
    }

    fn run(font: FontId, x: i32, baseline: i32, glyphs: Vec<Glyph>) -> GlyphRun {
        GlyphRun {
            font,
            size: Twip(240),
            ascent: Twip(200),
            descent: Twip(50),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point {
                x: Twip(x),
                y: Twip(baseline),
            },
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs,
            is_marker: false,
            node: None,
            is_leader: false,
        }
    }

    #[test]
    fn a_boxs_pen_extents_skip_leading_and_trailing_whitespace() {
        let r = run(
            LIBERATION_SERIF.face_id(false, false),
            1000,
            800,
            vec![
                glyph(50, true),
                glyph(100, false),
                glyph(100, false),
                glyph(50, true),
            ],
        );
        let b = run_text_box(&r).expect("a run with text has a box");
        assert_eq!(b.x0, 1050, "leading space is not inked");
        assert_eq!(b.x1, 1250, "trailing space is not inked");
        assert_eq!(b.y0, 600, "baseline − ascent");
        assert_eq!(b.y1, 850, "baseline + descent");
        assert_eq!(b.baseline, 800);
        assert!(b.pinned);
    }

    #[test]
    fn a_whitespace_only_run_contributes_no_box() {
        let r = run(
            LIBERATION_SERIF.face_id(false, false),
            1000,
            800,
            vec![glyph(50, true), glyph(50, true)],
        );
        assert!(run_text_box(&r).is_none());
    }

    #[test]
    fn a_run_with_no_face_metrics_cannot_be_measured_and_so_is_not_pinned() {
        let mut metricless = run(
            LIBERATION_SERIF.face_id(false, false),
            1000,
            800,
            vec![glyph(60, false)],
        );
        metricless.ascent = Twip(0);
        metricless.descent = Twip(0);
        let b = run_text_box(&metricless).expect("the run still carries a word");
        assert!(!b.pinned, "no metrics means no comparable vertical extent");
        assert!(!b.has_metrics);
    }

    #[test]
    fn a_tab_leader_is_pinned_by_its_face_and_does_not_collapse_its_line() {
        // A leader carries zero ascent/descent BY DESIGN (`tabs.rs`: "zero means
        // use the line's"). Before this was handled, the leader's degenerate box
        // marked the line unpinned and the whole line — a table-of-contents entry
        // — dropped out of every comparison.
        let face = LIBERATION_SERIF.face_id(false, false);
        let mut leader = run(face, 4000, 800, vec![glyph(40, false), glyph(40, false)]);
        leader.ascent = Twip(0);
        leader.descent = Twip(0);
        leader.is_leader = true;
        let leader_box = run_text_box(&leader).expect("the leader carries glyphs");
        assert!(
            leader_box.pinned,
            "a leader's face is known and real, so it is comparable"
        );
        assert!(!leader_box.has_metrics);

        // On a line with real text, the band comes from the text, not the leader.
        let text = run_text_box(&run(face, 1000, 800, vec![glyph(100, false)])).unwrap();
        let group = vec![(text, vec![false]), (leader_box, vec![false, false])];
        let lines = group_into_lines(group);
        assert_eq!(lines.len(), 1, "a zero-height leader still shares the line");
        assert!(lines[0].iter().all(|(b, _)| b.pinned));
    }

    #[test]
    fn roboto_is_not_a_pinned_face_but_the_metric_families_are() {
        assert!(!is_pinned_face(ROBOTO.face_id(false, false)));
        assert!(is_pinned_face(CARLITO.face_id(false, false)));
        assert!(is_pinned_face(LIBERATION_SERIF.face_id(true, true)));
        assert!(is_pinned_face(CALADEA.face_id(false, true)));
    }

    /// A box spanning `x0..x1` whose glyphs carry `flags`.
    fn counted(x0: i32, x1: i32, flags: &[bool]) -> (TextBox, Vec<bool>) {
        (
            TextBox {
                x0,
                y0: 0,
                x1,
                y1: 100,
                baseline: 80,
                pinned: true,
                has_metrics: true,
                words: 0,
                size: 240,
                font: LIBERATION_SERIF.face_id(false, false),
            },
            flags.to_vec(),
        )
    }

    #[test]
    fn a_word_split_across_runs_counts_once() {
        // One box: "hello world" — an internal space, two words.
        let one = [counted(0, 500, &[false, false, true, false, false])];
        assert_eq!(count_words(&one), 2);
        // Leading and repeated separators do not invent words.
        assert_eq!(
            count_words(&[counted(0, 500, &[true, true, false, true, true, false])]),
            2
        );
        assert_eq!(count_words(&[counted(0, 500, &[true, true])]), 0);
        assert_eq!(count_words(&[]), 0);
    }

    #[test]
    fn a_gap_between_boxes_separates_words_but_an_abutment_does_not() {
        // A box's x0/x1 are the pen positions of its first and last NON-SPACE
        // glyphs, so the space between two runs is not in either box's flags.
        // Concatenating them directly fused the last word of one to the first of
        // the next — measured on the corpus footnote fixture, that was every
        // single line reporting one word fewer than the oracle.
        let separated = [
            counted(0, 500, &[false, false]),
            counted(600, 900, &[false, false]),
        ];
        assert_eq!(count_words(&separated), 2, "a gap is a separator");

        // Bold in the middle of a word: the next run starts exactly where the
        // previous ended, and it is still one word.
        let abutting = [
            counted(0, 500, &[false, false]),
            counted(500, 900, &[false, false]),
        ];
        assert_eq!(count_words(&abutting), 1, "an abutment is not a separator");
    }

    #[test]
    fn lines_group_by_vertical_overlap_and_an_unpinned_run_excludes_its_line() {
        let pinned = LIBERATION_SERIF.face_id(false, false);
        let unpinned = ROBOTO.face_id(false, false);
        let boxes = vec![
            // Line 1: two boxes sharing a baseline, both pinned.
            (
                run_text_box(&run(pinned, 1000, 800, vec![glyph(100, false)])).unwrap(),
                vec![false],
            ),
            (
                run_text_box(&run(pinned, 2000, 800, vec![glyph(100, false)])).unwrap(),
                vec![false],
            ),
            // Line 2: one unpinned box, well below.
            (
                run_text_box(&run(unpinned, 1000, 1400, vec![glyph(100, false)])).unwrap(),
                vec![false],
            ),
        ];
        let lines = group_into_lines(boxes);
        assert_eq!(lines.len(), 2, "two vertically disjoint bands");
        assert_eq!(lines[0].len(), 2);
        assert!(lines[0].iter().all(|(b, _)| b.pinned));
        assert!(!lines[1][0].0.pinned);
    }

    #[test]
    fn a_dynamic_face_is_labelled_as_such_rather_than_named() {
        assert_eq!(font_label(CARLITO.face_id(false, false)), "Carlito");
        let dynamic = FontId(crate::font_registry::DYNAMIC_FONT_BASE);
        assert!(
            FontRegistry::is_dynamic(dynamic),
            "the test's id must be in the dynamic block for this to mean anything"
        );
        assert_eq!(
            font_label(dynamic),
            format!("dynamic(#{})", crate::font_registry::DYNAMIC_FONT_BASE)
        );
    }
}
