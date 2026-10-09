//! Paginated output — the immutable page fragments the paginator produces.
//!
//! Following the LayoutNG discipline (`42-…` §1.4), the paginator *produces*
//! immutable [`Page`]s and painting + hit-testing are read-only walks of them.
//! Each page records the model range it spans and the placement of every
//! fragment, which is exactly what makes incremental re-pagination (the
//! stabilization halt) and hit-testing cheap (`43-…` §3.5, §9).

use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;
// Kept on a separate `use` line (anti-conflict): the shape fill/outline/line-end
// model types the anchor paint content carries.
use casual_doc_model::v1::{DashStyle, Fill, LineEnd, PathFill};
// Own `use` line (anti-conflict): the outline geometry `a:ln` carries beyond
// colour, width and a preset dash.
use casual_doc_model::v1::{DashStop, LineCap, LineJoin};
// Own `use` line (anti-conflict): the `a:gradFill` geometry the model's `Fill`
// has nowhere to put, carried beside it by `AnchorFill`.
use casual_doc_model::v1::GradientDetail;
use serde::{Deserialize, Serialize};

use crate::block::{BlockFragment, ResolvedEdge};
// Separate `use` line to minimize import-block merge conflicts.
use crate::display::PathCommand;
use crate::display::ShapeTransform;
use crate::model::ModelPos;
use crate::text::{GlyphRun, TextBoxStroke};
use crate::units::{Point, Rect, Size, Twip};

/// A position in the galley's flow: a fragment (by index) and a line offset
/// within it (`0` for a whole fragment or a split paragraph's first chunk).
///
/// This is the *carry state* at a page boundary. Because every page begins at a
/// fresh content-top cursor, the flow position of a page's first content is the
/// only state that determines everything below it — so two paginations that
/// reach the same [`FlowPos`] over identical downstream content lay out
/// identically from there. That is the key the incremental paginator matches on
/// to reuse pages unchanged (the stabilization halt, `43-…` §3.4).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct FlowPos {
    /// Index of the fragment in the galley.
    pub fragment: u32,
    /// Line offset within that fragment (0 unless a paragraph was split).
    pub line: u32,
}

impl FlowPos {
    /// The flow position at galley index `fragment`, line 0.
    #[must_use]
    pub fn at(fragment: u32) -> Self {
        Self { fragment, line: 0 }
    }
}

/// The half-open span of the galley a page covers, `[start, end)`: `start` is
/// the flow position of the page's first content and `end` is the position of
/// the first content *not* on the page (i.e. the next page's `start`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FlowSpan {
    /// Flow position of the page's first content.
    pub start: FlowPos,
    /// Flow position one past the page's last content.
    pub end: FlowPos,
}

/// A block fragment placed at an absolute rectangle on a page.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PlacedFragment {
    /// The fragment (or the portion of it placed on this page).
    pub fragment: BlockFragment,
    /// Its rectangle in page-local twip coordinates.
    pub rect: Rect,
    /// The source section that produced this placed body fragment. `None` keeps
    /// older serialized layouts valid and is interpreted as the page's section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<SectionId>,
}

/// The stacking key of a floating object — how the float layer orders paints.
///
/// Word paints floating objects by `wp:anchor@relativeHeight` (higher paints
/// later, i.e. on top), with document order as the tiebreaker. A single stable
/// sort by `(relative_height, order)` reproduces Word's layering; `behind_doc`
/// (on [`PlacedAnchor`]) first partitions floats into the band below the text and
/// the band above it. Group children share their group's key and are ordered
/// among themselves by `order` (their document/paint order within the group).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct AnchorZ {
    /// `wp:anchor@relativeHeight` (0 when the producer omitted it).
    pub relative_height: u32,
    /// A monotonic document-order counter assigned during collection (the primary
    /// tiebreaker, and the intra-group paint order).
    pub order: u32,
}

