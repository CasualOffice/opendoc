//! Body block and inline nodes.

use serde::{Deserialize, Serialize};

use super::SharedParagraphProperties;
use super::SharedRunProperties;
use super::{BookmarkId, BreakKind, CommentId, MediaId, NoteId, RunProperties, Table};
// Separate `use` line (kept out of the sorted block above) to avoid import-list
// merge collisions with other agents editing this shared model file.
use super::FieldRangeId;
use crate::NodeId;

/// OOXML `ST_PositiveCoordinate` upper bound, in English Metric Units (EMU).
pub const MAX_EMU: i64 = 27_273_042_316_900;

/// A text run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Run {
    /// Stable run identity.
    pub id: NodeId,
    /// Run properties (always present; empty is `{}`). Shared and
    /// copy-on-write: see [`SharedRunProperties`].
    pub properties: SharedRunProperties,
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
    pub properties: SharedRunProperties,
}

fn is_default_run_properties(properties: &SharedRunProperties) -> bool {
    **properties == RunProperties::default()
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

/// Fully opaque, in the units DrawingML's `a:alphaModFix@amt` uses (1000ths of
/// a percent). A picture at this opacity is indistinguishable from one with no
/// `a:alphaModFix` at all, so the importer models neither.
pub const OPACITY_FULL: u32 = 100_000;

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
    /// The picture's opacity (`a:blip/a:alphaModFix@amt`), in 1000ths of a
    /// percent, when the picture is drawn less than fully opaque.
    ///
    /// `None` is fully opaque, which is what an absent `a:alphaModFix` means —
    /// and so is `amt="100000"`, so a producer writing the no-op explicitly
    /// does not become a document that carries a redundant field. This is how
    /// Word writes a watermark: the same picture, at 20%.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<u32>,
    /// The hyperlink this drawing follows when it is clicked
    /// (`…/cNvPr/a:hlinkClick`), if it is linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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

/// Which side(s) of a wrapped float the text flows down
/// (`wp:wrapSquare`/`wp:wrapTight`/`wp:wrapThrough@wrapText`, `ST_WrapText`).
///
/// This is not an alignment: it selects which of the two side channels beside the
/// float remain available to the flow. A float sitting mid-measure with
/// [`Self::Left`] leaves the whole channel to its right empty, however wide that
/// channel is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WrapSide {
    /// Text flows down both channels beside the float (`bothSides`).
    BothSides,
    /// Text flows only down the channel to the float's left (`left`); the channel
    /// to its right stays empty.
    Left,
    /// Text flows only down the channel to the float's right (`right`).
    Right,
    /// Text flows only down whichever of the two channels is wider (`largest`).
    Largest,
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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawingAnchor {
    /// The horizontal placement (`wp:positionH`).
    pub horizontal: AnchorHorizontal,
    /// The vertical placement (`wp:positionV`).
    pub vertical: AnchorVertical,
    /// How text flows around the drawing (`wp:wrap*`).
    pub wrap: WrapMode,
    /// Which side(s) the text flows down past the float
    /// (`wp:wrapSquare`/`wp:wrapTight`/`wp:wrapThrough@wrapText`).
    ///
    /// `None` is "the producer omitted the attribute". Word's default for an
    /// absent `@wrapText` is `bothSides`, and that is what
    /// [`DrawingAnchor::wrap_side`] resolves `None` to, so a consumer never has to
    /// re-derive the default. The model keeps the distinction rather than
    /// normalizing on import because writing `wrapText="bothSides"` into a package
    /// whose source carried no attribute is a fidelity loss that looks like
    /// fidelity: the saved file stops being the file that was opened.
    ///
    /// Only the three side-wrap elements carry `@wrapText` in the schema, so this
    /// is meaningless for [`WrapMode::None`] and [`WrapMode::TopAndBottom`] and is
    /// not written for them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_text: Option<WrapSide>,
    /// Text-exclusion distances around the object.
    #[serde(default, skip_serializing_if = "WrapDistances::is_zero")]
    pub wrap_distances: WrapDistances,
    /// The tight/through wrap contour (`wp:wrapTight`/`wp:wrapThrough` >
    /// `wp:wrapPolygon`): its ordered vertices (`wp:start` + `wp:lineTo`) in EMU,
    /// relative to the object's extent. `None` unless the producer authored a
    /// polygon; only meaningful for [`WrapMode::Tight`]/[`WrapMode::Through`].
    /// Carried through round-trips; layout still wraps to the bounding box (using
    /// the contour for exclusion geometry is a follow-up).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_polygon: Option<Vec<PointEmu>>,
    /// Whether the drawing paints behind the document text (`@behindDoc`),
    /// i.e. its z-order relative to the flow. Only meaningful for
    /// [`WrapMode::None`].
    pub behind_doc: bool,
}

impl DrawingAnchor {
    /// The side(s) text may flow down past this float, with Word's absent-attribute
    /// default applied: an omitted `@wrapText` resolves to
    /// [`WrapSide::BothSides`].
    ///
    /// Read this rather than [`DrawingAnchor::wrap_text`] when deciding geometry —
    /// the raw field exists to round-trip the absence, not to be interpreted. The
    /// answer is only meaningful for [`WrapMode::Square`], [`WrapMode::Tight`] and
    /// [`WrapMode::Through`]; for the other two wrap modes there are no side
    /// channels for it to describe.
    ///
    /// O(1).
    #[must_use]
    pub fn wrap_side(&self) -> WrapSide {
        self.wrap_text.unwrap_or(WrapSide::BothSides)
    }
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
    /// The picture's opacity (`a:blip/a:alphaModFix@amt`), in 1000ths of a
    /// percent, when the picture is drawn less than fully opaque.
    ///
    /// `None` is fully opaque, which is what an absent `a:alphaModFix` means —
    /// and so is `amt="100000"`, so a producer writing the no-op explicitly
    /// does not become a document that carries a redundant field. This is how
    /// Word writes a watermark: the same picture, at 20%.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<u32>,
    /// The hyperlink this drawing follows when it is clicked
    /// (`…/cNvPr/a:hlinkClick`), if it is linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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
/// A gradient retains its ordered stops and direction, and layout translates it
/// into a real display gradient (`casual-doc-layout`'s `fill_to_display`), which
/// the renderer and the PDF writer both paint. [`Fill::flat_color`] remains for
/// the surfaces that genuinely need one colour — a connector's default hairline,
/// a swatch in the wasm facade — and is NOT the shape paint path; this comment
/// used to claim gradients were flattened everywhere, which stopped being true
/// without the comment noticing. Colors are resolved to concrete channels at
/// import, exactly like [`Rgba`].
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

/// How many `a:custDash/a:ds` stops one outline may author before the pattern is
/// refused.
///
/// `CT_LineProperties` puts no ceiling on `a:custDash`, so an authored pattern is
/// unbounded input reaching a `Vec`. Sixteen dash/space pairs already describes
/// every pattern Word's own dash gallery can produce (its longest preset,
/// `sysDashDotDot`, is three), and a file that authors more is refused and
/// reported rather than silently truncated — truncating would paint a *different*
/// pattern while claiming the construct survived.
pub const MAX_CUSTOM_DASH_STOPS: usize = 16;

/// The longest `a:pattFill@prst` token retained. `ST_PresetPatternVal`'s longest
/// member is `wdUpDiag` at eight characters; the bound is generous so an
/// unrecognised token still round-trips, and exists only so the string is not
/// unbounded input.
pub const MAX_PATTERN_PRESET_LEN: usize = 64;

/// A `CT_RelativeRect`: four edge insets as `ST_Percentage` (1/1000 of a
/// percent), used by `a:fillRect` (how a stretched picture fill maps onto the
/// shape) and `a:fillToRect` (where a path gradient's innermost stop sits).
///
/// Distinct from [`CropRect`], which is the same markup shape with a different
/// domain: a crop edge is a *positive* fraction of the source that is hidden and
/// is clamped into `0..=100000`, while a fill-rect edge is legitimately
/// **negative** — `a:fillRect l="-20000"` pushes the picture outward past the
/// shape's left edge. Clamping these to zero would quietly centre every outset
/// fill, which is why this is a second type rather than a reuse.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelativeRect {
    /// `@l` in 1/1000 of a percent; negative insets outward.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub left: i32,
    /// `@t` in 1/1000 of a percent.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub top: i32,
    /// `@r` in 1/1000 of a percent.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub right: i32,
    /// `@b` in 1/1000 of a percent.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub bottom: i32,
}

impl RelativeRect {
    /// Whether every edge is zero — the identity rect, which is what an absent or
    /// empty `a:fillRect` means, so it is not modeled.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        self.left == 0 && self.top == 0 && self.right == 0 && self.bottom == 0
    }
}

