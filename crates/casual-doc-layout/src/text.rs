//! Line-level types and the [`LineShaper`] seam.
//!
//! A paragraph is laid out into a stack of [`Line`]s, each a sequence of
//! positioned [`GlyphRun`]s. The actual shaping + bidi + line breaking is done
//! behind the [`LineShaper`] trait so the block/flow and pagination layers never
//! depend on a concrete text stack; the default implementation (a `parley`-based
//! shaper) lands in a following slice, and a `cosmic-text` implementation can be
//! substituted without touching the paginator (`43-…` §5).

use serde::{Deserialize, Serialize};

use crate::model::ModelRange;
use crate::units::{Point, Twip};

/// A resolved font identity (an index into the engine's resolved font set). The
/// resolution itself is `casual-doc-fonts`' concern (`40-FONT-MANAGEMENT-DESIGN.md`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FontId(pub u32);

/// One positioned glyph within a [`GlyphRun`].
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Glyph {
    /// Glyph index within the font (not a Unicode scalar).
    pub id: u32,
    /// Pen advance to the next glyph.
    pub advance: Twip,
    /// The byte offset (within the source run's text) of the cluster this glyph
    /// belongs to — the anchor for caret placement and hit-testing.
    pub cluster: u32,
}

/// Text decoration flags applied to a glyph run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Decoration {
    /// Underline.
    pub underline: bool,
    /// Strike-through.
    pub strikethrough: bool,
}

/// A run of glyphs sharing one font, size, color, and bidi level, positioned at
/// `origin` (the run's left edge on the baseline). Produced by the shaper and
/// carried verbatim into the display list.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GlyphRun {
    /// The resolved font.
    pub font: FontId,
    /// Font size.
    pub size: Twip,
    /// Fill color (RGBA packed as `[r, g, b, a]` to avoid a `display` cycle).
    pub color: [u8; 4],
    /// Left edge on the baseline.
    pub origin: Point,
    /// The bidi embedding level (even = LTR, odd = RTL).
    pub bidi_level: u8,
    /// Decorations.
    pub decoration: Decoration,
    /// The positioned glyphs, in visual (left-to-right) order.
    pub glyphs: Vec<Glyph>,
}

/// How a line ends.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LineBreak {
    /// A soft wrap (the paragraph continues on the next line).
    Wrap,
    /// A hard line break within the paragraph (`w:br`).
    Hard,
    /// The last line of the paragraph.
    ParagraphEnd,
}

/// One laid-out line: its glyph runs (visually ordered), vertical metrics, the
/// model range it covers (for hit-testing), and how it ends.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Line {
    /// Glyph runs on this line, left to right.
    pub runs: Vec<GlyphRun>,
    /// Distance from the top of the line to the baseline.
    pub ascent: Twip,
    /// Distance from the baseline to the bottom of the line.
    pub descent: Twip,
    /// Total line height (`ascent + descent + leading`).
    pub height: Twip,
    /// The model positions this line covers.
    pub range: ModelRange,
    /// How the line ends.
    pub line_break: LineBreak,
}

/// The result of shaping one paragraph: its ordered lines.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LineLayout {
    /// The paragraph's lines, top to bottom.
    pub lines: Vec<Line>,
}

impl LineLayout {
    /// The total height of all lines.
    #[must_use]
    pub fn height(&self) -> Twip {
        self.lines
            .iter()
            .fold(Twip::ZERO, |acc, line| acc + line.height)
    }
}

/// A styled span of text handed to the shaper (one run of uniform properties).
#[derive(Clone, Debug)]
pub struct StyledRun<'a> {
    /// The run text (UTF-8).
    pub text: &'a str,
    /// The resolved font.
    pub font: FontId,
    /// Font size.
    pub size: Twip,
    /// Bold weight (`w:b`).
    pub bold: bool,
    /// Italic style (`w:i`).
    pub italic: bool,
    /// Inter-character spacing added to each glyph advance (`w:spacing`), twips.
    pub letter_spacing: Twip,
    /// Fill color (RGBA).
    pub color: [u8; 4],
    /// Decorations.
    pub decoration: Decoration,
}

/// Horizontal alignment of a paragraph's lines.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum TextAlignment {
    /// Leading-edge aligned (the default).
    #[default]
    Start,
    /// Trailing-edge aligned.
    End,
    /// Centered.
    Center,
    /// Justified (last line start-aligned).
    Justify,
}

/// The constraints a paragraph is shaped under.
#[derive(Clone, Copy, Debug)]
pub struct LineConstraints {
    /// Available inline width for wrapping.
    pub max_width: Twip,
    /// Base direction (`true` = right-to-left paragraph).
    pub rtl: bool,
    /// Horizontal alignment of the lines.
    pub alignment: TextAlignment,
    /// Line height as a percent of the single-spaced height (`w:spacing@line`
    /// with `lineRule="auto"`); `None` = the font's natural line height.
    pub line_height_percent: Option<u16>,
}

impl Default for LineConstraints {
    fn default() -> Self {
        Self {
            max_width: Twip::ZERO,
            rtl: false,
            alignment: TextAlignment::Start,
            line_height_percent: None,
        }
    }
}

/// The seam between the block/flow engine and the concrete text stack: shape a
/// paragraph's styled runs into positioned lines. Implementations own shaping
/// (HarfBuzz), the Unicode bidi + line-breaking algorithms, and font metrics;
/// the DOCX-specific tab-stop resolution is layered on top by the caller and is
/// not the shaper's concern.
pub trait LineShaper {
    /// Shapes and line-breaks one paragraph. `runs` are in logical order; the
    /// returned lines are in visual order per line.
    fn shape_paragraph(
        &self,
        runs: &[StyledRun<'_>],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelPos, ModelRange};
    use casual_doc_model::NodeId;

    fn range() -> ModelRange {
        let node = NodeId::from_parts(1, 1).unwrap();
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0))
    }

    #[test]
    fn line_layout_height_sums_line_heights() {
        let line = |h| Line {
            runs: Vec::new(),
            ascent: Twip(0),
            descent: Twip(0),
            height: Twip(h),
            range: range(),
            line_break: LineBreak::Wrap,
        };
        let layout = LineLayout {
            lines: vec![line(240), line(240), line(200)],
        };
        assert_eq!(layout.height(), Twip(680));
    }

    #[test]
    fn glyph_run_serializes() {
        let run = GlyphRun {
            font: FontId(0),
            size: Twip::from_points(11),
            color: [0, 0, 0, 255],
            origin: Point::default(),
            bidi_level: 0,
            decoration: Decoration::default(),
            glyphs: vec![Glyph {
                id: 5,
                advance: Twip(120),
                cluster: 0,
            }],
        };
        let json = serde_json::to_string(&run).unwrap();
        let back: GlyphRun = serde_json::from_str(&json).unwrap();
        assert_eq!(back.glyphs.len(), 1);
    }
}
