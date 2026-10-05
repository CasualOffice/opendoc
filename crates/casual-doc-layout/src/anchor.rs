//! Floating-object placement — the z-ordered float layer over body and bands.
//!
//! A floating object (`wp:anchor`) sits at an absolute position on the page rather
//! than in the inline flow. This is a **post-pagination** pass (like
//! [`crate::running::place_running_content`] and [`crate::paginate::resolve_fields`]):
//! it walks the document for anchored pictures, floating text boxes, and DrawingML
//! groups, finds the page each one's anchoring paragraph landed on, resolves each
//! against the page/margin/paragraph box, and records a [`PlacedAnchor`] with a
//! stacking key ([`AnchorZ`]) on that page. Composition then paints every float in
//! a single stable z-order, splicing the text layer at its own band.
//!
//! A **group** ([`WordprocessingGroup`]) is flattened here: its origin is resolved
//! like an anchored drawing (sized to the group `wp:extent`), then each child is
//! placed at `group_origin + transform(child.offset)` sized by its OWN extent — so
//! a grouped picture is never stretched to the group box, and the children paint in
//! document order (a shape can sit behind the picture and a later one in front).
//! Nested groups compose their transforms.
//!
//! A float is *positioned* against the geometry of the section that owns its
//! anchoring paragraph. Paragraph/line-relative side wrapping is handled during
//! ordinary flow; page/margin-relative square-family body wrapping is resolved
//! by the document driver's bounded fixed point. Contour wrapping still uses the
//! object's rectangular extent.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, DrawingAnchor, Extent, GroupChild, GroupShape, HorizontalAlign,
    HorizontalAnchor, HorizontalPosition, InlineNode, ReviewProjection, Rgba, SectionBoundary,
    SectionId, ShapeGeometry, ShapeStroke, VerticalAlign, VerticalAnchor, VerticalPosition,
    WordprocessingGroup, WrapDistances, WrapMode,
};
// Kept on a separate `use` line (anti-conflict): the outline dash style the anchor
// stroke now carries through to paint.
use casual_doc_model::v1::DashStyle;
// Its own `use` line on purpose: a new v1 import added into the sorted block
// above conflicts with every other branch doing the same.
use casual_doc_model::v1::{Fill, ShapeAdjustment};

use crate::block::BlockFragment;
// Separate `use` line to minimize import-block merge conflicts.
use crate::display::PathCommand;
use crate::display::ShapeTransform;
use crate::flow::flow_anchored_text_box;
use crate::page::{
    AnchorContent, AnchorStroke, AnchorZ, PaginatedLayout, PlacedAnchor, PlacedFragment,
};
use crate::paginate::PageConfig;
use crate::shape_guide::{GuideBox, guide_value};
// Own line (anti-conflict): the theme style resolution types.
use crate::text::{LineShaper, TextBoxStroke};
use crate::units::{
    Point, Rect, Size, Twip, emu_to_twip_extent, emu_to_twip_offset, emu_to_twip_rounded,
    twip_rounded,
};
// Own line (anti-conflict): the single wrap-side rule.
use crate::wrap_side::{WrapSides, wrap_sides};
use casual_doc_model::v1::{Definitions, StyleColor};

/// Places every floating object in the document (body and header/footer bands)
/// onto the pages their anchors landed on, with a resolved rectangle and stacking
/// key, ready for [`compose_page`](crate::compose::compose_page) to paint in
/// z-order.
///
/// `shaper` is needed because a floating (or grouped) text box flows its block
/// content through the *same* pipeline as the body ([`flow_header_footer`]), at the
/// box's own width, before being placed.
///
/// [`flow_header_footer`]: crate::flow::flow_header_footer
pub fn place_floats(
    layout: &mut PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
) {
    if layout.pages.is_empty() {
        return;
    }
    let mut ctx = FloatCtx {
        document,
        shaper,
        config,
        order: 0,
    };
    // Body floats: walk the body, resolving each float against the page its
    // anchoring paragraph landed on.
    let body_sections = body_section_ids(document, config.section);
    for (block, section) in document.body().iter().zip(body_sections) {
        if !block_is_placed(layout, block) {
            continue;
        }
        collect_block(layout, &mut ctx, block, None, PageScope::Body, section);
    }
    // Header/footer band floats: the SDS's floating objects live in the header
    // part. Each page's placed header/footer fragments are walked for the drawings
    // their paragraphs carry, resolved in the band's coordinate space. Collected
    // by page index first to avoid borrowing `layout` mutably while iterating it.
    let page_count = layout.pages.len();
    for page_index in 0..page_count {
        let section = layout.pages[page_index].section;
        for band in [PageScope::Header, PageScope::Footer] {
            let fragments = band_fragments(&layout.pages[page_index], band);
            for block in fragments {
                collect_band_block(layout, &mut ctx, &block, page_index, band, section);
            }
        }
    }
}

/// One top-level body float whose square-family wrap rectangle can exclude text
/// in paragraphs beyond its anchor paragraph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BodyWrapRect {
    pub(crate) page_index: usize,
    pub(crate) source: NodeId,
    pub(crate) rect: Rect,
    pub(crate) distances: WrapDistances,
    /// The authored `w:wrap@wrapText` of the float this rect came from, carried
    /// here because the exclusion side is an authored property and not a
    /// geometric guess (see [`crate::wrap_side`]).
    pub(crate) sides: WrapSides,
}

/// Resolves the page-local wrap rectangles of eligible top-level body floats
/// against an already paginated layout. The document driver converts these into
/// paragraph-local line exclusions and iterates to a fixed point.
pub(crate) fn body_wrap_rects(
    layout: &PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
    fit: crate::flow::MeasureFit,
) -> Vec<BodyWrapRect> {
    let ctx = FloatCtx {
        document,
        shaper,
        config,
        order: 0,
    };
    let sections = body_section_ids(document, config.section);
    let mut out = Vec::new();
    for (block, section) in document.body().iter().zip(sections) {
        let BlockNode::Paragraph(paragraph) = block else {
            continue;
        };
        if !block_is_placed(layout, block) {
            continue;
        }
        collect_body_wrap_inlines(
            layout,
            &ctx,
            paragraph.id,
            section,
            &paragraph.inlines,
            &mut out,
        );
    }
    // A positioned table (`w:tblpPr`) wraps text exactly as a square-wrapped
    // drawing does, so it contributes to the same exclusion set rather than to a
    // parallel one (`docs/109` row 64 / `105` FID-L-07).
    out.extend(crate::table_float::wrap_rects(
        layout, document, shaper, config, fit,
    ));
    out
}

fn collect_body_wrap_inlines(
    layout: &PaginatedLayout,
    ctx: &FloatCtx<'_>,
    paragraph: NodeId,
    section: SectionId,
    inlines: &[InlineNode],
    out: &mut Vec<BodyWrapRect>,
) {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) => {
                push_body_wrap_rect(
                    layout,
                    ctx,
                    paragraph,
                    section,
                    drawing.anchor.clone(),
                    drawing.extent,
                    None,
                    out,
                );
            }
            InlineNode::TextBox(text_box) if text_box.anchor.is_some() => {
                let anchor = text_box.anchor.clone().expect("guarded");
                let extent = text_box.extent.unwrap_or(Extent {
                    width_emu: 0,
                    height_emu: 0,
                });
                let refs = target(layout, ctx, Some(paragraph), PageScope::Body, section, None).1;
                let authored = resolve_anchor_rect(&anchor, extent, &refs);
                let flowed = flow_anchored_text_box(
                    ctx.document,
                    &text_box.blocks,
                    ctx.shaper,
                    authored.size,
                    &text_box.body_properties,
                );
                push_body_wrap_rect(
                    layout,
                    ctx,
                    paragraph,
                    section,
                    anchor,
                    extent,
                    Some(flowed.size),
                    out,
                );
            }
            InlineNode::Group(group) => {
                if let Some(anchor) = group.anchor.clone() {
                    push_body_wrap_rect(
                        layout,
                        ctx,
                        paragraph,
                        section,
                        anchor,
                        group.extent,
                        None,
                        out,
                    );
                }
            }
            InlineNode::Hyperlink(link) => {
                collect_body_wrap_inlines(layout, ctx, paragraph, section, &link.inlines, out)
            }
            InlineNode::Field(field) => {
                collect_body_wrap_inlines(layout, ctx, paragraph, section, &field.inlines, out)
            }
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                collect_body_wrap_inlines(layout, ctx, paragraph, section, &revision.inlines, out)
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => {
                collect_body_wrap_inlines(layout, ctx, paragraph, section, &sdt.inlines, out)
            }
            _ => {}
        }
    }
}

/// Whether the document anchors a floating object **anywhere** — any depth of
/// the body, or any header/footer part.
///
/// A **deliberately conservative superset** of what the float passes act on:
/// it ignores `behind_doc`, the wrap mode and the anchor kind, all of which
/// [`push_body_wrap_rect`] filters on, and it descends into tables, cells and
/// content controls that [`body_wrap_rects`] deliberately does not. So `false`
/// here proves both [`body_wrap_rects`] and [`place_floats`] are inert, while
/// `true` proves nothing.
///
/// That direction is the one the windowed driver needs (`docs/113` §7). Two
/// separate passes make floats non-page-local: `finish_pagination` re-flows
/// the whole body against computed wrap exclusions (which can move page
/// boundaries), and [`place_floats`] resolves an anchor against the page its
/// paragraph landed on while carrying a document-global z-order counter.
/// Neither can be evaluated for one window, so a document with any anchored
/// object is refused rather than windowed. Refusing a few documents that would
/// in fact have been safe costs a fallback; missing one costs a document that
/// paginates differently depending on how it was opened.
///
/// The superset relation is asserted by `no_anchored_object_means_no_floats`
/// in `tests/windowed_document.rs`.
#[must_use]
pub(crate) fn document_has_anchored_object(document: &Document) -> bool {
    let definitions = document.definitions();
    // A positioned table (`w:tblpPr`) is a float too: it is lifted out of the
    // galley and placed against the page, so a window cannot be paginated
    // independently of it either. Missing it here would let a windowed open
    // paginate a document differently from a full one.
    crate::table_float::has_floating_table(document)
        || document.body().iter().any(block_has_anchor)
        || definitions
            .headers
            .iter()
            .any(|(_, part)| part.blocks.iter().any(block_has_anchor))
        || definitions
            .footers
            .iter()
            .any(|(_, part)| part.blocks.iter().any(block_has_anchor))
}

/// The block half of [`document_has_anchored_object`], descending to any depth.
fn block_has_anchor(block: &BlockNode) -> bool {
    match block {
        BlockNode::Paragraph(paragraph) => inlines_have_anchor(&paragraph.inlines),
        BlockNode::Table(table) => table.rows.iter().any(|row| {
            row.cells
                .iter()
                .any(|cell| cell.blocks.iter().any(block_has_anchor))
        }),
        BlockNode::Sdt(sdt) => sdt.blocks.iter().any(block_has_anchor),
        BlockNode::AltChunk(_) => false,
    }
}

/// The inline half of [`document_has_anchored_object`], recursing through the
/// same transparent wrappers [`collect_body_wrap_inlines`] recurses through.
fn inlines_have_anchor(inlines: &[InlineNode]) -> bool {
    inlines.iter().any(|inline| match inline {
        InlineNode::AnchoredDrawing(_) => true,
        InlineNode::TextBox(text_box) => text_box.anchor.is_some(),
        InlineNode::Group(group) => group.anchor.is_some(),
        InlineNode::Hyperlink(link) => inlines_have_anchor(&link.inlines),
        InlineNode::Field(field) => inlines_have_anchor(&field.inlines),
        InlineNode::Revision(revision) => inlines_have_anchor(&revision.inlines),
        InlineNode::Sdt(sdt) => inlines_have_anchor(&sdt.inlines),
        _ => false,
    })
}