/// `ST_TileFlipMode`: whether alternate tiles of a tiled fill, or the two halves
/// of a gradient, are mirrored.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TileFlip {
    /// `none` — every tile drawn the same way up.
    #[default]
    None,
    /// `x` — alternate columns mirrored horizontally.
    X,
    /// `y` — alternate rows mirrored vertically.
    Y,
    /// `xy` — mirrored on both axes.
    Xy,
}

/// `ST_RectAlignment`: which corner or edge of the shape a tiled fill's first
/// tile is anchored to.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RectAlignment {
    /// `tl`.
    #[default]
    TopLeft,
    /// `t`.
    Top,
    /// `tr`.
    TopRight,
    /// `l`.
    Left,
    /// `ctr`.
    Center,
    /// `r`.
    Right,
    /// `bl`.
    BottomLeft,
    /// `b`.
    Bottom,
    /// `br`.
    BottomRight,
}

/// How a picture fill covers the shape: `a:stretch` (one copy mapped onto the
/// shape) or `a:tile` (repeated at its natural size).
///
/// This is the half of `a:blipFill` that changes what a reader sees, which is why
/// it is a required field rather than an option: a file that writes neither child
/// is stretch by DrawingML default, and recording that explicitly keeps export
/// from having to guess.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum PictureFillMode {
    /// `a:stretch`, with its `a:fillRect` when the file maps the picture onto
    /// something other than the whole shape.
    Stretch {
        /// `a:stretch/a:fillRect`; `None` for the absent or identity rect.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill_rect: Option<RelativeRect>,
    },
    /// `a:tile`: the picture repeated from `(tx, ty)` at `(sx, sy)` scale.
    Tile {
        /// `@tx`, the first tile's horizontal offset in EMU (signed).
        #[serde(default, skip_serializing_if = "is_zero_i64")]
        offset_x_emu: i64,
        /// `@ty`, the first tile's vertical offset in EMU (signed).
        #[serde(default, skip_serializing_if = "is_zero_i64")]
        offset_y_emu: i64,
        /// `@sx`, the horizontal scale as `ST_Percentage` (1/1000 of a percent;
        /// `100000` is natural size). `None` when the file states none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale_x: Option<i32>,
        /// `@sy`, the vertical scale as `ST_Percentage`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale_y: Option<i32>,
        /// `@flip`.
        #[serde(default, skip_serializing_if = "is_no_tile_flip")]
        flip: TileFlip,
        /// `@algn`.
        #[serde(default, skip_serializing_if = "is_top_left_alignment")]
        alignment: RectAlignment,
    },
}

/// A picture fill on a *shape* (`a:blipFill` inside `wps:spPr`), as opposed to
/// the picture frame `pic:blipFill` that [`Drawing`] already models.
///
/// Word writes these whenever a shape is filled from a photo or a texture, and
/// before this type the whole fill was dropped: the shape imported with no fill
/// and exported with none, so a save destroyed it. The media reference resolves
/// in `Definitions::media` exactly as [`GroupPicture::media`] does, so one media
/// table serves both.
///
/// **Nothing paints this.** The display list has no picture-fill primitive and
/// building one lives in `casual-doc-layout`; a shape wearing a picture fill is
/// drawn unfilled and the import report says so. Modeling it buys the round trip
/// and a finding that names the construct, which is the same trade
/// `FillStyle::Pattern` records one level up in the theme.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PictureFill {
    /// The referenced media entry (`a:blip@r:embed`; resolves in
    /// `Definitions::media`).
    pub media: MediaId,
    /// Stretch or tile.
    pub mode: PictureFillMode,
    /// `a:srcRect`, the source crop, when it hides anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<CropRect>,
    /// `a:blip/a:alphaModFix@amt` in 1/1000 of a percent, when the fill is drawn
    /// less than fully opaque. `None` is opaque, and so is an explicit `100000` —
    /// the rule [`GroupPicture::opacity`] already follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<u32>,
    /// `a:blipFill@rotWithShape`, when the file states it. `None` leaves the
    /// DrawingML default (the fill rotates with the shape).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_with_shape: Option<bool>,
}

/// A two-colour preset pattern fill on a shape (`a:pattFill`).
///
/// The same construct `PatternStyle` models one level up, in the theme's
/// fill-style matrix, and **deliberately the same policy**: the preset token is
/// retained as the file spells it, the two colours are resolved to concrete
/// channels like every other shape colour, and *nothing paints it*. There is no
/// pattern primitive in the display list, so the shape draws unfilled and the
/// import report names the pattern. Substituting the foreground colour as a solid
/// would look deliberate, which is worse than an obviously unfilled shape
/// (`FillStyle::Pattern`, and `themed_fill` in `casual-doc-layout`).
///
/// Inventing a second policy for the shape-level element was the alternative and
/// it is exactly the thing to avoid: one construct, two levels, one answer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatternFill {
    /// `@prst` (`ST_PresetPatternVal`, e.g. `pct25`, `ltHorz`), as the file
    /// spells it, bounded by [`MAX_PATTERN_PRESET_LEN`]. Retained as a token
    /// rather than enumerated because the fifty-four presets differ only in the
    /// hatch nothing here draws.
    pub preset: String,
    /// `a:fgClr`, resolved.
    pub foreground: Rgba,
    /// `a:bgClr`, resolved.
    pub background: Rgba,
}

/// The geometry of a path (radial) gradient: `a:path@path`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientPath {
    /// `shape` — the stops follow the shape's own outline.
    Shape,
    /// `circle` — concentric circles.
    Circle,
    /// `rect` — concentric rectangles.
    Rect,
}

/// The parts of an `a:gradFill` that [`Fill::Gradient`] cannot hold.
///
/// [`GradientKind::Radial`] collapses `shape`, `circle` and `rect` into one
/// value, and semantic export wrote `path="circle"` back for all three — so a
/// shape-following gradient became a concentric one on save, with nothing saying
/// so. That is the "radial gradients collapsed to concentric" loss in `docs/156`
/// §6 row 0.3, and it is a **silent change of appearance**, which is the kind
/// this engine's whole retention argument exists to prevent.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GradientDetail {
    /// `a:path@path` as authored, when the gradient is a path gradient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<GradientPath>,
    /// `a:path/a:fillToRect` — where the first stop sits inside the shape — when
    /// it is not the identity rect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_to_rect: Option<RelativeRect>,
    /// `a:lin@scaled`: whether the sweep angle is scaled into the shape's
    /// bounding box rather than measured against it. `None` when unstated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaled: Option<bool>,
    /// `a:gradFill@flip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip: Option<TileFlip>,
    /// `a:gradFill@rotWithShape`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_with_shape: Option<bool>,
}

impl GradientDetail {
    /// Whether this carries nothing — in which case the file stated no geometry
    /// beyond what [`GradientKind`] already holds and there is nothing to retain.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.path.is_none()
            && self.fill_to_rect.is_none()
            && self.scaled.is_none()
            && self.flip.is_none()
            && self.rotate_with_shape.is_none()
    }
}

/// `a:ln@cap` (`ST_LineCap`): how the two ends of a stroke are finished.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    /// `flat` — the stroke stops at the endpoint.
    Flat,
    /// `rnd` — a semicircle past the endpoint.
    Round,
    /// `sq` — a half-square past the endpoint.
    Square,
}

/// `a:ln@cmpd` (`ST_CompoundLine`): how many parallel lines the outline is drawn
/// as, and in what weights.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompoundLine {
    /// `sng` — one line (the DrawingML default).
    Single,
    /// `dbl` — two lines of equal weight.
    Double,
    /// `thickThin` — a thick line then a thin one.
    ThickThin,
    /// `thinThick` — a thin line then a thick one.
    ThinThick,
    /// `tri` — thin, thick, thin.
    Triple,
}

