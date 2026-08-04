//! Body block and inline nodes.

use serde::{Deserialize, Serialize};

use super::{
    BookmarkId, BreakKind, CommentId, MediaId, NoteId, ParagraphProperties, RunProperties, Table,
};
use crate::NodeId;

/// OOXML `ST_PositiveCoordinate` upper bound, in English Metric Units (EMU).
pub const MAX_EMU: i64 = 27_273_042_316_900;

/// A text run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Run {
    /// Stable run identity.
    pub id: NodeId,
    /// Run properties (always present; empty is `{}`).
    pub properties: RunProperties,
    /// Grapheme text (non-empty).
    pub text: String,
}

/// An explicit tab.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tab {
    /// Stable identity.
    pub id: NodeId,
}

/// An explicit break.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Break {
    /// Stable identity.
    pub id: NodeId,
    /// Break kind.
    pub kind: BreakKind,
}

/// A non-breaking hyphen glyph (`w:noBreakHyphen`): a visible hyphen that is
/// never a line-break opportunity (the words it joins stay on one line). An inert
/// leaf, like [`Tab`] — it carries only its identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoBreakHyphen {
    /// Stable identity.
    pub id: NodeId,
}

/// A soft (optional) hyphen glyph (`w:softHyphen`): a hyphenation point that is
/// drawn only when the line breaks there, and invisible otherwise. An inert leaf,
/// like [`Tab`] — it carries only its identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SoftHyphen {
    /// Stable identity.
    pub id: NodeId,
}

/// The alignment of an absolute-position tab (`w:ptab@w:alignment`,
/// `ST_PTabAlignment`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionalTabAlignment {
    /// Text following the tab is left-aligned at the tab position.
    Left,
    /// Text following the tab is centered on the tab position.
    Center,
    /// Text following the tab is right-aligned at the tab position.
    Right,
}

/// The base an absolute-position tab measures from (`w:ptab@w:relativeTo`,
/// `ST_PTabRelativeTo`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionalTabRelativeTo {
    /// Relative to the text margins.
    Margin,
    /// Relative to the paragraph indent.
    Indent,
}

/// The leader drawn in an absolute-position tab's whitespace (`w:ptab@w:leader`,
/// `ST_PTabLeader`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionalTabLeader {
    /// No leader.
    None,
    /// A dotted leader.
    Dot,
    /// A hyphenated leader.
    Hyphen,
    /// An underscore leader.
    Underscore,
    /// A middle-dot leader.
    MiddleDot,
}

/// An absolute-position tab (`w:ptab`): a tab whose stop is positioned relative to
/// the page margin or the paragraph indent, with an alignment and an optional
/// leader. Unlike an ordinary [`Tab`] (which advances to the next defined tab
/// stop), a positional tab names its own stop. All three attributes are required
/// by the schema, so each is modeled non-optionally.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionalTab {
    /// Stable identity.
    pub id: NodeId,
    /// How text following the tab aligns at the stop (`w:alignment`).
    pub alignment: PositionalTabAlignment,
    /// The base the stop is measured from (`w:relativeTo`).
    pub relative_to: PositionalTabRelativeTo,
    /// The leader drawn in the tab's whitespace (`w:leader`).
    pub leader: PositionalTabLeader,
}

/// The horizontal alignment of an inline horizontal rule within the content
/// width (VML `o:hralign`). A rule narrower than the content width (a
/// [`HorizontalRule::width_permille`] below full) sits at this edge or centered.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalRuleAlign {
    /// Flush with the content's leading edge.
    Left,
    /// Centered in the content width.
    Center,
    /// Flush with the content's trailing edge.
    Right,
}

/// Full width, in per-mille, for a [`HorizontalRule`] with no `o:hrpct`.
pub const HR_FULL_WIDTH_PERMILLE: u16 = 1000;

/// An inline horizontal rule (`w:pict` / `v:rect` with `o:hr="t"`): Word's
/// "Insert → Horizontal Line". Unlike an ordinary VML rectangle, an `o:hr` shape
/// spans the full content width (its CSS `width` is ignored), is `height` twips
/// thick, and is filled with its `fillcolor`. It occupies its paragraph's own
/// line, like an inline image. An inert leaf — it carries only its geometry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HorizontalRule {
    /// Stable identity.
    pub id: NodeId,
    /// Alignment within the content width (`o:hralign`).
    pub align: HorizontalRuleAlign,
    /// Rule width as a fraction of the content width in per-mille (`o:hrpct`,
    /// `1000` = full width). Clamped to `1..=1000`.
    pub width_permille: u16,
    /// Rule thickness in EMU (`v:rect` `height`; the drawing's `width` is ignored
    /// for a horizontal rule). Positive.
    pub thickness_emu: i64,
    /// Rule color (`fillcolor`).
    pub color: Rgba,
}

/// The upper bound on a [`Symbol`] font name, in bytes.
pub const MAX_SYMBOL_FONT_LEN: usize = 255;

/// An inline symbol: a single glyph named by a font and a code point.
///
/// Maps OOXML `w:sym` (`w:font` + `w:char`). Word uses this for glyphs pulled
/// from a specific font — most often a symbol font (Wingdings, Symbol, …) whose
/// code point sits in the Unicode Private Use Area (`0xF0xx`) — so the character
/// cannot be represented as ordinary run text without losing the font binding.
/// The glyph is an inert leaf: `char` is the raw code point and `font` names the
/// face to resolve it against; neither is decoded to display text here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Symbol {
    /// Stable identity.
    pub id: NodeId,
    /// The font the glyph is resolved against (non-empty, at most
    /// `MAX_SYMBOL_FONT_LEN` bytes; validated on `Document::validate`).
    pub font: String,
    /// The glyph's code point (`w:char`, a hex value, often PUA `0xF0xx`).
    pub char: u32,
    /// Formatting of the `w:r` that owns the symbol. This is required for form
    /// glyphs such as 16pt Wingdings checkboxes to retain their authored size and
    /// color instead of falling back to the paragraph default.
    #[serde(default, skip_serializing_if = "is_default_run_properties")]
    pub properties: RunProperties,
}

fn is_default_run_properties(properties: &RunProperties) -> bool {
    *properties == RunProperties::default()
}

/// The natural size of a drawing, in English Metric Units (EMU).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Extent {
    /// Width in EMU (`0..=MAX_EMU`).
    pub width_emu: i64,
    /// Height in EMU (`0..=MAX_EMU`).
    pub height_emu: i64,
}

/// One `a:srcRect` edge fraction meaning "no crop" (0), and the value meaning
/// "the whole edge" (`100000` = 100% in OOXML ST_Percentage — thousandths of a
/// percent).
pub const CROP_FULL: i32 = 100_000;

/// The bound applied to each [`CropRect`] edge at import. Word authors
/// `0..=CROP_FULL`, but DrawingML `a:srcRect` also permits a small negative value
/// (an *outset* / padding), so the range is bounded rather than assumed
/// non-negative; values outside it are clamped.
pub const CROP_MIN: i32 = -CROP_FULL;
/// The upper crop bound (see [`CROP_MIN`]).
pub const CROP_MAX: i32 = 2 * CROP_FULL;

/// An image crop (`a:srcRect`): how much of each edge of the **source** image to
/// hide, in OOXML ST_Percentage units — thousandths of a percent, where
/// [`CROP_FULL`] (`100000`) is the whole edge. The visible source rectangle is
/// `left ..= CROP_FULL - right` horizontally and `top ..= CROP_FULL - bottom`
/// vertically (fractions of the source dimensions), scaled to fill the drawing's
/// display extent. All-zero means no crop (the whole source fills the box).
///
/// Values round-trip verbatim within [`CROP_MIN`]..=[`CROP_MAX`].
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CropRect {
    /// Fraction of the source hidden at the left edge (`a:srcRect@l`).
    pub left: i32,
    /// Fraction hidden at the top edge (`a:srcRect@t`).
    pub top: i32,
    /// Fraction hidden at the right edge (`a:srcRect@r`).
    pub right: i32,
    /// Fraction hidden at the bottom edge (`a:srcRect@b`).
    pub bottom: i32,
}

impl CropRect {
    /// Whether this crop hides nothing (all four edges zero) — the identity crop,
    /// treated as "no crop" so an empty/absent `a:srcRect` is not modeled.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.left == 0 && self.top == 0 && self.right == 0 && self.bottom == 0
    }

    /// This crop with every edge clamped into [`CROP_MIN`]..=[`CROP_MAX`].
    #[must_use]
    pub fn clamped(self) -> Self {
        let clamp = |v: i32| v.clamp(CROP_MIN, CROP_MAX);
        Self {
            left: clamp(self.left),
            top: clamp(self.top),
            right: clamp(self.right),
            bottom: clamp(self.bottom),
        }
    }
}

/// An inline drawing that references an embedded picture in the media table.
///
/// Only the embedded-picture case (a resolvable `r:embed`) is modeled; linked
/// blips, charts, SmartArt, and text boxes remain reported and (in Retention)
/// preserved.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drawing {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced media entry (resolves in `Definitions::media`).
    pub media: MediaId,
    /// The drawing's natural size, if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<Extent>,
    /// The alt text (`wp:docPr@descr`), preserved for accessibility, if declared
    /// (non-empty, at most [`MAX_DESCR_BYTES`] bytes). Mirrors the anchored path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    /// The source-rectangle crop (`a:srcRect`), if the picture is cropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<CropRect>,
    /// The picture frame outline (`pic:spPr/a:ln`), if the picture is bordered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ShapeStroke>,
    /// Horizontal flip (`a:xfrm@flipH`): mirror the picture across its vertical
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip (`a:xfrm@flipV`): mirror the picture across its horizontal
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// Clockwise rotation about the box center (`a:xfrm@rot`, in 60000ths of a
    /// degree), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

/// Maximum drawing alt-text (`wp:docPr@descr`) length, in UTF-8 bytes.
pub const MAX_DESCR_BYTES: usize = 2048;

