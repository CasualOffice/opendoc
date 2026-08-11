//! Line-level types and the [`LineShaper`] seam.
//!
//! A paragraph is laid out into a stack of [`Line`]s, each a sequence of
//! positioned [`GlyphRun`]s. The actual shaping + bidi + line breaking is done
//! behind the [`LineShaper`] trait so the block/flow and pagination layers never
//! depend on a concrete text stack; the default implementation (a `parley`-based
//! shaper) lands in a following slice, and a `cosmic-text` implementation can be
//! substituted without touching the paginator (`43-…` §5).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use casual_doc_model::v1::{NoteId, NoteKind};

use crate::block::BlockFragment;
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
    /// Whether the glyph's complete source cluster consists of Unicode
    /// whitespace. This is source-derived shaping metadata, not a glyph-id
    /// heuristic; renderers use it for `w:u@val="words"` decoration gaps.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_whitespace: bool,
}

/// Text decoration flags applied to a glyph run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Decoration {
    /// Underline.
    pub underline: bool,
    /// The underline line style (`w:u@val`), drawn only when `underline` is on.
    #[serde(default, skip_serializing_if = "is_single_underline")]
    pub underline_style: casual_doc_model::v1::UnderlineStyle,
    /// Explicit underline color (`w:u@color`) resolved to RGBA; `None` means the
    /// underline takes the run's text color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline_color: Option<[u8; 4]>,
    /// Strike-through.
    pub strikethrough: bool,
    /// Double strike-through (`w:dstrike`): two parallel lines through the run.
    /// Independent of `strikethrough` (Word treats `w:strike`/`w:dstrike` as
    /// separate toggles; a run carries at most one in practice).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub double_strike: bool,
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
    /// Horizontal glyph scaling percentage (`w:w`). Advances are already scaled
    /// during shaping; the renderer uses this value to scale glyph outlines by
    /// the same factor without changing vertical metrics.
    #[serde(
        default = "default_character_scale",
        skip_serializing_if = "is_default_character_scale"
    )]
    pub character_scale_percent: u16,
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
    /// Run shading fill painted behind the run's glyph box (`w:rPr/w:shd`), RGBA;
    /// `None` when the run has no shading. Distinct from `highlight`: shading is an
    /// arbitrary `w:fill` color and is painted *under* the highlight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<[u8; 4]>,
    /// The positioned glyphs, in visual (left-to-right) order.
    pub glyphs: Vec<Glyph>,
    /// Whether this run is a list marker (a bullet/number/checkbox glyph injected
    /// ahead of the paragraph text), not model text. Lets the host locate an
    /// interactive checkbox marker's rect (`crate::hittest::LayoutSnapshot::marker_rects`)
    /// while a caret click still lands in the body. Additive: defaults false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_marker: bool,
    /// Whether this run is a tab **leader** fill (the tiled dot/underscore/hyphen
    /// glyphs drawn across a tab's advance), not model text. Its glyphs all share
    /// the paragraph's start offset — a rendering artifact, never a caret anchor —
    /// so `crate::hittest` excludes it from caret slots (a click on a leader
    /// underline resolves to the tab's real caret positions, not the paragraph
    /// start). Additive: defaults false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_leader: bool,
}

/// How a line ends.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LineBreak {
    /// A soft wrap (the paragraph continues on the next line).
    Wrap,
    /// A hard line break within the paragraph (`w:br`).
    Hard,
    /// A forced page break (`w:br w:type="page"` or a page-starting section).
    Page,
    /// A forced column break (`w:br w:type="column"`).
    Column,
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
    /// The source-rectangle crop (`a:srcRect`), if the picture is cropped
    /// (`P1G-OBJ-MODEL`); carried through to the display list's `PaintItem::Image`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<casual_doc_model::v1::CropRect>,
}

/// An inline image handed to the line shaper as an in-flow box. `index` is the
/// UTF-8 byte boundary in the concatenated paragraph text where the box occurs;
/// multiple boxes may share a boundary and retain slice order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineImageSpec {
    /// Stable media key copied into the positioned [`InlineImage`].
    pub media: String,
    /// Byte boundary in the shaper's concatenated text.
    pub index: u32,
    /// Authored image box size.
    pub size: Size,
    /// The source-rectangle crop (`a:srcRect`), copied into the positioned
    /// [`InlineImage`] (`P1G-OBJ-MODEL`).
    pub crop: Option<casual_doc_model::v1::CropRect>,
}