/// `a:ln@algn` (`ST_PenAlignment`): whether the stroke straddles the outline or
/// sits wholly inside it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PenAlignment {
    /// `ctr` — centred on the path (the DrawingML default).
    Center,
    /// `in` — entirely inside the path.
    Inset,
}

/// The join an outline uses at a corner: `a:round`, `a:bevel` or `a:miter`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "join", rename_all = "camelCase")]
pub enum LineJoin {
    /// `a:round`.
    Round,
    /// `a:bevel`.
    Bevel,
    /// `a:miter`, with its `@lim` (the miter length ceiling as
    /// `ST_PositivePercentage`, 1/1000 of a percent of the line width) when the
    /// file states one.
    Miter {
        /// `a:miter@lim`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<i32>,
    },
}

/// One `a:custDash/a:ds`: a dash and the gap after it, each as
/// `ST_PositivePercentage` of the line width (1/1000 of a percent).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashStop {
    /// `@d`, the dash length.
    pub dash: i32,
    /// `@sp`, the gap after it.
    pub space: i32,
}

/// The parts of an `a:ln` that [`ShapeStroke`] cannot hold: the cap, the corner
/// join, the compound (multi-line) form, the pen alignment, and an **authored**
/// dash pattern as opposed to the preset [`DashStyle`] already modeled.
///
/// A side-table payload rather than fields on [`ShapeStroke`], and the reason is
/// mechanical: `ShapeStroke` is `Copy` and has literal construction sites in
/// eight crates including `casual-doc-wasm`, so a dash `Vec` would take `Copy`
/// away from every one of them and a new field would break each literal — a
/// change with nothing for a merge to conflict on (`SKILL` §5a shape 1).
///
/// **Nothing paints any of this**; the stroke still draws as a plain centred,
/// single, round-capped line of its modeled width and preset dash. Each part is
/// reported at import, and export re-emits it, so the round trip is honest even
/// though the render is not.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrokeDetail {
    /// `a:ln@cap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<LineCap>,
    /// `a:ln@cmpd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compound: Option<CompoundLine>,
    /// `a:ln@algn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<PenAlignment>,
    /// `a:round`/`a:bevel`/`a:miter`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<LineJoin>,
    /// `a:custDash/a:ds` in order, at most [`MAX_CUSTOM_DASH_STOPS`]. Empty when
    /// the outline authors no custom pattern; a pattern longer than the ceiling
    /// is refused whole and reported, never truncated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_dash: Vec<DashStop>,
}

impl StrokeDetail {
    /// Whether this carries nothing, in which case the outline is exactly what
    /// [`ShapeStroke`] already says it is and there is nothing to retain.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cap.is_none()
            && self.compound.is_none()
            && self.align.is_none()
            && self.join.is_none()
            && self.custom_dash.is_empty()
    }
}

/// The fill-and-line detail of one shape that [`Fill`] and [`ShapeStroke`] cannot
/// hold, kept in `Definitions::shape_fill_detail` keyed by the shape's node id.
///
/// # Why a side table
///
/// Every alternative is a breaking change to code outside this model file. A new
/// [`Fill`] variant breaks the exhaustive `match` in `casual-doc-layout`'s
/// display-list compose step and in the ODF writer; a new [`ShapeStroke`] field
/// breaks ten literals in `casual-doc-wasm` and takes `Copy` away besides; a new
/// [`GroupShape`] field breaks twenty-three literals across six crates. All three
/// are the `E0063` merge shape `SKILL` §5a describes, and
/// `Definitions::shape_styles` and `Definitions::charts` already took this exact
/// route for the same reason.
///
/// # What it does NOT buy
///
/// Reachability. Nothing in layout, render, PDF or the editor reads this table —
/// a shape with a picture or pattern fill still paints unfilled, a custom dash
/// still draws solid, a `cmpd="dbl"` outline still draws single, and a
/// `path="shape"` gradient still paints concentric. What the table buys is that
/// **a save no longer destroys any of it** and that the import report names each
/// one. `docs/156` §6 row 0.3 is **not** closed by this: painting is a separate,
/// unlanded piece of work in `casual-doc-layout`, and calling the row done here
/// would be the "modeled but not consumed" claim `SKILL` §9.4 calls the most
/// expensive recurring mistake in this repository.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeFillDetail {
    /// `a:blipFill` on the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<PictureFill>,
    /// `a:pattFill` on the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<PatternFill>,
    /// The `a:gradFill` geometry [`Fill::Gradient`] discards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<GradientDetail>,
    /// The `a:ln` geometry [`ShapeStroke`] discards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<StrokeDetail>,
    /// `a:effectLst/a:outerShdw`, the only effect this build can paint.
    ///
    /// In this table rather than a second one because the table is already
    /// "appearance detail the hot types have nowhere to put" — it carries
    /// `StrokeDetail`, not only fills — and a second side table would mean a second
    /// lookup per shape on the paint path for no gain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_shadow: Option<OuterShadow>,
}

/// An outer drop shadow (`a:effectLst/a:outerShdw`).
///
/// # What is modeled and what is not
///
/// The four attributes that decide where the shadow lands and what it looks like:
/// blur, distance, direction and colour. `a:outerShdw` also carries `@sx`/`@sy`
/// (scale), `@kx`/`@ky` (skew), `@algn` and `@rotWithShape`, which together let a
/// shadow be a sheared, scaled copy — a perspective shadow. Those are NOT modeled:
/// the layer shadow this paints through is an offset blur, and a sheared shadow
/// approximated by an offset one would be in the wrong place. The importer reports
/// them.
///
/// `a:innerShdw`, `a:glow`, `a:softEdge`, `a:reflection` and the 3-D effects are not
/// modeled either. `docs/156` §6 row 0.4 judged outer shadow, glow and soft edge to
/// be most of the value and 3-D none of it; this is the first of those three.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OuterShadow {
    /// `@blurRad`, the blur radius in EMU. Zero is a hard-edged offset copy, which
    /// is a legal shadow rather than an absent one.
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub blur_radius_emu: i64,
    /// `@dist`, how far the shadow is displaced, in EMU.
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub distance_emu: i64,
    /// `@dir`, the direction of that displacement in 60000ths of a degree,
    /// clockwise from the positive x-axis. Polar with `distance_emu`; resolved to a
    /// cartesian offset at layout, where the DPI is known.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub direction: i32,
    /// The shadow's colour with its alpha, resolved from the effect's own colour
    /// child. A shadow is almost always a partly transparent black.
    pub color: Rgba,
}

impl ShapeFillDetail {
    /// Whether this entry carries nothing. An empty entry is never inserted: a
    /// table with a row for every shape would make an unstyled shape
    /// indistinguishable from a styled one, which is the mistake `commit_shape`
    /// already avoids for `shape_styles`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.picture.is_none()
            && self.pattern.is_none()
            && self.gradient.is_none()
            && self.stroke.is_none()
            && self.outer_shadow.is_none()
    }
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

fn is_no_tile_flip(value: &TileFlip) -> bool {
    matches!(value, TileFlip::None)
}

fn is_top_left_alignment(value: &RectAlignment) -> bool {
    matches!(value, RectAlignment::TopLeft)
}