/// What the horizontal position of an anchored drawing is measured from
/// (`wp:positionH@relativeFrom`). The offset/alignment resolves against this
/// reference edge or box.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalAnchor {
    /// The page edge (`page`).
    Page,
    /// The text margin (`margin`).
    Margin,
    /// The current column (`column`).
    Column,
    /// The anchoring character (`character`).
    Character,
    /// The left margin strip (`leftMargin`).
    LeftMargin,
    /// The right margin strip (`rightMargin`).
    RightMargin,
    /// The inside margin, for mirrored (odd/even) layouts (`insideMargin`).
    InsideMargin,
    /// The outside margin, for mirrored layouts (`outsideMargin`).
    OutsideMargin,
}

/// What the vertical position of an anchored drawing is measured from
/// (`wp:positionV@relativeFrom`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAnchor {
    /// The page edge (`page`).
    Page,
    /// The text margin (`margin`).
    Margin,
    /// The anchoring paragraph (`paragraph`).
    Paragraph,
    /// The current line (`line`).
    Line,
    /// The top margin strip (`topMargin`).
    TopMargin,
    /// The bottom margin strip (`bottomMargin`).
    BottomMargin,
    /// The inside margin, for mirrored layouts (`insideMargin`).
    InsideMargin,
    /// The outside margin, for mirrored layouts (`outsideMargin`).
    OutsideMargin,
}

/// A relative horizontal alignment within the reference box (`wp:align`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalAlign {
    /// Flush with the reference's left edge (`left`).
    Left,
    /// Centered in the reference box (`center`).
    Center,
    /// Flush with the reference's right edge (`right`).
    Right,
    /// The inside edge, for mirrored layouts (`inside`).
    Inside,
    /// The outside edge, for mirrored layouts (`outside`).
    Outside,
}

/// A relative vertical alignment within the reference box (`wp:align`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlign {
    /// Flush with the reference's top edge (`top`).
    Top,
    /// Centered in the reference box (`center`).
    Center,
    /// Flush with the reference's bottom edge (`bottom`).
    Bottom,
    /// The inside edge, for mirrored layouts (`inside`).
    Inside,
    /// The outside edge, for mirrored layouts (`outside`).
    Outside,
}

/// The horizontal placement of an anchored drawing: either an absolute offset
/// from the reference edge (`wp:posOffset`, EMU, may be negative) or a relative
/// alignment within the reference box (`wp:align`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalPosition {
    /// An absolute offset in EMU from the reference edge
    /// (`-MAX_EMU..=MAX_EMU`). Positive is toward the reference's trailing edge.
    Offset(i64),
    /// A relative alignment within the reference box.
    Align(HorizontalAlign),
}

/// The vertical placement of an anchored drawing (`wp:posOffset` / `wp:align`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalPosition {
    /// An absolute offset in EMU from the reference edge (`-MAX_EMU..=MAX_EMU`).
    Offset(i64),
    /// A relative alignment within the reference box.
    Align(VerticalAlign),
}

/// How text flows around an anchored drawing (the `wp:wrap*` element).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WrapMode {
    /// Text wraps around the drawing's bounding box (`wp:wrapSquare`).
    Square,
    /// Text wraps tight to the drawing's contour (`wp:wrapTight`).
    Tight,
    /// Text flows through the drawing's transparent regions (`wp:wrapThrough`).
    Through,
    /// Text is pushed above and below the drawing (`wp:wrapTopAndBottom`).
    TopAndBottom,
    /// No wrapping: the drawing floats over or behind the text (`wp:wrapNone`).
    None,
}

/// The horizontal component of an anchor: the reference edge and the placement
/// against it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorHorizontal {
    /// What the position is measured from (`@relativeFrom`).
    pub relative_from: HorizontalAnchor,
    /// The offset or alignment within that reference.
    pub position: HorizontalPosition,
}

/// The vertical component of an anchor: the reference edge and the placement.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorVertical {
    /// What the position is measured from (`@relativeFrom`).
    pub relative_from: VerticalAnchor,
    /// The offset or alignment within that reference.
    pub position: VerticalPosition,
}

/// Text-exclusion distances around a floating anchor
/// (`wp:anchor@distT/distB/distL/distR`), in non-negative EMU.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WrapDistances {
    /// Distance above the object (`distT`).
    pub top_emu: i64,
    /// Distance below the object (`distB`).
    pub bottom_emu: i64,
    /// Distance on the leading/left side (`distL`).
    pub start_emu: i64,
    /// Distance on the trailing/right side (`distR`).
    pub end_emu: i64,
}

impl WrapDistances {
    /// Whether every exclusion distance is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// The position, wrap, and z-order of an anchored (floating) drawing — the
/// `wp:anchor` frame around a `pic:pic`, as opposed to an inline `wp:inline`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawingAnchor {
    /// The horizontal placement (`wp:positionH`).
    pub horizontal: AnchorHorizontal,
    /// The vertical placement (`wp:positionV`).
    pub vertical: AnchorVertical,
    /// How text flows around the drawing (`wp:wrap*`).
    pub wrap: WrapMode,
    /// Text-exclusion distances around the object.
    #[serde(default, skip_serializing_if = "WrapDistances::is_zero")]
    pub wrap_distances: WrapDistances,
    /// Whether the drawing paints behind the document text (`@behindDoc`),
    /// i.e. its z-order relative to the flow. Only meaningful for
    /// [`WrapMode::None`].
    pub behind_doc: bool,
}

/// An anchored (floating) drawing: an embedded picture placed at an absolute
/// position on the page rather than in the inline flow. Unlike [`Drawing`]
/// (which flows inline), this carries a [`DrawingAnchor`] describing where the
/// image sits, how text wraps, and its z-order.
///
/// The referenced picture's bytes flow through the media table exactly like an
/// inline drawing; only the placement and text-exclusion behavior differ.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchoredDrawing {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced media entry (resolves in `Definitions::media`).
    pub media: MediaId,
    /// The drawing's rendered size (`wp:extent`, EMU). Always present on an
    /// anchor.
    pub extent: Extent,
    /// The anchor: position, wrap, and z-order.
    pub anchor: DrawingAnchor,
    /// The alt text (`wp:docPr@descr`), preserved for accessibility, if declared
    /// (non-empty, at most [`MAX_DESCR_BYTES`] bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    /// The stacking key (`wp:anchor@relativeHeight`, `ST_RelFromV`-independent):
    /// the monotonic z-order Word paints floating objects by (higher paints
    /// later, i.e. on top), with document order as the tiebreaker. `None` when the
    /// producer omitted it. `behind_doc` still decides whether the object sits
    /// below or above the text layer; `relative_height` orders objects *within*
    /// each of those two bands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_height: Option<u32>,
    /// The source-rectangle crop (`a:srcRect`), if the picture is cropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<CropRect>,
    /// The picture frame outline (`pic:spPr/a:ln`), if the picture is bordered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ShapeStroke>,
    /// Horizontal flip (`a:xfrm@flipH`): mirror the picture across its vertical
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip (`a:xfrm@flipV`): mirror the picture across its horizontal
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// Clockwise rotation about the box center (`a:xfrm@rot`, in 60000ths of a
    /// degree), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

/// An 8-bit-per-channel RGBA color used by floating-object fills and outlines.
///
/// Unlike a run's [`Color`](crate::v1::Color) (a deferred theme-or-RGB reference
/// resolved at layout), a floating shape's fill/outline is resolved to a concrete
/// color at import — the DrawingML `a:solidFill` (`a:srgbClr`/`a:schemeClr`/
/// `a:sysClr`) plus its luminance/tint/shade/alpha modifiers are folded against
/// the theme color scheme into these four channels.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (`255` = opaque).
    pub a: u8,
}

/// A floating shape/text-box/callout background fill: either a single flat color
/// (`a:solidFill`) or a multi-stop gradient (`a:gradFill`).
///
/// A gradient retains its ordered stops and direction so it round-trips, but
/// layout currently flattens it to its first stop's color (see
/// [`Fill::flat_color`]); real gradient rendering is a follow-up. Colors are
/// resolved to concrete channels at import, exactly like [`Rgba`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    /// A single flat color (`a:solidFill`).
    Solid(Rgba),
    /// A multi-stop gradient (`a:gradFill`): its stops (`a:gsLst/a:gs`) and
    /// direction (`a:lin`/`a:path`).
    Gradient {
        /// The gradient stops in document order (`a:gsLst/a:gs`); always at least
        /// one when imported.
        stops: Vec<GradientStop>,
        /// The gradient geometry (`a:lin` linear or `a:path` radial).
        kind: GradientKind,
    },
}

impl Fill {
    /// The flat color layout paints for this fill: the solid color, or a
    /// gradient's first stop (opaque black if a gradient somehow has no stops).
    #[must_use]
    pub fn flat_color(&self) -> Rgba {
        match self {
            Fill::Solid(color) => *color,
            Fill::Gradient { stops, .. } => stops.first().map_or(
                Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                |stop| stop.color,
            ),
        }
    }
}

/// One gradient stop (`a:gsLst/a:gs`): a position along the gradient and the
/// resolved color painted there.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GradientStop {
    /// The stop position (`a:gs@pos`, `ST_PositiveFixedPercentage`) in per-100000
    /// units (`0` = start, `100000` = 100% = end).
    pub position: i32,
    /// The resolved stop color.
    pub color: Rgba,
}

/// The geometry of a gradient fill: a linear sweep (`a:lin`) or a radial/path
/// gradient (`a:path`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientKind {
    /// A linear gradient (`a:lin`).
    Linear {
        /// The sweep angle (`a:lin@ang`) in 60000ths of a degree, clockwise from
        /// the positive x-axis.
        angle: i32,
    },
    /// A radial/path gradient (`a:path`), collapsed to a concentric fill.
    Radial,
}

/// A point in English Metric Units (EMU): a group child's offset within its
/// group's child coordinate space (`a:off`), or any DrawingML absolute point.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointEmu {
    /// X coordinate in EMU (signed; a child may sit left of the group origin).
    pub x_emu: i64,
    /// Y coordinate in EMU (signed).
    pub y_emu: i64,
}