/// A pre-laid-out equation handed to the paragraph shaper as one atomic in-flow
/// box. Child glyph/rule origins are relative to the box's top-left; the shaper
/// supplies only its final line position and wrapping decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineMathSpec {
    /// Byte boundary in the shaper's concatenated paragraph text.
    pub index: u32,
    /// Atomic equation box size.
    pub size: Size,
    /// Equation glyphs relative to the box top-left.
    pub runs: Vec<GlyphRun>,
    /// Fraction bars and radical overbars relative to the box top-left.
    pub rules: Vec<InlineRule>,
}

/// Which inline edge of a paragraph-local floating object excludes text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineFloatSide {
    /// The float occupies the paragraph's leading/left edge.
    Left,
    /// The float occupies the paragraph's trailing/right edge.
    Right,
}

/// A non-painting paragraph-local exclusion handed to the line shaper.
///
/// The anchored drawing is painted separately by the page float layer; this
/// marker only changes the available line geometry for the vertical span of the
/// object. `index` is a UTF-8 byte boundary in the concatenated paragraph text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InlineFloatSpec {
    /// Byte boundary where the anchored object occurs in the paragraph stream.
    pub index: u32,
    /// Inline edge occupied by the object.
    pub side: InlineFloatSide,
    /// Horizontal exclusion including authored wrap distances.
    pub width: Twip,
    /// Vertical exclusion from the anchor paragraph's top.
    pub height: Twip,
}

/// An inline horizontal rule (`w:pict` / `v:rect@o:hr`) placed on a line: a filled
/// rectangle spanning (a fraction of) the content width at the line's leading
/// edge. `origin` is the rule's top-left relative to the paragraph content box
/// (twips); `size` is its resolved width and thickness. Composition paints it as a
/// filled rect. Laid out on its own line, like an [`InlineImage`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct InlineRule {
    /// Top-left of the rule box, relative to the paragraph content box (twips).
    pub origin: Point,
    /// The rule box size (twips): resolved width and thickness.
    pub size: Size,
    /// The rule fill color (RGBA).
    pub color: [u8; 4],
}

/// A resolved text-box outline ready for layout and paint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct TextBoxStroke {
    /// Resolved RGBA color.
    pub color: [u8; 4],
    /// Authored outline width converted to twips.
    pub width: Twip,
}

/// Resolved placement and clipping of a text box's flowed block stack, relative
/// to the box's top-left corner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct TextBoxContentLayout {
    /// Content origin after physical-side insets and vertical anchoring.
    pub origin: Point,
    /// Clip nested paint at the shape's horizontal bounds.
    pub clip_horizontal: bool,
    /// Clip nested paint at the shape's vertical bounds.
    pub clip_vertical: bool,
}

/// An inline text box (`wps:txbx` / `v:textbox`) placed within a paragraph: a
/// bordered/filled box whose recursive block content was flowed through the *same*
/// pipeline as the document body (paragraphs, tables incl. nested, inline images —
/// the uniform-flow-pipeline invariant). `origin` is the box's top-left relative to
/// the paragraph content box (twips); `blocks` are the flowed fragments, positioned
/// relative to the box's content origin (the box's top-left inset by the internal
/// margin). Composition paints the fill and border, then composes the fragments
/// offset into the box. Only the inline case is modeled here; anchored / floating
/// text-box placement reuses the anchored-drawing path (`P1F-28`) in a later slice.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct InlineTextBox {
    /// Top-left of the box, relative to the paragraph content box (twips).
    pub origin: Point,
    /// The box's resolved outer size (twips). Positive authored dimensions win;
    /// missing dimensions are resolved from available width and flowed content.
    pub size: Size,
    /// The flowed block fragments — the box's content, laid out through the shared
    /// flow pipeline, positioned relative to the box's content origin.
    pub blocks: Vec<BlockFragment>,
    /// The box outline, including its authored color and width; `None` = no
    /// border. Serialized only when present so a plain galley stays byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<TextBoxStroke>,
    /// The box background fill (RGBA), painted behind the content; `None` =
    /// transparent. Serialized only when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[u8; 4]>,
    /// Resolved content offset and selected-axis overflow clipping.
    #[serde(default)]
    pub content_layout: TextBoxContentLayout,
}