/// A stroke (outline) painted for a floating shape or connector: a resolved color,
/// a width in twips, and a preset dash pattern (`a:ln > a:prstDash`).
/// # Why this is no longer `Copy`
///
/// `a:custDash` is a list the author stated, so its natural representation is a
/// `Vec` — and a `Vec` cannot be `Copy`. The alternative was a fixed `[DashStop; 16]`
/// plus a length, which keeps `Copy` by making every outline in every document carry
/// 128 bytes for a feature almost none of them use. For a display-layer value with
/// six literal sites that is the wrong trade; the compiler found all of them.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AnchorStroke {
    /// The stroke color (RGBA).
    pub color: [u8; 4],
    /// The stroke width in twips (a hairline when `0`).
    pub width: Twip,
    /// The preset dash pattern (`DashStyle::Solid` = an unbroken line).
    #[serde(default = "solid_dash", skip_serializing_if = "is_solid_dash")]
    pub dash: DashStyle,
    /// `a:ln@cap` — how a dash and an open end terminate. `None` leaves the
    /// backend's default, which is what every outline used to get regardless of what
    /// the file said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<LineCap>,
    /// `a:ln`'s join child — how two segments meet at a corner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<LineJoin>,
    /// `a:custDash` as dash/space pairs. Non-empty OUTRANKS `dash`: it is the
    /// pattern the author stated, where `a:prstDash` is one picked from a gallery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_dash: Vec<DashStop>,
}

/// The default dash for a serialized [`AnchorStroke`] that predates the field.
fn solid_dash() -> DashStyle {
    DashStyle::Solid
}

fn is_solid_dash(dash: &DashStyle) -> bool {
    matches!(dash, DashStyle::Solid)
}

/// A floating shape's resolved fill: the model's `a:solidFill`/`a:gradFill` plus
/// the `a:gradFill` geometry that model value has nowhere to put.
///
/// # Why this wraps the model fill instead of replacing it
///
/// Exactly the [`AnchorStroke`] arrangement, one layer down. `ShapeStroke` is a small
/// `Copy` value with literals across six crates, so the cap, join and `a:custDash`
/// it cannot hold live in `Definitions::shape_fill_detail` and are folded in here, at
/// placement, by `anchor::shape_stroke`. `Fill` has the same problem: `a:path@path`
/// and `a:fillToRect` are in the same side table, under the same shape id, and were
/// never read — so a `path="rect"` gradient reached the display list as the identical
/// value a `path="circle"` one did, and painted as concentric circles.
///
/// Wrapping rather than mirroring the model enum keeps ONE description of a stop
/// list: a parallel `AnchorGradient` would have to be kept in step with
/// `Fill::Gradient` by hand, and the first thing to change would be the thing that
/// drifted. It also keeps [`AnchorContent`] `Eq`, which a display-layer gradient (with
/// `f32` positions) cannot be.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AnchorFill {
    /// The resolved model fill — a solid colour or an ordered stop list.
    pub fill: Fill,
    /// The `a:gradFill` geometry from `Definitions::shape_fill_detail`, when the file
    /// stated any. `None` for a solid fill, for a gradient with nothing beyond its
    /// stops and sweep, and for a fill that came from the theme's style matrix (which
    /// files no row under the shape's id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<GradientDetail>,
}

impl AnchorFill {
    /// A fill with no retained `a:gradFill` geometry — a solid colour, or a gradient
    /// whose sweep `Fill::Gradient` already describes in full.
    #[must_use]
    pub const fn plain(fill: Fill) -> Self {
        Self {
            fill,
            gradient: None,
        }
    }
}