/// The outline (`a:ln`) of a floating shape: a resolved color and a width in EMU,
/// plus an optional preset dash pattern and head/tail line-end decorations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeStroke {
    /// The resolved outline color.
    pub color: Rgba,
    /// The outline width in EMU (`a:ln@w`; `0..=MAX_EMU`).
    pub width_emu: i64,
    /// The preset dash pattern (`a:ln > a:prstDash@val`), if authored. `None`
    /// leaves the outline solid (the DrawingML default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<DashStyle>,
    /// The line's start decoration (`a:ln > a:headEnd`), e.g. an arrowhead on a
    /// connector or callout leader, if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_end: Option<LineEnd>,
    /// The line's end decoration (`a:ln > a:tailEnd`), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_end: Option<LineEnd>,
}

/// A preset line dash pattern (`a:prstDash@val`, `ST_PresetLineDashVal`). An
/// unrecognized token is not captured (the outline stays solid); only the common
/// preset patterns are modeled. This carries the dash choice through round-trips;
/// rendering the pattern is a follow-up.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DashStyle {
    /// An unbroken line (`solid`).
    Solid,
    /// A dotted line (`dot`).
    Dot,
    /// A dashed line (`dash`).
    Dash,
    /// A large-dash line (`lgDash`).
    LargeDash,
    /// A dash-dot line (`dashDot`).
    DashDot,
    /// A large-dash-dot line (`lgDashDot`).
    LargeDashDot,
    /// A large-dash-dot-dot line (`lgDashDotDot`).
    LargeDashDotDot,
    /// A system dashed line (`sysDash`).
    SystemDash,
    /// A system dotted line (`sysDot`).
    SystemDot,
    /// A system dash-dot line (`sysDashDot`).
    SystemDashDot,
    /// A system dash-dot-dot line (`sysDashDotDot`).
    SystemDashDotDot,
}

/// A line-end decoration (`a:headEnd`/`a:tailEnd`): the arrowhead type plus the
/// optional relative width/length size tokens. Carried through round-trips;
/// drawing the arrowhead is a follow-up.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineEnd {
    /// The arrowhead type (`@type`, `ST_LineEndType`).
    pub kind: LineEndKind,
    /// The arrowhead width relative to the line (`@w`, `ST_LineEndWidth`), if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<LineEndSize>,
    /// The arrowhead length relative to the line (`@len`, `ST_LineEndLength`), if
    /// set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<LineEndSize>,
}

/// A line-end arrowhead type (`a:headEnd`/`a:tailEnd` `@type`, `ST_LineEndType`).
/// An unrecognized token is treated as [`LineEndKind::None`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEndKind {
    /// No decoration (`none`).
    None,
    /// A triangle arrowhead (`triangle`).
    Triangle,
    /// A stealth (concave) arrowhead (`stealth`).
    Stealth,
    /// A diamond terminator (`diamond`).
    Diamond,
    /// An oval terminator (`oval`).
    Oval,
    /// An open arrow (`arrow`).
    Arrow,
}

/// A line-end size token (`@w`, `ST_LineEndWidth`; `@len`, `ST_LineEndLength`):
/// the arrowhead's width/length relative to the line weight.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEndSize {
    /// Small (`sm`).
    Small,
    /// Medium (`med`).
    Medium,
    /// Large (`lg`).
    Large,
}

/// The preset geometry of a simple DrawingML shape (`a:prstGeom@prst`). Only the
/// bounded primitive subset implemented by layout/render is distinguished;
/// every other preset is [`ShapeGeometry::Other`] (drawn as its bounding
/// rectangle while its original token is retained by [`GroupShape::preset`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeGeometry {
    /// A rectangle (`rect`).
    Rectangle,
    /// A rounded rectangle (`roundRect`).
    RoundRectangle,
    /// An ellipse (`ellipse`).
    Ellipse,
    /// An isosceles triangle (`triangle`).
    Triangle,
    /// A right triangle (`rtTriangle`).
    RightTriangle,
    /// A diamond (`diamond`).
    Diamond,
    /// A straight line / connector (`line`, or a `wps:cxnSp` straight connector).
    Line,
    /// Any other preset, drawn as its bounding rectangle.
    Other,
}

/// Maximum UTF-8 length of a retained DrawingML preset-geometry token.
pub const MAX_SHAPE_PRESET_BYTES: usize = 64;

/// Maximum adjustment guides retained for one preset shape.
pub const MAX_SHAPE_ADJUSTMENTS: usize = 32;

/// Maximum UTF-8 length of an adjustment-guide name.
pub const MAX_SHAPE_GUIDE_NAME_BYTES: usize = 64;

/// Maximum UTF-8 length of an adjustment-guide formula.
pub const MAX_SHAPE_FORMULA_BYTES: usize = 256;

/// One ordered DrawingML preset adjustment (`a:avLst/a:gd`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeAdjustment {
    /// Guide name (`a:gd@name`).
    pub name: String,
    /// Guide formula (`a:gd@fmla`, commonly `val 16667`).
    pub formula: String,
}

/// A group transform (`wpg:grpSpPr`/`a:grpSpPr` `a:xfrm`): the group's box in its
/// parent's coordinate space (`a:off`/`a:ext`) and the child coordinate space the
/// children's offsets/extents are expressed in (`a:chOff`/`a:chExt`). A child
/// point `p` maps to the parent space by
/// `off + (p - chOff) * (ext / chExt)` per axis, so a group can translate and
/// scale its children.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupTransform {
    /// The group box origin in the parent space (`a:off`).
    pub offset: PointEmu,
    /// The group box size in the parent space (`a:ext`).
    pub extent: Extent,
    /// The child-space origin (`a:chOff`).
    pub child_offset: PointEmu,
    /// The child-space size (`a:chExt`).
    pub child_extent: Extent,
    /// Horizontal flip (`a:xfrm@flipH`): mirror the group across its vertical
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip (`a:xfrm@flipV`): mirror the group across its horizontal
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// Clockwise rotation about the box center (`a:xfrm@rot`, in 60000ths of a
    /// degree), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

/// A picture child of a [`WordprocessingGroup`] (`pic:pic`): an embedded picture
/// sized by its OWN `a:ext` (not the enclosing group's extent — this is what
/// keeps a grouped logo from being stretched to the group box).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupPicture {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced media entry (resolves in `Definitions::media`).
    pub media: MediaId,
    /// The picture's top-left in the group's child coordinate space (`a:off`).
    pub offset: PointEmu,
    /// The picture's own size (`a:ext`, EMU).
    pub extent: Extent,
    /// The alt text (`wp:docPr@descr`/`pic:cNvPr@descr`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    /// The source-rectangle crop (`a:srcRect`), if the picture is cropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<CropRect>,
    /// The picture frame outline (`pic:spPr/a:ln`), if the picture is bordered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ShapeStroke>,
    /// Horizontal flip (`a:xfrm@flipH`): mirror the picture across its vertical
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip (`a:xfrm@flipV`): mirror the picture across its horizontal
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// Clockwise rotation about the box center (`a:xfrm@rot`, in 60000ths of a
    /// degree), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

/// DrawingML text-box internal margins (`wps:bodyPr@lIns/tIns/rIns/bIns`), in
/// signed EMU. The asymmetric defaults are defined by DrawingML: 0.1 inch on
/// the physical left/right and 0.05 inch on the top/bottom.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBoxInsets {
    /// Physical left inset (`lIns`), in EMU.
    pub left_emu: i32,
    /// Top inset (`tIns`), in EMU.
    pub top_emu: i32,
    /// Physical right inset (`rIns`), in EMU.
    pub right_emu: i32,
    /// Bottom inset (`bIns`), in EMU.
    pub bottom_emu: i32,
}

impl TextBoxInsets {
    /// DrawingML's implied left/right inset (0.1 inch).
    pub const DEFAULT_HORIZONTAL_EMU: i32 = 91_440;
    /// DrawingML's implied top/bottom inset (0.05 inch).
    pub const DEFAULT_VERTICAL_EMU: i32 = 45_720;

    /// Whether every side is at its DrawingML default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for TextBoxInsets {
    fn default() -> Self {
        Self {
            left_emu: Self::DEFAULT_HORIZONTAL_EMU,
            top_emu: Self::DEFAULT_VERTICAL_EMU,
            right_emu: Self::DEFAULT_HORIZONTAL_EMU,
            bottom_emu: Self::DEFAULT_VERTICAL_EMU,
        }
    }
}

/// Vertical placement of a text body inside its shape
/// (`wps:bodyPr@anchor`, `ST_TextAnchoringType`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextBoxVerticalAnchor {
    /// Place content at the top inset (`anchor="t"`, the schema default).
    #[default]
    Top,
    /// Center the content stack in the available inner height (`anchor="ctr"`).
    Center,
    /// Place content against the bottom inset (`anchor="b"`).
    Bottom,
}

/// Horizontal overflow policy for a DrawingML text body.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextBoxHorizontalOverflow {
    /// Allow content to paint outside the shape horizontally (schema default).
    #[default]
    Overflow,
    /// Clip content at the shape's horizontal bounds.
    Clip,
}

/// Vertical overflow policy for a DrawingML text body.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextBoxVerticalOverflow {
    /// Allow content to paint outside the shape vertically (schema default).
    #[default]
    Overflow,
    /// Clip content at the shape's vertical bounds.
    Clip,
    /// Clip overflowing content and request a terminal ellipsis.
    Ellipsis,
}

/// DrawingML text autofit choice (`a:noAutofit` / `a:spAutoFit` /
/// `a:normAutofit`). Omission is semantically equivalent to no autofit.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextBoxAutoFit {
    /// Keep a positive authored shape extent fixed.
    #[default]
    None,
    /// Grow the shape to contain the flowed text (`a:spAutoFit`).
    Shape,
    /// Scale text and percentage line spacing inside a fixed shape.
    Normal {
        /// Percentage in per-100000 units (`100000` = 100%, schema default).
        #[serde(
            default = "default_text_box_font_scale",
            skip_serializing_if = "is_default_text_box_font_scale"
        )]
        font_scale: u32,
        /// Percentage-point reduction in per-100000 units (`0` = none).
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        line_spacing_reduction: u32,
    },
}

const fn default_text_box_font_scale() -> u32 {
    100_000
}