/// The kind of an inline field, resolved from its `w:instr` instruction. Only the
/// page-dependent fields are recomputed by the post-pagination field pass
/// ([`crate::paginate::resolve_fields`]); every other field is displayed from its
/// cached result verbatim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// `PAGE` — the current page's number.
    Page,
    /// `NUMPAGES` — the total number of pages.
    NumPages,
    /// Any other field: the cached result is shown verbatim, never restamped.
    Passthrough,
}

/// The run styling captured for a field so the field pass can reshape a recomputed
/// value (a new page number / count) with the same face, size, and color the
/// producer's cached result used.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FieldStyle {
    /// Resolved font face.
    pub font: FontId,
    /// Font size (twips).
    pub size: Twip,
    /// Horizontal glyph scaling percentage (`w:w`).
    #[serde(
        default = "default_character_scale",
        skip_serializing_if = "is_default_character_scale"
    )]
    pub character_scale_percent: u16,
    /// Fill color (RGBA).
    pub color: [u8; 4],
    /// Bold weight.
    pub bold: bool,
    /// Italic style.
    pub italic: bool,
    /// Inter-character spacing (twips).
    pub letter_spacing: Twip,
    /// Decorations (underline / strike).
    pub decoration: Decoration,
}

/// An inline field placed on a line. It carries only a *marker* through
/// pagination — never a baked page number — so a page reused by the incremental
/// paginator never keeps a stale value. The post-pagination field pass
/// ([`crate::paginate::resolve_fields`]) stamps each `Page`/`NumPages` marker with
/// the final page number / count and reshapes its glyph run in place.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FieldMarker {
    /// The field kind (what value to stamp).
    pub kind: FieldKind,
    /// Index into [`Line::runs`] of the glyph run holding this field's value.
    pub run: u32,
    /// The field run's flow-time left origin (twips, from the paragraph content
    /// box). The field pass repositions the run — and the runs after it on the
    /// line — from this stable anchor, so resolving is idempotent (running it twice
    /// yields the same layout, which is what keeps `repaginate == paginate`).
    pub base_x: Twip,
    /// The styling used to reshape a recomputed value.
    pub style: FieldStyle,
    /// The resolved display value — a placeholder (the producer's cached result)
    /// until the field pass runs, then the stamped page number / count.
    pub value: String,
}

/// A note reference carried on a laid-out line. The visible reference glyph is a
/// normal glyph run; this marker is the pagination side channel used by the later
/// footnote/endnote placement pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct NoteMarker {
    /// Whether this reference points to a footnote or an endnote definition.
    pub kind: NoteKind,
    /// The referenced note definition id.
    pub note: NoteId,
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
    /// Whether paint must be clipped to this line's vertical box. Set for
    /// `w:spacing@lineRule="exact"` so oversized glyph ink cannot collide with
    /// the following line even though pagination honors the authored height.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub clip: bool,
    /// The model positions this line covers.
    pub range: ModelRange,
    /// How the line ends.
    pub line_break: LineBreak,
    /// A forced page/column break follows this line (`w:br` type `page`/`column`).
    /// [`line_break`](Self::line_break) retains which kind. The single-column
    /// paginator treats either as a page transition; the column paginator advances
    /// a column break to the next physical column. Default `false`; serialized only
    /// when set so a plain line's galley stays byte-identical.
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
    /// Inline fields on this line (`PAGE`, `NUMPAGES`, …) — markers the field pass
    /// resolves after pagination. Empty for the common line; serialized only when
    /// non-empty so a plain galley stays byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldMarker>,
    /// Footnote/endnote references on this line. Empty for the common line; the
    /// paginator consumes it to reserve/place page-local note bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<NoteMarker>,
    /// Inline text boxes placed on this line (`wps:txbx` / `v:textbox`), each a
    /// bordered box of flowed block content. Empty for the common line; serialized
    /// only when non-empty so a plain galley stays byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_boxes: Vec<InlineTextBox>,
    /// Inline horizontal rules placed on this line (`w:pict` / `v:rect@o:hr`), each
    /// a filled full-content-width line. Empty for the common line; serialized only
    /// when non-empty so a plain galley stays byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<InlineRule>,
}

