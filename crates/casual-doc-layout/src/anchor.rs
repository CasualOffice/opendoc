//! Anchored (floating) drawing placement — the first-cut `P1F-28` positioning.
//!
//! An anchored `w:drawing` (`wp:anchor`) sits at an absolute position on the page
//! rather than in the inline flow. This module is a **post-pagination** pass (like
//! [`crate::running::place_running_content`] and [`crate::paginate::resolve_fields`]):
//! it walks the document for [`AnchoredDrawing`] nodes, finds the page each one's
//! anchoring paragraph landed on, resolves its `wp:positionH`/`wp:positionV` against
//! the page/margin/paragraph box, and records a [`PlacedAnchor`] on that page.
//! Composition then paints it behind or above the text per `behind_doc`.
//!
//! Kept off the pagination hot path, so page reuse (the stabilization halt) is
//! unaffected: `paginate`/`repaginate` produce pages with an empty `anchored`
//! list, and this pass fills it identically afterward.
//!
//! First cut (this slice): the image is *positioned* correctly; text does not yet
//! re-flow around it (`wrapSquare`/`wrapTight`/… are modeled but laid out like
//! `wrapNone` — the image floats over or behind the flow). Multi-section documents
//! resolve every anchor against the single geometry passed in; per-section anchor
//! geometry is a follow-up.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchoredDrawing, BlockNode, Document, DrawingAnchor, Extent, HorizontalAlign, HorizontalAnchor,
    HorizontalPosition, InlineNode, VerticalAlign, VerticalAnchor, VerticalPosition,
};

use crate::page::{PaginatedLayout, PlacedAnchor};
use crate::paginate::PageConfig;
use crate::units::{Point, Rect, Size, Twip};

/// An anchored drawing collected from the document, with its media resolved to a
/// package part name and tagged with the paragraph it is anchored in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocAnchor {
    /// The node id of the paragraph the anchor appears in (used to find the page
    /// and the paragraph reference box). `None` if the anchor was not inside a
    /// resolvable paragraph.
    pub paragraph: Option<NodeId>,
    /// The media key (package part name) to paint.
    pub media: String,
    /// The drawing's size.
    pub extent: Extent,
    /// The placement.
    pub anchor: DrawingAnchor,
    /// The alt text.
    pub descr: Option<String>,
}

/// Collects every anchored drawing in the document body, resolving each one's
/// media id to its package part name. Anchors whose media id does not resolve are
/// skipped (nothing to paint), mirroring the inline-image path.
#[must_use]
pub fn collect_anchored(document: &Document) -> Vec<DocAnchor> {
    let mut out = Vec::new();
    for block in document.body() {
        collect_block(document, block, &mut out);
    }
    out
}

fn collect_block(document: &Document, block: &BlockNode, out: &mut Vec<DocAnchor>) {
    match block {
        BlockNode::Paragraph(para) => {
            collect_inlines(document, &para.inlines, Some(para.id), out);
        }
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for nested in &cell.blocks {
                        collect_block(document, nested, out);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for nested in &sdt.blocks {
                collect_block(document, nested, out);
            }
        }
        // An alt chunk holds no inline content and hence no anchored drawings.
        BlockNode::AltChunk(_) => {}
    }
}

fn collect_inlines(
    document: &Document,
    inlines: &[InlineNode],
    paragraph: Option<NodeId>,
    out: &mut Vec<DocAnchor>,
) {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) => {
                if let Some(anchor) = doc_anchor(document, drawing, paragraph) {
                    out.push(anchor);
                }
            }
            // Anchored drawings can appear inside inline wrappers; recurse so
            // none is missed. A text box is a separate positioning context whose
            // own anchors are a follow-up, so it is not descended here.
            InlineNode::Hyperlink(link) => {
                collect_inlines(document, &link.inlines, paragraph, out);
            }
            InlineNode::Field(field) => {
                collect_inlines(document, &field.inlines, paragraph, out);
            }
            InlineNode::Revision(revision) => {
                collect_inlines(document, &revision.inlines, paragraph, out);
            }
            InlineNode::Sdt(sdt) => {
                collect_inlines(document, &sdt.inlines, paragraph, out);
            }
            _ => {}
        }
    }
}