/// What a [`PlacedAnchor`] paints: an image, a filled/stroked shape, a line/
/// connector, or a text box (flowed block content with an optional fill/border).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum AnchorContent {
    /// An embedded picture, resolved by the backend against `Definitions::media`.
    Image {
        /// The media key (package part name).
        media: String,
        /// The source-rectangle crop (`a:srcRect`), if the picture is cropped
        /// (`P1G-OBJ-MODEL`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop: Option<casual_doc_model::v1::CropRect>,
        /// The picture frame outline (`pic:spPr/a:ln`), if the picture is bordered.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        border: Option<AnchorStroke>,
        /// The picture's opacity (`a:alphaModFix`), in 1000ths of a percent;
        /// `None` is fully opaque.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<u32>,
    },
    /// A shape whose FILL is a picture (`a:blipFill` on a `wps:spPr`): the image
    /// painted clipped to the shape's own outline, with the outline stroked over it.
    ///
    /// A distinct variant rather than an `Image` with a clip bolted on, because the
    /// placed layer is one anchor per shape and the expansion into
    /// `PushClipPath` + `Image` + `PopClip` + stroke belongs where the other
    /// multi-item expansions live — `AnchorContent::TextBox` already works that way.
    ///
    /// Only `a:stretch` reaches here. `a:tile` repeats the picture, and the display
    /// list has no tiling primitive, so a tiled fill stays reported-and-unpainted
    /// rather than being stretched instead — a tiled logo drawn stretched looks
    /// deliberate and is not what the file says.
    PictureFilledShape {
        /// The shape's outline, which the picture is clipped to.
        commands: Vec<crate::display::PathCommand>,
        /// Whether that outline closes.
        closed: bool,
        /// The media key (package part name), resolved as for an `Image`.
        media: String,
        /// `a:srcRect`, the source crop.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop: Option<casual_doc_model::v1::CropRect>,
        /// The fill's opacity in 1000ths of a percent; `None` is opaque.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<u32>,
        /// The shape's outline, stroked over the picture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<AnchorStroke>,
    },
    /// A rectangle (a group's background/foreground shape).
    Rectangle {
        /// The fill (solid or gradient), if filled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<AnchorFill>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// An ellipse fitted to the anchor rectangle.
    Ellipse {
        /// The fill (solid or gradient), if filled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<AnchorFill>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// A rounded rectangle with a resolved corner radius.
    RoundedRectangle {
        /// Corner radius in twips, clamped to half the shorter side.
        radius: Twip,
        /// The fill (solid or gradient), if filled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<AnchorFill>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// A DrawingML geometry resolved to page-local paths: every preset but the
    /// rectangle, ellipse and line, and every authored `a:custGeom`.
    ///
    /// One variant for both because they ARE one thing — a preset is a geometry
    /// the standard wrote in advance (`docs/119` §6) — and both arrive here
    /// through `casual_doc_model::v1::GeometryProgram`.
    Path {
        /// The geometry's paths, in paint order: one for a freeform, up to six
        /// for a preset (the lid of a `can`, the faces of a `cube`, a callout's
        /// leader). Each carries its own fill mode and stroke switch.
        paths: Vec<AnchorPath>,
        /// The shape's fill (solid or gradient), which a path's fill mode paints
        /// as is, lightened, darkened, or not at all.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<AnchorFill>,
        /// The shape's outline, drawn on every path whose stroke switch is on.
        stroke: Option<AnchorStroke>,
        /// The start (`a:headEnd`) arrowhead, drawn at the start of the first
        /// open stroked path — a connector's, an `arc`'s.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        head_end: Option<LineEnd>,
        /// The end (`a:tailEnd`) arrowhead, drawn at the end of the last open
        /// stroked path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail_end: Option<LineEnd>,
    },
    /// A straight line / connector, from `from` to `to` (page-local twips).
    Line {
        /// The line's start point.
        from: Point,
        /// The line's end point.
        to: Point,
        /// The line's stroke.
        stroke: AnchorStroke,
        /// The start (`a:headEnd`) arrowhead, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        head_end: Option<LineEnd>,
        /// The end (`a:tailEnd`) arrowhead, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail_end: Option<LineEnd>,
    },
    /// A text box: block content flowed through the shared pipeline, with an
    /// optional fill and border.
    TextBox {
        /// The flowed block fragments, positioned relative to the box's content
        /// origin (the box top-left inset by the internal margin).
        blocks: Vec<BlockFragment>,
        /// The box background fill (solid or gradient), if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<AnchorFill>,
        /// The box border color and width, if any.
        border: Option<TextBoxStroke>,
        /// Resolved content offset and overflow clipping.
        content_layout: crate::text::TextBoxContentLayout,
        /// The shape painted BEHIND the text when the text-bearing `wps:wsp`
        /// carries a preset geometry that is not a plain rectangle — an
        /// ellipse, a star, a block arrow. It is produced by the same
        /// geometry → content mapping a text-free `GroupChild::Shape` uses, so
        /// there is one description of what a preset looks like, and it carries
        /// the box's own fill and outline (which are then left unset above, so
        /// nothing paints twice).
        ///
        /// `None` is the plain rectangular text box: `fill`/`border` paint it,
        /// exactly as before this field existed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backdrop: Option<Box<AnchorContent>>,
        /// The rotation the text CONTENT paints at (`wps:bodyPr@vert` and
        /// `@rot` combined), about the box centre — or `None` for upright text.
        ///
        /// Separate from [`PlacedAnchor::transform`], which rotates the box and its
        /// chrome. They are different rotations: a shape can be rotated while its
        /// text is upright, and a text block can be rotated inside an unrotated
        /// shape. Folding them together would make one unexpressible.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_transform: Option<crate::display::ShapeTransform>,
    },
    /// A **positioned (floating) table** — `w:tblPr/w:tblpPr` — lifted out of
    /// block flow and placed against a page/margin/text reference frame, with
    /// the body text wrapping around it (the engine's `table_float` pass).
    ///
    /// Its rows are the ordinary [`BlockFragment::TableRow`]s the flow engine
    /// produces, positioned relative to the anchor rectangle's origin, so the
    /// table paints, hit-tests and carries its cells' model identity exactly as
    /// an in-flow table does — the only difference is who computed its rect.
    Table {
        /// The flowed rows, stacked from the anchor rectangle's origin.
        rows: Vec<BlockFragment>,
    },
}

/// One painted path of an [`AnchorContent::Path`] (`a:pathLst/a:path`), in
/// page-local twips.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AnchorPath {
    /// The commands, beginning with a [`PathCommand::MoveTo`]; a
    /// [`PathCommand::Close`] closes the subpath it ends, and a path may hold
    /// several subpaths (a `donut`'s two rings, wound opposite ways).
    pub commands: Vec<PathCommand>,
    /// How the shape's fill paints this path (`a:path@fill`).
    #[serde(default, skip_serializing_if = "is_norm_fill")]
    pub fill: PathFill,
    /// Whether the shape's outline is drawn on this path (`a:path@stroke`).
    pub stroke: bool,
}

fn is_norm_fill(fill: &PathFill) -> bool {
    *fill == PathFill::Norm
}

/// A floating object resolved to its absolute rectangle and stacking key on a
/// page: an anchored picture, a floating text box, or a group child (picture,
/// text box, shape, or connector). Unlike a [`PlacedFragment`], a float does
/// not participate in the flow — it is placed at the position computed from its
/// anchor (and, for a group child, the group transform), then painted in z-order
/// by [`compose_page`](crate::compose::compose_page).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PlacedAnchor {
    /// The model anchor node this float came from — a top-level
    /// [`AnchoredDrawing`](casual_doc_model::v1::AnchoredDrawing) or a floating
    /// [`TextBox`](casual_doc_model::v1::TextBox) — so a click can be resolved
    /// back to a selectable object (docs/85 §3). `None` for a group child (not
    /// individually selectable yet) and any float with no model identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    /// What this float paints.
    pub content: AnchorContent,
    /// The absolute rectangle in page-local twip coordinates (the paint box; the
    /// bounding box for a [`AnchorContent::Line`]).
    pub rect: Rect,
    /// Whether the float paints behind the document text (its band).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub behind_doc: bool,
    /// The stacking key (`relativeHeight` + document order).
    pub z: AnchorZ,
    /// The float's alt text (`wp:docPr@descr`), preserved for accessibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    /// The rotation/flip (`a:xfrm@rot`/`@flipH`/`@flipV`) painted about the
    /// float's center, if any. Applied to the emitted shape/image; rotated
    /// text-box content is a follow-up (content stays axis-aligned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<ShapeTransform>,
    /// The drop shadow this float casts (`a:effectLst/a:outerShdw`), already
    /// resolved from polar `@dist`/`@dir` to a twip offset.
    ///
    /// On the ANCHOR rather than inside `AnchorContent`, so one bracket serves every
    /// content kind: a shape, a picture-filled shape and a text box all cast the
    /// shadow of what they paint without each variant carrying the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<AnchorShadow>,
}