fn is_default_text_box_font_scale(value: &u32) -> bool {
    *value == default_text_box_font_scale()
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// The supported `wps:bodyPr` box-model, overflow, alignment, and autofit
/// properties shared by standalone and grouped DrawingML text boxes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBoxBodyProperties {
    /// Independent physical-side internal margins.
    #[serde(default, skip_serializing_if = "TextBoxInsets::is_default")]
    pub insets: TextBoxInsets,
    /// Vertical placement of the flowed block stack.
    #[serde(default, skip_serializing_if = "is_default_text_box_anchor")]
    pub vertical_anchor: TextBoxVerticalAnchor,
    /// Horizontal paint overflow.
    #[serde(
        default,
        skip_serializing_if = "is_default_text_box_horizontal_overflow"
    )]
    pub horizontal_overflow: TextBoxHorizontalOverflow,
    /// Vertical paint overflow.
    #[serde(default, skip_serializing_if = "is_default_text_box_vertical_overflow")]
    pub vertical_overflow: TextBoxVerticalOverflow,
    /// Text/shape autofit behavior.
    #[serde(default, skip_serializing_if = "is_default_text_box_auto_fit")]
    pub auto_fit: TextBoxAutoFit,
}

impl TextBoxBodyProperties {
    /// Whether every body property has its DrawingML implied value.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_default_text_box_anchor(value: &TextBoxVerticalAnchor) -> bool {
    *value == TextBoxVerticalAnchor::default()
}

fn is_default_text_box_horizontal_overflow(value: &TextBoxHorizontalOverflow) -> bool {
    *value == TextBoxHorizontalOverflow::default()
}

fn is_default_text_box_vertical_overflow(value: &TextBoxVerticalOverflow) -> bool {
    *value == TextBoxVerticalOverflow::default()
}

fn is_default_text_box_auto_fit(value: &TextBoxAutoFit) -> bool {
    *value == TextBoxAutoFit::default()
}

/// A text-box child of a [`WordprocessingGroup`] (`wps:wsp` with a `wps:txbx`):
/// self-positioning block content with an optional fill and outline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupTextBox {
    /// Stable identity.
    pub id: NodeId,
    /// The box's top-left in the group's child coordinate space (`a:off`).
    pub offset: PointEmu,
    /// The box's size (`a:ext`, EMU).
    pub extent: Extent,
    /// The box's block content (non-empty; paragraphs and nested tables), flowed
    /// through the same pipeline as the body.
    pub blocks: Vec<BlockNode>,
    /// The box background fill (`a:solidFill`/`a:gradFill`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// The box outline (`a:ln`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ShapeStroke>,
    /// Internal margins, vertical anchoring, overflow, and autofit (`wps:bodyPr`).
    #[serde(default, skip_serializing_if = "TextBoxBodyProperties::is_default")]
    pub body_properties: TextBoxBodyProperties,
    /// Horizontal flip (`a:xfrm@flipH`): mirror the box across its vertical axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip (`a:xfrm@flipV`): mirror the box across its horizontal axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// Clockwise rotation about the box center (`a:xfrm@rot`, in 60000ths of a
    /// degree), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

/// A shape child of a [`WordprocessingGroup`] (`wps:wsp`/`wps:cxnSp` with no
/// text): a preset geometry with an optional fill and outline. Rectangles and
/// lines/connectors that layer around the group's pictures are the common case.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupShape {
    /// Stable identity.
    pub id: NodeId,
    /// The shape's top-left in the group's child coordinate space (`a:off`).
    pub offset: PointEmu,
    /// The shape's size (`a:ext`, EMU). A line's `cy` (or `cx`) may be `0`.
    pub extent: Extent,
    /// The preset geometry (`a:prstGeom@prst`).
    pub geometry: ShapeGeometry,
    /// Original bounded preset token when [`ShapeGeometry::Other`] has no typed
    /// primitive yet. Semantic export re-emits it instead of rewriting to `rect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Ordered preset adjustment guides (`a:avLst/a:gd`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adjustments: Vec<ShapeAdjustment>,
    /// The fill (`a:solidFill`/`a:gradFill`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// The outline (`a:ln`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<ShapeStroke>,
    /// Horizontal flip (`a:xfrm@flipH`): mirror the shape across its vertical
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip (`a:xfrm@flipV`): mirror the shape across its horizontal
    /// axis.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// Clockwise rotation about the box center (`a:xfrm@rot`, in 60000ths of a
    /// degree), if authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
}

/// A child of a [`WordprocessingGroup`], in the group's child coordinate space.
/// The children are stored in DOCUMENT ORDER; Word paints them in that order, so
/// the child's index in [`WordprocessingGroup::children`] IS its intra-group
/// z-index (a later child paints on top of an earlier one).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GroupChild {
    /// An embedded picture (`pic:pic`).
    Picture(GroupPicture),
    /// A text box (`wps:wsp` with `wps:txbx`).
    TextBox(GroupTextBox),
    /// A rectangle / line / other preset shape (`wps:wsp`/`wps:cxnSp`).
    Shape(GroupShape),
    /// A nested group (`wpg:grpSp`), positioned by its own transform.
    Group(WordprocessingGroup),
}

/// The maximum nesting depth of DrawingML groups (a `wpg:grpSp` inside a
/// `wpg:grpSp` inside …), bounding recursion during import and validation.
pub const MAX_GROUP_DEPTH: u32 = 16;

/// A DrawingML group (`wpg:wgp` as an anchored object, or `wpg:grpSp` nested):
/// a positioned container that paints an ordered list of children (pictures, text
/// boxes, shapes, and nested groups) in its own coordinate space.
///
/// The top-level anchored group carries the floating [`anchor`](Self::anchor),
/// the anchor `wp:extent`, and the [`relative_height`](Self::relative_height) z
/// key; a nested group leaves those `None`/`0` and is positioned purely by its
/// [`transform`](Self::transform). Sizing each picture by its OWN extent (rather
/// than the group's) is what fixes the stretched-logo defect; painting children
/// in order is what reproduces Word's "one shape behind the image, one in front"
/// layering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordprocessingGroup {
    /// Stable identity.
    pub id: NodeId,
    /// The floating anchor (`wp:anchor`) — `Some` for a top-level anchored group,
    /// `None` for a nested `wpg:grpSp` (positioned by its parent's child space).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<DrawingAnchor>,
    /// The stacking key (`wp:anchor@relativeHeight`) for a top-level group; `None`
    /// for a nested group. See [`AnchoredDrawing::relative_height`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_height: Option<u32>,
    /// The anchor size (`wp:extent`, EMU) for a top-level group. Unused for a
    /// nested group, whose box is its [`transform`](Self::transform)'s `extent`.
    pub extent: Extent,
    /// The group transform (`a:xfrm`): parent-space box + child coordinate space.
    pub transform: GroupTransform,
    /// The children, in document (paint) order.
    pub children: Vec<GroupChild>,
}

/// The relationship type and package part a first-class embedded object
/// references. The referenced part's BYTES are not modeled (they live in the
/// preservation side-table, doc-45 invariant I4); this carries only the pointer
/// the writer needs to re-emit the referencing relationship.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedPart {
    /// Source relationship id (`r:id`), reused verbatim on export so the body's
    /// reference and the emitted relationship agree.
    pub relationship_id: String,
    /// Relationship type URI (`.../chart`, `.../diagramData`, `.../oleObject`, …).
    pub relationship_type: String,
    /// Package part name (e.g. `word/charts/chart1.xml`).
    pub part_name: String,
}

/// What kind of embedded object a first-class reference points at.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddedKind {
    /// A DrawingML chart (`a:graphicData` with a `c:chart`).
    Chart,
    /// A SmartArt diagram (`a:graphicData` with a `dgm:relIds`).
    Diagram,
    /// An embedded OLE object (`w:object` with an `o:OLEObject`).
    OleObject,
    /// Any other `a:graphicData` payload, keyed by its `uri`.
    Other(String),
}

/// An inline embedded object — a chart, SmartArt diagram, or OLE object — modeled
/// as a first-class reference to its preserved package part(s).
///
/// Unlike an inline picture (whose bytes flow through the media table), an
/// embedded object's parts (`word/charts/chart1.xml`, `word/diagrams/*`,
/// `word/embeddings/*`) are opaque XML/binary the semantic model does not parse;
/// they are byte-preserved by the side-table (P1F-2). This node re-links the
/// regenerated body to those parts so they round-trip as an *editable reference*
/// rather than surviving as orphaned bytes. Anchored/floating positioning is not
/// modeled (P1F-28); the object is treated as inline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedObject {
    /// Stable identity.
    pub id: NodeId,
    /// Which kind of embedded object this references.
    pub kind: EmbeddedKind,
    /// The primary referenced part (the chart, the diagram *data model*, or the
    /// OLE embedding).
    pub part: EmbeddedPart,
    /// Additional referenced parts (a diagram's layout/quick-style/colors), in
    /// the order they were declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_parts: Vec<EmbeddedPart>,
    /// The fallback preview image (a chart/diagram cached bitmap or an OLE
    /// `o:OLEObject` imagedata), resolving in `Definitions::media`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<MediaId>,
    /// The object's natural size in EMU (from `wp:extent` or `w:object` origins;
    /// `0×0` when the producer declared none).
    pub extent: Extent,
    /// The OLE `ProgID` (`o:OLEObject@ProgID`), if declared (non-empty, <= 255
    /// bytes). `None` for charts and diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prog_id: Option<String>,
}

/// Properties of an aggregated external content chunk (`w:altChunkPr`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltChunkProperties {
    /// `w:matchSrc` — whether the imported content is rendered using the source
    /// chunk's own formatting (`Some(true)`/`Some(false)`) rather than the host
    /// document's. `None` when the producer left it unspecified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_source: Option<bool>,
}

/// An aggregated external content chunk (`w:altChunk`): a reference to an imported
/// sub-document part — an HTML, RTF, plain-text, or nested WordprocessingML chunk
/// — that a consuming application merges into the main document when it opens the
/// package.
///
/// Like an [`EmbeddedObject`], the chunk part's bytes are not modeled; they are
/// byte-preserved by the opaque side-table (P1F-2) and this node re-links the
/// regenerated body to the part by its verbatim relationship id (so the part
/// round-trips as an *editable reference* rather than surviving as orphaned
/// bytes). The chunk is treated as an opaque block: its inner structure is not
/// parsed here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltChunk {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced chunk part (`w:altChunk@r:id`). Its bytes live in the
    /// preservation side-table; this carries the pointer the writer needs to
    /// re-emit the referencing relationship.
    pub part: EmbeddedPart,
    /// Chunk properties (`w:altChunkPr`; always present, empty is `{}`).
    pub properties: AltChunkProperties,
}