/// The preset geometry of a simple DrawingML shape (`a:prstGeom@prst`). Only the
/// bounded primitive subset implemented by layout/render is distinguished;
/// every other preset is [`ShapeGeometry::Other`] (drawn as its bounding
/// rectangle while its original token is retained by [`GroupShape::preset`]).
///
/// A variant is added here **only** when layout can draw the preset's real
/// outline. A variant that painted its bounding rectangle would be
/// [`ShapeGeometry::Other`] with a longer name and no more fidelity, so the
/// typed set is exactly the set of presets a reader sees the right shape for.
///
/// Every variant names the `ST_ShapeType` token it maps to (ECMA-376 Part 1
/// §20.1.10.56) and, where the preset's outline is governed by an `a:avLst`
/// adjustment guide, says which guide is honored and what the preset default is
/// when the document authors none.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeGeometry {
    /// A rectangle (`rect`).
    #[default]
    Rectangle,
    /// A rounded rectangle (`roundRect`). Honors `adj` (corner radius as a
    /// 100000-based fraction of the shorter side); preset default `16667`.
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
    /// A regular pentagon, apex up (`pentagon`). The preset declares no
    /// adjustment guide: its outline is fixed, inscribed so it fills the
    /// bounding box exactly.
    Pentagon,
    /// A hexagon with its two points on the left and right edges (`hexagon`).
    /// Honors `adj` (the horizontal inset of the four corner vertices, as a
    /// 100000-based fraction of the shorter side, clamped to half the width);
    /// preset default `25000`. The `vf` guide, which stretches the preset
    /// vertically, is not honored — the hexagon is fitted to the shape's own
    /// box instead.
    Hexagon,
    /// An octagon (`octagon`). Honors `adj` (the corner cut, as a 100000-based
    /// fraction of the shorter side, clamped to `50000`); preset default
    /// `29289`, which is the regular octagon when the box is square.
    Octagon,
    /// A five-pointed star (`star5`). Honors `adj` (the inner radius, as a
    /// fraction `adj / 50000` of the outer radius); preset default `19098`,
    /// which is the regular pentagram. The `hf`/`vf` guides are not honored:
    /// the star is fitted to the shape's own box, which is what they encode.
    Star5,
    /// A four-pointed star (`star4`). Honors `adj` (the inner radius, as a
    /// fraction `adj / 50000` of the outer radius); preset default `12500`.
    Star4,
    /// A block arrow pointing right (`rightArrow`). Honors `adj1` (shaft
    /// thickness as a 200000-based fraction of the height) and `adj2` (head
    /// length as a 100000-based fraction of the shorter side); preset defaults
    /// `50000` and `50000`.
    RightArrow,
    /// A block arrow pointing left (`leftArrow`); guides as
    /// [`ShapeGeometry::RightArrow`].
    LeftArrow,
    /// A block arrow pointing up (`upArrow`). Honors `adj1` (shaft thickness as
    /// a 200000-based fraction of the width) and `adj2` (head length as a
    /// 100000-based fraction of the shorter side); preset defaults `50000`.
    UpArrow,
    /// A block arrow pointing down (`downArrow`); guides as
    /// [`ShapeGeometry::UpArrow`].
    DownArrow,
    /// A double-headed block arrow (`leftRightArrow`). Honors `adj1` (shaft
    /// thickness as a 200000-based fraction of the height) and `adj2` (each
    /// head's length as a 100000-based fraction of the shorter side); preset
    /// defaults `50000`.
    LeftRightArrow,
    /// A parallelogram leaning right (`parallelogram`). Honors `adj` (the
    /// horizontal offset of the top edge, as a 100000-based fraction of the
    /// shorter side); preset default `25000`.
    Parallelogram,
    /// An isosceles trapezoid with the wide edge at the bottom (`trapezoid`).
    /// Honors `adj` (each top inset, as a 100000-based fraction of the shorter
    /// side); preset default `25000`.
    Trapezoid,
    /// A chevron — an arrow head with a notched back (`chevron`). Honors `adj`
    /// (the point depth, as a 100000-based fraction of the shorter side);
    /// preset default `50000`.
    Chevron,
    /// The "pentagon" block arrow of Word's shape gallery — a rectangle with a
    /// pointed right end (`homePlate`, which is the OOXML token; the regular
    /// pentagon is [`ShapeGeometry::Pentagon`]). Honors `adj` (the point depth,
    /// as a 100000-based fraction of the shorter side); preset default `50000`.
    HomePlate,
    /// A cross / plus sign (`plus`). Honors `adj` (the arm thickness inset, as
    /// a 100000-based fraction of the shorter side, clamped to `50000`); preset
    /// default `25000`.
    Plus,
    /// Any other preset, drawn as its bounding rectangle.
    Other,
}

impl ShapeGeometry {
    /// Every typed preset, in declaration order. [`ShapeGeometry::Other`] is
    /// excluded: it is the catch-all, not a preset, and carries its authored
    /// token in `GroupShape::preset` instead.
    ///
    /// Exhaustive by construction — a variant added without being listed here
    /// fails [`ShapeGeometry::preset_token`]'s match, which has no wildcard.
    pub const TYPED: [Self; 22] = [
        Self::Rectangle,
        Self::RoundRectangle,
        Self::Ellipse,
        Self::Triangle,
        Self::RightTriangle,
        Self::Diamond,
        Self::Line,
        Self::Pentagon,
        Self::Hexagon,
        Self::Octagon,
        Self::Star5,
        Self::Star4,
        Self::RightArrow,
        Self::LeftArrow,
        Self::UpArrow,
        Self::DownArrow,
        Self::LeftRightArrow,
        Self::Parallelogram,
        Self::Trapezoid,
        Self::Chevron,
        Self::HomePlate,
        Self::Plus,
    ];

    /// The canonical `a:prstGeom@prst` token for this geometry, or `None` for
    /// [`ShapeGeometry::Other`], which has none of its own.
    ///
    /// This is the ONE token table. Import, export and the host-facing
    /// insert-shape command all resolve through it, so a preset cannot be
    /// readable and unwritable, or modeled and uninsertable — which is what
    /// three separate matches drift into.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn preset_token(self) -> Option<&'static str> {
        Some(match self {
            Self::Rectangle => "rect",
            Self::RoundRectangle => "roundRect",
            Self::Ellipse => "ellipse",
            Self::Triangle => "triangle",
            Self::RightTriangle => "rtTriangle",
            Self::Diamond => "diamond",
            Self::Line => "line",
            Self::Pentagon => "pentagon",
            Self::Hexagon => "hexagon",
            Self::Octagon => "octagon",
            Self::Star5 => "star5",
            Self::Star4 => "star4",
            Self::RightArrow => "rightArrow",
            Self::LeftArrow => "leftArrow",
            Self::UpArrow => "upArrow",
            Self::DownArrow => "downArrow",
            Self::LeftRightArrow => "leftRightArrow",
            Self::Parallelogram => "parallelogram",
            Self::Trapezoid => "trapezoid",
            Self::Chevron => "chevron",
            Self::HomePlate => "homePlate",
            Self::Plus => "plus",
            Self::Other => return None,
        })
    }

    /// The geometry an `a:prstGeom@prst` token names, or `None` when this build
    /// has no typed primitive for it (the caller then keeps the token verbatim
    /// in `GroupShape::preset` and paints the bounding rectangle).
    ///
    /// Accepts the aliases a real producer writes as well as the canonical
    /// token — `straightConnector1` is the `wps:cxnSp` spelling of a line.
    ///
    /// Complexity: O(typed presets), a fixed 22-element scan over `&'static
    /// str` — no allocation and no document access.
    #[must_use]
    pub fn from_preset_token(token: &str) -> Option<Self> {
        if token == "straightConnector1" {
            return Some(Self::Line);
        }
        Self::TYPED
            .into_iter()
            .find(|geometry| geometry.preset_token() == Some(token))
    }
}

/// Maximum UTF-8 length of a retained DrawingML preset-geometry token.
pub const MAX_SHAPE_PRESET_BYTES: usize = 64;

/// Maximum adjustment guides retained for one preset shape.
pub const MAX_SHAPE_ADJUSTMENTS: usize = 32;

/// Maximum UTF-8 length of an adjustment-guide name.
pub const MAX_SHAPE_GUIDE_NAME_BYTES: usize = 64;

/// Maximum UTF-8 length of an adjustment-guide formula.
pub const MAX_SHAPE_FORMULA_BYTES: usize = 256;

/// Maximum path commands retained for one custom shape geometry
/// (`a:custGeom/a:pathLst/a:path`). A bound, not a fidelity target: a hand-drawn
/// freeform is tens of points, and the cap stops a hostile package from turning
/// one shape into an unbounded vertex list (docs/119 §6).
pub const MAX_SHAPE_PATH_COMMANDS: usize = 1024;