/// The drop shadow a placed float casts, in layout units.
///
/// `a:outerShdw`'s `@dist`/`@dir` are polar; they are resolved to a cartesian twip
/// offset here, where the shape's rectangle is known, rather than in the backend —
/// the backend should not need DrawingML's angle convention.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorShadow {
    /// The blur radius in twips. Zero is a hard-edged offset copy.
    pub blur: Twip,
    /// The horizontal displacement in twips.
    pub offset_x: Twip,
    /// The vertical displacement in twips.
    pub offset_y: Twip,
    /// The shadow colour, alpha included.
    pub color: [u8; 4],
}

/// A column separator rule (`w:cols/@w:sep`) to paint on a page: a thin vertical
/// line centered in an inter-column gap, spanning its column band. Produced by the
/// column paginator and painted by [`compose_page`](crate::compose::compose_page);
/// it participates in neither flow nor hit-testing.
/// A resolved page-border frame (`w:pgBorders`) for one page: the outer frame
/// rectangle in page-local twips plus the resolved line for each present edge
/// (`None` where the section declares none for that side). Produced off the
/// pagination hot path by the page-border resolution pass and painted by
/// [`compose_page`](crate::compose::compose_page), like the running
/// header/footer and column separators; participates in neither flow nor
/// hit-testing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ResolvedPageBorders {
    /// The frame's outer rectangle — each side already offset per `offsetFrom`.
    pub rect: Rect,
    /// Top edge (`w:top`), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<ResolvedEdge>,
    /// Bottom edge (`w:bottom`), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<ResolvedEdge>,
    /// Leading (left) edge (`w:left`), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ResolvedEdge>,
    /// Trailing (right) edge (`w:right`), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ResolvedEdge>,
}