/// An external hyperlink target (a resolved relationship URL).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalTarget {
    /// The target URL (non-empty, at most 2048 bytes).
    pub url: String,
    /// An in-target fragment (`w:hyperlink@w:anchor` alongside `r:id`): a named
    /// location within the external target (e.g. a bookmark in another document).
    /// Non-empty, at most 255 bytes; absent when the link carries no fragment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

/// An internal hyperlink target (a document bookmark anchor).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InternalTarget {
    /// The bookmark anchor name (non-empty, at most 255 bytes).
    pub anchor: String,
}

/// Where a hyperlink points.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HyperlinkTarget {
    /// An external URL resolved through the relationship graph.
    External(ExternalTarget),
    /// An internal bookmark anchor.
    Internal(InternalTarget),
}

/// An inline hyperlink wrapping a sequence of inline content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hyperlink {
    /// Stable identity.
    pub id: NodeId,
    /// Where the hyperlink points.
    pub target: HyperlinkTarget,
    /// A screen-tip, if declared (non-empty, at most 255 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// The hyperlinked inline content (non-empty; never a nested wrapper).
    pub inlines: Vec<InlineNode>,
}

/// Maximum field-instruction length, in UTF-8 bytes.
pub const MAX_FIELD_INSTRUCTION_BYTES: usize = 4096;

/// An inline field: a retained instruction and its cached result.
///
/// A field's dynamic value is not evaluated; `instruction` is the opaque field
/// code (`w:instr` / concatenated `w:instrText`) and `inlines` is the producer's
/// cached result (the runs a reader last computed). `inlines` may be empty and,
/// like a hyperlink, contains only leaf inlines — never a nested wrapper.
///
/// A legacy form field (Word's FORMTEXT / FORMCHECKBOX / FORMDROPDOWN, delimited
/// by `w:fldChar` with a `w:ffData` block) additionally carries `form` — its
/// input configuration (field name, type, default, entries, checkbox state).
/// `None` for an ordinary field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Field {
    /// Stable identity.
    pub id: NodeId,
    /// The field instruction (non-empty, at most `MAX_FIELD_INSTRUCTION_BYTES`).
    pub instruction: String,
    /// The typed field-kind projection derived from `instruction`.
    ///
    /// This is an additive, best-effort classification of the leading field
    /// keyword and its common switch/argument (see [`FieldKind::parse`]); the
    /// raw `instruction` string remains authoritative for export and exact
    /// round-trip. Defaults to [`FieldKind::Other`] for legacy payloads that
    /// predate this field.
    #[serde(default)]
    pub kind: FieldKind,
    /// The cached-result inline content (possibly empty; leaf inlines only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inlines: Vec<InlineNode>,
    /// Legacy form-field configuration (`w:ffData`), when this field is a legacy
    /// form field. `None` for an ordinary field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<FormFieldData>,
}

/// A typed projection of a Word field's leading instruction keyword and its
/// common switch/argument.
///
/// This is an additive, best-effort classification: [`Field::instruction`]
/// stays authoritative for export, and any unrecognized instruction projects to
/// [`FieldKind::Other`] carrying the (upper-cased) leading keyword.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldKind {
    /// `PAGE` — the current page number.
    Page,
    /// `NUMPAGES` — the total number of pages.
    NumPages,
    /// `DATE` — the current date, with the optional `\@` picture switch.
    Date {
        /// The date format picture (`\@ "…"`), if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    /// `TIME` — the current time, with the optional `\@` picture switch.
    Time {
        /// The time format picture (`\@ "…"`), if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    /// `REF` — a cross-reference to a bookmark.
    Ref {
        /// The referenced bookmark name.
        bookmark: String,
    },
    /// `PAGEREF` — the page number of a bookmark.
    PageRef {
        /// The referenced bookmark name.
        bookmark: String,
    },
    /// `TOC` — a table of contents.
    Toc,
    /// `SEQ` — a sequence counter.
    Seq {
        /// The sequence name.
        name: String,
    },
    /// `STYLEREF` — text from the nearest paragraph of a named style.
    StyleRef {
        /// The referenced style name or id.
        style: String,
    },
    /// `HYPERLINK` — a hyperlink to a target URL or internal anchor.
    Hyperlink {
        /// The hyperlink target (URL, or the `\l` anchor), if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// Any other field: the upper-cased leading keyword is retained for
    /// classification (the full instruction stays on [`Field::instruction`]).
    Other {
        /// The upper-cased leading keyword (may be empty for a blank
        /// instruction).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        keyword: String,
    },
}

impl Default for FieldKind {
    fn default() -> Self {
        FieldKind::Other {
            keyword: String::new(),
        }
    }
}

impl FieldKind {
    /// Classifies a field `instruction` by its leading keyword (case-insensitive)
    /// and its common switch/argument.
    ///
    /// This never fails: an unrecognized or malformed instruction yields
    /// [`FieldKind::Other`]. The result is a projection only — the raw
    /// instruction remains authoritative for export.
    pub fn parse(instruction: &str) -> FieldKind {
        let tokens = tokenize_field_instruction(instruction);
        let Some(keyword) = tokens.first() else {
            return FieldKind::Other {
                keyword: String::new(),
            };
        };
        let keyword = keyword.to_ascii_uppercase();
        let arguments = &tokens[1..];
        // The first token that is not a `\`-switch: the primary identifier a
        // producer always writes immediately after the keyword.
        let first_argument = || {
            arguments
                .iter()
                .find(|token| !token.starts_with('\\'))
                .cloned()
        };
        // The token immediately following a named `\`-switch (e.g. `\@`).
        let switch_argument = |name: &str| {
            arguments
                .iter()
                .position(|token| token.eq_ignore_ascii_case(name))
                .and_then(|index| arguments.get(index + 1))
                .cloned()
        };
        match keyword.as_str() {
            "PAGE" => FieldKind::Page,
            "NUMPAGES" => FieldKind::NumPages,
            "DATE" => FieldKind::Date {
                format: switch_argument("\\@"),
            },
            "TIME" => FieldKind::Time {
                format: switch_argument("\\@"),
            },
            "TOC" => FieldKind::Toc,
            "REF" => match first_argument() {
                Some(bookmark) => FieldKind::Ref { bookmark },
                None => FieldKind::Other { keyword },
            },
            "PAGEREF" => match first_argument() {
                Some(bookmark) => FieldKind::PageRef { bookmark },
                None => FieldKind::Other { keyword },
            },
            "SEQ" => match first_argument() {
                Some(name) => FieldKind::Seq { name },
                None => FieldKind::Other { keyword },
            },
            "STYLEREF" => match first_argument() {
                Some(style) => FieldKind::StyleRef { style },
                None => FieldKind::Other { keyword },
            },
            "HYPERLINK" => FieldKind::Hyperlink {
                target: first_argument(),
            },
            _ => FieldKind::Other { keyword },
        }
    }
}

/// Splits a field instruction into whitespace-separated tokens, treating a
/// double-quoted span as a single token (quotes stripped) and a `\`-switch as
/// its own token. Best-effort: unterminated quotes run to the end.
fn tokenize_field_instruction(instruction: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut characters = instruction.chars().peekable();
    while let Some(&character) = characters.peek() {
        if character.is_whitespace() {
            characters.next();
        } else if character == '"' {
            characters.next();
            let mut token = String::new();
            for character in characters.by_ref() {
                if character == '"' {
                    break;
                }
                token.push(character);
            }
            tokens.push(token);
        } else {
            let mut token = String::new();
            while let Some(&character) = characters.peek() {
                if character.is_whitespace() || character == '"' {
                    break;
                }
                token.push(character);
                characters.next();
            }
            tokens.push(token);
        }
    }
    tokens
}

/// Maximum length, in UTF-8 bytes, of a form-field string (name, default text,
/// help/status text, macro name, format, or a drop-down list entry).
pub const MAX_FORM_FIELD_STRING_BYTES: usize = 255;

/// Maximum number of entries in a form drop-down list (`w:ddList`).
pub const MAX_FORM_FIELD_ENTRIES: usize = 512;

/// Legacy form-field configuration (`w:ffData`): the common `CT_FFData`
/// properties plus exactly one kind-specific payload (text input, checkbox, or
/// drop-down). Attached to a [`Field`] whose instruction is FORMTEXT /
/// FORMCHECKBOX / FORMDROPDOWN.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormFieldData {
    /// The form-field name (`w:name@w:val`), if declared (non-empty, bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the field accepts input (`w:enabled`, `CT_OnOff`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Whether to recalculate fields on exit (`w:calcOnExit`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calc_on_exit: Option<bool>,
    /// Associated help text (`w:helpText@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    /// Associated status-bar text (`w:statusText@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    /// Macro run on entry (`w:entryMacro@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_macro: Option<String>,
    /// Macro run on exit (`w:exitMacro@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_macro: Option<String>,
    /// The kind-specific payload; must agree with the field's instruction.
    pub kind: FormFieldKind,
}

/// The kind-specific payload of a [`FormFieldData`] (`CT_FFData`'s one-of
/// `w:textInput` / `w:checkBox` / `w:ddList`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum FormFieldKind {
    /// A text-input form field (`w:textInput`, FORMTEXT).
    TextInput(FormTextInput),
    /// A checkbox form field (`w:checkBox`, FORMCHECKBOX).
    CheckBox(FormCheckBox),
    /// A drop-down form field (`w:ddList`, FORMDROPDOWN).
    DropDown(FormDropDown),
}

/// The value type of a text-input form field (`w:textInput/w:type@w:val`,
/// `ST_FFTextType`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FormTextType {
    /// Unconstrained text (`regular`).
    Regular,
    /// A number (`number`).
    Number,
    /// A date (`date`).
    Date,
    /// The current time (`currentTime`).
    CurrentTime,
    /// The current date (`currentDate`).
    CurrentDate,
    /// A calculated result (`calculated`).
    Calculation,
}