fn doc_anchor(
    document: &Document,
    drawing: &AnchoredDrawing,
    paragraph: Option<NodeId>,
) -> Option<DocAnchor> {
    let media = document.definitions().media.get(&drawing.media)?;
    Some(DocAnchor {
        paragraph,
        media: media.part_name.clone(),
        extent: drawing.extent,
        anchor: drawing.anchor,
        descr: drawing.descr.clone(),
    })
}

/// Resolves and places every anchored drawing onto the page its anchoring
/// paragraph landed on (falling back to the first page when the paragraph cannot
/// be located, e.g. it was fully consumed by a header/footer). Each anchor's
/// rectangle is computed against `config`'s page/margin geometry and the
/// paragraph's placed rectangle.
pub fn place_anchored_drawings(
    layout: &mut PaginatedLayout,
    anchors: &[DocAnchor],
    config: &PageConfig,
) {
    if layout.pages.is_empty() {
        return;
    }
    for anchor in anchors {
        let (page_index, paragraph_box) = locate(layout, anchor.paragraph, config);
        let refs = AnchorRefs::new(config, paragraph_box);
        let rect = resolve_anchor_rect(&anchor.anchor, &anchor.extent, &refs);
        layout.pages[page_index].anchored.push(PlacedAnchor {
            media: anchor.media.clone(),
            rect,
            behind_doc: anchor.anchor.behind_doc,
            descr: anchor.descr.clone(),
        });
    }
}

/// Finds the page and paragraph rectangle for an anchor's paragraph. Returns the
/// first page whose placed fragments include that paragraph's node; if the
/// paragraph is not found (or unknown), the first page and its content area.
fn locate(
    layout: &PaginatedLayout,
    paragraph: Option<NodeId>,
    config: &PageConfig,
) -> (usize, Rect) {
    if let Some(id) = paragraph {
        for (index, page) in layout.pages.iter().enumerate() {
            if let Some(placed) = page
                .placed
                .iter()
                .find(|placed| placed.fragment.node_id() == id)
            {
                return (index, placed.rect);
            }
        }
    }
    (0, config.content_area())
}

/// The reference boxes an anchor's offsets/alignments resolve against, all in
/// page-local twips.
struct AnchorRefs {
    /// The whole page box (`relativeFrom="page"`).
    page: Rect,
    /// The text margin box (`margin`/`column`/`character`).
    margin: Rect,
    /// The left margin strip (`leftMargin`/`insideMargin`).
    left_margin: Rect,
    /// The right margin strip (`rightMargin`/`outsideMargin`).
    right_margin: Rect,
    /// The anchoring paragraph's box (`paragraph`/`line`).
    paragraph: Rect,
}

impl AnchorRefs {
    fn new(config: &PageConfig, paragraph: Rect) -> Self {
        let page = Rect::new(Point::new(Twip::ZERO, Twip::ZERO), config.page_size);
        let margin = Rect::new(
            Point::new(config.margin_start, config.margin_top),
            Size::new(
                (config.page_size.width - config.margin_start - config.margin_end).max(Twip::ZERO),
                (config.page_size.height - config.margin_top - config.margin_bottom)
                    .max(Twip::ZERO),
            ),
        );
        let left_margin = Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(config.margin_start, config.page_size.height),
        );
        let right_margin = Rect::new(
            Point::new(
                (config.page_size.width - config.margin_end).max(Twip::ZERO),
                Twip::ZERO,
            ),
            Size::new(config.margin_end, config.page_size.height),
        );
        Self {
            page,
            margin,
            left_margin,
            right_margin,
            paragraph,
        }
    }
}