/// One command of a custom shape geometry path (`a:custGeom/a:pathLst/a:path`).
///
/// Modeled: `a:moveTo`, `a:lnTo`, `a:cubicBezTo`, `a:quadBezTo` and `a:close`.
/// Still deliberately absent, and still `109` FID-G-02: `a:arcTo`, which needs
/// `wR`/`hR`/`stAng`/`swAng` and an angle-to-Bézier conversion, and any coordinate
/// that is a guide NAME rather than an integer, which needs the `a:gdLst` formula
/// language. A geometry using either is not imported as a path at all, so this enum
/// never half-describes one (docs/119 §6).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShapePathCommand {
    /// Start a subpath at a point (`a:moveTo/a:pt`).
    MoveTo {
        /// The point, in the path's own coordinate space.
        point: PointEmu,
    },
    /// Draw a straight segment to a point (`a:lnTo/a:pt`).
    LineTo {
        /// The point, in the path's own coordinate space.
        point: PointEmu,
    },
    /// Draw a cubic Bézier (`a:cubicBezTo`): two control points, then the
    /// endpoint, in the authored order.
    CubicBezTo {
        /// The control point leaving the previous endpoint.
        control1: PointEmu,
        /// The control point entering `point`.
        control2: PointEmu,
        /// The curve's endpoint.
        point: PointEmu,
    },
    /// Draw a quadratic Bézier (`a:quadBezTo`): one control point, then the
    /// endpoint.
    ///
    /// Kept distinct from [`ShapePathCommand::CubicBezTo`] rather than promoted on
    /// import, so the authored command round-trips as itself. A backend with no
    /// quadratic operator promotes it instead; PDF does.
    QuadBezTo {
        /// The single control point.
        control: PointEmu,
        /// The curve's endpoint.
        point: PointEmu,
    },
    /// Close the subpath back to its starting point (`a:close`).
    Close,
}

impl ShapePathCommand {
    /// Whether this command draws, as opposed to only moving the pen or closing.
    ///
    /// Import uses this to decide whether a path draws anything at all. It is a
    /// method rather than a `matches!` at the call site because that call site
    /// tested for `LineTo` specifically, which silently rejected every curve-only
    /// geometry the moment curves existed.
    #[must_use]
    pub fn is_segment(&self) -> bool {
        matches!(
            self,
            Self::LineTo { .. } | Self::CubicBezTo { .. } | Self::QuadBezTo { .. }
        )
    }

    /// Every point the command names, control points included.
    ///
    /// Validation and layout's coordinate resolution both go through this rather
    /// than re-enumerating the variants, because a missed control point would
    /// validate a path and then paint it wrong — the two places that must agree on
    /// what "every coordinate" means.
    pub fn points(&self) -> impl Iterator<Item = PointEmu> + '_ {
        let (a, b, c) = match *self {
            Self::MoveTo { point } | Self::LineTo { point } => (Some(point), None, None),
            Self::CubicBezTo {
                control1,
                control2,
                point,
            } => (Some(control1), Some(control2), Some(point)),
            Self::QuadBezTo { control, point } => (Some(control), Some(point), None),
            Self::Close => (None, None, None),
        };
        a.into_iter().chain(b).chain(c)
    }
}

/// A custom shape geometry path (`a:custGeom/a:pathLst/a:path`): an ordered
/// command list in its own coordinate space.
///
/// [`width_emu`](Self::width_emu) / [`height_emu`](Self::height_emu) are
/// `a:path@w` / `@h`. Per ECMA-376 Part 1 §20.1.9.15 they default to `0`, and
/// the default is meaningful: a **positive** value is the extent of the path's
/// own coordinate space, so a coordinate maps to the shape box by
/// `x / width_emu`; **zero** means the coordinates are absolute EMU offsets from
/// the shape's top-left and do NOT scale with the box. The two axes are
/// independent — the loan-agreement rules in docs/119 set `@w` and omit `@h`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapePath {
    /// `a:path@w`: the path coordinate space's width, or `0` for absolute EMU.
    #[serde(default)]
    pub width_emu: i64,
    /// `a:path@h`: the path coordinate space's height, or `0` for absolute EMU.
    #[serde(default)]
    pub height_emu: i64,
    /// The commands, in path order. Always starts with a
    /// [`ShapePathCommand::MoveTo`].
    pub commands: Vec<ShapePathCommand>,
}

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
    /// The picture's opacity (`a:blip/a:alphaModFix@amt`), in 1000ths of a
    /// percent, when the picture is drawn less than fully opaque.
    ///
    /// `None` is fully opaque, which is what an absent `a:alphaModFix` means —
    /// and so is `amt="100000"`, so a producer writing the no-op explicitly
    /// does not become a document that carries a redundant field. This is how
    /// Word writes a watermark: the same picture, at 20%.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<u32>,
    /// The hyperlink this drawing follows when it is clicked
    /// (`…/cNvPr/a:hlinkClick`), if it is linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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

/// The flow direction of text in a DrawingML text body (`a:bodyPr@vert`,
/// `ST_TextVerticalType`).
///
/// # Why this lives in the document model
///
/// It is a **document** construct first: `wps:bodyPr@vert` is what a rotated Word
/// text box carries, and `docs/105` FID-L-08 tracks it as a DOCX defect. A
/// presentation needs the identical enum — two of `ST_SlideLayoutType`'s own kinds
/// (`vertTx`, `vertTitleAndTx`) exist to use it — so declaring it in the
/// presentation model, as a first draft of this did, put the shared vocabulary in
/// the layer that depends rather than the layer that is depended on. One
/// declaration, re-exported upward.
///
/// Modeled in full even where this build draws only the horizontal case: a vertical
/// layout silently rendered horizontally is a different document, and the renderer
/// must be able to branch on [`TextVertical::is_rotated`] and report rather than
/// draw it wrongly.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextVertical {
    /// `horz` — ordinary horizontal rows. The default.
    #[default]
    Horizontal,
    /// `vert` — the text block is rotated 90 degrees clockwise.
    Vertical,
    /// `vert270` — rotated 270 degrees.
    Vertical270,
    /// `wordArtVert` — stacked, one character per line, upright.
    WordArtVertical,
    /// `eaVert` — East Asian vertical.
    EastAsianVertical,
    /// `mongolianVert` — Mongolian vertical.
    MongolianVertical,
    /// `wordArtVertRtl` — stacked, right to left.
    WordArtVerticalRtl,
}

impl TextVertical {
    /// The `a:bodyPr@vert` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Horizontal => "horz",
            Self::Vertical => "vert",
            Self::Vertical270 => "vert270",
            Self::WordArtVertical => "wordArtVert",
            Self::EastAsianVertical => "eaVert",
            Self::MongolianVertical => "mongolianVert",
            Self::WordArtVerticalRtl => "wordArtVertRtl",
        }
    }

    /// Reads an `a:bodyPr@vert` token; anything unrecognized is the default, because
    /// the attribute is a hint about flow and refusing the file over one would cost
    /// the whole document.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.token() == token)
            .unwrap_or(Self::Horizontal)
    }

    /// Whether the flow is anything other than ordinary horizontal rows.
    ///
    /// What a renderer that draws only the horizontal case branches on, so the rest
    /// are reported rather than drawn axis-aligned and wrong.
    #[must_use]
    pub const fn is_rotated(self) -> bool {
        !matches!(self, Self::Horizontal)
    }

    /// The clockwise rotation, in 60000ths of a degree, this flow direction paints
    /// its text block at — or `None` when the direction is not a plain rotation.
    ///
    /// `vert` and `vert270` are rotations and nothing else, so they can be painted
    /// exactly by the existing layer transform. `eaVert` and the WordArt directions
    /// are NOT: they re-order and re-orient individual glyphs, which a layer
    /// transform cannot express, so they return `None` and must be reported rather
    /// than approximated by a rotation that would look deliberate.
    #[must_use]
    pub const fn layer_rotation(self) -> Option<i32> {
        match self {
            Self::Horizontal => Some(0),
            Self::Vertical => Some(5_400_000),
            Self::Vertical270 => Some(16_200_000),
            Self::WordArtVertical
            | Self::EastAsianVertical
            | Self::MongolianVertical
            | Self::WordArtVerticalRtl => None,
        }
    }

    /// Every direction, so a guard derives the count rather than hand-maintaining it.
    pub const ALL: [Self; 7] = [
        Self::Horizontal,
        Self::Vertical,
        Self::Vertical270,
        Self::WordArtVertical,
        Self::EastAsianVertical,
        Self::MongolianVertical,
        Self::WordArtVerticalRtl,
    ];
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
    /// `wps:bodyPr@vert` — the flow direction of the text block.
    ///
    /// Additive: omitted when horizontal, so existing snapshots serialize
    /// byte-identically. Until this landed the attribute was not read at all, so a
    /// Word text box with vertical text imported and rendered axis-aligned with
    /// nothing reported (`105` FID-L-08).
    #[serde(default, skip_serializing_if = "is_horizontal_text")]
    pub vertical: TextVertical,
    /// `wps:bodyPr@rot` — the text block's OWN rotation, in 60000ths of a degree,
    /// separate from the shape's `a:xfrm@rot`. Both apply, which is why this is not
    /// folded into the shape's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_rotation: Option<i32>,
}