/// A text-input form field's configuration (`w:textInput`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormTextInput {
    /// The value type (`w:type`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_type: Option<FormTextType>,
    /// The default text (`w:default@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The maximum input length (`w:maxLength@w:val`), if declared. `0` means
    /// unlimited, mirroring the OOXML sentinel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// The text format string (`w:format@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// The size of a checkbox form field (`w:checkBox`'s `w:size` / `w:sizeAuto`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum FormCheckBoxSize {
    /// An explicit size in half-points (`w:size@w:val`, `CT_HpsMeasure`).
    Explicit(u32),
    /// Automatically sized to the surrounding text (`w:sizeAuto`).
    Auto,
}

/// A checkbox form field's configuration (`w:checkBox`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormCheckBox {
    /// The checkbox size (`w:size` / `w:sizeAuto`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<FormCheckBoxSize>,
    /// The default checked state (`w:default`, `CT_OnOff`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    /// The current checked state (`w:checked`, `CT_OnOff`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

/// A drop-down form field's configuration (`w:ddList`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormDropDown {
    /// The selected entry index (`w:result@w:val`), if declared. Zero-based into
    /// `entries`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<u32>,
    /// The list entries (`w:listEntry@w:val`), in document order (each bounded;
    /// at most `MAX_FORM_FIELD_ENTRIES`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
}

/// Maximum text-box nesting depth (a text box inside a text box inside ...).
pub const MAX_TEXTBOX_DEPTH: u32 = 8;

/// A text box holding block content (a DrawingML `wps:txbx` or a legacy VML
/// `v:textbox`). It may participate in inline flow or carry a floating anchor;
/// its `blocks` reuse the recursive block model in either case.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBox {
    /// Stable identity.
    pub id: NodeId,
    /// The floating anchor (`wp:anchor`), when this text box is positioned rather
    /// than inline. `None` = the inline case (laid out in the run flow, as before);
    /// `Some` = a self-positioning floating text box placed at its anchor, painted
    /// through the float layer with its own [`fill`](Self::fill)/[`border`](Self::border).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<DrawingAnchor>,
    /// The stacking key (`wp:anchor@relativeHeight`) when floating; see
    /// [`AnchoredDrawing::relative_height`]. `None` for an inline text box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_height: Option<u32>,
    /// The box's authored size (`wp:extent`/`a:xfrm/a:ext`, EMU), for either an
    /// inline or floating box. A missing dimension is resolved from the flow
    /// context and content during layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<Extent>,
    /// The box background fill (`a:solidFill`/`a:gradFill`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// The box outline (`a:ln`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ShapeStroke>,
    /// Internal margins, vertical anchoring, overflow, and autofit (`wps:bodyPr`).
    #[serde(default, skip_serializing_if = "TextBoxBodyProperties::is_default")]
    pub body_properties: TextBoxBodyProperties,
    /// The text box's block content (non-empty; paragraphs and nested tables).
    pub blocks: Vec<BlockNode>,
}

/// Whether a note reference points at a footnote or an endnote.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// A footnote.
    Footnote,
    /// An endnote.
    Endnote,
}

/// An inline reference to a footnote or endnote definition (`w:footnoteReference`
/// / `w:endnoteReference`). The referenced note's content is a definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteReference {
    /// Stable identity.
    pub id: NodeId,
    /// Whether this references a footnote or an endnote.
    pub kind: NoteKind,
    /// The referenced note (resolves in `Definitions::footnotes`/`endnotes`).
    pub note: NoteId,
}

/// The auto-number mark inside a note's own body (`w:footnoteRef` /
/// `w:endnoteRef`): the point where the note renders its own number. Unlike
/// [`NoteReference`] (the body-side mark that points AT a note), this appears
/// INSIDE the footnote/endnote definition and prints that note's number. It
/// carries the run formatting of its enclosing run (typically the note's
/// reference character style), so the number round-trips with its styling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteNumberMark {
    /// Stable identity.
    pub id: NodeId,
    /// Whether this is a footnote's (`w:footnoteRef`) or an endnote's
    /// (`w:endnoteRef`) auto-number mark.
    pub kind: NoteKind,
    /// The run properties of the enclosing run (the mark's own formatting).
    pub properties: RunProperties,
}

/// An inline reference to a comment definition (`w:commentReference`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentReference {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced comment (resolves in `Definitions::comments`).
    pub comment: CommentId,
}

/// The start marker of a comment's anchored range (`w:commentRangeStart`). A
/// zero-width point; the commented span runs from here to the [`CommentRangeEnd`]
/// sharing its `comment`. The comment's content and metadata live in
/// `Definitions::comments`, reached through the paired [`CommentReference`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentRangeStart {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The comment this range opens (resolves in `Definitions::comments`).
    pub comment: CommentId,
}

/// The end marker of a comment's anchored range (`w:commentRangeEnd`). Closes the
/// [`CommentRangeStart`] sharing its `comment`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentRangeEnd {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The comment this range closes (resolves in `Definitions::comments`).
    pub comment: CommentId,
}

/// Maximum revision-wrapper nesting depth (a `w:ins` around a `w:del`, ...).
pub const MAX_REVISION_DEPTH: u32 = 8;

/// Whether a tracked-change range was inserted, deleted, or moved.
///
/// A move is a two-ended revision: the run range at the source location is a
/// `MoveFrom` (its runs carry `w:delText`, like a deletion) and the range at the
/// destination is a `MoveTo` (its runs carry `w:t`, like an insertion). The two
/// ends are correlated by the enclosing [`MoveRangeStart`]/[`MoveRangeEnd`]
/// markers that share a move `name`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    /// An inserted run range (`w:ins`).
    Insertion,
    /// A deleted run range (`w:del`); its runs carry `w:delText` content.
    Deletion,
    /// The source range of a tracked move (`w:moveFrom`); like a deletion, its
    /// runs carry `w:delText` content.
    MoveFrom,
    /// The destination range of a tracked move (`w:moveTo`); like an insertion,
    /// its runs carry `w:t` content.
    MoveTo,
}

/// The text/content projection used when reading tracked revisions.
///
/// The active editor uses [`FinalWithMarkup`](Self::FinalWithMarkup): accepted
/// content is not mutated, but insertion/destination text contributes to the
/// active byte space while deletion/source text is zero-width. `Original` and
/// `Final` make the contract explicit for future read-only view switching.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReviewProjection {
    /// Content before pending tracked changes.
    Original,
    /// Content after pending tracked changes, without requiring review chrome.
    Final,
    /// Final content while comments/revision metadata remain visible.
    #[default]
    FinalWithMarkup,
}

impl RevisionKind {
    /// Whether this revision contributes content to `projection`.
    #[must_use]
    pub const fn contributes_to(self, projection: ReviewProjection) -> bool {
        match projection {
            ReviewProjection::Original => {
                matches!(self, Self::Deletion | Self::MoveFrom)
            }
            ReviewProjection::Final | ReviewProjection::FinalWithMarkup => {
                matches!(self, Self::Insertion | Self::MoveTo)
            }
        }
    }
}

/// OpenDoc-only logical grouping for revisions that form one review decision.
///
/// This metadata is deliberately separate from [`Revision::revision_id`]:
/// `revision_id` is the producer-facing WordprocessingML `w:id`, while this
/// value controls editor card composition and atomic decisions. The semantic
/// DOCX writer does not serialize it as `w:id`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionGroup {
    /// Stable opaque identity for the editor decision group.
    pub id: NodeId,
    /// The member/composition contract enforced before an atomic decision.
    pub kind: RevisionGroupKind,
}

/// Closed composition kinds for editor-authored revision groups.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionGroupKind {
    /// One or more adjacent insertions created by one typing gesture.
    Typing,
    /// Exactly one deletion followed by one insertion.
    Replacement,
    /// One or more contiguous `w:rPrChange` runs authored as one format action.
    ///
    /// The runtime can still validate an older in-memory deletion/insertion pair
    /// long enough to decide it, but new documents never author that shape.
    Formatting,
}

/// A tracked-change (revision) range wrapping inline content (`w:ins`/`w:del`).
///
/// Author/date/id are retained as the producer wrote them (opaque, bounded),
/// mirroring `Comment` metadata. Deleted text is preserved verbatim in the
/// wrapped runs' `text`; the `Deletion` kind marks it deleted. A revision is a
/// transparent range marker: it may wrap leaf inlines, a hyperlink/field, or a
/// nested revision, and may itself appear inside a hyperlink/field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    /// Stable identity (this inline node's own id).
    pub id: NodeId,
    /// Whether the range was inserted or deleted.
    pub kind: RevisionKind,
    /// The revision author, if declared (non-empty, at most 255 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The revision date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The producer's revision id (`w:id`) as written, if declared (<= 64 bytes).
    /// Opaque and non-unique across imported ranges; editor-authored values are
    /// unique decimal strings. This is not an OpenDoc decision-group identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
    /// OpenDoc-only card/decision grouping, separate from serialized `w:id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_group: Option<RevisionGroup>,
    /// The wrapped inline content (non-empty; may include a nested revision).
    pub inlines: Vec<InlineNode>,
}

/// The start marker of a bookmark range (`w:bookmarkStart`). A zero-width point;
/// the range is the span to the `BookmarkEnd` sharing its `bookmark`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkStart {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The bookmark this opens (resolves in `Definitions::bookmarks`).
    pub bookmark: BookmarkId,
}

/// The end marker of a bookmark range (`w:bookmarkEnd`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkEnd {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The bookmark this closes (resolves in `Definitions::bookmarks`).
    pub bookmark: BookmarkId,
}

/// Whether a move range marks the source or the destination of a tracked move.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveKind {
    /// The source of a move (`w:moveFromRangeStart`/`End`), paired with the
    /// `w:moveFrom` run wrapper.
    From,
    /// The destination of a move (`w:moveToRangeStart`/`End`), paired with the
    /// `w:moveTo` run wrapper.
    To,
}