/// Resolves an anchor to its absolute page-local rectangle: the horizontal box
/// and vertical box are selected by `relativeFrom`, and the offset (`posOffset`,
/// signed EMU) or alignment (`align`) placed within them. The size is the
/// `wp:extent` in twips.
fn resolve_anchor_rect(anchor: &DrawingAnchor, extent: &Extent, refs: &AnchorRefs) -> Rect {
    let size = Size::new(
        emu_to_twip(extent.width_emu),
        emu_to_twip(extent.height_emu),
    );
    let hbox = match anchor.horizontal.relative_from {
        HorizontalAnchor::Page => refs.page,
        HorizontalAnchor::LeftMargin | HorizontalAnchor::InsideMargin => refs.left_margin,
        HorizontalAnchor::RightMargin | HorizontalAnchor::OutsideMargin => refs.right_margin,
        // `margin`, `column`, and `character` resolve against the text margin box
        // in this single-column first cut.
        HorizontalAnchor::Margin | HorizontalAnchor::Column | HorizontalAnchor::Character => {
            refs.margin
        }
    };
    let x = match anchor.horizontal.position {
        HorizontalPosition::Offset(emu) => hbox.origin.x + emu_to_twip_signed(emu),
        HorizontalPosition::Align(align) => align_horizontal(align, hbox, size.width),
    };
    let vbox = match anchor.vertical.relative_from {
        VerticalAnchor::Page => refs.page,
        // `paragraph`/`line` resolve against the anchoring paragraph's box.
        VerticalAnchor::Paragraph | VerticalAnchor::Line => refs.paragraph,
        // `margin`, `topMargin`, `bottomMargin`, `inside/outsideMargin` resolve
        // against the text margin box in this first cut.
        VerticalAnchor::Margin
        | VerticalAnchor::TopMargin
        | VerticalAnchor::BottomMargin
        | VerticalAnchor::InsideMargin
        | VerticalAnchor::OutsideMargin => refs.margin,
    };
    let y = match anchor.vertical.position {
        VerticalPosition::Offset(emu) => vbox.origin.y + emu_to_twip_signed(emu),
        VerticalPosition::Align(align) => align_vertical(align, vbox, size.height),
    };
    Rect::new(Point::new(x, y), size)
}

/// Places a `width`-wide image horizontally within `hbox` by `align`.
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

/// Places a `height`-tall image vertically within `vbox` by `align`.
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

/// EMU → twips for a signed offset: `posOffset` may be negative (a drawing that
/// overhangs its reference edge), so the result is not clamped to non-negative.
fn emu_to_twip_signed(emu: i64) -> Twip {
    Twip((emu / 635).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        use casual_doc_model::v1::{AnchorHorizontal, AnchorVertical, WrapMode};
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
            behind_doc: false,
        }
    }

    #[test]
    fn page_relative_offset_resolves_to_the_absolute_page_point() {
        // 914400 EMU = 1 inch = 1440 twips from the page corner.
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
        let refs = AnchorRefs::new(
            &config(),
            Rect::new(Point::new(Twip(0), Twip(0)), Size::default()),
        );
        let rect = resolve_anchor_rect(&a, &extent, &refs);
        assert_eq!(rect.origin, Point::new(Twip(1_440), Twip(2_880)));
        assert_eq!(rect.size, Size::new(Twip(1_440), Twip(1_440)));
    }

    #[test]
    fn margin_relative_offset_adds_the_margin_origin() {
        let a = anchor(
            HorizontalAnchor::Margin,
            HorizontalPosition::Offset(0),
            VerticalAnchor::Margin,
            VerticalPosition::Offset(0),
        );
        let extent = Extent {
            width_emu: 635_000,
            height_emu: 635_000,
        };
        let refs = AnchorRefs::new(&config(), Rect::default());
        let rect = resolve_anchor_rect(&a, &extent, &refs);
        // The margin box origin is the (start, top) margin corner.
        assert_eq!(rect.origin, Point::new(Twip(1_440), Twip(1_440)));
    }

    #[test]
    fn a_negative_offset_overhangs_the_reference_edge() {
        let a = anchor(
            HorizontalAnchor::Margin,
            HorizontalPosition::Offset(-635_000),
            VerticalAnchor::Page,
            VerticalPosition::Offset(0),
        );
        let extent = Extent {
            width_emu: 100,
            height_emu: 100,
        };
        let refs = AnchorRefs::new(&config(), Rect::default());
        let rect = resolve_anchor_rect(&a, &extent, &refs);
        // 1440 (margin) - 1000 (offset) = 440.
        assert_eq!(rect.origin.x, Twip(440));
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
            width_emu: 635_000, // 1000 twips
            height_emu: 100,
        };
        let refs = AnchorRefs::new(&config(), Rect::default());
        let rect = resolve_anchor_rect(&a, &extent, &refs);
        // Margin box: origin x 1440, width 12240-2880 = 9360; center a 1000-wide
        // image: 1440 + (9360-1000)/2 = 1440 + 4180 = 5620.
        assert_eq!(rect.origin.x, Twip(5_620));
    }
}
