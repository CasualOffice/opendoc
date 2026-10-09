//! Where an anchored drawing sits on a web page.

use casual_doc_model::v1::{
    DrawingAnchor, HorizontalAlign, HorizontalAnchor, HorizontalPosition, InlineNode, PageMargins,
    VerticalAnchor, VerticalPosition, WrapMode,
};

use super::{Writer, emu};
use crate::ModelOutcome;
use crate::html::css::Declarations;

impl Writer<'_> {
    /// Places an anchored drawing (`anchor_css`), reporting a placement that
    /// is not the page's.
    pub(super) fn place(&mut self, anchor: &DrawingAnchor, css: &mut Declarations) {
        if !anchor_css(anchor, self.page_margins.as_ref(), css) {
            self.losses
                .record("html.anchor_position", ModelOutcome::Degraded);
        }
    }
}

/// How an anchored drawing sits on a web page.
///
/// - Text wraps around it (square, tight, through): it floats to the side it is
///   anchored to, its wrap distances as margins, a column offset as an indent.
/// - Text sits above and below it: it stands on its own line, aligned or
///   indented as anchored.
/// - Text does not wrap at all (in front of or behind the text): it is placed
///   at its offsets in its anchor paragraph's box, over or under the text, as
///   on the page. A page has a top and a left margin a web page does not, so an
///   offset from the page edge is taken from the text area's edge instead —
///   exact horizontally, and vertically exact for a drawing anchored in the
///   first lines of the page.
///
/// Returns whether the placement is the page's: only a drawing in front of or
/// behind the text, offset from its paragraph or line and from the column, the
/// margin or the page's left edge, lands exactly where the page puts it. A
/// float keeps its side but not its vertical offset or its wrap outline, and a
/// page-relative vertical offset is measured from the paragraph here; those
/// are approximations the caller reports (`html.anchor_position`).
fn anchor_css(
    anchor: &DrawingAnchor,
    margins: Option<&PageMargins>,
    css: &mut Declarations,
) -> bool {
    let side = match anchor.horizontal.position {
        HorizontalPosition::Align(HorizontalAlign::Right | HorizontalAlign::Outside) => {
            Some("right")
        }
        HorizontalPosition::Align(HorizontalAlign::Center) => None,
        HorizontalPosition::Align(HorizontalAlign::Left | HorizontalAlign::Inside) => Some("left"),
        HorizontalPosition::Offset(_) => Some("left"),
    };
    let horizontal_offset = match anchor.horizontal.position {
        HorizontalPosition::Offset(offset) => Some(
            offset
                - match anchor.horizontal.relative_from {
                    HorizontalAnchor::Page
                    | HorizontalAnchor::LeftMargin
                    | HorizontalAnchor::InsideMargin => {
                        margins.map_or(0, |margins| i64::from(margins.start_twips) * 635)
                    }
                    _ => 0,
                },
        ),
        HorizontalPosition::Align(_) => None,
    };
    let distances = anchor.wrap_distances;
    let mut exact = false;
    match anchor.wrap {
        WrapMode::Square | WrapMode::Tight | WrapMode::Through => {
            let side = side.unwrap_or("left");
            css.set("float", side);
            css.set("margin-top", emu(distances.top_emu));
            css.set("margin-bottom", emu(distances.bottom_emu));
            if side == "left" {
                css.set("margin-right", emu(distances.end_emu));
                if let Some(offset) = horizontal_offset.filter(|offset| *offset > 0) {
                    css.set("margin-left", emu(offset));
                }
            } else {
                css.set("margin-left", emu(distances.start_emu));
            }
        }
        WrapMode::TopAndBottom => {
            css.set("display", "block");
            match side {
                None => {
                    css.set("margin-left", "auto");
                    css.set("margin-right", "auto");
                }
                Some("right") => css.set("margin-left", "auto"),
                Some(_) => {
                    if let Some(offset) = horizontal_offset.filter(|offset| *offset > 0) {
                        css.set("margin-left", emu(offset));
                    }
                }
            }
        }
        WrapMode::None => {
            exact = matches!(
                anchor.vertical.relative_from,
                VerticalAnchor::Paragraph | VerticalAnchor::Line
            ) && matches!(anchor.vertical.position, VerticalPosition::Offset(_))
                && match anchor.horizontal.position {
                    HorizontalPosition::Offset(_) => matches!(
                        anchor.horizontal.relative_from,
                        HorizontalAnchor::Page
                            | HorizontalAnchor::Margin
                            | HorizontalAnchor::Column
                            | HorizontalAnchor::LeftMargin
                            | HorizontalAnchor::InsideMargin
                    ),
                    HorizontalPosition::Align(_) => matches!(
                        anchor.horizontal.relative_from,
                        HorizontalAnchor::Margin | HorizontalAnchor::Column
                    ),
                };
            css.set("position", "absolute");
            css.set("z-index", if anchor.behind_doc { "-1" } else { "1" });
            let mut shift = None;
            match anchor.horizontal.position {
                HorizontalPosition::Offset(_) => {
                    css.set("left", emu(horizontal_offset.unwrap_or(0)));
                }
                HorizontalPosition::Align(HorizontalAlign::Center) => {
                    css.set("left", "50%");
                    shift = Some("translateX(-50%)");
                }
                HorizontalPosition::Align(HorizontalAlign::Right | HorizontalAlign::Outside) => {
                    css.set("right", "0");
                }
                HorizontalPosition::Align(_) => css.set("left", "0"),
            }
            let top = match anchor.vertical.position {
                VerticalPosition::Offset(offset) => {
                    offset
                        - match anchor.vertical.relative_from {
                            VerticalAnchor::Page | VerticalAnchor::TopMargin => {
                                margins.map_or(0, |margins| i64::from(margins.top_twips) * 635)
                            }
                            _ => 0,
                        }
                }
                VerticalPosition::Align(_) => 0,
            };
            css.set("top", emu(top));
            if let Some(shift) = shift {
                let transform = css.get("transform").map_or_else(
                    || shift.to_owned(),
                    |existing| format!("{shift} {existing}"),
                );
                css.set("transform", transform);
            }
        }
    }
    exact
}

/// Whether a paragraph holds a drawing placed over or under its text — the
/// paragraph is then the box it is placed in.
pub(super) fn holds_free_drawing(inlines: &[InlineNode]) -> bool {
    let free =
        |anchor: Option<&DrawingAnchor>| anchor.is_some_and(|anchor| anchor.wrap == WrapMode::None);
    inlines.iter().any(|inline| match inline {
        InlineNode::AnchoredDrawing(drawing) => free(Some(&drawing.anchor)),
        InlineNode::Group(group) => free(group.anchor.as_ref()),
        InlineNode::TextBox(text_box) => free(text_box.anchor.as_ref()),
        InlineNode::Hyperlink(link) => holds_free_drawing(&link.inlines),
        InlineNode::Sdt(sdt) => holds_free_drawing(&sdt.inlines),
        InlineNode::Revision(revision) => holds_free_drawing(&revision.inlines),
        _ => false,
    })
}
