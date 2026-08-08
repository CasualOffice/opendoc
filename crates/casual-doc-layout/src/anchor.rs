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

use crate::block::BlockFragment;
// Separate `use` line to minimize import-block merge conflicts.
use crate::display::ShapeTransform;
use crate::flow::flow_anchored_text_box;
use crate::page::{
    AnchorContent, AnchorStroke, AnchorZ, PaginatedLayout, PlacedAnchor, PlacedFragment,
};
use crate::paginate::PageConfig;
use crate::text::{LineShaper, TextBoxStroke};
use crate::units::{Point, Rect, Size, Twip};

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
}

/// Resolves the page-local wrap rectangles of eligible top-level body floats
/// against an already paginated layout. The document driver converts these into
/// paragraph-local line exclusions and iterates to a fixed point.
pub(crate) fn body_wrap_rects(
    layout: &PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
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
        collect_body_wrap_inlines(
            layout,
            &ctx,
            paragraph.id,
            section,
            &paragraph.inlines,
            &mut out,
        );
    }
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
        self.document
            .definitions()
            .sections
            .iter()
            .find(|boundary| boundary.id == section)
            .map_or_else(
                || AnchorGeometry::from_config(self.config),
                AnchorGeometry::from_section,
            )
    }
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
fn body_section_ids(document: &Document, fallback: SectionId) -> Vec<SectionId> {
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
                let origin = resolve_anchor_rect(&anchor, group.extent, &refs).origin;
                let relative_height = group.relative_height.unwrap_or(0);
                let behind_doc = anchor.behind_doc;
                let mapper = GroupMapper::root(group);
                place_group_children(
                    layout,
                    ctx,
                    group,
                    page_index,
                    origin,
                    &mapper,
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
                let rect = mapper.child_rect(origin, picture.offset, picture.extent);
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
                        },
                        rect,
                        behind_doc,
                        z,
                        descr: picture.descr.clone(),
                        transform: shape_transform(
                            rect,
                            picture.flip_h,
                            picture.flip_v,
                            picture.rotation,
                        ),
                    },
                );
            }
            GroupChild::TextBox(text_box) => {
                let mut rect = mapper.child_rect(origin, text_box.offset, text_box.extent);
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
                let rect = mapper.child_rect(origin, shape.offset, shape.extent);
                let z = AnchorZ {
                    relative_height,
                    order: ctx.next_order(),
                };
                let content = match shape.geometry {
                    ShapeGeometry::Line => AnchorContent::Line {
                        from: rect.origin,
                        to: Point::new(rect.right(), rect.bottom()),
                        // A line without an explicit stroke still draws a hairline
                        // in its fill color (Word's connector default).
                        stroke: shape_stroke(shape.stroke).unwrap_or(AnchorStroke {
                            color: shape
                                .fill
                                .as_ref()
                                .map_or([0, 0, 0, 255], |fill| rgba(fill.flat_color())),
                            width: Twip::ZERO,
                            dash: DashStyle::Solid,
                        }),
                        head_end: shape.stroke.and_then(|s| s.head_end),
                        tail_end: shape.stroke.and_then(|s| s.tail_end),
                    },
                    ShapeGeometry::Ellipse => AnchorContent::Ellipse {
                        fill: shape.fill.clone(),
                        stroke: shape_stroke(shape.stroke),
                    },
                    ShapeGeometry::RoundRectangle => AnchorContent::RoundedRectangle {
                        radius: rounded_rectangle_radius(shape, rect),
                        fill: shape.fill.clone(),
                        stroke: shape_stroke(shape.stroke),
                    },
                    ShapeGeometry::Triangle => AnchorContent::Polygon {
                        points: vec![
                            Point::new(
                                rect.origin.x + Twip(rect.size.width.raw() / 2),
                                rect.origin.y,
                            ),
                            Point::new(rect.right(), rect.bottom()),
                            Point::new(rect.origin.x, rect.bottom()),
                        ],
                        fill: shape.fill.clone(),
                        stroke: shape_stroke(shape.stroke),
                    },
                    ShapeGeometry::RightTriangle => AnchorContent::Polygon {
                        points: vec![
                            rect.origin,
                            Point::new(rect.right(), rect.bottom()),
                            Point::new(rect.origin.x, rect.bottom()),
                        ],
                        fill: shape.fill.clone(),
                        stroke: shape_stroke(shape.stroke),
                    },
                    ShapeGeometry::Diamond => AnchorContent::Polygon {
                        points: vec![
                            Point::new(
                                rect.origin.x + Twip(rect.size.width.raw() / 2),
                                rect.origin.y,
                            ),
                            Point::new(
                                rect.right(),
                                rect.origin.y + Twip(rect.size.height.raw() / 2),
                            ),
                            Point::new(
                                rect.origin.x + Twip(rect.size.width.raw() / 2),
                                rect.bottom(),
                            ),
                            Point::new(
                                rect.origin.x,
                                rect.origin.y + Twip(rect.size.height.raw() / 2),
                            ),
                        ],
                        fill: shape.fill.clone(),
                        stroke: shape_stroke(shape.stroke),
                    },
                    ShapeGeometry::Rectangle | ShapeGeometry::Other => AnchorContent::Rectangle {
                        fill: shape.fill.clone(),
                        stroke: shape_stroke(shape.stroke),
                    },
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
                        transform: shape_transform(
                            rect,
                            shape.flip_h,
                            shape.flip_v,
                            shape.rotation,
                        ),
                    },
                );
            }
            GroupChild::Group(nested) => {
                let nested_mapper = mapper.compose(nested);
                place_group_children(
                    layout,
                    ctx,
                    nested,
                    page_index,
                    origin,
                    &nested_mapper,
                    relative_height,
                    behind_doc,
                );
            }
        }
    }
}