/// `skip_serializing_if` helper: an omitted `wps:bodyPr@vert` is horizontal.
fn is_horizontal_text(vertical: &TextVertical) -> bool {
    matches!(vertical, TextVertical::Horizontal)
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

/// Whether a text-bearing shape's geometry is the plain rectangle every text box
/// had before the geometry was modeled — the value a snapshot written without the
/// field deserializes to, so writing it back would only churn the JSON.
fn is_rectangle_geometry(value: &ShapeGeometry) -> bool {
    *value == ShapeGeometry::Rectangle
}

/// A text-box child of a [`WordprocessingGroup`] (`wps:wsp` with a `wps:txbx`):
/// self-positioning block content with an optional fill and outline.
///
/// A `wps:wsp` carrying text is still a SHAPE — it has an `a:prstGeom` like any
/// other, and Word draws that geometry with the text inside it. So this type
/// carries the same [`geometry`](Self::geometry)/[`preset`](Self::preset)/
/// [`adjustments`](Self::adjustments) triple as [`GroupShape`]; without them an
/// authored ellipse or star was silently rewritten to `rect` on every save.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupTextBox {
    /// Stable identity.
    pub id: NodeId,
    /// The box's top-left in the group's child coordinate space (`a:off`).
    pub offset: PointEmu,
    /// The box's size (`a:ext`, EMU).
    pub extent: Extent,
    /// The preset geometry drawn behind the text (`a:prstGeom@prst`). Defaults
    /// to [`ShapeGeometry::Rectangle`], which is both the plain text box and
    /// what a snapshot written before this field carried implicitly.
    #[serde(default, skip_serializing_if = "is_rectangle_geometry")]
    pub geometry: ShapeGeometry,
    /// Original bounded preset token when [`geometry`](Self::geometry) is
    /// [`ShapeGeometry::Other`] because no typed primitive covers it yet.
    /// Semantic export re-emits it instead of rewriting to `rect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Ordered preset adjustment guides (`a:avLst/a:gd`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adjustments: Vec<ShapeAdjustment>,
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
    /// The hyperlink this object follows when it is clicked
    /// (`…/cNvPr/a:hlinkClick`), if it is linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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
    /// The authored custom geometry (`a:custGeom/a:pathLst/a:path`), when the
    /// shape declares one this build can draw. `Some` always wins over
    /// [`geometry`](Self::geometry), which stays [`ShapeGeometry::Other`] so
    /// nothing mistakes a freeform for a preset. `None` for a preset shape, and
    /// also for a custom geometry outside the modeled subset — which keeps
    /// painting its bounding rectangle and keeps reporting the loss
    /// (docs/119 §6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<ShapePath>,
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
    /// The hyperlink this shape follows when it is clicked
    /// (`wps:cNvPr/a:hlinkClick`), if it is linked.
    ///
    /// On the CHILD, not only on the group: Word writes the link onto each
    /// linked shape, and a corpus file measured for this carries four of them
    /// — two on `wps:cNvPr`, two on `pic:cNvPr`, none on the `wp:docPr` above
    /// them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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
    Group(Box<WordprocessingGroup>),
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
    /// The hyperlink this object follows when it is clicked
    /// (`…/cNvPr/a:hlinkClick`), if it is linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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

/// A hyperlink on a DRAWING — `a:hlinkClick`, the element that makes a picture
/// or shape clickable.
///
/// Separate from [`Hyperlink`] because that one WRAPS inline content and this
/// one is a property of an object: a drawing has no inlines to wrap, and a
/// wrapper would make every drawing's position depend on whether it happened
/// to be linked. The TARGET is shared, so the two cannot disagree about what a
/// link can point at.
///
/// `a:hlinkClick` carries an `r:id` and no `anchor`, unlike `w:hyperlink` —
/// so a link to a bookmark in the same document is a relationship whose target
/// is the fragment `#name`. The importer resolves both shapes into the same
/// [`HyperlinkTarget`], which is what keeps that asymmetry out of everything
/// downstream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawingHyperlink {
    /// Where it points.
    pub target: HyperlinkTarget,
    /// A screen-tip (`@tooltip`), if declared (non-empty, at most 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

/// Maximum field-instruction length, in UTF-8 bytes.
pub const MAX_FIELD_INSTRUCTION_BYTES: usize = 4096;

/// A field's OOXML **update** attributes: `w:fldLock` and `w:dirty`.
///
/// Both are author intent about *recalculation*, not formatting, and dropping
/// either changes what a reader sees rather than how it looks:
///
/// * `locked` (`w:fldLock`) — **do not update this field.** The author froze the
///   cached result deliberately (a dated letter, a quoted total, a `REF` to text
///   that has since moved). Losing it lets Word refresh the field on the
///   reader's machine, so the document's *content* changes silently.
/// * `dirty` (`w:dirty`) — **the cached result is stale; recalculate on open.**
///   Losing it makes a reader trust a cached value the producer had already
///   marked out of date.
///
/// They are independent: a field may be both (Word's own UI can produce it, and
/// `w:fldLock` wins there), so this is two flags rather than one tri-state.
///
/// One type for both encodings on purpose. `w:fldSimple` and the `w:fldChar`
/// sequence carry the same two attributes (`CT_SimpleField` and `CT_FldChar`,
/// ECMA-376 Part 1 §17.16.19 / §17.16.18), and so do the two model shapes a
/// field can take — the inline [`Field`] and the paragraph-spanning
/// `FieldRange`. Sharing the type is what makes it impossible for the two to
/// drift into different semantics, and lets one writer emit both (`docs/128`
/// §5).
///
/// Absent from the JSON snapshot when neither flag is set, which is the
/// overwhelming majority of fields.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldUpdateState {
    /// `w:fldLock` — Word must not update this field.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub locked: bool,
    /// `w:dirty` — the cached result is stale and must be recalculated on open.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub dirty: bool,
}

impl FieldUpdateState {
    /// Whether neither flag is set (serializes to nothing, and writes no
    /// attribute).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.locked && !self.dirty
    }

    /// Folds another reading of the same field's attributes into this one.
    ///
    /// A complex field may legally carry `w:fldLock` / `w:dirty` on any of its
    /// `w:fldChar` markers, not only the `begin` Word writes them on, so the
    /// importer merges what it finds rather than letting a later marker's flag
    /// overwrite — or be overwritten by — an earlier one. Set wins, because the
    /// attribute's absence is its default rather than an assertion of `false`.
    pub const fn merge(&mut self, other: Self) {
        self.locked |= other.locked;
        self.dirty |= other.dirty;
    }
}

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
///
/// `update` carries `w:fldLock` / `w:dirty` — see [`FieldUpdateState`]. The
/// *encoding* the field arrived in (`w:fldSimple` or the four-run `w:fldChar`
/// sequence) is deliberately **not** recorded: the two spellings are the same
/// field to any reader, and everything is written back in the complex spelling
/// Word itself writes (`docs/128` §5a).
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
    /// The `w:fldLock` / `w:dirty` update attributes. Default (neither set) for
    /// a field the producer left updatable and current, which is most of them.
    #[serde(default, skip_serializing_if = "FieldUpdateState::is_empty")]
    pub update: FieldUpdateState,
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

