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
use crate::units::{Point, Size, Twip};

/// A resolved font identity (an index into the engine's resolved font set). The
/// resolution itself is `casual-doc-fonts`' concern (`40-FONT-MANAGEMENT-DESIGN.md`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FontId(pub u32);

/// One positioned glyph within a [`GlyphRun`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Glyph {
    /// Glyph index within the font (not a Unicode scalar).
    pub id: u32,
    /// Pen advance to the next glyph.
    pub advance: Twip,
    /// The UTF-8 byte offset, within the paragraph node's text, of the cluster
    /// this glyph belongs to — the anchor for caret placement and hit-testing
    /// (`crate::hittest`).
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
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
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
    /// Text-highlight fill painted behind the run's glyph box (`w:highlight`),
    /// RGBA; `None` when the run is not highlighted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<[u8; 4]>,
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

/// An inline image (embedded picture) placed within a paragraph, positioned like
/// a glyph run: `origin` is the box's top-left relative to the paragraph content
/// box, in twips. The renderer resolves `media` to bytes (a [`MediaSource`]) and
/// blits them scaled into the box. Only the inline case is modeled; anchored /
/// floating drawings are a later slice (`P1F-28`).
///
/// [`MediaSource`]: https://docs.rs/casual-doc-render
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct InlineImage {
    /// The media part name (the display list's stable media key), resolved by the
    /// backend against the document's media table.
    pub media: String,
    /// Top-left of the image box, relative to the paragraph content box (twips).
    pub origin: Point,
    /// The image box size (twips), derived from the drawing's EMU extent.
    pub size: Size,
}

/// One laid-out line: its glyph runs (visually ordered), vertical metrics, the
/// model range it covers (for hit-testing), and how it ends.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
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
    /// A forced page/column break follows this line (`w:br` type `page`/`column`).
    /// The paginator ends the page after this line and starts the paragraph's
    /// remainder on the next page (a column break collapses to a page break while
    /// the engine is single-column). Default `false`; serialized only when set so a
    /// plain line's galley stays byte-identical.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub page_break_after: bool,
    /// X positions (twips, from the paragraph content box's leading edge) of `bar`
    /// tab stops (`w:tab@val="bar"`) to draw as vertical rules across this line.
    /// Empty for the common case; serialized only when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bars: Vec<Twip>,
    /// Inline images placed on this line (embedded pictures). Empty for the common
    /// text-only line; serialized only when non-empty so a plain galley stays
    /// byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<InlineImage>,
}

/// The result of shaping one paragraph: its ordered lines.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
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
    /// Text-highlight fill (`w:highlight`) resolved to RGBA, painted behind the
    /// run's glyph box; `None` when unset.
    pub highlight: Option<[u8; 4]>,
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
    /// Available inline width for wrapping. Already reduced by the paragraph's
    /// start/end indents so wrapping happens at the indented column.
    pub max_width: Twip,
    /// Base direction (`true` = right-to-left paragraph).
    pub rtl: bool,
    /// Horizontal alignment of the lines.
    pub alignment: TextAlignment,
    /// Line height as a percent of the single-spaced height (`w:spacing@line`
    /// with `lineRule="auto"`); `None` = the font's natural line height.
    pub line_height_percent: Option<u16>,
    /// First-line indent applied to the paragraph's first line only, relative to
    /// the (already start-indented) column: positive out-dents the body to the
    /// right (`w:ind@firstLine`), negative protrudes the first line to the left
    /// (`w:ind@hanging`). The shaper narrows/widens the first line accordingly.
    pub first_line_indent: Twip,
}

impl Default for LineConstraints {
    fn default() -> Self {
        Self {
            max_width: Twip::ZERO,
            rtl: false,
            alignment: TextAlignment::Start,
            line_height_percent: None,
            first_line_indent: Twip::ZERO,
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
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
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
            highlight: None,
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