/// Resolves the common `roundRect` `adj` guide. DrawingML uses 100000-based
/// percentages; the preset default is 16667 (one sixth of the shorter side).
fn rounded_rectangle_radius(shape: &GroupShape, rect: Rect) -> Twip {
    let adjustment = shape
        .adjustments
        .iter()
        .find(|guide| guide.name == "adj")
        .and_then(|guide| guide.formula.strip_prefix("val "))
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(16_667)
        .clamp(0, 50_000);
    let shorter = i64::from(rect.size.width.raw().min(rect.size.height.raw()).max(0));
    Twip((shorter * adjustment / 100_000).clamp(0, i64::from(i32::MAX)) as i32)
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
    let stores: Vec<&BlockNode> = match band {
        PageScope::Header => defs.headers.iter().flat_map(|(_, hf)| &hf.blocks).collect(),
        PageScope::Footer => defs.footers.iter().flat_map(|(_, hf)| &hf.blocks).collect(),
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
            Point::new(origin.x + emu_to_twip_f(x), origin.y + emu_to_twip_f(y)),
            Size::new(emu_to_twip_f(w), emu_to_twip_f(h)),
        )
    }
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
        width: emu_to_twip(s.width_emu),
        dash: s.dash.unwrap_or(DashStyle::Solid),
    })
}

fn text_box_stroke(stroke: ShapeStroke) -> TextBoxStroke {
    TextBoxStroke {
        color: rgba(stroke.color),
        width: emu_to_twip(stroke.width_emu),
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
        emu_to_twip(extent.width_emu),
        emu_to_twip(extent.height_emu),
    );
    let hbox = match anchor.horizontal.relative_from {
        HorizontalAnchor::Page => refs.page,
        HorizontalAnchor::LeftMargin | HorizontalAnchor::InsideMargin => refs.left_margin,
        HorizontalAnchor::RightMargin | HorizontalAnchor::OutsideMargin => refs.right_margin,
        HorizontalAnchor::Margin | HorizontalAnchor::Character => refs.margin,
        HorizontalAnchor::Column => refs.column,
    };
    let x = match anchor.horizontal.position {
        HorizontalPosition::Offset(emu) => hbox.origin.x + emu_to_twip_signed(emu),
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
        VerticalPosition::Offset(emu) => vbox.origin.y + emu_to_twip_signed(emu),
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

/// EMU → twips for a size (non-negative): 635 EMU per twip.
fn emu_to_twip(emu: i64) -> Twip {
    Twip((emu / 635).clamp(0, i64::from(i32::MAX)) as i32)
}

/// EMU (as `f64`, from an affine transform) → twips, clamped to the twip range.
fn emu_to_twip_f(emu: f64) -> Twip {
    Twip(
        (emu / 635.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
    )
}

/// EMU → twips for a signed offset (a float may overhang its reference edge).
fn emu_to_twip_signed(emu: i64) -> Twip {
    Twip((emu / 635).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
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
