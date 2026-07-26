//! The default [`LineShaper`] — a `parley` (HarfBuzz + Unicode) implementation.
//!
//! `parley` shapes each run, applies the Unicode bidi and line-breaking
//! algorithms, and breaks the paragraph into lines; this adapter maps its output
//! into the crate's device-independent [`crate::text`] types. The shaper works
//! entirely in twips: run sizes are fed to `parley` in twips (with `scale = 1`),
//! so every advance, metric, and offset it returns is already in twips.
//!
//! Fonts: to stay deterministic and WASM-safe (`43-…` §1 decision 5), the shaper
//! registers a single bundled Apache-2.0 font into an *empty* font collection
//! (no system-font discovery). Fuller font resolution — multiple faces, DOCX
//! font-name matching, fallback — is `P1C-002` (`40-FONT-MANAGEMENT-DESIGN.md`).

use std::cell::RefCell;
use std::sync::Arc;

use parley::fontique::Blob;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, IndentOptions,
    LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
};

use crate::model::{ModelPos, ModelRange};
use crate::text::{
    Decoration, FontId, Glyph, GlyphRun, Line, LineBreak, LineConstraints, LineLayout, LineShaper,
    StyledRun, TextAlignment,
};
use crate::units::{Point, Twip};

/// Per-run data carried through `parley` and recovered from the shaped layout.
/// `Brush` is blanket-implemented for any `Clone + PartialEq + Default + Debug`,
/// so this struct is a valid brush and round-trips the run's fill color and the
/// resolved [`FontId`] (so the renderer can outline the exact face).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RunBrush {
    color: [u8; 4],
    font: u32,
    /// The run's resolved highlight fill (RGBA); alpha `0` means no highlight.
    highlight: [u8; 4],
    /// Baseline shift in twips (positive = raised); subtracted from the run's
    /// glyph-run origin so super/subscript and `w:position` offsets survive shaping.
    baseline_shift: i32,
}

/// The default `parley`-backed line shaper.
///
/// `parley`'s builder needs `&mut` access to its font and layout contexts, while
/// [`LineShaper::shape_paragraph`] takes `&self` (shaping is logically pure — the
/// contexts are caches); the contexts therefore live behind `RefCell`.
pub struct ParleyShaper {
    fonts: RefCell<FontContext>,
    layout_cx: RefCell<LayoutContext<RunBrush>>,
    default_family: String,
    /// The registered family name for each bundled family, paired with the family's
    /// base [`FontId`], in ascending `base` order — used to push the resolved
    /// family per run so `parley` shapes with the same face the resolver chose.
    families: Vec<(u32, String)>,
}

impl ParleyShaper {
    /// Creates a shaper with every bundled family (Roboto and Caladea, each
    /// regular/bold/italic/bold-italic) registered into an empty collection (no
    /// system fonts — deterministic). Each run pushes its resolved family plus
    /// weight/style, so `parley` selects the same face the resolver did; the run's
    /// [`FontId`] rides the brush so the renderer draws the same.
    #[must_use]
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        let mut families: Vec<(u32, String)> = Vec::with_capacity(crate::fonts::FAMILIES.len());
        for family in crate::fonts::FAMILIES {
            let mut family_id = None;
            for offset in 0..4u32 {
                let bytes = family.face_bytes(offset);
                let registered = fonts
                    .collection
                    .register_fonts(Blob::new(Arc::new(bytes.to_vec())), None);
                if family_id.is_none() {
                    family_id = registered.first().map(|(id, _)| *id);
                }
            }
            let name = family_id
                .and_then(|id| fonts.collection.family_name(id).map(str::to_owned))
                .unwrap_or_else(|| family.name.to_owned());
            families.push((family.base, name));
        }
        let default_family = families
            .first()
            .map(|(_, name)| name.clone())
            .expect("the bundled faces register at least one family");
        Self {
            fonts: RefCell::new(fonts),
            layout_cx: RefCell::new(LayoutContext::new()),
            default_family,
            families,
        }
    }

    /// The registered family name for a run's resolved [`FontId`] (the family
    /// whose id block contains it), falling back to the default family name.
    fn family_for(&self, font: FontId) -> &str {
        self.families
            .iter()
            .rev()
            .find(|(base, _)| font.0 >= *base)
            .map_or(self.default_family.as_str(), |(_, name)| name.as_str())
    }
}