/// The start marker of a tracked-move range (`w:moveFromRangeStart` /
/// `w:moveToRangeStart`). A zero-width point; the range is the span to the
/// [`MoveRangeEnd`] of the same `kind` sharing its `move_id`. Its `name`
/// correlates the source (`From`) and destination (`To`) ends of one logical
/// move — Word writes the same `w:name` on all four markers of a move.
///
/// The pairing key `move_id` (`w:id`) and the correlating `name` (`w:name`) are
/// retained as the producer wrote them (opaque, bounded), and `author`/`date`
/// mirror the `w:moveFrom`/`w:moveTo` wrapper metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveRangeStart {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// Whether this opens a move source (`From`) or destination (`To`) range.
    pub kind: MoveKind,
    /// The producer's range pairing id (`w:id`) as written (non-empty, at most
    /// 64 bytes). Opaque; pairs this start with its matching [`MoveRangeEnd`].
    pub move_id: String,
    /// The move name (`w:name`) as written (non-empty, at most 255 bytes).
    /// Correlates the source and destination ends of one logical move.
    pub name: String,
    /// The move author, if declared (non-empty, at most 255 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The move date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// The end marker of a tracked-move range (`w:moveFromRangeEnd` /
/// `w:moveToRangeEnd`). Closes the [`MoveRangeStart`] of the same `kind` whose
/// `move_id` it shares.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveRangeEnd {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// Whether this closes a move source (`From`) or destination (`To`) range.
    pub kind: MoveKind,
    /// The producer's range pairing id (`w:id`) as written (non-empty, at most
    /// 64 bytes). Pairs this end with its matching [`MoveRangeStart`].
    pub move_id: String,
}

/// Maximum content-control (structured document tag) nesting depth (an `sdt`
/// inside an `sdt` inside …). Block and inline sdt nesting share this budget.
pub const MAX_SDT_DEPTH: u32 = 8;

/// The editing behaviour of a content control (`w:sdtPr` type marker). `None`
/// means the producer wrote no type marker — the OOXML default, rich text — or a
/// marker this slice does not map (then also reported). Producer-specific detail
/// of each type (list entries, date format, checkbox glyphs) is deferred.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SdtControlKind {
    /// A rich-text control (`w:richText`).
    RichText,
    /// A plain-text control (`w:text`).
    PlainText,
    /// A combo-box control (`w:comboBox`).
    ComboBox,
    /// A drop-down-list control (`w:dropDownList`).
    DropDownList,
    /// A date-picker control (`w:date`).
    Date,
    /// A picture control (`w:picture`).
    Picture,
    /// A checkbox control (`w14:checkbox`).
    Checkbox,
    /// A grouping control (`w:group`).
    Group,
    /// A building-block gallery (`w:docPartObj` / `w:docPartList`).
    BuildingBlockGallery,
    /// A repeating-section control (`w:repeatingSection`).
    RepeatingSection,
    /// A citation control (`w:citation`).
    Citation,
    /// A bibliography control (`w:bibliography`).
    Bibliography,
}

/// Typed content-control properties (`w:sdtPr`). An empty value serializes to
/// `{}`. The cross-cutting properties (`lock`, `placeholder`,
/// `showing_placeholder`, `temporary`, `data_binding`) and the control-specific
/// `data` (list entries, date, checkbox detail) are modeled here; the remaining
/// long tail (end-mark `w:rPr`, `w15` label/tabIndex) is retained-and-reported.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtProperties {
    /// Editing behaviour, if a recognized type marker was present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_kind: Option<SdtControlKind>,
    /// Friendly name (`w:alias@w:val`), if declared (non-empty, <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Programmatic tag (`w:tag@w:val`), if declared (non-empty, <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The producer's `w:id@w:val` as written, if declared (<= 64 bytes). Opaque
    /// and non-unique across controls — a grouping key, NOT a node identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_id: Option<String>,
    /// The edit-lock behaviour (`w:lock@w:val`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<SdtLock>,
    /// The placeholder building-block name (`w:placeholder`/`w:docPart@w:val`), if
    /// declared (non-empty, <= 255 bytes): the prompt shown while the control is
    /// empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The control is currently displaying its placeholder text
    /// (`w:showingPlcHdr`) rather than real user content.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub showing_placeholder: bool,
    /// The control is temporary and removed once its contents are edited
    /// (`w:temporary`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub temporary: bool,
    /// The customXML data binding (`w:dataBinding`), if declared: pairs the
    /// control with an element in a preserved custom XML data part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_binding: Option<SdtDataBinding>,
    /// The control-specific detail (list entries, date, checkbox), when the
    /// control kind carries any. Validated to agree with `control_kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<SdtControlData>,
    /// The building-block gallery name (`w:docPartObj/w:docPartGallery@w:val`), for
    /// a [`SdtControlKind::BuildingBlockGallery`] control. Non-empty, <= 255 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// The building-block category (`w:docPartObj/w:docPartCategory@w:val`), for a
    /// [`SdtControlKind::BuildingBlockGallery`] control. Non-empty, <= 255 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// The edit-lock behaviour of a content control (`w:lock@w:val`, `ST_Lock`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SdtLock {
    /// `unlocked` — the control may be edited and deleted (an explicit default).
    Unlocked,
    /// `sdtLocked` — the control may not be deleted, but its contents may edit.
    SdtLocked,
    /// `contentLocked` — the contents may not be edited, but the control may delete.
    ContentLocked,
    /// `sdtContentLocked` — neither the control nor its contents may be changed.
    SdtContentLocked,
}

/// A customXML data binding (`w:dataBinding`): maps a content control to an
/// element in a custom XML data part, so edits flow to and from that stored XML.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtDataBinding {
    /// The XPath selecting the bound element (`w:xpath`; non-empty, <= 1024 bytes).
    pub xpath: String,
    /// The bound custom XML part's store id (`w:storeItemID`; typically a GUID,
    /// <= 128 bytes), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_item_id: Option<String>,
    /// The prefix-to-namespace declarations the `xpath` resolves against
    /// (`w:prefixMappings`; <= 1024 bytes), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_mappings: Option<String>,
}

/// The control-specific data of a content control, keyed to its `control_kind`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SdtControlData {
    /// Choice entries for a combo-box or drop-down-list (`w:listItem`).
    List(Vec<SdtListItem>),
    /// Date-picker detail (`w:date`).
    Date(SdtDate),
    /// Checkbox detail (`w14:checkbox`).
    Checkbox(SdtCheckbox),
}

/// A single choice entry of a combo-box / drop-down-list control (`w:listItem`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtListItem {
    /// The label shown to the user (`w:displayText`; <= 255 bytes). When absent,
    /// `value` is displayed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// The stored value selected by this entry (`w:value`; <= 255 bytes). May be
    /// empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

/// Date-picker detail (`w:date`). Every field is optional; all-empty means the
/// producer wrote a bare `<w:date/>` type marker.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtDate {
    /// The stored full date (`w:date@w:fullDate`, an ISO datetime; <= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_date: Option<String>,
    /// The display format string (`w:dateFormat@w:val`; <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
    /// The calendar type (`w:calendar@w:val`; <= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<String>,
    /// The language id keying the format (`w:lid@w:val`; <= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lid: Option<String>,
    /// How the mapped date is stored (`w:storeMappedDataAs@w:val`; <= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_mapped_as: Option<String>,
}

/// Checkbox detail (`w14:checkbox`, the `w14` compatibility namespace).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtCheckbox {
    /// Whether the box is currently checked (`w14:checked@w14:val`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub checked: bool,
    /// The glyph drawn when checked (`w14:checkedState`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_state: Option<SdtCheckboxSymbol>,
    /// The glyph drawn when unchecked (`w14:uncheckedState`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchecked_state: Option<SdtCheckboxSymbol>,
}

/// A checkbox state glyph (`w14:checkedState` / `w14:uncheckedState`): a code
/// point drawn in a named font.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtCheckboxSymbol {
    /// The glyph code point as a hex string (`w14:val`, e.g. `2612`; <= 8 bytes).
    pub val: String,
    /// The font that provides the glyph (`w14:font`; <= 64 bytes), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

/// A block-level content control (`w:sdt` around paragraphs/tables). Its content
/// reuses the recursive block model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockSdt {
    /// Stable identity.
    pub id: NodeId,
    /// Control properties (always present; empty is `{}`).
    pub properties: SdtProperties,
    /// The wrapped block content (non-empty; paragraphs and nested tables).
    pub blocks: Vec<BlockNode>,
}

/// An inline-level content control (`w:sdt` around runs). A transparent inline
/// range wrapper (like `Revision`): it may wrap leaf inlines, a hyperlink/field,
/// or a nested inline sdt, and may itself appear inside a hyperlink/field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InlineSdt {
    /// Stable identity.
    pub id: NodeId,
    /// Control properties (always present; empty is `{}`).
    pub properties: SdtProperties,
    /// The wrapped inline content (non-empty).
    pub inlines: Vec<InlineNode>,
}

/// Maximum retained OMML markup length, in UTF-8 bytes.
pub const MAX_MATH_BYTES: usize = 65_536;

/// Maximum nesting depth of a typed math expression.
pub const MAX_MATH_DEPTH: usize = 32;

/// Maximum number of nodes in one typed math expression.
pub const MAX_MATH_NODES: usize = 4_096;