/// One line number stamped in a page's margin (`w:lnNumType`).
///
/// Line numbers are page **furniture**, like the page border and the column
/// separator: produced by a post-pagination pass, painted by
/// [`compose_page`](crate::compose::compose_page), and part of neither the flow
/// nor the caret/selection model — a click in the margin never lands "in" a line
/// number, and selecting a paragraph never copies one.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PlacedLineNumber {
    /// The counter value this run displays. Kept alongside the shaped glyphs so
    /// the numbering *policy* (`countBy`/`start`/`restart`) is assertable without
    /// decoding glyph ids, and so a host can expose the number to assistive
    /// technology.
    pub number: u32,
    /// The shaped number, already positioned in page-local twips: right-aligned
    /// to `distance` before the numbered line's column, on that line's baseline.
    pub run: GlyphRun,
}

/// A resolved watermark, ready to paint: its content already positioned in
/// page-local twips, plus the rotation applied to the whole stamp.
///
/// The rotation is on the STAMP and not on its parts because that is what a
/// watermark is — one angled object whose words are not individually angled.
/// `compose_page` emits it as a `PushTransform`/`PopTransform` bracket.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PlacedWatermark {
    /// What is stamped.
    pub content: PlacedWatermarkContent,
    /// Rotation about the page centre, or `None` for a level watermark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<ShapeTransform>,
}

/// The two kinds of stamp.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum PlacedWatermarkContent {
    /// Shaped words, positioned unrotated and centred on the page.
    Text {
        /// The shaped runs, in visual order.
        runs: Vec<GlyphRun>,
    },
    /// An image, scaled into `rect`.
    Picture {
        /// The media reference id, resolved by the backend against
        /// `Definitions::media` (stringly, matching `PaintItem::Image`).
        media: String,
        /// The destination box, centred on the page.
        rect: Rect,
        /// Opacity in 1000ths of a percent; `None` is fully opaque.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<u32>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ColumnSeparator {
    /// The rule's x in page-local twips (the gap's horizontal center).
    pub x: Twip,
    /// The band top in page-local twips (the rule's upper end).
    pub top: Twip,
    /// The band bottom in page-local twips (the rule's lower end).
    pub bottom: Twip,
}