impl Default for ParleyShaper {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps the crate's [`TextAlignment`] to `parley`'s.
fn alignment(alignment: TextAlignment) -> Alignment {
    match alignment {
        TextAlignment::Start => Alignment::Start,
        TextAlignment::End => Alignment::End,
        TextAlignment::Center => Alignment::Center,
        TextAlignment::Justify => Alignment::Justify,
    }
}

impl core::fmt::Debug for ParleyShaper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `parley`'s font/layout contexts are opaque caches and not `Debug`.
        f.debug_struct("ParleyShaper")
            .field("default_family", &self.default_family)
            .finish_non_exhaustive()
    }
}

impl LineShaper for ParleyShaper {
    fn shape_paragraph(
        &self,
        runs: &[StyledRun<'_>],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        let mut fonts = self.fonts.borrow_mut();
        let mut layout_cx = self.layout_cx.borrow_mut();

        // Concatenate run texts, tracking each run's byte range in the paragraph.
        let mut text = String::new();
        let mut spans: Vec<(usize, usize, &StyledRun<'_>)> = Vec::with_capacity(runs.len());
        for run in runs {
            let start = text.len();
            text.push_str(&run.text);
            spans.push((start, text.len(), run));
        }

        // Byte offsets `parley` reports are indices into `text`, the paragraph's
        // runs concatenated in document order. That is exactly the paragraph
        // node's text (offset 0 = the node's start, `range.start.offset` in the
        // general case), so a concatenated-text byte offset *is* the node-relative
        // caret anchor `hittest` expects — no per-run remapping is needed. `base`
        // is the node offset at which this shaped text begins.
        let base = range.start.offset;
        let node = range.start.node;
        let to_offset = |byte: usize| base.saturating_add(byte as u32);

        // Feed sizes in twips with scale = 1 so all outputs are in twips.
        let mut builder = layout_cx.ranged_builder(&mut fonts, &text, 1.0, false);
        builder.push_default(FontFamily::from(self.default_family.as_str()));
        // Line height as a percent of the metrics line height (`w:spacing@line`).
        if let Some(percent) = constraints.line_height_percent {
            builder.push_default(StyleProperty::LineHeight(LineHeight::MetricsRelative(
                f32::from(percent) / 100.0,
            )));
        }
        for (start, end, run) in &spans {
            builder.push(StyleProperty::FontSize(run.size.raw() as f32), *start..*end);
            // Push the run's resolved family so `parley` shapes with the exact
            // face the resolver selected (the same one the renderer outlines).
            builder.push(
                StyleProperty::FontFamily(FontFamily::from(self.family_for(run.font))),
                *start..*end,
            );
            builder.push(
                StyleProperty::Brush(RunBrush {
                    color: run.color,
                    font: run.font.0,
                    highlight: run.highlight.unwrap_or([0, 0, 0, 0]),
                    baseline_shift: run.baseline_shift.raw(),
                }),
                *start..*end,
            );
            if run.bold {
                builder.push(
                    StyleProperty::FontWeight(FontWeight::new(700.0)),
                    *start..*end,
                );
            }
            if run.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), *start..*end);
            }
            if run.letter_spacing != Twip::ZERO {
                builder.push(
                    StyleProperty::LetterSpacing(run.letter_spacing.raw() as f32),
                    *start..*end,
                );
            }
            if run.decoration.underline {
                builder.push(StyleProperty::Underline(true), *start..*end);
            }
            if run.decoration.strikethrough {
                builder.push(StyleProperty::Strikethrough(true), *start..*end);
            }
        }

        let mut layout = builder.build(&text);
        // First-line indent (`w:ind@firstLine`/`@hanging`): parley applies the
        // indent as a start-edge margin on the first line, reducing (or, for a
        // negative/`hanging` amount, extending) its wrap width and offsetting it.
        // The start indent itself is applied downstream at composition; here we
        // only shape the first line's differing width.
        if constraints.first_line_indent != Twip::ZERO {
            layout.set_text_indent(
                constraints.first_line_indent.raw() as f32,
                IndentOptions::default(),
            );
        }
        layout.break_all_lines(Some(constraints.max_width.raw() as f32));
        layout.align(
            alignment(constraints.alignment),
            AlignmentOptions::default(),
        );

