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
    Alignment, AlignmentOptions, FontContext, FontFamily, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};

use crate::model::ModelRange;
use crate::text::{
    Decoration, FontId, Glyph, GlyphRun, Line, LineBreak, LineConstraints, LineLayout, LineShaper,
    StyledRun,
};
use crate::units::{Point, Twip};

/// The bundled default font — Roboto Regular, Apache-2.0 (see `fonts/README.md`).
const ROBOTO_REGULAR: &[u8] = include_bytes!("../fonts/Roboto-Regular.ttf");

/// A glyph color carried through `parley`. `Brush` is blanket-implemented for any
/// `Clone + PartialEq + Default + Debug`, so this newtype is a valid brush and
/// round-trips the run color out of the shaped layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ColorBrush([u8; 4]);

/// The default `parley`-backed line shaper.
///
/// `parley`'s builder needs `&mut` access to its font and layout contexts, while
/// [`LineShaper::shape_paragraph`] takes `&self` (shaping is logically pure — the
/// contexts are caches); the contexts therefore live behind `RefCell`.
pub struct ParleyShaper {
    fonts: RefCell<FontContext>,
    layout_cx: RefCell<LayoutContext<ColorBrush>>,
    default_family: String,
}

impl ParleyShaper {
    /// Creates a shaper with the bundled font registered into an empty
    /// collection (no system fonts — deterministic).
    #[must_use]
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        let registered = fonts
            .collection
            .register_fonts(Blob::new(Arc::new(ROBOTO_REGULAR.to_vec())), None);
        let family_id = registered
            .first()
            .expect("the bundled font registers a family")
            .0;
        let default_family = fonts
            .collection
            .family_name(family_id)
            .expect("the registered family has a name")
            .to_owned();
        Self {
            fonts: RefCell::new(fonts),
            layout_cx: RefCell::new(LayoutContext::new()),
            default_family,
        }
    }
}

impl Default for ParleyShaper {
    fn default() -> Self {
        Self::new()
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
            text.push_str(run.text);
            spans.push((start, text.len(), run));
        }

        // Feed sizes in twips with scale = 1 so all outputs are in twips.
        let mut builder = layout_cx.ranged_builder(&mut fonts, &text, 1.0, false);
        builder.push_default(FontFamily::from(self.default_family.as_str()));
        for (start, end, run) in &spans {
            builder.push(StyleProperty::FontSize(run.size.raw() as f32), *start..*end);
            builder.push(StyleProperty::Brush(ColorBrush(run.color)), *start..*end);
            if run.decoration.underline {
                builder.push(StyleProperty::Underline(true), *start..*end);
            }
            if run.decoration.strikethrough {
                builder.push(StyleProperty::Strikethrough(true), *start..*end);
            }
        }

        let mut layout = builder.build(&text);
        layout.break_all_lines(Some(constraints.max_width.raw() as f32));
        layout.align(Alignment::Start, AlignmentOptions::default());

        let line_count = layout.lines().count();
        let mut lines = Vec::with_capacity(line_count);
        for (index, line) in layout.lines().enumerate() {
            let metrics = line.metrics();
            let mut out_runs = Vec::new();
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let style = glyph_run.style();
                let size = Twip(glyph_run.run().font_size().round() as i32);
                let origin = Point::new(
                    Twip(glyph_run.offset().round() as i32),
                    Twip(glyph_run.baseline().round() as i32),
                );
                let glyphs = glyph_run
                    .positioned_glyphs()
                    .map(|glyph| Glyph {
                        id: glyph.id,
                        advance: Twip(glyph.advance.round() as i32),
                        // Byte-accurate cluster mapping lands with hit-testing (1E).
                        cluster: 0,
                    })
                    .collect();
                out_runs.push(GlyphRun {
                    font: FontId(0),
                    size,
                    color: style.brush.0,
                    origin,
                    bidi_level: 0,
                    decoration: Decoration {
                        underline: style.underline.is_some(),
                        strikethrough: style.strikethrough.is_some(),
                    },
                    glyphs,
                });
            }
            let line_break = if index + 1 == line_count {
                LineBreak::ParagraphEnd
            } else {
                LineBreak::Wrap
            };
            lines.push(Line {
                runs: out_runs,
                ascent: Twip(metrics.ascent.round() as i32),
                descent: Twip(metrics.descent.round() as i32),
                height: Twip(metrics.line_height.round() as i32),
                // Per-line model ranges are refined with hit-testing (1E); for now
                // each line carries the paragraph range.
                range,
                line_break,
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
            text,
            font: FontId(0),
            size: Twip::from_points(11),
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
        }
    }

    #[test]
    fn shapes_a_single_line_of_text() {
        let shaper = ParleyShaper::new();
        let layout = shaper.shape_paragraph(
            &[run("Hello world")],
            LineConstraints {
                max_width: Twip::from_points(500),
                rtl: false,
            },
            para_range(),
        );
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
    fn wraps_when_the_line_is_narrow() {
        let shaper = ParleyShaper::new();
        // A narrow column forces the two words onto separate lines.
        let layout = shaper.shape_paragraph(
            &[run("Hello world")],
            LineConstraints {
                max_width: Twip::from_points(30),
                rtl: false,
            },
            para_range(),
        );
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
            text: "x",
            font: FontId(0),
            size: Twip::from_points(11),
            color: [255, 0, 0, 255],
            decoration: Decoration {
                underline: true,
                strikethrough: false,
            },
        };
        let layout = shaper.shape_paragraph(
            &[styled],
            LineConstraints {
                max_width: Twip::from_points(500),
                rtl: false,
            },
            para_range(),
        );
        let run = &layout.lines[0].runs[0];
        assert_eq!(
            run.color,
            [255, 0, 0, 255],
            "run color round-trips through parley"
        );
        assert!(run.decoration.underline, "underline round-trips");
    }
}