/// A field whose value is **recomputed from pagination** rather than displayed
/// from the cached result the producer wrote.
///
/// These two are special everywhere in the engine and the reason is one fact:
/// their value is not a property of the model, it is a property of the page the
/// field lands on. Layout therefore restamps them after pagination and never
/// shows [`Field::inlines`], and an edit to those inlines would be text nothing
/// renders. Every other field's cached result *is* what the reader sees.
///
/// This lives in the model so the layout pass that restamps them and the edit
/// layer that must refuse to edit them key off ONE rule. It had two homes
/// before (`casual-doc-layout`'s `field_kind` and an anchor-width predicate in
/// `casual-doc-wasm`), which is exactly the shape that drifts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaginatedField {
    /// `PAGE` — the number of the page the field sits on.
    PageNumber,
    /// `NUMPAGES` — the document's total page count.
    PageCount,
}

impl PaginatedField {
    /// Which pagination-dependent field `instruction` names, if either
    /// (case-insensitive, leading whitespace and trailing switches ignored).
    ///
    /// O(the instruction's first token) and allocation-free — never O(document).
    /// [`FieldKind::parse`] answers the same question, but it tokenizes the whole
    /// instruction into owned `String`s; this one runs per keystroke over every
    /// inline of the caret's paragraph, so it may not allocate. Both read the
    /// leading keyword through the same helper, and a model test pins their
    /// answers against each other so they cannot drift.
    #[must_use]
    pub fn parse(instruction: &str) -> Option<Self> {
        let keyword = leading_field_token(instruction);
        if keyword.eq_ignore_ascii_case("PAGE") {
            Some(Self::PageNumber)
        } else if keyword.eq_ignore_ascii_case("NUMPAGES") {
            Some(Self::PageCount)
        } else {
            None
        }
    }
}

/// The instruction's leading keyword as a borrowed slice, without allocating.
///
/// Mirrors [`tokenize_field_instruction`]'s first token exactly — leading
/// whitespace skipped, a leading double-quoted span unquoted, an unquoted token
/// ending at whitespace or a quote — so the cheap predicate and the full
/// classification cannot disagree about which keyword an instruction names.
fn leading_field_token(instruction: &str) -> &str {
    let rest = instruction.trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.split('"').next().unwrap_or(quoted);
    }
    let end = rest
        .find(|character: char| character.is_whitespace() || character == '"')
        .unwrap_or(rest.len());
    &rest[..end]
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

impl FormCheckBox {
    /// The box character this checkbox shows for its current state.
    ///
    /// U+25A1 WHITE SQUARE / U+2611 BALLOT BOX WITH CHECK: the same widely
    /// covered BMP box glyphs the symbol map resolves legacy Wingdings
    /// checkboxes to, so they paint reliably (U+2610 BALLOT BOX is often
    /// absent and renders blank).
    ///
    /// It lives on the model because three crates need the same answer. A
    /// `FORMCHECKBOX` has no content: the box is SYNTHESISED from this state,
    /// so layout (which paints it), the edit crate (which measures how many
    /// bytes it occupies) and the accessibility mirror (which reads it aloud)
    /// each need it, and any two of them disagreeing puts the caret somewhere
    /// the box is not.
    #[must_use]
    pub fn glyph(&self) -> char {
        if self.checked.or(self.default).unwrap_or(false) {
            '\u{2611}'
        } else {
            '\u{25A1}'
        }
    }
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
    /// The hyperlink this object follows when it is clicked
    /// (`…/cNvPr/a:hlinkClick`), if it is linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<DrawingHyperlink>,
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
    pub properties: SharedRunProperties,
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

/// The start marker of a **paragraph-spanning** complex field
/// (`w:fldChar w:fldCharType="begin"`, with its instruction and the following
/// `separate`). A zero-width point; the field's cached result is the span to the
/// [`FieldRangeEnd`] sharing its `field`.
///
/// In OOXML a complex field is a range, not a container: the `w:fldChar` markers
/// are run-level, so `begin` and `end` may sit in different paragraphs — which is
/// the only way a table of contents, whose result is one paragraph per entry, can
/// be a field at all. The shape is deliberately the bookmark's: two markers plus a
/// definition-table payload (`Definitions::field_ranges`), so the two ends cannot
/// disagree about the instruction.
///
/// A complex field whose markers fall in the **same** paragraph stays an inline
/// [`Field`] and is unaffected. See `docs/128`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldRangeStart {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The field range this opens (resolves in `Definitions::field_ranges`).
    pub field: FieldRangeId,
}

/// The end marker of a paragraph-spanning complex field
/// (`w:fldChar w:fldCharType="end"`). A zero-width point closing the
/// [`FieldRangeStart`] that shares its `field`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldRangeEnd {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The field range this closes (resolves in `Definitions::field_ranges`).
    pub field: FieldRangeId,
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
    /// A base with a preceding (left) subscript and/or superscript, from `m:sPre` —
    /// the left-script mirror of [`MathExpression::Script`].
    PreScript {
        /// The base expression (the `m:e`).
        base: Box<MathExpression>,
        /// The preceding subscript (the `m:sub`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subscript: Option<Box<MathExpression>>,
        /// The preceding superscript (the `m:sup`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        superscript: Option<Box<MathExpression>>,
    },
    /// A formula enclosed in a border/strike box, from `m:borderBox`.
    BorderBox {
        /// The boxed content (the `m:e`).
        content: Box<MathExpression>,
        /// Which box edges are drawn and which strikes are present
        /// (`m:borderBoxPr`); the default draws all four borders and no strike.
        #[serde(default, skip_serializing_if = "MathBorderBox::is_default")]
        borders: MathBorderBox,
    },
    /// A logical grouping box, from `m:box` — a transparent wrapper Word uses for
    /// break/alignment grouping. Carries only its content; the box's layout-hint
    /// `m:boxPr` stays authoritative in the retained OMML.
    Box {
        /// The grouped content (the `m:e`).
        content: Box<MathExpression>,
    },
}

/// The visible edges and strikes of a [`MathExpression::BorderBox`]
/// (`m:borderBoxPr`). By default every border is drawn and no strike is present;
/// each flag, when set, HIDES a border or ADDS a strike (matching the OMML
/// `m:hide*`/`m:strike*` `CT_OnOff` children).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MathBorderBox {
    /// Hide the top border (`m:hideTop`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hide_top: bool,
    /// Hide the bottom border (`m:hideBot`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hide_bottom: bool,
    /// Hide the leading (left) border (`m:hideLeft`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hide_left: bool,
    /// Hide the trailing (right) border (`m:hideRight`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hide_right: bool,
    /// A horizontal strike-through (`m:strikeH`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub strike_horizontal: bool,
    /// A vertical strike-through (`m:strikeV`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub strike_vertical: bool,
    /// A bottom-left to top-right diagonal strike (`m:strikeBLTR`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub strike_bltr: bool,
    /// A top-left to bottom-right diagonal strike (`m:strikeTLBR`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub strike_tlbr: bool,
}