/// One laid-out page.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Page {
    /// 1-based page number.
    pub number: u32,
    /// The section whose geometry + header/footer set applies to this page.
    pub section: SectionId,
    /// The immutable physical page box resolved for this page's section.
    ///
    /// Consumers must use this value rather than a document-global page size:
    /// DOCX sections can switch paper size or orientation mid-document.
    pub page_size: Size,
    /// The content area (page box minus margins, header/footer, and any
    /// footnote reservation), in page-local twips.
    pub content_area: Rect,
    /// Fragments placed in the content area, in flow order.
    pub placed: Vec<PlacedFragment>,
    /// The running header laid out in the top band (the per-page-selected
    /// header for this page's number + section). Empty until the running-content
    /// pass ([`crate::running::place_running_content`]) fills it; kept off the
    /// pagination hot path so page reuse stays field-value-free.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header: Vec<PlacedFragment>,
    /// The running footer laid out in the bottom band (see [`Page::header`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footer: Vec<PlacedFragment>,
    /// Anchored (floating) drawings resolved onto this page. Empty until the
    /// anchored-placement pass ([`crate::anchor::place_floats`]) fills
    /// it; kept off the pagination hot path so page reuse (the stabilization halt)
    /// stays position-free, exactly like the running header/footer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchored: Vec<PlacedAnchor>,
    /// Footnotes placed at the bottom of this page.
    pub footnotes: Vec<PlacedFragment>,
    /// Column separator rules (`w:cols/@w:sep`) to paint between the columns of
    /// this page's multi-column section bands. Empty for single-column pages and
    /// for multi-column sections that declare no separator; produced by the column
    /// paginator, off the single-column hot path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub separators: Vec<ColumnSeparator>,
    /// The resolved page-border frame (`w:pgBorders`) for this page, if the
    /// page's section declares one and its `display` policy includes this page.
    /// `None` otherwise; filled by the post-pagination pass off the hot path so
    /// page reuse (the stabilization halt) stays position-free, like the running
    /// header/footer and anchored floats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_borders: Option<ResolvedPageBorders>,
    /// The margin line numbers (`w:lnNumType`) for this page, in flow order.
    /// Empty unless this page's section declares line numbering; filled by the
    /// post-pagination pass off the hot path so page reuse (the stabilization
    /// halt) stays position-free, like the running header/footer, the page
    /// border, and anchored floats.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_numbers: Vec<PlacedLineNumber>,
    /// The section's watermark, stamped behind everything else on this page.
    /// `None` unless this page's section declares one; filled by the
    /// post-pagination `watermark` pass off the hot path so page reuse (the
    /// stabilization halt) stays position-free, like the running header/footer,
    /// the page border, and the line numbers.
    ///
    /// The pass is named in prose rather than linked: its module is private, like
    /// `line_number` and `page_border` beside it, so a doc link from this PUBLIC
    /// field is a hard error under the docs gate's `-D warnings`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<PlacedWatermark>,
    /// First model position on this page (the stabilization-halt key).
    pub start: ModelPos,
    /// One-past-last model position on this page.
    pub end: ModelPos,
    /// The half-open galley span this page covers — the flow provenance the
    /// incremental paginator uses to reuse pages (`43-…` §3.4).
    pub flow: FlowSpan,
}

impl Page {
    /// Drops everything the post-pagination passes put on this page, leaving the
    /// pagination result — the placed body content, the page geometry and the flow
    /// span — untouched.
    ///
    /// This is what lets a page be **reused** across an edit. The passes that
    /// place running content, page borders, anchored floats, margin line numbers
    /// and the watermark deliberately write to fields pagination leaves empty
    /// (each of those fields says so), so re-running them over a page produced by
    /// an earlier layout is correct as soon as their previous output is cleared.
    /// Without the clear, the second run would place a second header on the page.
    ///
    /// `footnotes` is cleared with them: the only paginator that fills it is the
    /// footnote one, and the incremental path that calls this runs only for a body
    /// with no footnote in it, where a fresh pagination leaves the field empty too.
    ///
    /// `O(1)` in page content — it drops, it does not walk.
    pub fn clear_post_pagination(&mut self) {
        self.header.clear();
        self.footer.clear();
        self.anchored.clear();
        self.footnotes.clear();
        self.line_numbers.clear();
        self.page_borders = None;
        self.watermark = None;
    }
}

/// The full paginated layout — the immutable result consumed by rendering and
/// hit-testing.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PaginatedLayout {
    /// Pages in order.
    pub pages: Vec<Page>,
}

impl PaginatedLayout {
    /// The number of pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}