impl Line {
    /// Translates every paintable child into a new vertical coordinate space.
    ///
    /// Line children are paragraph-relative after shaping and fragment-relative
    /// after pagination. Keeping the translation in one place prevents a split
    /// continuation from rebasing glyphs while leaving images, text boxes, or
    /// rules behind in the preceding fragment's coordinates.
    pub(crate) fn translate_contents_y(&mut self, delta: Twip) {
        for run in &mut self.runs {
            run.origin.y = run.origin.y + delta;
        }
        for image in &mut self.images {
            image.origin.y = image.origin.y + delta;
        }
        for text_box in &mut self.text_boxes {
            text_box.origin.y = text_box.origin.y + delta;
        }
        for rule in &mut self.rules {
            rule.origin.y = rule.origin.y + delta;
        }
    }
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
    /// The run text (UTF-8). Borrowed from the model in the common case; owned
    /// when a case transform (`w:caps`/`w:smallCaps`) rewrote it, so the transform
    /// costs an allocation only when it actually applies.
    pub text: Cow<'a, str>,
    /// The originally requested font family (`w:rFonts`, theme-resolved) before
    /// bundled substitution, so the shaper can prefer a real installed face of that
    /// name (e.g. system Arial) over the bundled fallback the resolver picked when
    /// one is available — the `system-fonts` / host-registry path. `None` when the
    /// run declared no family (it inherits the bundled default). Deterministic
    /// builds (no `system-fonts`, no host faces) never find the name in the
    /// collection, so [`font`](Self::font) — the bundled resolution — is used and
    /// output is unchanged.
    pub requested_family: Option<Cow<'a, str>>,
    /// The resolved font.
    pub font: FontId,
    /// Font size.
    pub size: Twip,
    /// Horizontal glyph scaling percentage (`w:w`), 100 for natural width.
    pub character_scale_percent: u16,
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
    /// Run shading fill (`w:rPr/w:shd`) resolved to RGBA, painted behind the run's
    /// glyph box (under the highlight); `None` when unset.
    pub shading: Option<[u8; 4]>,
    /// Baseline shift in twips, positive = raised toward the top of the line
    /// (screen-y grows downward, so the shaper subtracts this from the glyph-run
    /// origin). Carries `w:vertAlign` super/subscript and the `w:position` offset;
    /// `Twip::ZERO` for an unshifted run.
    pub baseline_shift: Twip,
}

const fn default_character_scale() -> u16 {
    100
}

fn is_default_character_scale(value: &u16) -> bool {
    *value == default_character_scale()
}