impl MathBorderBox {
    /// Whether every field is the OOXML default (all borders drawn, no strikes).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
// A `Vec<InlineNode>` pays for the enum's *largest* variant on every element,
// so the enum's size — not the common payload's — is what a paragraph is
// charged. That used to be 416 bytes because `Run` carried a `RunProperties`
// by value; now that run and paragraph formatting is shared
// (`super::Shared`), `Run` is 40 bytes and what sets the size is the long
// tail of rare, structurally large variants. Every one of them larger than a
// `Run` is therefore stored out of line, which is the same trade `BlockNode`
// makes below: one pointer hop on a path that already allocates a `Vec` of
// children or a `String` of opaque XML, against 368 bytes saved on every
// inline in the document.
//
// The three still inline at 48 bytes — `Symbol`, `NoteReference`,
// `MoveRangeEnd` — set the enum's size, so boxing any *one* boxed variant
// above would buy nothing; they are left alone because the remaining 8 bytes
// are not worth an allocation. Measured with `casual-doc-layout`'s
// `model_footprint` example, which prints every payload largest-first so the
// variant that sets the size is named rather than guessed at.
//
// `Run` stays inline deliberately: it is the common case, and boxing it would
// add a heap allocation per run while shrinking nothing — the enum is already
// the size of the three 48-byte leaves. The guard that matters is in
// `v1::tests`: `InlineNode` must not exceed `Run` plus a discriminant.
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
    /// An inline drawing referencing embedded media. Boxed: see the note on
    /// this enum.
    Drawing(Box<Drawing>),
    /// An anchored (floating) drawing placed at an absolute page position.
    /// Boxed: see the note on this enum.
    AnchoredDrawing(Box<AnchoredDrawing>),
    /// An inline embedded object (chart, SmartArt diagram, or OLE object)
    /// referencing preserved package part(s).
    EmbeddedObject(Box<EmbeddedObject>),
    /// An inline hyperlink wrapping inline content. Boxed: see the note on
    /// this enum.
    Hyperlink(Box<Hyperlink>),
    /// An inline field: an instruction and its cached result. Boxed: see the
    /// note on this enum.
    Field(Box<Field>),
    /// An inline text box holding block content (inline, or floating when it
    /// carries a [`TextBox::anchor`]).
    TextBox(Box<TextBox>),
    /// A DrawingML group (`wpg:wgp`): a floating, z-ordered container of
    /// pictures, text boxes, and shapes.
    Group(Box<WordprocessingGroup>),
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
    /// Boxed: see the note on this enum.
    Revision(Box<Revision>),
    /// The start marker of a bookmark range.
    BookmarkStart(BookmarkStart),
    /// The end marker of a bookmark range.
    BookmarkEnd(BookmarkEnd),
    /// The start marker of a paragraph-spanning complex field. The field's cached
    /// result is the content up to the matching [`InlineNode::FieldRangeEnd`].
    FieldRangeStart(FieldRangeStart),
    /// The end marker of a paragraph-spanning complex field.
    FieldRangeEnd(FieldRangeEnd),
    /// The start marker of a tracked-move (source or destination) range.
    /// Boxed: see the note on this enum.
    MoveRangeStart(Box<MoveRangeStart>),
    /// The end marker of a tracked-move (source or destination) range.
    MoveRangeEnd(MoveRangeEnd),
    /// An inline-level content control wrapping inline content. Boxed: see
    /// the note on this enum.
    Sdt(Box<InlineSdt>),
    /// An inline math object retaining its OMML subtree verbatim. Boxed: see
    /// the note on this enum.
    Math(Box<Math>),
    /// An inline symbol glyph (a font plus a code point).
    Symbol(Box<Symbol>),
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
            Self::FieldRangeStart(node) => node.id,
            Self::FieldRangeEnd(node) => node.id,
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
    /// Paragraph properties (always present; empty is `{}`). Shared and
    /// copy-on-write: see [`SharedParagraphProperties`].
    pub properties: SharedParagraphProperties,
    /// Ordered inline content.
    pub inlines: Vec<InlineNode>,
}

/// A body-level node.
//
// A `Vec<BlockNode>` pays for the enum's *largest* variant on every element, so
// the two rare-but-large variants are stored out of line: `Table` was 800 bytes
// and `BlockSdt` 384, against a `Paragraph` of 352. Boxing them makes the enum
// cost what a paragraph costs — which is what a body is nearly entirely made of
// — for one pointer's indirection on the table/content-control paths, where a
// heap allocation is already dwarfed by the rows or blocks being allocated.
// `Paragraph` itself stays inline: it is the common case, and boxing it would
// add an allocation per paragraph without shrinking anything (`docs/111` §4) —
// which is exactly what the lint below now suggests, so it stays allowed. The
// guard that matters is in `v1::tests`: `BlockNode` must not exceed `Paragraph`
// plus a discriminant, which the lint cannot express.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    /// A paragraph block.
    Paragraph(Paragraph),
    /// A table block. Boxed: see the note on this enum.
    Table(Box<Table>),
    /// A block-level content control wrapping block content. Boxed: see the
    /// note on this enum.
    Sdt(Box<BlockSdt>),
    /// An aggregated external content chunk (`w:altChunk`) referencing a
    /// preserved package part. Boxed: see the note on this enum.
    AltChunk(Box<AltChunk>),
}

impl BlockNode {
    /// Releases every byte of spare `Vec` and `String` capacity this block and
    /// its descendants hold.
    ///
    /// # Why the model does this at all
    ///
    /// `Vec::push` onto an empty vector does not allocate one element — it
    /// allocates `RawVec::MIN_NON_ZERO_CAP`, which is **four** elements for any
    /// element of 1,024 bytes or less. Every importer builds a paragraph's
    /// `inlines` by pushing, and the overwhelming majority of paragraphs hold
    /// one inline, so the overwhelming majority of paragraphs carried three
    /// empty `InlineNode` slots. Measured on the owner's own file shape
    /// (`docs/111` §4a) that was **1,248 bytes per paragraph of capacity
    /// holding nothing** — 56% of what a paragraph cost, and more than any
    /// struct field in the model. The body vector's own doubling adds a further
    /// 0-100% on top.
    ///
    /// Spare capacity is invisible to `size_of` and to any test that inspects
    /// values, which is why it survived three rounds of shrinking the structs.
    /// It is released here, at the point a document is constructed, rather than
    /// in each importer: there is one `Document::new` and there are five
    /// importers.
    ///
    /// Nothing observable changes — capacity is not part of a document's value,
    /// its serialization, or its identity. Editing afterwards regrows the
    /// vectors it touches, which is correct: a paragraph that was just edited
    /// is about to be edited again.
    pub fn shrink_to_fit(&mut self) {
        match self {
            Self::Paragraph(paragraph) => shrink_inlines(&mut paragraph.inlines),
            Self::Table(table) => {
                table.grid.shrink_to_fit();
                table.rows.shrink_to_fit();
                for row in &mut table.rows {
                    row.cells.shrink_to_fit();
                    for cell in &mut row.cells {
                        shrink_blocks(&mut cell.blocks);
                    }
                }
            }
            Self::Sdt(sdt) => shrink_blocks(&mut sdt.blocks),
            Self::AltChunk(_) => {}
        }
    }
}

impl InlineNode {
    /// Releases every byte of spare capacity this inline and its descendants
    /// hold. The measurement that motivates it is on
    /// `BlockNode::shrink_to_fit`.
    pub fn shrink_to_fit(&mut self) {
        match self {
            Self::Run(run) => run.text.shrink_to_fit(),
            Self::Hyperlink(hyperlink) => shrink_inlines(&mut hyperlink.inlines),
            Self::Field(field) => shrink_inlines(&mut field.inlines),
            Self::Revision(revision) => shrink_inlines(&mut revision.inlines),
            Self::Sdt(sdt) => shrink_inlines(&mut sdt.inlines),
            Self::TextBox(text_box) => shrink_blocks(&mut text_box.blocks),
            Self::Group(group) => shrink_group(group),
            Self::Tab(_)
            | Self::Break(_)
            | Self::Drawing(_)
            | Self::AnchoredDrawing(_)
            | Self::EmbeddedObject(_)
            | Self::NoteReference(_)
            | Self::NoteNumberMark(_)
            | Self::CommentReference(_)
            | Self::CommentRangeStart(_)
            | Self::CommentRangeEnd(_)
            | Self::BookmarkStart(_)
            | Self::BookmarkEnd(_)
            | Self::FieldRangeStart(_)
            | Self::FieldRangeEnd(_)
            | Self::MoveRangeStart(_)
            | Self::MoveRangeEnd(_)
            | Self::Math(_)
            | Self::Symbol(_)
            | Self::HorizontalRule(_)
            | Self::NoBreakHyphen(_)
            | Self::SoftHyphen(_)
            | Self::PositionalTab(_) => {}
        }
    }
}

/// Releases spare capacity in a block list and everything under it.
pub(super) fn shrink_blocks(blocks: &mut Vec<BlockNode>) {
    blocks.shrink_to_fit();
    for block in blocks.iter_mut() {
        block.shrink_to_fit();
    }
}

/// Releases spare capacity in an inline list and everything under it.
fn shrink_inlines(inlines: &mut Vec<InlineNode>) {
    inlines.shrink_to_fit();
    for inline in inlines.iter_mut() {
        inline.shrink_to_fit();
    }
}

/// Releases spare capacity in a DrawingML group and its nested groups.
fn shrink_group(group: &mut WordprocessingGroup) {
    group.children.shrink_to_fit();
    for child in &mut group.children {
        match child {
            GroupChild::TextBox(text_box) => shrink_blocks(&mut text_box.blocks),
            GroupChild::Group(nested) => shrink_group(nested),
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
}