/// Whether a top-level body block that anchors a float was actually **laid
/// out** — the gate both body float passes open with.
///
/// Derived from the LAYOUT, deliberately, and not from the fold set: `locate`
/// falls back to page 1 and the page's margin box whenever it cannot find the
/// anchoring paragraph, so a float in a paragraph that produced no fragment
/// would paint on the first page of the document, which is exactly the visible
/// evidence of invisible content folding must not produce. Asking the layout
/// covers every reason a paragraph can be absent — a collapsed heading's range
/// today, and whatever else later — where a fold-set test would cover only one
/// and would have to be kept in step with the filter by hand.
///
/// Cheap to the point of free for the document that anchors nothing: the anchor
/// test comes first and is a walk of this block's own inlines, so a layout walk
/// happens only for a block that really does carry a float. `O(pages ×
/// fragments)` when it does, which is the same order `locate` is about to pay
/// for that float anyway.
fn block_is_placed(layout: &PaginatedLayout, block: &BlockNode) -> bool {
    let BlockNode::Paragraph(paragraph) = block else {
        // A table or a content control is descended into, and its nested
        // paragraphs answer for themselves; folding only ever hides whole
        // top-level blocks, so a visible table is not the case this gates.
        return true;
    };
    if !inlines_have_anchor(&paragraph.inlines) {
        return true;
    }
    layout.pages.iter().any(|page| {
        page.placed.iter().any(|placed| {
            find_paragraph_rect(
                &placed.fragment,
                placed.rect.origin,
                placed.rect.size.width,
                paragraph.id,
            )
            .is_some()
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn push_body_wrap_rect(
    layout: &PaginatedLayout,
    ctx: &FloatCtx<'_>,
    paragraph: NodeId,
    section: SectionId,
    anchor: DrawingAnchor,
    extent: Extent,
    flowed_size: Option<Size>,
    out: &mut Vec<BodyWrapRect>,
) {
    if anchor.behind_doc
        || !matches!(
            anchor.wrap,
            WrapMode::Square | WrapMode::Tight | WrapMode::Through
        )
    {
        return;
    }
    let (page_index, refs) = target(layout, ctx, Some(paragraph), PageScope::Body, section, None);
    let mut rect = resolve_anchor_rect(&anchor, extent, &refs);
    if let Some(size) = flowed_size {
        rect.size = size;
    }
    out.push(BodyWrapRect {
        page_index,
        source: paragraph,
        rect,
        distances: anchor.wrap_distances,
        sides: wrap_sides(&anchor),
    });
}

/// The header band height needed so the body content clears the section's
/// **positioned header floats** — the VML/DrawingML text boxes and images anchored
/// in the header part (the Chinese SDS positions its title/version/date boxes there
/// with page-relative offsets that reach *past* the top margin). Those floats are
/// placed by [`place_floats`] **after** pagination and so contribute nothing to the
/// flowed [`RunningContent::band_heights`](crate::running::RunningContent::band_heights);
/// without this the body starts at `margin_top` and collides with them.
///
/// Returns the extra header-band height to reserve, i.e. `max(0, extent -
/// header_distance)` where `extent` is the lowest painted bottom edge of any header
/// float (a text box uses the taller of its box and its *flowed* content, so an
/// overflowing box still reserves its true extent). Feeding this into
/// `PageConfig::header_height` makes `body_top = max(margin_top, header_distance +
/// header_height)` cover the header content extent (Word's geometry). Zero for a
/// document whose header has no positioned float (the common case), so it never
/// perturbs an ordinary header/footer document.
#[must_use]
pub fn header_float_reserve(
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
) -> Twip {
    let Some(section) = document.definitions().sections.first() else {
        return Twip::ZERO;
    };
    header_float_reserve_for_section(document, shaper, config, section)
}

/// Section-scoped form of [`header_float_reserve`], used by the document driver
/// when each section owns a distinct running-content band and page geometry.
#[must_use]
pub(crate) fn header_float_reserve_for_section(
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
    section: &SectionBoundary,
) -> Twip {
    let defs = document.definitions();
    let ctx = FloatCtx {
        document,
        shaper,
        config,
        order: 0,
    };
    // Header floats resolve their `paragraph`/`line` anchors against the header
    // band; `page`/`margin` anchors ignore it. Using the band rect (anchored at
    // `header_distance`, before its own height is known) is exact for the common
    // page-relative header boxes and a safe reference for the rest. The band is a
    // document-first-section reservation, so it resolves against the document
    // geometry (the first section owns the header definition).
    let geometry = AnchorGeometry::from_config(config);
    let refs = AnchorRefs::new(geometry, config.header_band(), geometry.margin_box());
    let mut bottom = Twip::ZERO;
    for reference in &section.headers {
        if let Some(hf) = defs.headers.get(&reference.reference) {
            for block in &hf.blocks {
                float_extent_block(&ctx, block, &refs, &mut bottom);
            }
        }
    }
    Twip((bottom.raw() - config.header_distance.raw()).max(0))
}

/// Accumulates the lowest painted bottom of the positioned floats in one header
/// block (recursing tables/SDT) into `bottom`.
fn float_extent_block(ctx: &FloatCtx<'_>, block: &BlockNode, refs: &AnchorRefs, bottom: &mut Twip) {
    match block {
        BlockNode::Paragraph(para) => float_extent_inlines(ctx, &para.inlines, refs, bottom),
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for nested in &cell.blocks {
                        float_extent_block(ctx, nested, refs, bottom);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for nested in &sdt.blocks {
                float_extent_block(ctx, nested, refs, bottom);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

/// Accumulates the lowest painted bottom of the positioned floats among `inlines`.
fn float_extent_inlines(
    ctx: &FloatCtx<'_>,
    inlines: &[InlineNode],
    refs: &AnchorRefs,
    bottom: &mut Twip,
) {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) => {
                let rect = resolve_anchor_rect(&drawing.anchor, drawing.extent, refs);
                *bottom = (*bottom).max(rect.bottom());
            }
            InlineNode::TextBox(text_box) if text_box.anchor.is_some() => {
                let anchor = text_box.anchor.clone().expect("guarded");
                let extent = text_box.extent.unwrap_or(Extent {
                    width_emu: 0,
                    height_emu: 0,
                });
                let rect = resolve_anchor_rect(&anchor, extent, refs);
                // Flow the box exactly as `place_floats` does, so the reserved
                // extent matches what will paint.
                let flowed = flow_anchored_text_box(
                    ctx.document,
                    &text_box.blocks,
                    ctx.shaper,
                    rect.size,
                    &text_box.body_properties,
                );
                // The painted bottom is the box bottom, or — when the box does not
                // clip vertically and its content overflows (the SDS version box is
                // one line taller than its authored height) — the content bottom,
                // which paints past the box (`compose` places it at
                // `rect.origin + content_layout.origin`, unclipped).
                let mut painted = rect.origin.y + flowed.size.height;
                if !flowed.content_layout.clip_vertical {
                    let content_h = flowed
                        .blocks
                        .iter()
                        .map(BlockFragment::height)
                        .fold(Twip::ZERO, |a, h| a + h);
                    painted =
                        painted.max(rect.origin.y + flowed.content_layout.origin.y + content_h);
                }
                *bottom = (*bottom).max(painted);
            }
            InlineNode::Group(group) => {
                if let Some(anchor) = group.anchor.clone() {
                    let origin = resolve_anchor_rect(&anchor, group.extent, refs).origin;
                    let mapper = GroupMapper::root(group);
                    group_extent_children(group, origin, &mapper, bottom);
                }
            }
            InlineNode::Hyperlink(link) => float_extent_inlines(ctx, &link.inlines, refs, bottom),
            InlineNode::Field(field) => float_extent_inlines(ctx, &field.inlines, refs, bottom),
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                float_extent_inlines(ctx, &revision.inlines, refs, bottom);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => float_extent_inlines(ctx, &sdt.inlines, refs, bottom),
            _ => {}
        }
    }
}

/// Accumulates the lowest bottom of a group's children (sized by their own extent,
/// composing nested-group transforms) into `bottom`.
fn group_extent_children(
    group: &WordprocessingGroup,
    origin: Point,
    mapper: &GroupMapper,
    bottom: &mut Twip,
) {
    for child in &group.children {
        match child {
            GroupChild::Picture(p) => {
                *bottom = (*bottom).max(mapper.child_rect(origin, p.offset, p.extent).bottom());
            }
            GroupChild::TextBox(t) => {
                *bottom = (*bottom).max(mapper.child_rect(origin, t.offset, t.extent).bottom());
            }
            GroupChild::Shape(s) => {
                *bottom = (*bottom).max(mapper.child_rect(origin, s.offset, s.extent).bottom());
            }
            GroupChild::Group(nested) => {
                let nested_mapper = mapper.compose(nested);
                group_extent_children(nested, origin, &nested_mapper, bottom);
            }
        }
    }
}

/// The shared context threaded through the float walk.
struct FloatCtx<'a> {
    document: &'a Document,
    shaper: &'a dyn LineShaper,
    config: &'a PageConfig,
    /// Monotonic document-order counter, the z-key tiebreaker + intra-group paint
    /// order.
    order: u32,
}

impl FloatCtx<'_> {
    fn next_order(&mut self) -> u32 {
        let order = self.order;
        self.order = self.order.saturating_add(1);
        order
    }

    /// Resolves anchor-only page geometry for `section`. The caller-supplied
    /// config is the deterministic fallback for a sectionless or malformed
    /// document; header/footer band reservation is deliberately irrelevant to
    /// OOXML page/margin reference frames.
    fn geometry(&self, section: SectionId) -> AnchorGeometry {
        anchor_geometry(self.document, self.config, section)
    }
}

/// The free form of [`FloatCtx::geometry`], so a caller that has no float walk
/// of its own (positioned tables, [`crate::table_float`]) resolves the very same
/// page/margin reference frames.
fn anchor_geometry(document: &Document, config: &PageConfig, section: SectionId) -> AnchorGeometry {
    document
        .definitions()
        .sections
        .iter()
        .find(|boundary| boundary.id == section)
        .map_or_else(
            || AnchorGeometry::from_config(config),
            AnchorGeometry::from_section,
        )
}

/// Resolves a body float's page-local rectangle when the caller has already
/// located its page, its text-flow anchor box, and its text column.
///
/// This is the seam positioned (floating) tables place through
/// ([`crate::table_float`]). It exists so a `w:tblpPr` table resolves against
/// **exactly** the reference frames, named alignments and signed offsets a
/// `wp:anchor` drawing does — one placement rule, not two. `anchor_box` is what
/// a `paragraph`/`line`-relative anchor resolves against (for a table, the
/// zero-height line at the flow position its rows were lifted from), and
/// `column` is the text column a `column`-relative anchor resolves against.
#[must_use]
pub(crate) fn resolve_body_float_rect(
    document: &Document,
    config: &PageConfig,
    section: SectionId,
    anchor_box: Rect,
    column: Rect,
    anchor: &DrawingAnchor,
    extent: Extent,
) -> Rect {
    let refs = AnchorRefs::new(
        anchor_geometry(document, config, section),
        anchor_box,
        column,
    );
    resolve_anchor_rect(anchor, extent, &refs)
}

/// Which page region a float's anchoring paragraph lives in.
#[derive(Clone, Copy)]
enum PageScope {
    Body,
    Header,
    Footer,
}

/// Mirrors `document_layout::section_break_points`: each top-level paragraph
/// carrying a section break belongs to the section it closes, while trailing
/// blocks belong to the final body-level section. Pre-filling with the final
/// section also gives malformed multi-section input the same deterministic
/// behavior as the paginator when a break is absent.
pub(crate) fn body_section_ids(document: &Document, fallback: SectionId) -> Vec<SectionId> {
    let body = document.body();
    let sections = &document.definitions().sections;
    let Some(final_section) = sections.last() else {
        return vec![fallback; body.len()];
    };
    let mut result = vec![final_section.id; body.len()];
    let mut start = 0usize;
    for (index, block) in body.iter().enumerate() {
        let BlockNode::Paragraph(paragraph) = block else {
            continue;
        };
        let Some(section_break) = paragraph.properties.section_break else {
            continue;
        };
        let boundary = sections
            .iter()
            .find(|section| section.id == section_break)
            .unwrap_or(&sections[0]);
        result[start..=index].fill(boundary.id);
        start = index.saturating_add(1);
    }
    result
}

/// Clones the placed fragments of a page's band (so the walk can borrow `layout`
/// mutably to push anchors without aliasing).
fn band_fragments(page: &crate::page::Page, band: PageScope) -> Vec<PlacedFragment> {
    match band {
        PageScope::Header => page.header.clone(),
        PageScope::Footer => page.footer.clone(),
        PageScope::Body => Vec::new(),
    }
}

// --- Body walk -------------------------------------------------------------

fn collect_block(
    layout: &mut PaginatedLayout,
    ctx: &mut FloatCtx<'_>,
    block: &BlockNode,
    _reserved: Option<NodeId>,
    scope: PageScope,
    section: SectionId,
) {
    match block {
        BlockNode::Paragraph(para) => {
            collect_inlines(
                layout,
                ctx,
                &para.inlines,
                Some(para.id),
                scope,
                section,
                None,
            );
        }
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for nested in &cell.blocks {
                        collect_block(layout, ctx, nested, None, scope, section);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for nested in &sdt.blocks {
                collect_block(layout, ctx, nested, None, scope, section);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

fn collect_inlines(
    layout: &mut PaginatedLayout,
    ctx: &mut FloatCtx<'_>,
    inlines: &[InlineNode],
    paragraph: Option<NodeId>,
    scope: PageScope,
    section: SectionId,
    known_target: Option<(usize, Rect)>,
) {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) => {
                let Some(media) = ctx.document.definitions().media.get(&drawing.media) else {
                    continue;
                };
                let media = media.part_name.clone();
                let z = AnchorZ {
                    relative_height: drawing.relative_height.unwrap_or(0),
                    order: ctx.next_order(),
                };
                let (page_index, refs) =
                    target(layout, ctx, paragraph, scope, section, known_target);
                let rect = resolve_anchor_rect(&drawing.anchor, drawing.extent, &refs);
                push(
                    layout,
                    page_index,
                    PlacedAnchor {
                        node: Some(drawing.id),
                        content: AnchorContent::Image {
                            media,
                            crop: drawing.crop,
                            border: shape_stroke(drawing.border),
                            opacity: drawing.opacity,
                        },
                        rect,
                        behind_doc: drawing.anchor.behind_doc,
                        z,
                        descr: drawing.descr.clone(),
                        transform: shape_transform(
                            rect,
                            drawing.flip_h,
                            drawing.flip_v,
                            drawing.rotation,
                        ),
                    },
                );
            }
            InlineNode::TextBox(text_box) if text_box.anchor.is_some() => {
                let anchor = text_box.anchor.clone().expect("guarded");
                let extent = text_box.extent.unwrap_or(Extent {
                    width_emu: 0,
                    height_emu: 0,
                });
                let z = AnchorZ {
                    relative_height: text_box.relative_height.unwrap_or(0),
                    order: ctx.next_order(),
                };
                let (page_index, refs) =
                    target(layout, ctx, paragraph, scope, section, known_target);
                let mut rect = resolve_anchor_rect(&anchor, extent, &refs);
                let flowed = flow_anchored_text_box(
                    ctx.document,
                    &text_box.blocks,
                    ctx.shaper,
                    rect.size,
                    &text_box.body_properties,
                );
                rect.size = flowed.size;
                push(
                    layout,
                    page_index,
                    PlacedAnchor {
                        node: Some(text_box.id),
                        content: AnchorContent::TextBox {
                            blocks: flowed.blocks,
                            fill: text_box.fill.clone(),
                            border: text_box.border.map(text_box_stroke),
                            content_layout: flowed.content_layout,
                            // A standalone `TextBox` models no geometry yet, so
                            // there is nothing but the rectangle to paint. The
                            // importer REPORTS the dropped preset rather than
                            // letting it vanish.
                            backdrop: None,
                        },
                        rect,
                        behind_doc: anchor.behind_doc,
                        z,
                        descr: None,
                        // Rotated text-box CONTENT is a follow-up; the box paints
                        // axis-aligned for now.
                        transform: None,
                    },
                );
            }
            InlineNode::Group(group) => {
                let Some(anchor) = group.anchor.clone() else {
                    continue;
                };
                let (page_index, refs) =
                    target(layout, ctx, paragraph, scope, section, known_target);
                let group_box = resolve_anchor_rect(&anchor, group.extent, &refs);
                let origin = group_box.origin;
                let relative_height = group.relative_height.unwrap_or(0);
                let behind_doc = anchor.behind_doc;
                let mapper = GroupMapper::root(group);
                // `GroupTransform`'s rot/flip were modelled and round-tripped but
                // never applied, so a rotated Word group painted unrotated.
                let pose = GroupPose::about(
                    rect_center(group_box),
                    group.transform.rotation.unwrap_or(0),
                    group.transform.flip_h,
                    group.transform.flip_v,
                );
                place_group_children(
                    layout,
                    ctx,
                    group,
                    page_index,
                    origin,
                    &mapper,
                    pose,
                    relative_height,
                    behind_doc,
                );
            }
            InlineNode::Hyperlink(link) => {
                collect_inlines(
                    layout,
                    ctx,
                    &link.inlines,
                    paragraph,
                    scope,
                    section,
                    known_target,
                );
            }
            InlineNode::Field(field) => {
                collect_inlines(
                    layout,
                    ctx,
                    &field.inlines,
                    paragraph,
                    scope,
                    section,
                    known_target,
                );
            }
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                collect_inlines(
                    layout,
                    ctx,
                    &revision.inlines,
                    paragraph,
                    scope,
                    section,
                    known_target,
                );
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => {
                collect_inlines(
                    layout,
                    ctx,
                    &sdt.inlines,
                    paragraph,
                    scope,
                    section,
                    known_target,
                );
            }
            _ => {}
        }
    }
}

/// Places every child of a group at `origin + mapper(child.offset)`, each sized by
/// its own extent, in document (paint) order. Nested groups compose the mapper.
#[allow(clippy::too_many_arguments)]
fn place_group_children(
    layout: &mut PaginatedLayout,
    ctx: &mut FloatCtx<'_>,
    group: &WordprocessingGroup,
    page_index: usize,
    origin: Point,
    mapper: &GroupMapper,
    pose: GroupPose,
    relative_height: u32,
    behind_doc: bool,
) {
    for child in &group.children {
        match child {
            GroupChild::Picture(picture) => {
                let Some(media) = ctx.document.definitions().media.get(&picture.media) else {
                    continue;
                };
                let media = media.part_name.clone();
                let rect =
                    pose.reposition(mapper.child_rect(origin, picture.offset, picture.extent));
                let (rotation, flip_h, flip_v) =
                    pose.compose_child(picture.rotation, picture.flip_h, picture.flip_v);
                let z = AnchorZ {
                    relative_height,
                    order: ctx.next_order(),
                };
                push(
                    layout,
                    page_index,
                    PlacedAnchor {
                        // A group child's identity, so a click on it resolves to
                        // the model like any other object. Left as `None` before,
                        // which is why grouped content rendered but could not be
                        // selected, entered, or edited at all.
                        node: Some(picture.id),
                        content: AnchorContent::Image {
                            media,
                            crop: picture.crop,
                            border: shape_stroke(picture.border),
                            opacity: picture.opacity,
                        },
                        rect,
                        behind_doc,
                        z,
                        descr: picture.descr.clone(),
                        transform: shape_transform(rect, flip_h, flip_v, rotation),
                    },
                );
            }
            GroupChild::TextBox(text_box) => {
                // Repositioned but NOT reoriented. `compose_anchor` hands
                // `anchor.transform` to the box's backdrop, fill and border but
                // composes its text blocks outside any layer, so a rotation here
                // would spin the chrome and leave the glyphs behind — visibly worse
                // than an unrotated box in the right place. Rotated text-box content
                // is the already-tracked follow-up this waits on.
                let mut rect =
                    pose.reposition(mapper.child_rect(origin, text_box.offset, text_box.extent));
                let flowed = flow_anchored_text_box(
                    ctx.document,
                    &text_box.blocks,
                    ctx.shaper,
                    rect.size,
                    &text_box.body_properties,
                );
                rect.size = flowed.size;
                let z = AnchorZ {
                    relative_height,
                    order: ctx.next_order(),
                };
                // "Modeled is not shipped": a text-bearing `wps:wsp` whose
                // preset is an ellipse or a star must PAINT as one, with its
                // text inside. The backdrop comes from the same mapping a
                // text-free shape uses, and takes the fill/outline with it so
                // the rectangular box path below paints nothing over it.
                let backdrop = text_box_backdrop(text_box, rect);
                push(
                    layout,
                    page_index,
                    PlacedAnchor {
                        node: Some(text_box.id),
                        content: AnchorContent::TextBox {
                            blocks: flowed.blocks,
                            fill: backdrop.is_none().then(|| text_box.fill.clone()).flatten(),
                            border: backdrop
                                .is_none()
                                .then(|| text_box.border.map(text_box_stroke))
                                .flatten(),
                            content_layout: flowed.content_layout,
                            backdrop,
                        },
                        rect,
                        behind_doc,
                        z,
                        descr: None,
                        // Rotated text-box CONTENT is a follow-up; the box paints
                        // axis-aligned for now.
                        transform: None,
                    },
                );
            }
            GroupChild::Shape(shape) => {
                let rect = pose.reposition(mapper.child_rect(origin, shape.offset, shape.extent));
                let (rotation, flip_h, flip_v) =
                    pose.compose_child(shape.rotation, shape.flip_h, shape.flip_v);
                let z = AnchorZ {
                    relative_height,
                    order: ctx.next_order(),
                };
                // A custom geometry outranks the preset enum: the importer only
                // attaches a path when the authored `a:custGeom` is inside the
                // drawable subset, and `geometry` stays `Other` beside it
                // (docs/119 §6).
                let (fill, stroke) = themed_appearance(shape, ctx.document.definitions());
                let content = if let Some(path) = shape.path.as_ref() {
                    custom_path_content(path, rect, fill, stroke)
                } else {
                    preset_geometry_content(
                        shape.geometry,
                        &shape.adjustments,
                        rect,
                        fill.as_ref(),
                        stroke,
                    )
                };
                push(
                    layout,
                    page_index,
                    PlacedAnchor {
                        node: Some(shape.id),
                        content,
                        rect,
                        behind_doc,
                        z,
                        descr: None,
                        transform: shape_transform(rect, flip_h, flip_v, rotation),
                    },
                );
            }
            GroupChild::Group(nested) => {
                let nested_mapper = mapper.compose(nested);
                // The nested group's own `a:xfrm` rot/flip act about ITS box centre,
                // measured in the parent's UNROTATED space; the parent pose then
                // applies on top, which is why these compose rather than add.
                let nested_box =
                    mapper.child_rect(origin, nested.transform.offset, nested.transform.extent);
                let nested_pose = pose.after(GroupPose::about(
                    rect_center(nested_box),
                    nested.transform.rotation.unwrap_or(0),
                    nested.transform.flip_h,
                    nested.transform.flip_v,
                ));
                place_group_children(
                    layout,
                    ctx,
                    nested,
                    page_index,
                    origin,
                    &nested_mapper,
                    nested_pose,
                    relative_height,
                    behind_doc,
                );
            }
        }
    }
}

/// Resolves the common `roundRect` `adj` guide. DrawingML uses 100000-based
/// percentages; the preset default is 16667 (one sixth of the shorter side).
fn rounded_rectangle_radius(adjustments: &[ShapeAdjustment], rect: Rect) -> Twip {
    let guides = GuideBox::new(
        f64::from(rect.size.width.raw()),
        f64::from(rect.size.height.raw()),
    );
    let adjustment = adjustment_value(adjustments, "adj", 16_667, guides).clamp(0, 50_000);
    let shorter = i64::from(rect.size.width.raw().min(rect.size.height.raw()).max(0));
    Twip((shorter * adjustment / 100_000).clamp(0, i64::from(i32::MAX)) as i32)
}

/// Resolves one `a:avLst` adjustment guide by name, falling back to the preset's
/// documented default when the document authors none, or authors one that cannot be
/// read at all. A guide that COMPUTES its value — `*/ h 1 2`, say — is evaluated
/// through [`crate::shape_guide`] rather than passed over, which it previously was:
/// every guide in the modeled preset set is a literal `val N`, so an authored formula
/// hit the default and the shape drew with proportions nobody chose.
///
/// Complexity: O(g) over the shape's own guides, bounded by
/// `MAX_SHAPE_ADJUSTMENTS` (32) at import — O(1) in document size.
fn adjustment_value(
    adjustments: &[ShapeAdjustment],
    name: &str,
    default: i64,
    shape: GuideBox,
) -> i64 {
    // Through the formula evaluator rather than a `val ` prefix match, so an
    // authored guide that computes its value is honoured instead of silently losing
    // to the preset default. `val N` is simply the one-operand case.
    //
    // The unit convention is unchanged: a preset's `adj` is a 100000-based fraction
    // and the caller scales it the same way whether it arrived as a literal or as an
    // expression.
    guide_value(adjustments, name, shape)
        .map(|value| value.round() as i64)
        .unwrap_or(default)
}

/// A preset shape's bounding box in the coordinate names ECMA-376's preset
/// geometry definitions use (Part 1 §20.1.9.18): `l`/`t`/`r`/`b` edges,
/// `hc`/`vc` centers, `w`/`h` extents, and `ss` — the "shortest side", the
/// reference length nearly every adjustment guide is expressed against.
#[derive(Clone, Copy)]
struct PresetBox {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
    w: f64,
    h: f64,
    hc: f64,
    vc: f64,
    ss: f64,
}

impl PresetBox {
    fn new(rect: Rect) -> Self {
        let l = f64::from(rect.origin.x.raw());
        let t = f64::from(rect.origin.y.raw());
        let w = f64::from(rect.size.width.raw());
        let h = f64::from(rect.size.height.raw());
        Self {
            l,
            t,
            r: f64::from(rect.right().raw()),
            b: f64::from(rect.bottom().raw()),
            w,
            h,
            // Whole-twip halves, so a preset's midpoint lands exactly where the
            // triangle and diamond already put theirs on an odd-width box.
            hc: l + f64::from(rect.size.width.raw() / 2),
            vc: t + f64::from(rect.size.height.raw() / 2),
            ss: w.min(h),
        }
    }

    /// One vertex, rounded to whole twips and clamped into the coordinate range.
    fn at(self, x: f64, y: f64) -> Point {
        Point::new(twip_rounded(x), twip_rounded(y))
    }
}

/// The closed outline of a polygonal preset geometry, in page-local twips, or
/// `None` for the presets that are not polygons (the rectangle family, the
/// ellipse, the line, and the untyped `Other` that paints its bounding box).
///
/// Exhaustive on purpose: a [`ShapeGeometry`] variant added without an outline
/// here fails to compile, which is the point — a variant that fell through to a
/// bounding rectangle would be `Other` with a longer name.
///
/// Winding is clockwise from the vertex ECMA-376's own `a:pathLst` starts at, so
/// a filled and a stroked preset trace the same outline. Each shape's
/// adjustment guides and preset defaults are documented on the [`ShapeGeometry`]
/// variant.
///
/// Complexity: O(1) — a fixed vertex count per preset (at most the 12 of
/// `plus`), over at most 32 authored guides.
#[allow(clippy::too_many_lines)]
fn preset_polygon(
    geometry: ShapeGeometry,
    adjustments: &[ShapeAdjustment],
    rect: Rect,
) -> Option<Vec<Point>> {
    let g = PresetBox::new(rect);
    // Guide formulas resolve in the shape's OWN space, so the environment carries
    // the extents only, never the absolute page position.
    let guides = GuideBox::new(g.w, g.h);
    // `ss`-relative guide lengths are clamped to the box so a hostile or simply
    // out-of-range `a:avLst` cannot push a vertex outside the shape.
    let along = |value: f64, span: f64| value.clamp(0.0, span.max(0.0));
    Some(match geometry {
        ShapeGeometry::Rectangle
        | ShapeGeometry::RoundRectangle
        | ShapeGeometry::Ellipse
        | ShapeGeometry::Line
        | ShapeGeometry::Other => return None,
        ShapeGeometry::Triangle => vec![g.at(g.hc, g.t), g.at(g.r, g.b), g.at(g.l, g.b)],
        ShapeGeometry::RightTriangle => vec![g.at(g.l, g.t), g.at(g.r, g.b), g.at(g.l, g.b)],
        ShapeGeometry::Diamond => vec![
            g.at(g.hc, g.t),
            g.at(g.r, g.vc),
            g.at(g.hc, g.b),
            g.at(g.l, g.vc),
        ],
        ShapeGeometry::Pentagon => {
            // The regular pentagon inscribed so it fills the box: apex at the
            // top edge, the shoulders at 0.381966·h (the apex-to-shoulder share
            // of the 1 + cos36° apex-to-base span) and the feet on the bottom
            // edge at 0.190983·w / 0.809017·w.
            let shoulder = g.t + 0.381_966 * g.h;
            vec![
                g.at(g.hc, g.t),
                g.at(g.r, shoulder),
                g.at(g.l + 0.809_017 * g.w, g.b),
                g.at(g.l + 0.190_983 * g.w, g.b),
                g.at(g.l, shoulder),
            ]
        }
        ShapeGeometry::Hexagon => {
            let inset = along(
                g.ss * adjustment_value(adjustments, "adj", 25_000, guides) as f64 / 100_000.0,
                g.w / 2.0,
            );
            vec![
                g.at(g.l, g.vc),
                g.at(g.l + inset, g.t),
                g.at(g.r - inset, g.t),
                g.at(g.r, g.vc),
                g.at(g.r - inset, g.b),
                g.at(g.l + inset, g.b),
            ]
        }
        ShapeGeometry::Octagon => {
            let cut = g.ss
                * adjustment_value(adjustments, "adj", 29_289, guides).clamp(0, 50_000) as f64
                / 100_000.0;
            let (dx, dy) = (along(cut, g.w / 2.0), along(cut, g.h / 2.0));
            vec![
                g.at(g.l, g.t + dy),
                g.at(g.l + dx, g.t),
                g.at(g.r - dx, g.t),
                g.at(g.r, g.t + dy),
                g.at(g.r, g.b - dy),
                g.at(g.r - dx, g.b),
                g.at(g.l + dx, g.b),
                g.at(g.l, g.b - dy),
            ]
        }
        ShapeGeometry::Star5 => star_points(g, &PENTAGRAM, star_ratio(adjustments, 19_098, guides)),
        ShapeGeometry::Star4 => {
            star_points(g, &FOUR_POINT_STAR, star_ratio(adjustments, 12_500, guides))
        }
        ShapeGeometry::RightArrow | ShapeGeometry::LeftArrow => {
            let shaft = along(
                g.h * adjustment_value(adjustments, "adj1", 50_000, guides).clamp(0, 100_000)
                    as f64
                    / 200_000.0,
                g.h / 2.0,
            );
            let head = along(
                g.ss * adjustment_value(adjustments, "adj2", 50_000, guides).max(0) as f64
                    / 100_000.0,
                g.w,
            );
            let (y1, y2) = (g.vc - shaft, g.vc + shaft);
            if geometry == ShapeGeometry::RightArrow {
                let neck = g.r - head;
                vec![
                    g.at(g.l, y1),
                    g.at(neck, y1),
                    g.at(neck, g.t),
                    g.at(g.r, g.vc),
                    g.at(neck, g.b),
                    g.at(neck, y2),
                    g.at(g.l, y2),
                ]
            } else {
                let neck = g.l + head;
                vec![
                    g.at(g.l, g.vc),
                    g.at(neck, g.t),
                    g.at(neck, y1),
                    g.at(g.r, y1),
                    g.at(g.r, y2),
                    g.at(neck, y2),
                    g.at(neck, g.b),
                ]
            }
        }
        ShapeGeometry::UpArrow | ShapeGeometry::DownArrow => {
            let shaft = along(
                g.w * adjustment_value(adjustments, "adj1", 50_000, guides).clamp(0, 100_000)
                    as f64
                    / 200_000.0,
                g.w / 2.0,
            );
            let head = along(
                g.ss * adjustment_value(adjustments, "adj2", 50_000, guides).max(0) as f64
                    / 100_000.0,
                g.h,
            );
            let (x1, x2) = (g.hc - shaft, g.hc + shaft);
            if geometry == ShapeGeometry::UpArrow {
                let neck = g.t + head;
                vec![
                    g.at(g.l, neck),
                    g.at(g.hc, g.t),
                    g.at(g.r, neck),
                    g.at(x2, neck),
                    g.at(x2, g.b),
                    g.at(x1, g.b),
                    g.at(x1, neck),
                ]
            } else {
                let neck = g.b - head;
                vec![
                    g.at(x1, g.t),
                    g.at(x2, g.t),
                    g.at(x2, neck),
                    g.at(g.r, neck),
                    g.at(g.hc, g.b),
                    g.at(g.l, neck),
                    g.at(x1, neck),
                ]
            }
        }
        ShapeGeometry::LeftRightArrow => {
            let shaft = along(
                g.h * adjustment_value(adjustments, "adj1", 50_000, guides).clamp(0, 100_000)
                    as f64
                    / 200_000.0,
                g.h / 2.0,
            );
            let head = along(
                g.ss * adjustment_value(adjustments, "adj2", 50_000, guides).max(0) as f64
                    / 100_000.0,
                g.w / 2.0,
            );
            let (y1, y2) = (g.vc - shaft, g.vc + shaft);
            let (x1, x2) = (g.l + head, g.r - head);
            vec![
                g.at(g.l, g.vc),
                g.at(x1, g.t),
                g.at(x1, y1),
                g.at(x2, y1),
                g.at(x2, g.t),
                g.at(g.r, g.vc),
                g.at(x2, g.b),
                g.at(x2, y2),
                g.at(x1, y2),
                g.at(x1, g.b),
            ]
        }
        ShapeGeometry::Parallelogram => {
            let lean = along(
                g.ss * adjustment_value(adjustments, "adj", 25_000, guides).max(0) as f64
                    / 100_000.0,
                g.w,
            );
            vec![
                g.at(g.l, g.b),
                g.at(g.l + lean, g.t),
                g.at(g.r, g.t),
                g.at(g.r - lean, g.b),
            ]
        }
        ShapeGeometry::Trapezoid => {
            let inset = along(
                g.ss * adjustment_value(adjustments, "adj", 25_000, guides).max(0) as f64
                    / 100_000.0,
                g.w / 2.0,
            );
            vec![
                g.at(g.l, g.b),
                g.at(g.l + inset, g.t),
                g.at(g.r - inset, g.t),
                g.at(g.r, g.b),
            ]
        }
        ShapeGeometry::Chevron => {
            let point = along(
                g.ss * adjustment_value(adjustments, "adj", 50_000, guides).max(0) as f64
                    / 100_000.0,
                g.w,
            );
            vec![
                g.at(g.l, g.t),
                g.at(g.r - point, g.t),
                g.at(g.r, g.vc),
                g.at(g.r - point, g.b),
                g.at(g.l, g.b),
                g.at(g.l + point, g.vc),
            ]
        }
        ShapeGeometry::HomePlate => {
            let point = along(
                g.ss * adjustment_value(adjustments, "adj", 50_000, guides).max(0) as f64
                    / 100_000.0,
                g.w,
            );
            vec![
                g.at(g.l, g.t),
                g.at(g.r - point, g.t),
                g.at(g.r, g.vc),
                g.at(g.r - point, g.b),
                g.at(g.l, g.b),
            ]
        }
        ShapeGeometry::Plus => {
            let arm = g.ss
                * adjustment_value(adjustments, "adj", 25_000, guides).clamp(0, 50_000) as f64
                / 100_000.0;
            let (dx, dy) = (along(arm, g.w / 2.0), along(arm, g.h / 2.0));
            let (x1, x2) = (g.l + dx, g.r - dx);
            let (y1, y2) = (g.t + dy, g.b - dy);
            vec![
                g.at(g.l, y1),
                g.at(x1, y1),
                g.at(x1, g.t),
                g.at(x2, g.t),
                g.at(x2, y1),
                g.at(g.r, y1),
                g.at(g.r, y2),
                g.at(x2, y2),
                g.at(x2, g.b),
                g.at(x1, g.b),
                g.at(x1, y2),
                g.at(g.l, y2),
            ]
        }
    })
}

/// A star preset's shape, independent of the box it is drawn in: the unit-circle
/// direction of each outer point (apex first, clockwise) interleaved with the
/// directions of the notches between them, plus the normalization that makes the
/// outer points touch the bounding box.
struct StarShape {
    /// `(cos, sin)` of each outer point, y-downward, apex first and clockwise.
    outer: &'static [(f64, f64)],
    /// `(cos, sin)` of each inner notch, in the same order, starting with the
    /// notch that follows the apex.
    inner: &'static [(f64, f64)],
    /// Fraction of the half-width the outer circle is scaled to, so the star's
    /// widest points land on the left and right edges.
    x_scale: f64,
    /// Fraction of the height the outer circle's radius is scaled to.
    y_scale: f64,
    /// Where the star's center sits, as a fraction of the height from the top.
    y_center: f64,
}

/// The five-pointed star (`star5`): outer points every 72° from the apex,
/// notches on the 36° bisectors. The scales place the apex on the top edge, the
/// arms on the side edges and the legs on the bottom edge — the same
/// normalization ECMA-376 spells as the `hf`/`vf` guides.
static PENTAGRAM: StarShape = StarShape {
    outer: &[
        (0.0, -1.0),
        (0.951_057, -0.309_017),
        (0.587_785, 0.809_017),
        (-0.587_785, 0.809_017),
        (-0.951_057, -0.309_017),
    ],
    inner: &[
        (0.587_785, -0.809_017),
        (0.951_057, 0.309_017),
        (0.0, 1.0),
        (-0.951_057, 0.309_017),
        (-0.587_785, -0.809_017),
    ],
    x_scale: 1.051_462,
    y_scale: 0.552_786,
    y_center: 0.552_786,
};

/// The 45 degree direction cosine a four-pointed star's notches sit on.
const DIAGONAL: f64 = core::f64::consts::FRAC_1_SQRT_2;

/// The four-pointed star (`star4`): outer points on the four edge midpoints,
/// notches on the diagonals. No normalization is needed — the outer points
/// already touch the box.
static FOUR_POINT_STAR: StarShape = StarShape {
    outer: &[(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)],
    inner: &[
        (DIAGONAL, -DIAGONAL),
        (DIAGONAL, DIAGONAL),
        (-DIAGONAL, DIAGONAL),
        (-DIAGONAL, -DIAGONAL),
    ],
    x_scale: 1.0,
    y_scale: 0.5,
    y_center: 0.5,
};

/// The `adj` guide of a star preset as the inner/outer radius ratio it encodes
/// (`adj / 50000`, clamped to the closed unit range).
fn star_ratio(adjustments: &[ShapeAdjustment], default: i64, guides: GuideBox) -> f64 {
    adjustment_value(adjustments, "adj", default, guides).clamp(0, 50_000) as f64 / 50_000.0
}

/// Interleaves a star's outer points and inner notches into one closed outline.
///
/// Complexity: O(p) in the star's point count — 5 or 4.
fn star_points(g: PresetBox, shape: &StarShape, ratio: f64) -> Vec<Point> {
    let (rx, ry) = (g.w / 2.0 * shape.x_scale, g.h * shape.y_scale);
    let (cx, cy) = (g.hc, g.t + g.h * shape.y_center);
    let mut points = Vec::with_capacity(shape.outer.len() * 2);
    for (index, (ox, oy)) in shape.outer.iter().enumerate() {
        points.push(g.at(cx + rx * ox, cy + ry * oy));
        let (ix, iy) = shape.inner[index];
        points.push(g.at(cx + rx * ratio * ix, cy + ry * ratio * iy));
    }
    points
}

/// Maps a preset geometry onto the [`AnchorContent`] that paints it inside
/// `rect`.
///
/// This is the ONE place a [`ShapeGeometry`] becomes something paintable. A
/// `wps:wsp` that carries text and one that does not differ only in what is
/// drawn *on top*, so both come through here; a second copy for text boxes
/// would be a second answer to "what does `star5` look like", and the two would
/// drift.
///
/// Complexity: O(1) in document size — see [`preset_polygon`].
fn preset_geometry_content(
    geometry: ShapeGeometry,
    adjustments: &[ShapeAdjustment],
    rect: Rect,
    fill: Option<&Fill>,
    stroke: Option<ShapeStroke>,
) -> AnchorContent {
    if let Some(points) = preset_polygon(geometry, adjustments, rect) {
        return AnchorContent::Path {
            commands: polyline_commands(&points),
            closed: true,
            fill: fill.cloned(),
            stroke: shape_stroke(stroke),
        };
    }
    match geometry {
        ShapeGeometry::Line => AnchorContent::Line {
            from: rect.origin,
            to: Point::new(rect.right(), rect.bottom()),
            // A line without an explicit stroke still draws a hairline in its
            // fill color (Word's connector default).
            stroke: shape_stroke(stroke).unwrap_or(AnchorStroke {
                color: fill.map_or([0, 0, 0, 255], |fill| rgba(fill.flat_color())),
                width: Twip::ZERO,
                dash: DashStyle::Solid,
            }),
            head_end: stroke.and_then(|s| s.head_end),
            tail_end: stroke.and_then(|s| s.tail_end),
        },
        ShapeGeometry::Ellipse => AnchorContent::Ellipse {
            fill: fill.cloned(),
            stroke: shape_stroke(stroke),
        },
        ShapeGeometry::RoundRectangle => AnchorContent::RoundedRectangle {
            radius: rounded_rectangle_radius(adjustments, rect),
            fill: fill.cloned(),
            stroke: shape_stroke(stroke),
        },
        // A preset this build has no primitive for, and a custom geometry
        // outside the drawable subset, both paint their bounding rectangle.
        // ONLYOFFICE paints NOTHING here (`Geometry.draw` early-returns on an
        // invalid geometry); we deliberately differ, because silently erasing
        // every unsupported freeform is a larger change than showing a box where
        // an object is, and Word does not erase them either. Weighed and
        // recorded in docs/119 §6 "Rejected". Every remaining variant is
        // polygonal and returned above.
        _ => AnchorContent::Rectangle {
            fill: fill.cloned(),
            stroke: shape_stroke(stroke),
        },
    }
}

/// The shape a grouped text box paints behind its text, or `None` when the box
/// is the plain rectangle whose fill and outline the text-box content itself
/// already draws.
///
/// Complexity: O(1) in document size — see [`preset_polygon`].
fn text_box_backdrop(
    text_box: &casual_doc_model::v1::GroupTextBox,
    rect: Rect,
) -> Option<Box<AnchorContent>> {
    if matches!(
        text_box.geometry,
        ShapeGeometry::Rectangle | ShapeGeometry::Other
    ) {
        return None;
    }
    Some(Box::new(preset_geometry_content(
        text_box.geometry,
        &text_box.adjustments,
        rect,
        text_box.fill.as_ref(),
        text_box.border,
    )))
}

// --- Band walk -------------------------------------------------------------

/// Walks one placed band fragment (a header/footer paragraph or table row) for the
/// floats its paragraphs carry, resolving them in the band's coordinate space (the
/// band fragment's placed origin is the reference for `paragraph`/`line` anchors).
fn collect_band_block(
    layout: &mut PaginatedLayout,
    ctx: &mut FloatCtx<'_>,
    placed: &PlacedFragment,
    page_index: usize,
    band: PageScope,
    section: SectionId,
) {
    let mut paragraphs = Vec::new();
    collect_paragraph_rects(
        &placed.fragment,
        placed.rect.origin,
        placed.rect.size.width,
        &mut paragraphs,
    );
    for (id, rect) in paragraphs {
        if let Some(BlockNode::Paragraph(para)) = find_paragraph(ctx.document, id, band) {
            collect_inlines(
                layout,
                ctx,
                &para.inlines,
                Some(para.id),
                band,
                section,
                Some((page_index, rect)),
            );
        }
    }
}

/// Finds the header/footer source paragraph with `id` in the document's band
/// definitions (headers or footers), so its floating inlines can be resolved.
fn find_paragraph(document: &Document, id: NodeId, band: PageScope) -> Option<&BlockNode> {
    let defs = document.definitions();
    // Iterated lazily rather than collected: this runs once per band paragraph
    // per placed band fragment per page, and collecting every header (or footer)
    // block into a fresh `Vec` first made that an allocation per lookup — tens of
    // thousands of throwaway vectors on a long document, for a search that stops
    // at the first match anyway.
    let stores: Box<dyn Iterator<Item = &BlockNode>> = match band {
        PageScope::Header => Box::new(defs.headers.iter().flat_map(|(_, hf)| &hf.blocks)),
        PageScope::Footer => Box::new(defs.footers.iter().flat_map(|(_, hf)| &hf.blocks)),
        PageScope::Body => return None,
    };
    for block in stores {
        if let Some(found) = find_block_paragraph(block, id) {
            return Some(found);
        }
    }
    None
}

fn find_block_paragraph(block: &BlockNode, id: NodeId) -> Option<&BlockNode> {
    match block {
        BlockNode::Paragraph(para) if para.id == id => Some(block),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .flat_map(|cell| &cell.blocks)
            .find_map(|nested| find_block_paragraph(nested, id)),
        BlockNode::Sdt(sdt) => sdt
            .blocks
            .iter()
            .find_map(|nested| find_block_paragraph(nested, id)),
        _ => None,
    }
}

// --- Placement helpers -----------------------------------------------------

/// Resolves the page index and reference boxes a float anchored in `paragraph`
/// resolves against. Body floats use the page geometry; band floats offset the
/// `paragraph`/`line` reference to the band fragment's placed origin.
fn target(
    layout: &PaginatedLayout,
    ctx: &FloatCtx<'_>,
    paragraph: Option<NodeId>,
    scope: PageScope,
    section: SectionId,
    known_target: Option<(usize, Rect)>,
) -> (usize, AnchorRefs) {
    let geometry = ctx.geometry(section);
    if let Some((page_index, paragraph_box)) = known_target {
        let column_box = geometry.margin_box();
        return (
            page_index,
            AnchorRefs::new(geometry, paragraph_box, column_box),
        );
    }
    let (page_index, paragraph_box, column_box) =
        locate(layout, paragraph, geometry.margin_box(), scope);
    (
        page_index,
        AnchorRefs::new(geometry, paragraph_box, column_box),
    )
}

fn push(layout: &mut PaginatedLayout, page_index: usize, anchor: PlacedAnchor) {
    layout.pages[page_index].anchored.push(anchor);
}

/// A group's rigid pose in page space: the `a:xfrm` rotation and flips its
/// children inherit, accumulated across nesting.
///
/// Stored as the affine `x -> L*x + t` with `L = R(rotation) * Flip`, NOT as a
/// rotation about a remembered centre. That family is closed under composition, so
/// nesting is one multiply; carrying a centre instead would mean solving for the
/// composite's fixed point, which does not exist when two poses cancel.
///
/// Why a child can wear this on its own `ShapeTransform` rather than needing a
/// group container in the placed output: the painter applies
/// `T(c) * R * Flip * T(-c)` about whatever centre it is handed
/// (`casual-doc-render`'s `object_transform`), and `c` is a free parameter. Placing
/// the child's axis-aligned rect so its centre lands at `pose.apply(centre)` and
/// handing the painter the composed linear part about that new centre reproduces
/// the rigid group transform exactly — the rect position absorbs the translation,
/// so no fixed point is needed there either.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GroupPose {
    /// Clockwise rotation in 60000ths of a degree (`a:xfrm@rot`).
    rotation: i32,
    /// Mirror across the vertical axis (`a:xfrm@flipH`).
    flip_h: bool,
    /// Mirror across the horizontal axis (`a:xfrm@flipV`).
    flip_v: bool,
    /// Translation in twips, applied after the linear part.
    tx: f64,
    ty: f64,
}

impl GroupPose {
    /// The pose that changes nothing.
    const IDENTITY: Self = Self {
        rotation: 0,
        flip_h: false,
        flip_v: false,
        tx: 0.0,
        ty: 0.0,
    };

    fn is_identity(self) -> bool {
        self.rotation == 0 && !self.flip_h && !self.flip_v && self.tx == 0.0 && self.ty == 0.0
    }

    /// The pose of `rotation`/`flip_h`/`flip_v` applied about `center`, i.e.
    /// `T(c) * R * Flip * T(-c)` flattened into `L` and `t`.
    fn about(center: Point, rotation: i32, flip_h: bool, flip_v: bool) -> Self {
        let bare = Self {
            rotation,
            flip_h,
            flip_v,
            tx: 0.0,
            ty: 0.0,
        };
        if bare.rotation == 0 && !flip_h && !flip_v {
            return Self::IDENTITY;
        }
        let cx = f64::from(center.x.raw());
        let cy = f64::from(center.y.raw());
        let (lx, ly) = bare.apply_linear(cx, cy);
        Self {
            tx: cx - lx,
            ty: cy - ly,
            ..bare
        }
    }

    /// `self` applied AFTER `inner`, i.e. the composite `self ∘ inner`.
    ///
    /// `(L_s, t_s) ∘ (L_i, t_i) = (L_s*L_i, L_s*t_i + t_s)`.
    fn after(self, inner: Self) -> Self {
        if inner.is_identity() {
            return self;
        }
        if self.is_identity() {
            return inner;
        }
        let (rotation, flip_h, flip_v) = compose_linear(
            (self.rotation, self.flip_h, self.flip_v),
            (inner.rotation, inner.flip_h, inner.flip_v),
        );
        let (ix, iy) = self.apply_linear(inner.tx, inner.ty);
        Self {
            rotation,
            flip_h,
            flip_v,
            tx: ix + self.tx,
            ty: iy + self.ty,
        }
    }

    /// The linear part alone: flip first, then rotate, matching DrawingML's order
    /// and `object_transform`'s matrix.
    fn apply_linear(self, x: f64, y: f64) -> (f64, f64) {
        let x = if self.flip_h { -x } else { x };
        let y = if self.flip_v { -y } else { y };
        if self.rotation == 0 {
            return (x, y);
        }
        // Positive `rot` is clockwise, and the painter builds the same matrix from
        // `Transform::from_rotate`, whose `(c*x - s*y, s*x + c*y)` reads clockwise
        // in a y-down space. Diverging in sign here would paint a plausible but
        // mirrored result, so the two must be read together.
        let radians = f64::from(self.rotation) / 60_000.0 * core::f64::consts::PI / 180.0;
        let (sin, cos) = radians.sin_cos();
        (cos * x - sin * y, sin * x + cos * y)
    }

    /// Where a point lands under the whole pose.
    fn apply(self, point: Point) -> Point {
        let (x, y) = self.apply_linear(f64::from(point.x.raw()), f64::from(point.y.raw()));
        Point::new(twip_rounded(x + self.tx), twip_rounded(y + self.ty))
    }

    /// The child's rect, moved so its centre lands where the pose sends it. Size is
    /// unchanged: the rect stays axis-aligned and the orientation rides the
    /// `ShapeTransform` instead.
    fn reposition(self, rect: Rect) -> Rect {
        if self.is_identity() {
            return rect;
        }
        let moved = self.apply(rect_center(rect));
        Rect::new(
            Point::new(
                Twip(moved.x.raw() - rect.size.width.raw() / 2),
                Twip(moved.y.raw() - rect.size.height.raw() / 2),
            ),
            rect.size,
        )
    }

    /// The child's own `a:xfrm` rot/flip composed under this pose.
    fn compose_child(
        self,
        rotation: Option<i32>,
        flip_h: bool,
        flip_v: bool,
    ) -> (Option<i32>, bool, bool) {
        if self.is_identity() {
            return (rotation, flip_h, flip_v);
        }
        let (rot, fh, fv) = compose_linear(
            (self.rotation, self.flip_h, self.flip_v),
            (rotation.unwrap_or(0), flip_h, flip_v),
        );
        (Some(rot), fh, fv)
    }
}

/// Composes two `R(rot) * Flip` linear parts, `outer` after `inner`.
///
/// Flips compose by XOR, since each axis is scaled by ±1 independently. The angles
/// add, but `inner`'s is NEGATED when `outer` is a single-axis reflection, because a
/// reflection anticommutes with a rotation (`F*R(θ) = R(-θ)*F`). A double flip is
/// `R(180°)`, which commutes — hence the test is `outer.flip_h == outer.flip_v`
/// rather than "either flip set".
fn compose_linear(outer: (i32, bool, bool), inner: (i32, bool, bool)) -> (i32, bool, bool) {
    let (outer_rot, outer_fh, outer_fv) = outer;
    let (inner_rot, inner_fh, inner_fv) = inner;
    let inner_rot = if outer_fh == outer_fv {
        inner_rot
    } else {
        inner_rot.saturating_neg()
    };
    (
        outer_rot.saturating_add(inner_rot),
        outer_fh ^ inner_fh,
        outer_fv ^ inner_fv,
    )
}

/// A rectangle's centre, in twips.
fn rect_center(rect: Rect) -> Point {
    Point::new(
        Twip(rect.origin.x.raw() + rect.size.width.raw() / 2),
        Twip(rect.origin.y.raw() + rect.size.height.raw() / 2),
    )
}

/// A group's child-space → page-twips mapping: an affine (in EMU) from child
/// coordinates to the top group's box space, evaluated against the group's placed
/// `origin`. Composed for nested groups.
#[derive(Clone, Copy)]
struct GroupMapper {
    scale_x: f64,
    scale_y: f64,
    tx: f64,
    ty: f64,
}

impl GroupMapper {
    /// The mapper for a top-level group from its transform: child EMU → group-box
    /// EMU (the group box's top-left is EMU `(0,0)` at the placed `origin`).
    fn root(group: &WordprocessingGroup) -> Self {
        Self::from_transform(&group.transform)
    }

    fn from_transform(t: &casual_doc_model::v1::GroupTransform) -> Self {
        let scale_x = ratio(t.extent.width_emu, t.child_extent.width_emu);
        let scale_y = ratio(t.extent.height_emu, t.child_extent.height_emu);
        Self {
            scale_x,
            scale_y,
            tx: t.offset.x_emu as f64 - t.child_offset.x_emu as f64 * scale_x,
            ty: t.offset.y_emu as f64 - t.child_offset.y_emu as f64 * scale_y,
        }
    }

    /// Composes `self` (parent) with a nested group's transform: the nested
    /// transform applies first (child → nested-parent space), then `self`.
    fn compose(&self, nested: &WordprocessingGroup) -> Self {
        let inner = Self::from_transform(&nested.transform);
        Self {
            scale_x: self.scale_x * inner.scale_x,
            scale_y: self.scale_y * inner.scale_y,
            tx: self.scale_x * inner.tx + self.tx,
            ty: self.scale_y * inner.ty + self.ty,
        }
    }

    /// The page-twips rectangle of a child at `offset` (child EMU) sized `extent`,
    /// relative to the group's placed `origin`.
    fn child_rect(
        &self,
        origin: Point,
        offset: casual_doc_model::v1::PointEmu,
        extent: Extent,
    ) -> Rect {
        let x = self.scale_x * offset.x_emu as f64 + self.tx;
        let y = self.scale_y * offset.y_emu as f64 + self.ty;
        let w = self.scale_x * extent.width_emu as f64;
        let h = self.scale_y * extent.height_emu as f64;
        Rect::new(
            Point::new(
                origin.x + emu_to_twip_rounded(x),
                origin.y + emu_to_twip_rounded(y),
            ),
            Size::new(emu_to_twip_rounded(w), emu_to_twip_rounded(h)),
        )
    }
}

/// Resolves a custom shape geometry (`a:custGeom`) into a page-local polyline
/// inside the shape's already-placed `rect` (docs/119 §6).
///
/// `O(commands)`, bounded by `MAX_SHAPE_PATH_COMMANDS`, and run once per placed
/// shape rather than per repaint.
///
/// Each axis resolves independently, because `a:path@w` and `@h` are
/// independent and a real document sets one and omits the other:
///
/// - a **positive** `@w`/`@h` is the extent of the path's own coordinate space,
///   so the coordinate is a fraction of the box (`x / width_emu`) and the path
///   rescales with the shape;
/// - **zero** (absent, per ECMA-376 §20.1.9.15) means the coordinate is an
///   absolute EMU offset from the box's top-left and does NOT rescale. This is
///   the same rule ONLYOFFICE applies in `Path.recalculate` (docs/119 §3).
///
/// The path is closed only if it authored an `a:close`; an open path stays open,
/// which is what Word's own VML fallback writes for these shapes (docs/119 §4).
/// A shape's effective fill and outline: its own where it declares them, otherwise
/// the theme style its `wps:style` names (`156` §6 row 0.2).
///
/// Resolution happens HERE, at layout, and not at import, so the model keeps saying
/// what the file said: the shape has no `spPr` fill, it has a style reference. Baking
/// the resolved colour into the model would make export write an explicit fill where
/// the file carried a style link, which is a different document.
///
/// An explicit fill always wins — a shape that says `a:noFill` means it — and an
/// index the format scheme does not model resolves to nothing rather than to a
/// neighbouring entry, so an unsupported gradient style stays unfilled instead of
/// quietly becoming a solid.
///
/// Complexity: O(1) — two map lookups and an index.
fn themed_appearance(
    shape: &GroupShape,
    definitions: &Definitions,
) -> (Option<Fill>, Option<ShapeStroke>) {
    let mut fill = shape.fill.clone();
    let mut stroke = shape.stroke;
    let Some(reference) = definitions.shape_styles.get(&shape.id) else {
        return (fill, stroke);
    };
    let Some(scheme) = definitions.format_scheme.as_ref() else {
        return (fill, stroke);
    };
    if fill.is_none()
        && let Some(idx) = reference.fill_idx
        && let Some(style) = scheme.fill_style(idx)
    {
        // `a:phClr` takes the colour the reference names; without one there is
        // nothing to substitute and the entry stays unresolved.
        let color = match style.color {
            StyleColor::Fixed(color) => Some(color),
            StyleColor::Placeholder => reference.fill_color,
        };
        fill = color.map(Fill::Solid);
    }
    if stroke.is_none()
        && let Some(idx) = reference.line_idx
        && let Some(style) = scheme.line_style(idx)
    {
        let color = match style.color {
            StyleColor::Fixed(color) => Some(color),
            StyleColor::Placeholder => reference.line_color,
        };
        stroke = color.map(|color| ShapeStroke {
            color,
            width_emu: style.width_emu,
            dash: style.dash,
            head_end: None,
            tail_end: None,
        });
    }
    (fill, stroke)
}

fn custom_path_content(
    path: &casual_doc_model::v1::ShapePath,
    rect: Rect,
    fill: Option<Fill>,
    stroke: Option<ShapeStroke>,
) -> AnchorContent {
    use casual_doc_model::v1::ShapePathCommand;

    let resolve = |value: i64, space: i64, origin: Twip, size: Twip| -> Twip {
        if space > 0 {
            Twip(
                (f64::from(size.raw()) * (value as f64 / space as f64))
                    .round()
                    .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
            ) + origin
        } else {
            emu_to_twip_offset(value) + origin
        }
    };

    let resolve_point = |point: casual_doc_model::v1::PointEmu| {
        Point::new(
            resolve(point.x_emu, path.width_emu, rect.origin.x, rect.size.width),
            resolve(
                point.y_emu,
                path.height_emu,
                rect.origin.y,
                rect.size.height,
            ),
        )
    };

    let mut commands = Vec::with_capacity(path.commands.len());
    let mut closed = false;
    for command in &path.commands {
        // Each arm resolves EVERY coordinate the command names, controls included:
        // a control point left in the path's own space would bend the curve toward
        // the page origin instead of toward where it was authored.
        commands.push(match *command {
            ShapePathCommand::MoveTo { point } => PathCommand::MoveTo {
                point: resolve_point(point),
            },
            ShapePathCommand::LineTo { point } => PathCommand::LineTo {
                point: resolve_point(point),
            },
            ShapePathCommand::CubicBezTo {
                control1,
                control2,
                point,
            } => PathCommand::CubicTo {
                control1: resolve_point(control1),
                control2: resolve_point(control2),
                point: resolve_point(point),
            },
            ShapePathCommand::QuadBezTo { control, point } => PathCommand::QuadTo {
                control: resolve_point(control),
                point: resolve_point(point),
            },
            ShapePathCommand::Close => {
                closed = true;
                continue;
            }
        });
    }

    AnchorContent::Path {
        commands,
        closed,
        fill,
        stroke: shape_stroke(stroke),
    }
}

/// A resolved vertex list as path commands: a leading move, then straight
/// segments.
///
/// The typed presets still resolve to vertices, so this is the one place lifting
/// them into the path primitive. It goes away when they become table entries
/// (`109` FID-L-04).
fn polyline_commands(points: &[Point]) -> Vec<PathCommand> {
    let mut commands = Vec::with_capacity(points.len());
    let mut rest = points.iter();
    if let Some(first) = rest.next() {
        commands.push(PathCommand::MoveTo { point: *first });
    }
    commands.extend(rest.map(|point| PathCommand::LineTo { point: *point }));
    commands
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn rgba(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

/// Builds the paint transform for a float from its model `a:xfrm` fields,
/// rotating/flipping about the (already-resolved) `rect`'s center. Returns `None`
/// when the object is unrotated and unflipped (the common case) so it paints
/// through the identity path.
fn shape_transform(
    rect: Rect,
    flip_h: bool,
    flip_v: bool,
    rotation: Option<i32>,
) -> Option<ShapeTransform> {
    let rotation = rotation.unwrap_or(0);
    if rotation == 0 && !flip_h && !flip_v {
        return None;
    }
    Some(ShapeTransform {
        rotation,
        flip_h,
        flip_v,
        center: Point::new(
            Twip(rect.origin.x.raw() + rect.size.width.raw() / 2),
            Twip(rect.origin.y.raw() + rect.size.height.raw() / 2),
        ),
    })
}

fn shape_stroke(stroke: Option<ShapeStroke>) -> Option<AnchorStroke> {
    stroke.map(|s| AnchorStroke {
        color: rgba(s.color),
        width: emu_to_twip_extent(s.width_emu),
        dash: s.dash.unwrap_or(DashStyle::Solid),
    })
}

fn text_box_stroke(stroke: ShapeStroke) -> TextBoxStroke {
    TextBoxStroke {
        color: rgba(stroke.color),
        width: emu_to_twip_extent(stroke.width_emu),
    }
}

/// Finds the page and paragraph rectangle for a float's paragraph. Returns the
/// first page whose placed fragments include that paragraph's node. The returned
/// column box uses the top-level placed fragment's x/width, so a paragraph nested
/// in a table still resolves `relativeFrom="column"` against its containing flow
/// column rather than its cell. If the paragraph is not found (or unknown), the
/// first page and the supplied section margin box are used.
fn locate(
    layout: &PaginatedLayout,
    paragraph: Option<NodeId>,
    fallback: Rect,
    _scope: PageScope,
) -> (usize, Rect, Rect) {
    if let Some(id) = paragraph {
        for (index, page) in layout.pages.iter().enumerate() {
            for placed in &page.placed {
                if let Some(rect) = find_paragraph_rect(
                    &placed.fragment,
                    placed.rect.origin,
                    placed.rect.size.width,
                    id,
                ) {
                    let column = Rect::new(
                        Point::new(placed.rect.origin.x, fallback.origin.y),
                        Size::new(placed.rect.size.width, fallback.size.height),
                    );
                    return (index, rect, column);
                }
            }
        }
    }
    (0, fallback, fallback)
}

/// Finds one paragraph's page-local box inside a placed fragment tree. Geometry
/// mirrors composition: table-cell offsets and margins, vertical alignment, and
/// nested block stacking are applied once at every level.
fn find_paragraph_rect(
    fragment: &BlockFragment,
    origin: Point,
    width: Twip,
    target: NodeId,
) -> Option<Rect> {
    let mut found = None;
    walk_paragraph_rects(fragment, origin, width, &mut |id, rect| {
        if id == target {
            found = Some(rect);
            true
        } else {
            false
        }
    });
    found
}

/// Collects every paragraph and its page-local box in fragment order. Running
/// content uses this to discover floats inside selected header/footer tables.
fn collect_paragraph_rects(
    fragment: &BlockFragment,
    origin: Point,
    width: Twip,
    out: &mut Vec<(NodeId, Rect)>,
) {
    walk_paragraph_rects(fragment, origin, width, &mut |id, rect| {
        out.push((id, rect));
        false
    });
}

/// Visits paragraph boxes in fragment order. Returning `true` from `visit`
/// stops the walk, allowing lookup and collection to share one geometry path.
fn walk_paragraph_rects(
    fragment: &BlockFragment,
    origin: Point,
    width: Twip,
    visit: &mut impl FnMut(NodeId, Rect) -> bool,
) -> bool {
    match fragment {
        BlockFragment::Paragraph { id, .. } => {
            visit(*id, Rect::new(origin, Size::new(width, fragment.height())))
        }
        BlockFragment::TableRow { cells, .. } => {
            let row_height = fragment.height();
            for cell in cells {
                let content_origin = Point::new(
                    origin.x + cell.x + cell.margins.start,
                    origin.y
                        + cell.cell_spacing.top
                        + cell.content_y_offset(cell.box_height(row_height)),
                );
                let content_width = Twip(
                    (cell.width.raw() - cell.margins.start.raw() - cell.margins.end.raw()).max(1),
                );
                let mut y = content_origin.y;
                for block in &cell.blocks {
                    if walk_paragraph_rects(
                        block,
                        Point::new(content_origin.x, y),
                        content_width,
                        visit,
                    ) {
                        return true;
                    }
                    y = y + block.height();
                }
            }
            false
        }
    }
}

/// Anchor-only page geometry. Unlike [`PageConfig`], this intentionally excludes
/// running-band reservation: OOXML page and margin reference frames are defined
/// by section page size + `w:pgMar`, not the measured header/footer content.
#[derive(Clone, Copy)]
struct AnchorGeometry {
    page_size: Size,
    margin_top: Twip,
    margin_bottom: Twip,
    margin_start: Twip,
    margin_end: Twip,
}

impl AnchorGeometry {
    fn from_config(config: &PageConfig) -> Self {
        Self {
            page_size: config.page_size,
            margin_top: config.margin_top,
            margin_bottom: config.margin_bottom,
            margin_start: config.margin_start,
            margin_end: config.margin_end,
        }
    }

    fn from_section(section: &SectionBoundary) -> Self {
        Self {
            page_size: Size::new(
                Twip(section.page_size.width_twips),
                Twip(section.page_size.height_twips),
            ),
            margin_top: Twip(section.page_margins.top_twips),
            margin_bottom: Twip(section.page_margins.bottom_twips),
            margin_start: Twip(section.page_margins.start_twips),
            margin_end: Twip(section.page_margins.end_twips),
        }
    }

    fn page_box(self) -> Rect {
        Rect::new(Point::new(Twip::ZERO, Twip::ZERO), self.page_size)
    }

    fn margin_box(self) -> Rect {
        Rect::new(
            Point::new(self.margin_start, self.margin_top),
            Size::new(
                (self.page_size.width - self.margin_start - self.margin_end).max(Twip::ZERO),
                (self.page_size.height - self.margin_top - self.margin_bottom).max(Twip::ZERO),
            ),
        )
    }

    fn left_margin_box(self) -> Rect {
        Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(self.margin_start.max(Twip::ZERO), self.page_size.height),
        )
    }

    fn right_margin_box(self) -> Rect {
        let width = self.margin_end.max(Twip::ZERO);
        Rect::new(
            Point::new((self.page_size.width - width).max(Twip::ZERO), Twip::ZERO),
            Size::new(width, self.page_size.height),
        )
    }

    fn top_margin_box(self) -> Rect {
        Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(self.page_size.width, self.margin_top.max(Twip::ZERO)),
        )
    }

    fn bottom_margin_box(self) -> Rect {
        let height = self.margin_bottom.max(Twip::ZERO);
        Rect::new(
            Point::new(Twip::ZERO, (self.page_size.height - height).max(Twip::ZERO)),
            Size::new(self.page_size.width, height),
        )
    }
}

/// The reference boxes a float's offsets/alignments resolve against, in
/// page-local twips.
struct AnchorRefs {
    page: Rect,
    margin: Rect,
    left_margin: Rect,
    right_margin: Rect,
    top_margin: Rect,
    bottom_margin: Rect,
    column: Rect,
    paragraph: Rect,
}

impl AnchorRefs {
    fn new(geometry: AnchorGeometry, paragraph: Rect, column: Rect) -> Self {
        Self {
            page: geometry.page_box(),
            margin: geometry.margin_box(),
            left_margin: geometry.left_margin_box(),
            right_margin: geometry.right_margin_box(),
            top_margin: geometry.top_margin_box(),
            bottom_margin: geometry.bottom_margin_box(),
            column,
            paragraph,
        }
    }
}

/// Resolves an anchor to its absolute page-local rectangle for a drawing of the
/// given `extent`: the horizontal/vertical reference boxes are selected by
/// `relativeFrom`, and the offset (`posOffset`) or alignment placed within them.
fn resolve_anchor_rect(anchor: &DrawingAnchor, extent: Extent, refs: &AnchorRefs) -> Rect {
    let size = Size::new(
        emu_to_twip_extent(extent.width_emu),
        emu_to_twip_extent(extent.height_emu),
    );
    let hbox = match anchor.horizontal.relative_from {
        HorizontalAnchor::Page => refs.page,
        HorizontalAnchor::LeftMargin | HorizontalAnchor::InsideMargin => refs.left_margin,
        HorizontalAnchor::RightMargin | HorizontalAnchor::OutsideMargin => refs.right_margin,
        HorizontalAnchor::Margin | HorizontalAnchor::Character => refs.margin,
        HorizontalAnchor::Column => refs.column,
    };
    let x = match anchor.horizontal.position {
        HorizontalPosition::Offset(emu) => hbox.origin.x + emu_to_twip_offset(emu),
        HorizontalPosition::Align(align) => align_horizontal(align, hbox, size.width),
    };
    let vbox = match anchor.vertical.relative_from {
        VerticalAnchor::Page => refs.page,
        VerticalAnchor::Paragraph | VerticalAnchor::Line => refs.paragraph,
        VerticalAnchor::TopMargin => refs.top_margin,
        VerticalAnchor::BottomMargin => refs.bottom_margin,
        VerticalAnchor::Margin | VerticalAnchor::InsideMargin | VerticalAnchor::OutsideMargin => {
            refs.margin
        }
    };
    let y = match anchor.vertical.position {
        VerticalPosition::Offset(emu) => vbox.origin.y + emu_to_twip_offset(emu),
        VerticalPosition::Align(align) => align_vertical(align, vbox, size.height),
    };
    Rect::new(Point::new(x, y), size)
}

fn align_horizontal(align: HorizontalAlign, hbox: Rect, width: Twip) -> Twip {
    match align {
        HorizontalAlign::Left | HorizontalAlign::Inside => hbox.origin.x,
        HorizontalAlign::Center => {
            Twip(hbox.origin.x.raw() + ((hbox.size.width.raw() - width.raw()) / 2))
        }
        HorizontalAlign::Right | HorizontalAlign::Outside => {
            Twip(hbox.origin.x.raw() + hbox.size.width.raw() - width.raw())
        }
    }
}

fn align_vertical(align: VerticalAlign, vbox: Rect, height: Twip) -> Twip {
    match align {
        VerticalAlign::Top | VerticalAlign::Inside => vbox.origin.y,
        VerticalAlign::Center => {
            Twip(vbox.origin.y.raw() + ((vbox.size.height.raw() - height.raw()) / 2))
        }
        VerticalAlign::Bottom | VerticalAlign::Outside => {
            Twip(vbox.origin.y.raw() + vbox.size.height.raw() - height.raw())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 90° clockwise, in `a:xfrm@rot`'s 60000ths of a degree.
    const CW_90: i32 = 90 * 60_000;

    #[test]
    fn a_pose_rotates_a_point_clockwise_about_its_centre() {
        // A point to the RIGHT of the centre must land BELOW it: y grows downward,
        // so right -> down reads clockwise on screen, which is what positive
        // `a:xfrm@rot` means and what the painter's matrix does.
        let pose = GroupPose::about(Point::new(Twip(1_000), Twip(1_000)), CW_90, false, false);
        assert_eq!(
            pose.apply(Point::new(Twip(2_000), Twip(1_000))),
            Point::new(Twip(1_000), Twip(2_000))
        );
        // Four quarter turns return to the start.
        let mut p = Point::new(Twip(2_000), Twip(1_000));
        for _ in 0..4 {
            p = pose.apply(p);
        }
        assert_eq!(p, Point::new(Twip(2_000), Twip(1_000)));
    }

    #[test]
    fn a_pose_mirrors_a_point_about_its_centre() {
        let pose = GroupPose::about(Point::new(Twip(1_000), Twip(1_000)), 0, true, false);
        assert_eq!(
            pose.apply(Point::new(Twip(2_000), Twip(1_000))),
            Point::new(Twip(0), Twip(1_000))
        );
        // flipV leaves x alone.
        let pose = GroupPose::about(Point::new(Twip(1_000), Twip(1_000)), 0, false, true);
        assert_eq!(
            pose.apply(Point::new(Twip(2_000), Twip(1_500))),
            Point::new(Twip(2_000), Twip(500))
        );
    }

    #[test]
    fn flips_compose_by_xor() {
        assert_eq!(
            compose_linear((0, true, false), (0, true, false)),
            (0, false, false),
            "two flipH cancel"
        );
        assert_eq!(
            compose_linear((0, true, false), (0, false, true)),
            (0, true, true)
        );
    }

    #[test]
    fn a_single_axis_reflection_negates_the_inner_rotation() {
        // F*R(θ) = R(-θ)*F, so an outer reflection reverses the child's spin. This
        // is the one piece of the algebra that is easy to get backwards, and
        // getting it backwards mirrors the result plausibly rather than visibly.
        assert_eq!(
            compose_linear((0, true, false), (CW_90, false, false)),
            (-CW_90, true, false)
        );
        assert_eq!(
            compose_linear((0, false, true), (CW_90, false, false)),
            (-CW_90, false, true)
        );
    }

    #[test]
    fn a_double_flip_is_a_half_turn_and_commutes() {
        // flipH+flipV together are R(180°), which commutes with any rotation, so the
        // inner angle must NOT be negated — hence the test is `flip_h == flip_v`
        // rather than "either flip set".
        assert_eq!(
            compose_linear((0, true, true), (CW_90, false, false)),
            (CW_90, true, true)
        );
        assert_eq!(
            compose_linear((0, false, false), (CW_90, false, false)),
            (CW_90, false, false)
        );
    }

    #[test]
    fn composing_a_pose_with_its_inverse_restores_the_point() {
        let centre = Point::new(Twip(3_000), Twip(2_000));
        let forward = GroupPose::about(centre, CW_90, false, false);
        let back = GroupPose::about(centre, -CW_90, false, false);
        let probe = Point::new(Twip(4_321), Twip(765));
        assert_eq!(forward.after(back).apply(probe), probe);
        assert_eq!(back.after(forward).apply(probe), probe);
    }

    #[test]
    fn a_nested_pose_composes_rather_than_adding_about_the_wrong_centre() {
        // Two 90° turns about DIFFERENT centres are not one 180° turn about either.
        // Composing affines is what makes this come out right; remembering a single
        // centre could not.
        let outer = GroupPose::about(Point::new(Twip(0), Twip(0)), CW_90, false, false);
        let inner = GroupPose::about(Point::new(Twip(1_000), Twip(0)), CW_90, false, false);
        let probe = Point::new(Twip(2_000), Twip(0));
        // inner: (2000,0) about (1000,0) -> (1000,1000). outer: about origin -> (-1000,1000).
        assert_eq!(inner.apply(probe), Point::new(Twip(1_000), Twip(1_000)));
        assert_eq!(
            outer.after(inner).apply(probe),
            Point::new(Twip(-1_000), Twip(1_000))
        );
    }

    #[test]
    fn reposition_moves_the_centre_and_keeps_the_size() {
        let pose = GroupPose::about(Point::new(Twip(1_000), Twip(1_000)), CW_90, false, false);
        let rect = Rect::new(
            Point::new(Twip(1_800), Twip(900)),
            Size::new(Twip(400), Twip(200)),
        );
        // Centre (2000, 1000) -> (1000, 2000); size unchanged, rect stays axis-aligned.
        let moved = pose.reposition(rect);
        assert_eq!(moved.size, rect.size);
        assert_eq!(moved.origin, Point::new(Twip(800), Twip(1_900)));
    }

    #[test]
    fn an_identity_pose_changes_nothing() {
        // The fast path every unrotated group takes, which is why collecting this
        // machinery moved no committed golden.
        let pose = GroupPose::about(Point::new(Twip(500), Twip(500)), 0, false, false);
        assert!(pose.is_identity());
        assert_eq!(pose, GroupPose::IDENTITY);
        let rect = Rect::new(
            Point::new(Twip(10), Twip(20)),
            Size::new(Twip(30), Twip(40)),
        );
        assert_eq!(pose.reposition(rect), rect);
        assert_eq!(
            pose.compose_child(Some(CW_90), true, false),
            (Some(CW_90), true, false),
            "an identity pose must hand the child's own transform back untouched"
        );
    }

    use casual_doc_model::v1::{
        AnchorHorizontal, AnchorVertical, GroupTransform, PointEmu, WrapMode,
    };

    fn config() -> PageConfig {
        use casual_doc_model::v1::SectionId;
        PageConfig {
            section: SectionId::new(NodeId::from_parts(9, 1).unwrap()),
            page_size: Size::new(Twip(12_240), Twip(15_840)),
            margin_top: Twip(1_440),
            margin_bottom: Twip(1_440),
            margin_start: Twip(1_440),
            margin_end: Twip(1_440),
            header_distance: Twip(720),
            footer_distance: Twip(720),
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        }
    }

    fn anchor(
        h: HorizontalAnchor,
        hp: HorizontalPosition,
        v: VerticalAnchor,
        vp: VerticalPosition,
    ) -> DrawingAnchor {
        DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: h,
                position: hp,
            },
            vertical: AnchorVertical {
                relative_from: v,
                position: vp,
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: Default::default(),
            wrap_polygon: None,
            behind_doc: false,
        }
    }

    #[test]
    fn page_relative_offset_resolves_to_the_absolute_page_point() {
        let a = anchor(
            HorizontalAnchor::Page,
            HorizontalPosition::Offset(914_400),
            VerticalAnchor::Page,
            VerticalPosition::Offset(1_828_800),
        );
        let extent = Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        };
        let geometry = AnchorGeometry::from_config(&config());
        let refs = AnchorRefs::new(geometry, Rect::default(), geometry.margin_box());
        let rect = resolve_anchor_rect(&a, extent, &refs);
        assert_eq!(rect.origin, Point::new(Twip(1_440), Twip(2_880)));
        assert_eq!(rect.size, Size::new(Twip(1_440), Twip(1_440)));
    }

    #[test]
    fn center_alignment_centers_within_the_margin_box() {
        let a = anchor(
            HorizontalAnchor::Margin,
            HorizontalPosition::Align(HorizontalAlign::Center),
            VerticalAnchor::Page,
            VerticalPosition::Offset(0),
        );
        let extent = Extent {
            width_emu: 635_000,
            height_emu: 100,
        };
        let geometry = AnchorGeometry::from_config(&config());
        let refs = AnchorRefs::new(geometry, Rect::default(), geometry.margin_box());
        let rect = resolve_anchor_rect(&a, extent, &refs);
        assert_eq!(rect.origin.x, Twip(5_620));
    }

    #[test]
    fn top_and_bottom_margin_frames_are_the_physical_margin_strips() {
        let geometry = AnchorGeometry::from_config(&config());
        let refs = AnchorRefs::new(geometry, Rect::default(), geometry.margin_box());
        let extent = Extent {
            width_emu: 63_500,
            height_emu: 457_200, // 720 twips
        };
        let top = anchor(
            HorizontalAnchor::Page,
            HorizontalPosition::Offset(0),
            VerticalAnchor::TopMargin,
            VerticalPosition::Align(VerticalAlign::Bottom),
        );
        let bottom = anchor(
            HorizontalAnchor::Page,
            HorizontalPosition::Offset(0),
            VerticalAnchor::BottomMargin,
            VerticalPosition::Align(VerticalAlign::Top),
        );
        assert_eq!(
            resolve_anchor_rect(&top, extent, &refs).origin.y,
            Twip(720),
            "bottom alignment in the 1,440-twip top strip subtracts the float height"
        );
        assert_eq!(
            resolve_anchor_rect(&bottom, extent, &refs).origin.y,
            Twip(14_400),
            "the bottom strip begins one margin above the page edge"
        );
    }

    fn ident_transform(w: i64, h: i64) -> GroupTransform {
        GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: Extent {
                width_emu: w,
                height_emu: h,
            },
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: Extent {
                width_emu: w,
                height_emu: h,
            },
            flip_h: false,
            flip_v: false,
            rotation: None,
        }
    }

    #[test]
    fn a_group_child_is_placed_at_group_origin_plus_its_own_offset_and_sized_by_its_own_extent() {
        // Identity transform: a child at EMU offset (635_000, 1_270_000) sized
        // (1_270_000 x 635_000) placed at group origin (1000, 2000) twips lands at
        // origin + offset/635 and is sized extent/635 — NOT the group extent.
        let group = WordprocessingGroup {
            hyperlink: None,
            id: NodeId::from_parts(1, 1).unwrap(),
            anchor: None,
            relative_height: None,
            extent: Extent {
                width_emu: 6_350_000,
                height_emu: 6_350_000,
            },
            transform: ident_transform(6_350_000, 6_350_000),
            children: Vec::new(),
        };
        let mapper = GroupMapper::root(&group);
        let rect = mapper.child_rect(
            Point::new(Twip(1_000), Twip(2_000)),
            PointEmu {
                x_emu: 635_000,
                y_emu: 1_270_000,
            },
            Extent {
                width_emu: 1_270_000,
                height_emu: 635_000,
            },
        );
        assert_eq!(rect.origin, Point::new(Twip(2_000), Twip(4_000)));
        assert_eq!(rect.size, Size::new(Twip(2_000), Twip(1_000)));
    }

    #[test]
    fn a_nested_group_composes_its_parent_translation() {
        // Parent identity; nested group offset (28050, 112196) EMU. A child at
        // nested offset (0,0) lands at the nested group's offset in the parent.
        let parent = WordprocessingGroup {
            hyperlink: None,
            id: NodeId::from_parts(1, 1).unwrap(),
            anchor: None,
            relative_height: None,
            extent: Extent {
                width_emu: 2_000_000,
                height_emu: 800_000,
            },
            transform: ident_transform(2_000_000, 800_000),
            children: Vec::new(),
        };
        let mut nested = parent.clone();
        nested.id = NodeId::from_parts(2, 1).unwrap();
        nested.transform = GroupTransform {
            offset: PointEmu {
                x_emu: 28_050,
                y_emu: 112_196,
            },
            extent: Extent {
                width_emu: 1_917_065,
                height_emu: 476_885,
            },
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: Extent {
                width_emu: 1_917_065,
                height_emu: 476_885,
            },
            flip_h: false,
            flip_v: false,
            rotation: None,
        };
        let mapper = GroupMapper::root(&parent).compose(&nested);
        let rect = mapper.child_rect(
            Point::new(Twip::ZERO, Twip::ZERO),
            PointEmu { x_emu: 0, y_emu: 0 },
            Extent {
                width_emu: 1_917_065,
                height_emu: 476_885,
            },
        );
        // 28050/635 ≈ 44, 112196/635 ≈ 177.
        assert_eq!(rect.origin, Point::new(Twip(44), Twip(177)));
        assert_eq!(rect.size, Size::new(Twip(3_019), Twip(751)));
    }
}