fn is_single_underline(value: &casual_doc_model::v1::UnderlineStyle) -> bool {
    *value == casual_doc_model::v1::UnderlineStyle::Single
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
    /// Full text-margin width before paragraph start/end indents are removed.
    ///
    /// Absolute positional tabs (`w:ptab relativeTo="margin"`) resolve against
    /// this box, while ordinary shaping continues to wrap against `max_width`.
    pub margin_width: Twip,
    /// Paragraph start indent measured from the leading text margin. This lets a
    /// margin-relative positional tab convert its absolute target into the
    /// shaper's indent-local coordinate system.
    pub indent_start: Twip,
    /// Base direction (`true` = right-to-left paragraph).
    pub rtl: bool,
    /// Horizontal alignment of the lines.
    pub alignment: TextAlignment,
    /// Line height as a percent of the single-spaced height (`w:spacing@line`
    /// with `lineRule="auto"`); `None` = the font's natural line height.
    pub line_height_percent: Option<u16>,
    /// `w:spacing@lineRule="atLeast"`: the line box is at least this many twips
    /// tall; taller natural content grows it. `None` unless the rule is atLeast.
    pub line_at_least: Option<Twip>,
    /// `w:spacing@lineRule="exact"`: the line box is exactly this many twips tall
    /// regardless of content (content may clip). `None` unless the rule is exact.
    pub line_exact: Option<Twip>,
    /// Active section document-grid line pitch. The concrete shaper rounds the
    /// resolved natural/at-least box upward to an integral number of these grid
    /// units; exact line spacing takes precedence.
    pub line_grid_pitch: Option<Twip>,
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
            margin_width: Twip::ZERO,
            indent_start: Twip::ZERO,
            rtl: false,
            alignment: TextAlignment::Start,
            line_height_percent: None,
            line_at_least: None,
            line_exact: None,
            line_grid_pitch: None,
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

    /// Shapes a paragraph with true in-flow image boxes interleaved at byte
    /// boundaries. Implementations that do not override this retain a safe
    /// compatibility fallback: text is shaped normally and each image occupies a
    /// following standalone line. The default [`crate::shape::ParleyShaper`]
    /// overrides it with native inline-box line breaking.
    fn shape_paragraph_with_inline_images(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        let mut layout = self.shape_paragraph(runs, constraints, range);
        let mut y = layout.height();
        for image in images {
            layout.lines.push(Line {
                runs: Vec::new(),
                ascent: image.size.height,
                descent: Twip::ZERO,
                height: image.size.height,
                clip: false,
                range,
                line_break: LineBreak::Wrap,
                page_break_after: false,
                bars: Vec::new(),
                images: vec![InlineImage {
                    media: image.media.clone(),
                    origin: Point::new(Twip::ZERO, y),
                    size: image.size,
                    crop: image.crop,
                }],
                fields: Vec::new(),
                notes: Vec::new(),
                text_boxes: Vec::new(),
                rules: Vec::new(),
            });
            y = y + image.size.height;
        }
        layout
    }

    /// Shapes a paragraph containing both true inline images and paragraph-local
    /// floating exclusions. The default is deliberately conservative: it reduces
    /// every line by the largest exclusion so alternate shapers cannot overlap a
    /// float, while the default [`crate::shape::ParleyShaper`] applies the reduced
    /// geometry only to lines intersecting each float.
    fn shape_paragraph_with_inline_objects(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        floats: &[InlineFloatSpec],
        mut constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        let exclusion = floats
            .iter()
            .map(|float| float.width)
            .max()
            .unwrap_or(Twip::ZERO);
        constraints.max_width = Twip((constraints.max_width.raw() - exclusion.raw()).max(1));
        self.shape_paragraph_with_inline_images(runs, images, constraints, range)
    }

    /// Shapes text, images, equations, and paragraph-local float exclusions.
    ///
    /// Alternate shapers retain a safe fallback: equations are appended as
    /// atomic lines after ordinary inline-object shaping. The default Parley
    /// implementation places their boxes at the authored inline boundary.
    fn shape_paragraph_with_rich_inline_objects(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        maths: &[InlineMathSpec],
        floats: &[InlineFloatSpec],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        let mut layout =
            self.shape_paragraph_with_inline_objects(runs, images, floats, constraints, range);
        let mut y = layout.height();
        for math in maths {
            let mut runs = math.runs.clone();
            let mut rules = math.rules.clone();
            for run in &mut runs {
                run.origin.y = run.origin.y + y;
            }
            for rule in &mut rules {
                rule.origin.y = rule.origin.y + y;
            }
            layout.lines.push(Line {
                runs,
                ascent: math.size.height,
                descent: Twip::ZERO,
                height: math.size.height,
                clip: false,
                range,
                line_break: LineBreak::Wrap,
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                notes: Vec::new(),
                text_boxes: Vec::new(),
                rules,
            });
            y = y + math.size.height;
        }
        layout
    }
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
            clip: false,
            range: range(),
            line_break: LineBreak::Wrap,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
        };
        let layout = LineLayout {
            lines: vec![line(240), line(240), line(200)],
        };
        assert_eq!(layout.height(), Twip(680));
    }

    #[test]
    fn glyph_run_serializes() {
        let run = GlyphRun {
            is_marker: false,
            is_leader: false,
            font: FontId(0),
            size: Twip::from_points(11),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::default(),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs: vec![Glyph {
                id: 5,
                advance: Twip(120),
                cluster: 0,
                is_whitespace: false,
            }],
        };
        let json = serde_json::to_string(&run).unwrap();
        let back: GlyphRun = serde_json::from_str(&json).unwrap();
        assert_eq!(back.glyphs.len(), 1);
    }
}