        let line_count = layout.lines().count();
        let mut lines = Vec::with_capacity(line_count);
        for (index, line) in layout.lines().enumerate() {
            let metrics = line.metrics();
            let mut out_runs = Vec::new();
            // A single `parley` shaping run is split into one `GlyphRun` per
            // contiguous style span — a brush (color/highlight) or decoration
            // change splits the run *without* re-shaping, so the spans keep their
            // kerning and share one parent run. Each `GlyphRun` therefore covers
            // only a *slice* of the run's glyphs (`glyph_start`/`glyph_count`).
            // `run().visual_clusters()` walks the *whole* run, so we consume its
            // per-glyph byte offsets in lockstep with each slice via `cursor`;
            // walking the whole run per slice (the previous behavior) re-emitted
            // every glyph for each brush span, overprinting them. `GlyphRun`s of
            // one run are emitted consecutively, so the cursor stays aligned; a
            // change in the parent run's `text_range` marks a new run and resets
            // the offset stream.
            let mut run_offsets: Vec<u32> = Vec::new();
            let mut run_range: Option<std::ops::Range<usize>> = None;
            let mut cursor = 0usize;
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let style = glyph_run.style();
                let size = Twip(glyph_run.run().font_size().round() as i32);
                // Screen-y grows downward, so a positive `baseline_shift` (a raise,
                // e.g. a superscript or a positive `w:position`) subtracts from the
                // baseline row; a negative shift (subscript / lower) adds to it.
                let origin = Point::new(
                    Twip(glyph_run.offset().round() as i32),
                    Twip(glyph_run.baseline().round() as i32 - style.brush.baseline_shift),
                );
                let this_range = glyph_run.run().text_range();
                if run_range.as_ref() != Some(&this_range) {
                    // A new shaping run: rebuild its per-glyph node-relative byte
                    // offsets (visual order, one entry per glyph, each tagged with
                    // its cluster's start — see `to_offset`) and restart the slice
                    // cursor at the run's first glyph.
                    run_offsets = glyph_run
                        .run()
                        .visual_clusters()
                        .flat_map(|cluster| {
                            let offset = to_offset(cluster.text_range().start);
                            cluster.glyphs().map(move |_| offset)
                        })
                        .collect();
                    run_range = Some(this_range);
                    cursor = 0;
                }
                // Emit only this slice's glyphs (`glyphs()` honors the slice's
                // start/count), pairing each with its cluster byte offset from the
                // run-wide stream at the running cursor so the caret can anchor.
                let glyphs = glyph_run
                    .glyphs()
                    .map(|glyph| {
                        let cluster = run_offsets.get(cursor).copied().unwrap_or(base);
                        cursor += 1;
                        Glyph {
                            id: glyph.id,
                            advance: Twip(glyph.advance.round() as i32),
                            cluster,
                        }
                    })
                    .collect();
                // `parley` resolves the Unicode bidi level per run; its parity is
                // the run's direction (even = LTR, odd = RTL), which is all the
                // public API exposes and all `hittest` reads.
                let bidi_level = u8::from(glyph_run.run().is_rtl());
                // Alpha 0 is the "no highlight" sentinel carried through the brush.
                let highlight = (style.brush.highlight[3] != 0).then_some(style.brush.highlight);
                out_runs.push(GlyphRun {
                    font: FontId(style.brush.font),
                    size,
                    color: style.brush.color,
                    origin,
                    bidi_level,
                    decoration: Decoration {
                        underline: style.underline.is_some(),
                        strikethrough: style.strikethrough.is_some(),
                    },
                    highlight,
                    glyphs,
                });
            }
            let line_break = if index + 1 == line_count {
                LineBreak::ParagraphEnd
            } else {
                LineBreak::Wrap
            };
            // The line's model range is the node-relative span of the concatenated
            // text it covers; `parley` partitions the text across lines, so these
            // spans are contiguous and non-overlapping.
            let text_range = line.text_range();
            let line_range = ModelRange::new(
                ModelPos::new(node, to_offset(text_range.start)),
                ModelPos::new(node, to_offset(text_range.end)),
            );
            lines.push(Line {
                runs: out_runs,
                ascent: Twip(metrics.ascent.round() as i32),
                descent: Twip(metrics.descent.round() as i32),
                height: Twip(metrics.line_height.round() as i32),
                range: line_range,
                line_break,
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                text_boxes: Vec::new(),
            });
        }
        LineLayout { lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelPos, ModelRange};
    use casual_doc_model::NodeId;

    fn para_range() -> ModelRange {
        let node = NodeId::from_parts(1, 1).unwrap();
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0))
    }

    fn run(text: &str) -> StyledRun<'_> {
        StyledRun {
            text: text.into(),
            font: FontId(0),
            size: Twip::from_points(11),
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
            highlight: None,
            baseline_shift: Twip::ZERO,
        }
    }

    fn constraints(width_points: i32) -> LineConstraints {
        LineConstraints {
            max_width: Twip::from_points(width_points),
            ..LineConstraints::default()
        }
    }

    #[test]
    fn shapes_a_single_line_of_text() {
        let shaper = ParleyShaper::new();
        let layout = shaper.shape_paragraph(&[run("Hello world")], constraints(500), para_range());
        assert_eq!(layout.lines.len(), 1, "short text fits on one line");
        let line = &layout.lines[0];
        assert!(!line.runs.is_empty(), "the line has at least one glyph run");
        let glyph_count: usize = line.runs.iter().map(|r| r.glyphs.len()).sum();
        assert!(
            glyph_count >= 11,
            "one glyph per visible character at least"
        );
        assert!(
            line.ascent.raw() > 0 && line.height.raw() > 0,
            "positive metrics"
        );
    }

    #[test]
    fn alignment_shifts_the_line_position() {
        let shaper = ParleyShaper::new();
        let left = LineConstraints {
            max_width: Twip::from_points(400),
            alignment: TextAlignment::Start,
            ..LineConstraints::default()
        };
        let centered = LineConstraints {
            alignment: TextAlignment::Center,
            ..left
        };
        let x = |c| {
            shaper
                .shape_paragraph(&[run("short")], c, para_range())
                .lines[0]
                .runs[0]
                .origin
                .x
                .raw()
        };
        // Centered text starts further from the leading edge than start-aligned.
        assert!(
            x(centered) > x(left),
            "center alignment offsets the line inward"
        );
    }

    #[test]
    fn bold_and_letter_spacing_are_applied() {
        let shaper = ParleyShaper::new();
        let plain = run("iiiii");
        let spaced = StyledRun {
            letter_spacing: Twip::from_points(4),
            ..run("iiiii")
        };
        let width = |r: StyledRun<'_>| {
            let line = &shaper
                .shape_paragraph(&[r], constraints(500), para_range())
                .lines[0];
            line.runs
                .iter()
                .flat_map(|g| &g.glyphs)
                .map(|g| g.advance.raw())
                .sum::<i32>()
        };
        assert!(
            width(spaced) > width(plain),
            "letter spacing widens the run's advances"
        );
    }

    #[test]
    fn wraps_when_the_line_is_narrow() {
        let shaper = ParleyShaper::new();
        // A narrow column forces the two words onto separate lines.
        let layout = shaper.shape_paragraph(&[run("Hello world")], constraints(30), para_range());
        assert!(
            layout.lines.len() >= 2,
            "narrow width wraps to multiple lines"
        );
        assert_eq!(
            layout.lines.last().unwrap().line_break,
            LineBreak::ParagraphEnd
        );
    }

    #[test]
    fn preserves_run_color_and_decoration() {
        let shaper = ParleyShaper::new();
        let styled = StyledRun {
            text: "x".into(),
            font: FontId(0),
            size: Twip::from_points(11),
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [255, 0, 0, 255],
            decoration: Decoration {
                underline: true,
                strikethrough: false,
            },
            highlight: None,
            baseline_shift: Twip::ZERO,
        };
        let layout = shaper.shape_paragraph(&[styled], constraints(500), para_range());
        let run = &layout.lines[0].runs[0];
        assert_eq!(
            run.color,
            [255, 0, 0, 255],
            "run color round-trips through parley"
        );
        assert!(run.decoration.underline, "underline round-trips");
    }

    /// A styled run addressing a specific bundled face (for multi-font lines).
    fn run_with_font(text: &str, font: FontId) -> StyledRun<'_> {
        StyledRun { font, ..run(text) }
    }

    #[test]
    fn two_runs_of_different_fonts_yield_increasing_node_relative_clusters() {
        let shaper = ParleyShaper::new();
        // "Hello " in Roboto, "World" in Caladea: one line, two distinct faces.
        let roboto = run_with_font("Hello ", crate::fonts::ROBOTO.face_id(false, false));
        let caladea = run_with_font("World", crate::fonts::CALADEA.face_id(false, false));
        let layout = shaper.shape_paragraph(&[roboto, caladea], constraints(500), para_range());
        assert_eq!(layout.lines.len(), 1, "the text fits on one line");
        let clusters: Vec<u32> = layout.lines[0]
            .runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .map(|g| g.cluster)
            .collect();
        assert!(clusters.len() >= 11, "one cluster per ASCII char");
        assert!(
            clusters.iter().any(|&c| c != 0),
            "clusters are real byte offsets, not the placeholder 0"
        );
        // This LTR line is stored left-to-right, so byte offsets are
        // non-decreasing and reach into the second run (past byte 6, "World").
        assert!(
            clusters.windows(2).all(|w| w[0] <= w[1]),
            "clusters increase across the line: {clusters:?}"
        );
        assert_eq!(
            *clusters.first().unwrap(),
            0,
            "the first glyph anchors at byte 0"
        );
        assert!(
            clusters.iter().any(|&c| c >= 6),
            "the second run's glyphs carry offsets into its node text: {clusters:?}"
        );
        // Two distinct faces really were shaped (a genuine multi-font line).
        let fonts: std::collections::BTreeSet<u32> =
            layout.lines[0].runs.iter().map(|r| r.font.0).collect();
        assert_eq!(fonts.len(), 2, "the line carries two distinct faces");
    }

    #[test]
    fn first_line_indent_pushes_the_first_line_right() {
        let shaper = ParleyShaper::new();
        // A narrow column forces at least two lines; a positive first-line indent
        // out-dents the first line to the right of the continuation lines.
        let c = LineConstraints {
            max_width: Twip::from_points(60),
            first_line_indent: Twip::from_points(24),
            ..LineConstraints::default()
        };
        let layout = shaper.shape_paragraph(&[run("Hello world this wraps")], c, para_range());
        assert!(layout.lines.len() >= 2, "the text wraps to multiple lines");
        let first_x = layout.lines[0].runs[0].origin.x.raw();
        let second_x = layout.lines[1].runs[0].origin.x.raw();
        assert!(
            first_x > second_x,
            "first-line indent starts the first line ({first_x}) right of the rest ({second_x})"
        );
    }

    #[test]
    fn hanging_indent_protrudes_the_first_line_left() {
        let shaper = ParleyShaper::new();
        // A negative first-line indent (a hanging indent) protrudes the first line
        // to the left of the continuation lines (the bulleted-list shape).
        let c = LineConstraints {
            max_width: Twip::from_points(60),
            first_line_indent: Twip::from_points(-24),
            ..LineConstraints::default()
        };
        let layout = shaper.shape_paragraph(&[run("Hello world this wraps")], c, para_range());
        assert!(layout.lines.len() >= 2, "the text wraps to multiple lines");
        let first_x = layout.lines[0].runs[0].origin.x.raw();
        let second_x = layout.lines[1].runs[0].origin.x.raw();
        assert!(
            first_x < second_x,
            "a hanging indent starts the first line ({first_x}) left of the rest ({second_x})"
        );
    }

    #[test]
    fn bidi_levels_reflect_run_direction() {
        let shaper = ParleyShaper::new();
        // Pure LTR: every run is even (level 0).
        let ltr = shaper.shape_paragraph(&[run("abc")], constraints(500), para_range());
        assert!(
            ltr.lines[0].runs.iter().all(|r| r.bidi_level % 2 == 0),
            "LTR runs have an even bidi level"
        );
        // Pure RTL (Hebrew): at least one run is odd.
        let rtl = shaper.shape_paragraph(&[run("שלום")], constraints(500), para_range());
        assert!(
            rtl.lines
                .iter()
                .flat_map(|l| &l.runs)
                .any(|r| r.bidi_level % 2 == 1),
            "an RTL run has an odd bidi level"
        );
        // Mixed line: both parities present.
        let mixed = shaper.shape_paragraph(&[run("abc שלום def")], constraints(500), para_range());
        let levels: Vec<u8> = mixed
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.bidi_level)
            .collect();
        assert!(
            levels.iter().any(|&b| b % 2 == 0) && levels.iter().any(|&b| b % 2 == 1),
            "a mixed line carries both LTR and RTL runs: {levels:?}"
        );
    }

    #[test]
    fn line_ranges_are_contiguous_subranges_of_the_paragraph() {
        let shaper = ParleyShaper::new();
        let text = "Hello world this is a longer paragraph that wraps onto lines";
        let total = text.len() as u32;
        // A narrow column forces several lines.
        let layout = shaper.shape_paragraph(&[run(text)], constraints(60), para_range());
        assert!(
            layout.lines.len() >= 2,
            "the text wraps to multiple lines (got {})",
            layout.lines.len()
        );
        let node = para_range().start.node;
        let mut prev_end = 0u32;
        for (index, line) in layout.lines.iter().enumerate() {
            assert_eq!(line.range.start.node, node, "the line anchors to the node");
            assert_eq!(line.range.end.node, node);
            assert!(
                line.range.start.offset <= line.range.end.offset,
                "the range is well-formed"
            );
            assert!(
                line.range.end.offset <= total,
                "the range stays within the paragraph text"
            );
            if index == 0 {
                assert_eq!(
                    line.range.start.offset, 0,
                    "the first line starts at byte 0"
                );
            } else {
                assert_eq!(
                    line.range.start.offset, prev_end,
                    "consecutive lines are contiguous — no gap, no overlap"
                );
            }
            prev_end = line.range.end.offset;
        }
        assert_eq!(prev_end, total, "the last line ends at the paragraph's end");
    }
}
