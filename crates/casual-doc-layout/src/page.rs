//! Paginated output — the immutable page fragments the paginator produces.
//!
//! Following the LayoutNG discipline (`42-…` §1.4), the paginator *produces*
//! immutable [`Page`]s and painting + hit-testing are read-only walks of them.
//! Each page records the model range it spans and the placement of every
//! fragment, which is exactly what makes incremental re-pagination (the
//! stabilization halt) and hit-testing cheap (`43-…` §3.5, §9).

use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;
use serde::{Deserialize, Serialize};

use crate::block::{BlockFragment, ResolvedEdge};
use crate::model::ModelPos;
use crate::text::TextBoxStroke;
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

/// A stroke (outline) painted for a floating shape or connector: a resolved color
/// and a width in twips.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AnchorStroke {
    /// The stroke color (RGBA).
    pub color: [u8; 4],
    /// The stroke width in twips (a hairline when `0`).
    pub width: Twip,
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
    },
    /// A rectangle (a group's background/foreground shape).
    Rectangle {
        /// The fill color (RGBA), if filled.
        fill: Option<[u8; 4]>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// An ellipse fitted to the anchor rectangle.
    Ellipse {
        /// The fill color (RGBA), if filled.
        fill: Option<[u8; 4]>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// A rounded rectangle with a resolved corner radius.
    RoundedRectangle {
        /// Corner radius in twips, clamped to half the shorter side.
        radius: Twip,
        /// The fill color (RGBA), if filled.
        fill: Option<[u8; 4]>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// A closed polygon whose page-local vertices are already resolved.
    Polygon {
        /// Vertices in path order. Typed presets currently produce three or four
        /// points; arbitrary package-provided paths never enter this primitive.
        points: Vec<Point>,
        /// The fill color (RGBA), if filled.
        fill: Option<[u8; 4]>,
        /// The outline, if stroked.
        stroke: Option<AnchorStroke>,
    },
    /// A straight line / connector, from `from` to `to` (page-local twips).
    Line {
        /// The line's start point.
        from: Point,
        /// The line's end point.
        to: Point,
        /// The line's stroke.
        stroke: AnchorStroke,
    },
    /// A text box: block content flowed through the shared pipeline, with an
    /// optional fill and border.
    TextBox {
        /// The flowed block fragments, positioned relative to the box's content
        /// origin (the box top-left inset by the internal margin).
        blocks: Vec<BlockFragment>,
        /// The box background fill (RGBA), if any.
        fill: Option<[u8; 4]>,
        /// The box border color and width, if any.
        border: Option<TextBoxStroke>,
        /// Resolved content offset and overflow clipping.
        content_layout: crate::text::TextBoxContentLayout,
    },
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
}

/// A column separator rule (`w:cols/@w:sep`) to paint on a page: a thin vertical
/// line centered in an inter-column gap, spanning its column band. Produced by the
/// column paginator and painted by [`compose_page`](crate::compose::compose_page);
/// it participates in neither flow nor hit-testing.
/// A resolved page-border frame (`w:pgBorders`) for one page: the outer frame
/// rectangle in page-local twips plus the resolved line for each present edge
/// (`None` where the section declares none for that side). Produced off the
/// pagination hot path by [`crate::page_border::resolve_page_borders`] and
/// painted by [`compose_page`](crate::compose::compose_page), like the running
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
    /// First model position on this page (the stabilization-halt key).
    pub start: ModelPos,
    /// One-past-last model position on this page.
    pub end: ModelPos,
    /// The half-open galley span this page covers — the flow provenance the
    /// incremental paginator uses to reuse pages (`43-…` §3.4).
    pub flow: FlowSpan,
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