/// A bounded semantic projection of a supported OMML equation subtree.
///
/// The retained OMML on [`Math`] remains authoritative for export. This tree is
/// additive render/search structure: unsupported OMML safely has no projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MathExpression {
    /// Ordered expressions laid out on one math baseline.
    Row {
        /// Child expressions in logical order (non-empty).
        children: Vec<MathExpression>,
    },
    /// Literal math text collected from an OMML math run.
    Text {
        /// Non-empty UTF-8 text.
        value: String,
    },
    /// A numerator stacked above a denominator with a separating rule.
    Fraction {
        /// Numerator expression.
        numerator: Box<MathExpression>,
        /// Denominator expression.
        denominator: Box<MathExpression>,
    },
    /// A base with an optional subscript and/or superscript.
    Script {
        /// Base expression.
        base: Box<MathExpression>,
        /// Subscript expression.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subscript: Option<Box<MathExpression>>,
        /// Superscript expression.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        superscript: Option<Box<MathExpression>>,
    },
    /// A radical with an optional degree.
    Radical {
        /// Optional degree expression.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        degree: Option<Box<MathExpression>>,
        /// Radicand expression.
        radicand: Box<MathExpression>,
    },
    /// Nested content surrounded by authored delimiter characters.
    Delimiter {
        /// Opening delimiter; empty means no opening glyph.
        open: String,
        /// Closing delimiter; empty means no closing glyph.
        close: String,
        /// Delimited expression.
        content: Box<MathExpression>,
    },
    /// A named function applied to an argument (e.g. `sin x`), from `m:func`.
    Function {
        /// The function-name expression (the `m:fName`).
        name: Box<MathExpression>,
        /// The argument expression (the `m:e`).
        argument: Box<MathExpression>,
    },
    /// A base decorated with a combining accent character, from `m:acc`.
    Accent {
        /// The accent character (`m:accPr/m:chr@m:val`); empty means the OOXML
        /// default combining circumflex.
        accent: String,
        /// The accented base expression.
        base: Box<MathExpression>,
    },
    /// A base with a limit set below or above it, from `m:limLow`/`m:limUpp`.
    Limit {
        /// The base expression (the `m:e`).
        base: Box<MathExpression>,
        /// The limit expression (the `m:lim`).
        limit: Box<MathExpression>,
        /// Whether the limit sits below or above the base.
        position: LimitPosition,
    },
    /// An n-ary operator (integral, summation, product, …), from `m:nary`.
    Nary {
        /// The operator character (`m:naryPr/m:chr@m:val`); empty means the
        /// OOXML default integral sign.
        operator: String,
        /// The optional lower bound / subscript (the `m:sub`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lower: Option<Box<MathExpression>>,
        /// The optional upper bound / superscript (the `m:sup`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upper: Option<Box<MathExpression>>,
        /// The operand expression (the `m:e`).
        base: Box<MathExpression>,
    },
    /// A matrix of expression cells in row-major order, from `m:m`.
    Matrix {
        /// The rows of cells (non-empty).
        rows: Vec<MathMatrixRow>,
    },
    /// A vertically stacked equation array, from `m:eqArr`.
    EqArray {
        /// The stacked rows (non-empty).
        rows: Vec<MathExpression>,
    },
    /// A base with an overline or underline rule, from `m:bar`.
    Bar {
        /// Whether the rule sits above (overline) or below (underline) the base.
        position: BarPosition,
        /// The barred base expression.
        base: Box<MathExpression>,
    },
    /// A base grouped by a stretchy character (e.g. over-/under-brace), from
    /// `m:groupChr`.
    GroupChar {
        /// The grouping character (`m:groupChrPr/m:chr@m:val`); empty means the
        /// OOXML default top curly bracket.
        character: String,
        /// Whether the grouping character sits above or below the base.
        position: GroupPosition,
        /// The grouped base expression.
        base: Box<MathExpression>,
    },
}

/// Whether a [`MathExpression::Bar`] rule sits above or below its base.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BarPosition {
    /// The rule sits above the base (an overline; `m:pos` `top`).
    Top,
    /// The rule sits below the base (an underline; `m:pos` `bot`).
    Bottom,
}

/// Whether a [`MathExpression::GroupChar`] character sits above or below its
/// base.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupPosition {
    /// The character sits above the base (e.g. an over-brace; `m:pos` `top`).
    Top,
    /// The character sits below the base (e.g. an under-brace; `m:pos` `bot`).
    Bottom,
}

/// Whether a [`MathExpression::Limit`] places its limit below or above the base.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitPosition {
    /// The limit sits below the base (`m:limLow`).
    Lower,
    /// The limit sits above the base (`m:limUpp`).
    Upper,
}

/// One row of a [`MathExpression::Matrix`]: an ordered list of cell expressions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MathMatrixRow {
    /// The cell expressions in column order (non-empty).
    pub cells: Vec<MathExpression>,
}

/// An inline math object (an OMML `m:oMath` or `m:oMathPara` subtree).
///
/// The OMML subtree is retained verbatim in `omml` so it round-trips losslessly;
/// `expression` is an optional bounded projection of the supported common subset;
/// and `text` is a best-effort plain-text fallback for search/accessibility.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Math {
    /// Stable identity.
    pub id: NodeId,
    /// The retained OMML markup (non-empty, at most `MAX_MATH_BYTES` bytes).
    pub omml: String,
    /// Best-effort plain-text fallback (the concatenated `m:t` text); may be
    /// empty when the equation carries no literal text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Typed common-construct projection used for deterministic layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<MathExpression>,
}

/// Inline content supported by schema v1.
//
// `Run` is by far the largest and the most common variant (it carries the full
// `RunProperties`, which grows as run-property coverage expands). Boxing it (or
// its properties) would shrink the enum but add a heap allocation on the hot
// path — most inline nodes are runs — and it ripples across every
// construction/match site and the public API; tracked with the same follow-up
// as `BlockNode` so it can land as one focused change.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineNode {
    /// A text run.
    Run(Run),
    /// An explicit tab.
    Tab(Tab),
    /// An explicit break.
    Break(Break),
    /// An inline drawing referencing embedded media.
    Drawing(Drawing),
    /// An anchored (floating) drawing placed at an absolute page position.
    AnchoredDrawing(AnchoredDrawing),
    /// An inline embedded object (chart, SmartArt diagram, or OLE object)
    /// referencing preserved package part(s).
    EmbeddedObject(EmbeddedObject),
    /// An inline hyperlink wrapping inline content.
    Hyperlink(Hyperlink),
    /// An inline field: an instruction and its cached result.
    Field(Field),
    /// An inline text box holding block content (inline, or floating when it
    /// carries a [`TextBox::anchor`]).
    TextBox(TextBox),
    /// A DrawingML group (`wpg:wgp`): a floating, z-ordered container of
    /// pictures, text boxes, and shapes.
    Group(WordprocessingGroup),
    /// An inline reference to a footnote or endnote.
    NoteReference(NoteReference),
    /// The auto-number mark inside a note's own body (`w:footnoteRef` /
    /// `w:endnoteRef`), printing that note's own number.
    NoteNumberMark(NoteNumberMark),
    /// An inline reference to a comment.
    CommentReference(CommentReference),
    /// The start marker of a comment's anchored range.
    CommentRangeStart(CommentRangeStart),
    /// The end marker of a comment's anchored range.
    CommentRangeEnd(CommentRangeEnd),
    /// A tracked-change (insertion/deletion) range wrapping inline content.
    Revision(Revision),
    /// The start marker of a bookmark range.
    BookmarkStart(BookmarkStart),
    /// The end marker of a bookmark range.
    BookmarkEnd(BookmarkEnd),
    /// The start marker of a tracked-move (source or destination) range.
    MoveRangeStart(MoveRangeStart),
    /// The end marker of a tracked-move (source or destination) range.
    MoveRangeEnd(MoveRangeEnd),
    /// An inline-level content control wrapping inline content.
    Sdt(InlineSdt),
    /// An inline math object retaining its OMML subtree verbatim.
    Math(Math),
    /// An inline symbol glyph (a font plus a code point).
    Symbol(Symbol),
    /// An inline horizontal rule (`w:pict` / `v:rect@o:hr`): a full-content-width
    /// filled line occupying its paragraph's own line.
    HorizontalRule(HorizontalRule),
    /// A non-breaking hyphen glyph (`w:noBreakHyphen`).
    NoBreakHyphen(NoBreakHyphen),
    /// A soft (optional) hyphen glyph (`w:softHyphen`).
    SoftHyphen(SoftHyphen),
    /// An absolute-position tab (`w:ptab`).
    PositionalTab(PositionalTab),
}

impl InlineNode {
    /// Returns the stable identity of this inline node.
    #[must_use]
    pub fn id(&self) -> NodeId {
        match self {
            Self::Run(run) => run.id,
            Self::Tab(tab) => tab.id,
            Self::Break(node) => node.id,
            Self::Drawing(drawing) => drawing.id,
            Self::AnchoredDrawing(drawing) => drawing.id,
            Self::EmbeddedObject(object) => object.id,
            Self::Hyperlink(hyperlink) => hyperlink.id,
            Self::Field(field) => field.id,
            Self::TextBox(text_box) => text_box.id,
            Self::Group(group) => group.id,
            Self::NoteReference(note) => note.id,
            Self::NoteNumberMark(mark) => mark.id,
            Self::CommentReference(comment) => comment.id,
            Self::CommentRangeStart(node) => node.id,
            Self::CommentRangeEnd(node) => node.id,
            Self::Revision(revision) => revision.id,
            Self::BookmarkStart(node) => node.id,
            Self::BookmarkEnd(node) => node.id,
            Self::MoveRangeStart(node) => node.id,
            Self::MoveRangeEnd(node) => node.id,
            Self::Sdt(sdt) => sdt.id,
            Self::Math(math) => math.id,
            Self::Symbol(symbol) => symbol.id,
            Self::HorizontalRule(rule) => rule.id,
            Self::NoBreakHyphen(hyphen) => hyphen.id,
            Self::SoftHyphen(hyphen) => hyphen.id,
            Self::PositionalTab(tab) => tab.id,
        }
    }
}

/// A paragraph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Paragraph {
    /// Stable paragraph identity.
    pub id: NodeId,
    /// Paragraph properties (always present; empty is `{}`).
    pub properties: ParagraphProperties,
    /// Ordered inline content.
    pub inlines: Vec<InlineNode>,
}

/// A body-level node.
// `Table` is larger than `Paragraph` now that tables carry borders/margins.
// Boxing it (or its properties) is a worthwhile memory optimization for
// paragraph-heavy bodies, but it ripples across every construction/match site
// and the enum is part of the public API; tracked as a follow-up so it can land
// as one focused change rather than entangled with feature slices.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    /// A paragraph block.
    Paragraph(Paragraph),
    /// A table block.
    Table(Table),
    /// A block-level content control wrapping block content.
    Sdt(BlockSdt),
    /// An aggregated external content chunk (`w:altChunk`) referencing a preserved
    /// package part.
    AltChunk(AltChunk),
}
