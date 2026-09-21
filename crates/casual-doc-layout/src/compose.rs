//! Composition — turning a shaped [`LineLayout`] into a [`DisplayList`].
//!
//! This is the seam between layout and rendering: each shaped glyph run is
//! translated to its position on the page and emitted as a paint item. The list
//! stays in device-independent twips (consistent with the whole engine); the
//! rendering backend applies the device scale (DPI × zoom) when it paints, which
//! is the "scale only at paint" rule from `43-…`.

use crate::block::{
    BlockFragment, BorderPattern, CellBorders, CellVerticalMerge, ParagraphDecor,
    ResolvedBorderSegment, ResolvedEdge,
};
use crate::display::{
    Color, DisplayList, Fill, Gradient, GradientKind, GradientStop, PaintItem, ShapeGeometry,
    ShapeOutline, Stroke,
};
use crate::page::{AnchorContent, AnchorStroke, Page, PlacedAnchor, ResolvedPageBorders};
use crate::text::{LineLayout, TextBoxContentLayout};
use crate::units::{Point, Rect, Size, Twip};

/// Width (twips) of a `bar` tab stop's vertical rule (~0.5pt, Word's hairline).
const BAR_TAB_WIDTH: Twip = Twip(10);

/// Width (twips) of a column separator rule (`w:cols/@w:sep`) — Word's ~0.5pt
/// hairline, the same weight as a bar-tab rule.
const COLUMN_SEPARATOR_WIDTH: Twip = Twip(10);

/// Stroke width (twips) of the footnote separator rule — Word's ~0.5pt hairline
/// between the body and the footnote band, the same weight as a column rule.
const FOOTNOTE_SEPARATOR_WIDTH: Twip = Twip(10);

/// Length (twips) of a *fresh* footnote separator: Word draws a short 2-inch
/// left-aligned hairline above a note first placed on this page. A note continued
/// from the previous page instead gets a full-band-width continuation rule.
pub(crate) const FOOTNOTE_SEPARATOR_LENGTH: Twip = Twip(2_880);

/// Stroke width (device px) of an inline text box's border (a hairline).
/// Builds a display list for one paragraph's shaped lines, placed with the
/// paragraph's top-left at `origin` (in twips). The shaper positions each glyph
/// run relative to the paragraph's own origin (run `origin` = the run's left edge
/// on its baseline); composition translates those into page coordinates.
#[must_use]
pub fn compose_paragraph(layout: &LineLayout, origin: Point) -> DisplayList {
    let mut list = DisplayList::new();
    // Tracks the top of the current line (twips from the paragraph content top) so
    // `bar` tab stops can be drawn as vertical rules spanning the line's box.
    let mut line_top = Twip::ZERO;
    for line in &layout.lines {
        let current_top = line_top;
        if line.clip {
            // Exact line spacing clips only the block axis. Use a deliberately
            // huge horizontal span so hanging indents and positioned tabs remain
            // visible while glyph ink cannot escape into the next line.
            const HORIZONTAL_CLIP_EXTENT: Twip = Twip(1 << 27);
            list.push(PaintItem::PushClip(Rect::new(
                Point::new(origin.x - HORIZONTAL_CLIP_EXTENT, origin.y + current_top),
                Size::new(HORIZONTAL_CLIP_EXTENT + HORIZONTAL_CLIP_EXTENT, line.height),
            )));
        }
        // Bar tab stops (`w:tab@val="bar"`): a thin vertical rule at each stop's x,
        // spanning the full line height, painted behind the glyphs.
        for &bar_x in &line.bars {
            list.push(PaintItem::Rect {
                rect: Rect::new(
                    Point::new(origin.x + bar_x, origin.y + line_top),
                    Size::new(BAR_TAB_WIDTH, line.height),
                ),
                fill: Some(Color::BLACK),
                stroke: None,
            });
        }
        line_top = line_top + line.height;
        for run in &line.runs {
            let placed_x = origin.x + run.origin.x;
            let baseline_y = origin.y + run.origin.y;
            // Run backgrounds fill the run's glyph box *before* the glyphs (behind
            // the text): the box spans the run's total advance horizontally and the
            // line's ascent+descent vertically. Shading (`w:rPr/w:shd`) is painted
            // first, then the highlight (`w:highlight`) over it — Word's order.
            let run_box = |advance: Twip| {
                Rect::new(
                    Point::new(placed_x, baseline_y - line.ascent),
                    Size::new(advance, line.ascent + line.descent),
                )
            };
            if run.shading.is_some() || run.highlight.is_some() {
                let advance = run.glyphs.iter().fold(Twip::ZERO, |acc, g| acc + g.advance);
                if let Some(shading) = run.shading {
                    list.push(PaintItem::Rect {
                        rect: run_box(advance),
                        fill: Some(rgba(shading)),
                        stroke: None,
                    });
                }
                if let Some(highlight) = run.highlight {
                    list.push(PaintItem::Rect {
                        rect: run_box(advance),
                        fill: Some(rgba(highlight)),
                        stroke: None,
                    });
                }
            }
            let mut placed = run.clone();
            placed.origin = Point::new(placed_x, baseline_y);
            list.push(PaintItem::Glyphs { run: placed });
            // Run furniture painted OVER the glyphs (`docs/105` FID-L-14): the
            // `w:bdr` box frames the run the way a page border frames the page,
            // and `w:em` marks sit clear of the glyph box above or below it.
            // Both are keyed off the same run box the shading/highlight use.
            if run.decoration.border.is_some() || run.decoration.emphasis.is_some() {
                let advance = run.glyphs.iter().fold(Twip::ZERO, |acc, g| acc + g.advance);
                if let Some(edge) = run.decoration.border {
                    compose_run_border(&mut list, run_box(advance), edge);
                }
                if let Some(mark) = run.decoration.emphasis {
                    compose_emphasis_marks(
                        &mut list,
                        run,
                        Point::new(placed_x, baseline_y),
                        line.ascent,
                        line.descent,
                        mark,
                    );
                }
            }
        }
        // Inline images (embedded pictures): the box's `origin` is already
        // paragraph-absolute; translate into page space and emit a blit. The
        // backend resolves `media` to pixels and scales them into the box.
        for image in &line.images {
            list.push(PaintItem::Image {
                media: image.media.clone(),
                rect: Rect::new(
                    Point::new(origin.x + image.origin.x, origin.y + image.origin.y),
                    image.size,
                ),
                crop: image.crop,
                // Inline images are not rotated (a:xfrm applies to floats).
                transform: None,
                opacity: image.opacity,
            });
        }
        // Inline text boxes: the fill and border paint first, then the box's flowed
        // fragments compose offset into it by the internal margin — the *same*
        // fragment composition the body and table cells use (the uniform-flow
        // invariant), so a text box renders paragraphs, nested tables, and images.
        for text_box in &line.text_boxes {
            let box_origin = Point::new(origin.x + text_box.origin.x, origin.y + text_box.origin.y);
            let box_rect = Rect::new(box_origin, text_box.size);
            if let Some(fill) = text_box.fill {
                list.push(PaintItem::Rect {
                    rect: box_rect,
                    fill: Some(rgba(fill)),
                    stroke: None,
                });
            }
            if let Some(border) = text_box.border {
                list.push(PaintItem::Rect {
                    rect: box_rect,
                    fill: None,
                    stroke: Some(Stroke {
                        color: rgba(border.color),
                        width: stroke_px(border.width),
                    }),
                });
            }
            let content_origin = Point::new(
                box_origin.x + text_box.content_layout.origin.x,
                box_origin.y + text_box.content_layout.origin.y,
            );
            let clip = text_box_clip(box_rect, text_box.content_layout);
            list.push(PaintItem::PushClip(clip));
            compose_blocks(&mut list, &text_box.blocks, content_origin);
            list.push(PaintItem::PopClip);
        }
        // Inline horizontal rules (`w:pict` / `v:rect@o:hr`): a filled rectangle
        // spanning (a fraction of) the content width, translated into page space.
        for rule in &line.rules {
            list.push(PaintItem::Rect {
                rect: Rect::new(
                    Point::new(origin.x + rule.origin.x, origin.y + rule.origin.y),
                    rule.size,
                ),
                fill: Some(rgba(rule.color)),
                stroke: None,
            });
        }
        if line.clip {
            list.push(PaintItem::PopClip);
        }
    }
    list
}

/// A shape/line stroke's device-pixel width: the twip width at 96 DPI, floored at
/// a 1px hairline (matching the inline text-box border). A `0`-twip outline (the
/// common thin DrawingML `a:ln`) is a hairline.
fn stroke_px(width: Twip) -> f32 {
    (width.raw() as f32 * 96.0 / 1440.0).max(1.0)
}

/// Builds a [`Color`] from a packed RGBA quad.
fn rgba(c: [u8; 4]) -> Color {
    Color {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}

/// Translates a resolved model fill (`a:solidFill`/`a:gradFill`) into the display
/// [`Fill`]. Gradient stop positions (per-100000) become `0.0..=1.0`; a linear
/// angle (60000ths of a degree) becomes degrees clockwise from +x.
fn fill_to_display(fill: &casual_doc_model::v1::Fill) -> Fill {
    use casual_doc_model::v1::{Fill as ModelFill, GradientKind as ModelGradientKind};
    match fill {
        ModelFill::Solid(color) => Fill::Solid(rgba([color.r, color.g, color.b, color.a])),
        ModelFill::Gradient { stops, kind } => Fill::Gradient(Gradient {
            stops: stops
                .iter()
                .map(|stop| GradientStop {
                    position: (stop.position as f32 / 100_000.0).clamp(0.0, 1.0),
                    color: rgba([stop.color.r, stop.color.g, stop.color.b, stop.color.a]),
                })
                .collect(),
            kind: match kind {
                ModelGradientKind::Linear { angle } => GradientKind::Linear {
                    angle_deg: *angle as f32 / 60_000.0,
                },
                ModelGradientKind::Radial => GradientKind::Radial,
            },
        }),
    }
}

/// Translates a resolved anchor stroke into the display [`ShapeOutline`].
fn shape_outline(stroke: &AnchorStroke) -> ShapeOutline {
    ShapeOutline {
        color: rgba(stroke.color),
        width: stroke_px(stroke.width),
        dash: stroke.dash,
    }
}

/// Builds the display list for a whole paginated [`Page`]: the running header, the
/// body content (each placed paragraph or table row), then the running footer —
/// each fragment composed at its position on the page. Header and footer are laid
/// out in their reserved bands by [`crate::running::place_running_content`] and
/// their fields resolved by [`crate::paginate::resolve_fields`]; here they paint
/// exactly like body fragments.
#[must_use]
pub fn compose_page(page: &Page) -> DisplayList {
    let mut list = DisplayList::new();
    // The float layer is a single stable z-order: `behindDoc` floats paint below
    // the text layer, the rest above, each band ordered by (relativeHeight,
    // document order) so group children paint in child order and a shape can sit
    // behind the group's picture while a later shape sits in front.
    let mut floats: Vec<&PlacedAnchor> = page.anchored.iter().collect();
    floats.sort_by_key(|anchor| anchor.z);
    for anchor in floats.iter().filter(|anchor| anchor.behind_doc) {
        compose_anchor(&mut list, anchor);
    }
    // Column separator rules (`w:cols/@w:sep`): a thin vertical hairline centered in
    // each inter-column gap, painted under the text layer (the gap carries no
    // glyphs, so z-order is immaterial).
    for sep in &page.separators {
        list.push(PaintItem::Line {
            from: Point::new(sep.x, sep.top),
            to: Point::new(sep.x, sep.bottom),
            stroke: Stroke {
                color: Color::BLACK,
                width: stroke_px(COLUMN_SEPARATOR_WIDTH),
            },
        });
    }
    for placed in &page.header {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for placed in &page.placed {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    // The footnote separator rule: Word draws a short hairline between the body
    // and the footnote band (a full-band-width rule when the band continues a note
    // from the previous page). Painted before the note bodies as non-text
    // furniture — it never enters the caret/selection model.
    compose_footnote_separators(&mut list, page);
    for placed in &page.footnotes {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for placed in &page.footer {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    // Margin line numbers (`w:lnNumType`): page furniture in the margin, already
    // positioned in page-local twips by the post-pagination pass. Painted after
    // the body — nothing overlaps them, the margin carries no glyphs — and before
    // the page border, which frames everything (`docs/105` FID-L-09).
    for stamp in &page.line_numbers {
        list.push(PaintItem::Glyphs {
            run: stamp.run.clone(),
        });
    }
    // Page borders (`w:pgBorders`) frame the page as furniture, painted on top of
    // the body so a text-offset frame over wide content still reads as a frame.
    if let Some(borders) = &page.page_borders {
        compose_page_borders(&mut list, borders);
    }
    for anchor in floats.iter().filter(|anchor| !anchor.behind_doc) {
        compose_anchor(&mut list, anchor);
    }
    list
}

/// Paints the footnote separator rule(s) at the top of a page's footnote band(s).
///
/// Word separates the body from the footnotes with a short (~2 inch) left-aligned
/// hairline for a note first placed on the page, and a full-band-width rule when
/// the band continues a note carried over from the previous page. Each physical
/// band (one per column, keyed by its left edge) gets one rule at its top edge —
/// the y where its first note fragment sits. The rule is layout furniture: it is
/// emitted only into the display list, never into the caret/selection model.
fn compose_footnote_separators(list: &mut DisplayList, page: &Page) {
    if page.footnotes.is_empty() {
        return;
    }
    // A page whose band originates no reference of its own is showing only notes
    // continued from an earlier page, so its separator spans the full band width.
    let continuation = !crate::notes::page_originates_footnote(page);

    // Group the placed note fragments into physical bands by their left edge, and
    // for each band take its top y and widest fragment as the band geometry.
    let mut bands: std::collections::BTreeMap<Twip, (Twip, Twip)> =
        std::collections::BTreeMap::new();
    for placed in &page.footnotes {
        let entry = bands
            .entry(placed.rect.origin.x)
            .or_insert((placed.rect.origin.y, placed.rect.size.width));
        entry.0 = entry.0.min(placed.rect.origin.y);
        entry.1 = entry.1.max(placed.rect.size.width);
    }

    for (x, (top, width)) in bands {
        let length = if continuation {
            width
        } else {
            FOOTNOTE_SEPARATOR_LENGTH.min(width)
        };
        list.push(PaintItem::Line {
            from: Point::new(x, top),
            to: Point::new(x + length, top),
            stroke: Stroke {
                color: Color::BLACK,
                width: stroke_px(FOOTNOTE_SEPARATOR_WIDTH),
            },
        });
    }
}

/// Paints a resolved page-border frame: each present edge as a filled band along
/// its side of the frame rectangle, reusing the shared edge painter (so pattern
/// styles — double/dashed/… — match paragraph and table borders).
fn compose_page_borders(list: &mut DisplayList, borders: &ResolvedPageBorders) {
    let rect = borders.rect;
    if let Some(edge) = borders.top {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(rect.size.width, edge.width)),
            edge,
            BorderAxis::Horizontal,
        );
    }
    if let Some(edge) = borders.bottom {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x, rect.bottom() - edge.width),
                Size::new(rect.size.width, edge.width),
            ),
            edge,
            BorderAxis::Horizontal,
        );
    }
    if let Some(edge) = borders.start {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(edge.width, rect.size.height)),
            edge,
            BorderAxis::Vertical,
        );
    }
    if let Some(edge) = borders.end {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.right() - edge.width, rect.origin.y),
                Size::new(edge.width, rect.size.height),
            ),
            edge,
            BorderAxis::Vertical,
        );
    }
}

/// Paints a run border (`w:bdr`) as a four-sided box around the run's glyph box,
/// each side going through the shared [`paint_border`] so the authored width and
/// pattern (double / dashed / dot-dash / …) are the same ink a paragraph, cell,
/// or page border of that style produces.
///
/// The box is the run box — the run's advance by the line's ascent+descent — and
/// is drawn inward from it, so a bordered run never grows the line box. Word
/// insets the glyphs slightly from the frame; the engine does not reserve that
/// padding (it would change line breaking), so a tight box is the deliberate
/// difference (`docs/105` FID-L-14).
fn compose_run_border(list: &mut DisplayList, rect: Rect, edge: ResolvedEdge) {
    if rect.size.width <= Twip::ZERO || rect.size.height <= Twip::ZERO {
        return;
    }
    paint_border(
        list,
        Rect::new(rect.origin, Size::new(rect.size.width, edge.width)),
        edge,
        BorderAxis::Horizontal,
    );
    paint_border(
        list,
        Rect::new(
            Point::new(rect.origin.x, rect.bottom() - edge.width),
            Size::new(rect.size.width, edge.width),
        ),
        edge,
        BorderAxis::Horizontal,
    );
    paint_border(
        list,
        Rect::new(rect.origin, Size::new(edge.width, rect.size.height)),
        edge,
        BorderAxis::Vertical,
    );
    paint_border(
        list,
        Rect::new(
            Point::new(rect.right() - edge.width, rect.origin.y),
            Size::new(edge.width, rect.size.height),
        ),
        edge,
        BorderAxis::Vertical,
    );
}

/// The diameter of an emphasis mark as a fraction of the run's font size —
/// Word's bōten is roughly a quarter em, small enough to read as a mark rather
/// than a character.
const EMPHASIS_MARK_EM_DIVISOR: i32 = 4;

/// Paints an emphasis mark (`w:em`) once per non-blank cluster of `run`.
///
/// Word draws the mark per *character*, centered on the character's advance and
/// clear of its em box — above the text for `dot`/`comma`/`circle`, below the
/// baseline for `underDot`. Clusters are the unit, not glyphs: one mark belongs
/// over one grapheme even when the shaper emitted several glyphs for it, so
/// consecutive glyphs sharing a `cluster` offset are grouped. Blank clusters
/// carry no mark (Word does not mark the spaces between words).
///
/// The marks are geometry, not glyphs, deliberately: the mark characters
/// (`U+3001`, `U+25CB`, …) are missing from most bundled faces, so shaping them
/// would render tofu on the deterministic build. `dot`/`underDot` are filled
/// circles and `circle` is a hollow one, matching Word; `comma` (Word's sesame
/// mark, a filled teardrop) is approximated by a narrow filled ellipse — the
/// only one of the four that is not shape-exact, recorded as such rather than
/// left ambiguous.
///
/// `list` receives nothing for a run with no measurable clusters (a marker or
/// leader run, or an empty one), and marker/leader runs are skipped outright:
/// they are rendering artifacts, not model text Word would mark.
///
/// **Partial, stated:** Word grows the line box so a marked line gains room for
/// its marks; this pass does not. It is composition, downstream of the shaper's
/// line metrics, so reserving the space would have to happen during shaping and
/// would move line breaking and pagination. An above-text mark is therefore drawn
/// just outside the run's ascent and can encroach on the descenders of the line
/// above when line spacing is tight. Reserving the band is the follow-up; drawing
/// the mark at the correct position relative to its own character is what makes
/// the text readable, and that is what this does.
fn compose_emphasis_marks(
    list: &mut DisplayList,
    run: &crate::text::GlyphRun,
    origin: Point,
    line_ascent: Twip,
    line_descent: Twip,
    mark: casual_doc_model::v1::EmphasisMark,
) {
    use casual_doc_model::v1::EmphasisMark;

    if run.is_marker || run.is_leader || matches!(mark, EmphasisMark::None) {
        return;
    }
    let diameter = Twip((run.size.raw() / EMPHASIS_MARK_EM_DIVISOR).max(1));
    // A deliberately tight optical gap, and it is tight for a reason: the line box
    // is NOT grown to reserve room for the mark (that would move line breaking and
    // pagination), so every twip of clearance is a twip of intrusion into the line
    // above. See the doc comment.
    let gap = Twip((diameter.raw() / 4).max(1));
    // The run's OWN metrics where the shaper recorded them, so a small run on a
    // tall line marks its own text rather than the line's extremes.
    let ascent = if run.ascent > Twip::ZERO {
        run.ascent
    } else {
        line_ascent
    };
    let descent = if run.descent > Twip::ZERO {
        run.descent
    } else {
        line_descent
    };
    let top = match mark {
        EmphasisMark::UnderDot => origin.y + descent + gap,
        _ => origin.y - ascent - gap - diameter,
    };
    // Narrower than tall for the sesame approximation; circular otherwise.
    let width = match mark {
        EmphasisMark::Comma => Twip((diameter.raw() * 3 / 5).max(1)),
        _ => diameter,
    };
    let color = rgba(run.color);

    let mut x = origin.x;
    let mut index = 0;
    while index < run.glyphs.len() {
        let cluster = run.glyphs[index].cluster;
        let mut advance = Twip::ZERO;
        let mut blank = true;
        while index < run.glyphs.len() && run.glyphs[index].cluster == cluster {
            advance = advance + run.glyphs[index].advance;
            blank &= run.glyphs[index].is_whitespace;
            index += 1;
        }
        let next_x = x + advance;
        if !blank && advance > Twip::ZERO {
            let center = Twip(x.raw() + advance.raw() / 2);
            let rect = Rect::new(
                Point::new(Twip(center.raw() - width.raw() / 2), top),
                Size::new(width, diameter),
            );
            match mark {
                EmphasisMark::Circle => list.push(PaintItem::Ellipse {
                    rect,
                    fill: None,
                    stroke: Some(Stroke {
                        color,
                        width: stroke_px(Twip((diameter.raw() / 5).max(1))),
                    }),
                }),
                _ => list.push(PaintItem::Ellipse {
                    rect,
                    fill: Some(color),
                    stroke: None,
                }),
            }
        }
        x = next_x;
    }
}

/// Paints one placed float at its resolved rectangle: an image blit, a filled/
/// stroked rectangle, a line/connector, or a text box (fill + border + its flowed
/// content, offset into the box by the internal margin, exactly like an inline
/// text box).
fn compose_anchor(list: &mut DisplayList, anchor: &PlacedAnchor) {
    match &anchor.content {
        AnchorContent::Image {
            media,
            crop,
            border,
            opacity,
        } => {
            list.push(PaintItem::Image {
                media: media.clone(),
                rect: anchor.rect,
                crop: *crop,
                transform: anchor.transform,
                opacity: *opacity,
            });
            // A framed picture's `pic:spPr/a:ln` paints as a stroked rectangle over
            // the picture box (pictures are rectangular). It rides the same
            // [`PaintItem::Shape`] outline path as shape borders so its color,
            // width, and preset dash (`a:prstDash`) all paint — a dashed frame
            // dashes, a solid frame stays solid.
            if let Some(border) = border {
                list.push(PaintItem::Shape {
                    geometry: ShapeGeometry::Rect { rect: anchor.rect },
                    fill: None,
                    stroke: Some(shape_outline(border)),
                    head_end: None,
                    tail_end: None,
                    transform: anchor.transform,
                });
            }
        }
        AnchorContent::Rectangle { fill, stroke } => {
            list.push(PaintItem::Shape {
                geometry: ShapeGeometry::Rect { rect: anchor.rect },
                fill: fill.as_ref().map(fill_to_display),
                stroke: stroke.as_ref().map(shape_outline),
                head_end: None,
                tail_end: None,
                transform: anchor.transform,
            });
        }
        AnchorContent::Ellipse { fill, stroke } => {
            list.push(PaintItem::Shape {
                geometry: ShapeGeometry::Ellipse { rect: anchor.rect },
                fill: fill.as_ref().map(fill_to_display),
                stroke: stroke.as_ref().map(shape_outline),
                head_end: None,
                tail_end: None,
                transform: anchor.transform,
            });
        }
        AnchorContent::RoundedRectangle {
            radius,
            fill,
            stroke,
        } => {
            list.push(PaintItem::Shape {
                geometry: ShapeGeometry::RoundedRect {
                    rect: anchor.rect,
                    radius: *radius,
                },
                fill: fill.as_ref().map(fill_to_display),
                stroke: stroke.as_ref().map(shape_outline),
                head_end: None,
                tail_end: None,
                transform: anchor.transform,
            });
        }
        AnchorContent::Polygon {
            points,
            fill,
            stroke,
        } => {
            list.push(PaintItem::Shape {
                geometry: ShapeGeometry::Polygon {
                    points: points.clone(),
                },
                fill: fill.as_ref().map(fill_to_display),
                stroke: stroke.as_ref().map(shape_outline),
                head_end: None,
                tail_end: None,
                transform: anchor.transform,
            });
        }
        AnchorContent::Line {
            from,
            to,
            stroke,
            head_end,
            tail_end,
        } => {
            list.push(PaintItem::Shape {
                geometry: ShapeGeometry::Line {
                    from: *from,
                    to: *to,
                },
                fill: None,
                stroke: Some(shape_outline(stroke)),
                head_end: *head_end,
                tail_end: *tail_end,
                transform: anchor.transform,
            });
        }
        AnchorContent::TextBox {
            blocks,
            fill,
            border,
            content_layout,
        } => {
            if let Some(fill) = fill {
                // The box background paints as a shape rect so a gradient fill
                // (`a:gradFill`) is honored, not just a solid color.
                list.push(PaintItem::Shape {
                    geometry: ShapeGeometry::Rect { rect: anchor.rect },
                    fill: Some(fill_to_display(fill)),
                    stroke: None,
                    head_end: None,
                    tail_end: None,
                    transform: anchor.transform,
                });
            }
            if let Some(border) = border {
                list.push(PaintItem::Rect {
                    rect: anchor.rect,
                    fill: None,
                    stroke: Some(Stroke {
                        color: rgba(border.color),
                        width: stroke_px(border.width),
                    }),
                });
            }
            let content_origin = Point::new(
                anchor.rect.origin.x + content_layout.origin.x,
                anchor.rect.origin.y + content_layout.origin.y,
            );
            let clip = text_box_clip(anchor.rect, *content_layout);
            list.push(PaintItem::PushClip(clip));
            compose_blocks(list, blocks, content_origin);
            list.push(PaintItem::PopClip);
        }
    }
}

/// Builds the clip rectangle that contains a text box's flowed content.
///
/// Text is always wrapped to the box's inner width, but a centered or
/// unbreakable label can still be wider than the box, and a DrawingML shape
/// never paints its text past its own horizontal bounds. So horizontal clipping
/// to the box is applied **unconditionally as a backstop** — this is what keeps a
/// label like `Paint` from spilling past a narrow node's right edge — regardless
/// of the authored `horzOverflow` policy (the schema default is `overflow`, which
/// Word honors only up to the shape's own edge).
///
/// Vertical clipping still follows the authored `vertOverflow` policy so autofit
/// growth is not hidden; the unclipped axis receives a deliberately broad
/// page-independent span, and any enclosing table/page clip still intersects it
/// in the renderer.
fn text_box_clip(rect: Rect, layout: TextBoxContentLayout) -> Rect {
    const PAD: i32 = 1_000_000;
    let (y, height) = if layout.clip_vertical {
        (rect.origin.y, rect.size.height)
    } else {
        (
            Twip(rect.origin.y.raw().saturating_sub(PAD)),
            Twip(rect.size.height.raw().saturating_add(PAD.saturating_mul(2))),
        )
    };
    Rect::new(
        Point::new(rect.origin.x, y),
        Size::new(rect.size.width, height),
    )
}

/// Composes one block fragment at `origin` (top-left, twips) into `list`.
fn compose_fragment(list: &mut DisplayList, fragment: &BlockFragment, origin: Point) {
    match fragment {
        BlockFragment::Paragraph {
            lines,
            box_metrics,
            decor,
            ..
        } => {
            // The leading indent (`w:ind@start`) shifts the whole paragraph's
            // content origin to the indented column; the shaper already wrapped its
            // lines to the reduced width, and the first-line indent is baked into
            // the first line's run origins.
            let content_origin = Point::new(
                origin.x + box_metrics.indent_start,
                origin.y + box_metrics.space_before,
            );
            // Background shading + borders are painted first, behind the text.
            if !decor.is_empty() {
                let box_width = Twip(
                    (decor.width.raw()
                        - box_metrics.indent_start.raw()
                        - box_metrics.indent_end.raw())
                    .max(0),
                );
                let box_rect = Rect::new(content_origin, Size::new(box_width, lines.height()));
                compose_paragraph_decor(list, box_rect, decor);
            }
            list.items
                .extend(compose_paragraph(lines, content_origin).items);
        }
        BlockFragment::TableRow { cells, clip, .. } => {
            let row_height = fragment.height();
            for cell in cells {
                if matches!(cell.vertical_merge, CellVerticalMerge::Continue) {
                    continue;
                }
                let cell_height = cell.box_height(row_height);
                let cell_origin = Point::new(origin.x + cell.x, origin.y + cell.cell_spacing.top);
                let cell_rect = Rect::new(cell_origin, Size::new(cell.width, cell_height));
                if !cell.table_borders.is_empty() {
                    let table_slot = Rect::new(
                        Point::new(cell_origin.x - cell.cell_spacing.start, origin.y),
                        Size::new(
                            cell.width + cell.cell_spacing.start + cell.cell_spacing.end,
                            cell_height + cell.cell_spacing.top + cell.cell_spacing.bottom,
                        ),
                    );
                    compose_cell_borders(list, table_slot, &cell.table_borders);
                }
                // Cell background shading (`w:shd`) fills the cell before anything
                // else, behind both the grid line and the content.
                if let Some(fill) = cell.shading {
                    list.push(PaintItem::Rect {
                        rect: cell_rect,
                        fill: Some(rgba(fill)),
                        stroke: None,
                    });
                }
                // Only the cell's RESOLVED borders are drawn — no default grid
                // line. Word draws nothing for a border-less cell (common in
                // layout tables); a gray default grid would show boundaries Word
                // hides. Bordered tables get their borders via the resolved edges.
                compose_cell_borders(list, cell_rect, &cell.borders);
                // Content is inset by the cell margins (`w:tcMar`/`w:tblCellMar`)
                // and shifted down by the vertical-alignment slack (`w:vAlign`), so
                // it no longer hugs the top-left grid line; bottom-aligned labels
                // sit on the answer line. The shading/border/clip rects still span
                // the whole cell — only the flowed content moves.
                let content_origin = Point::new(
                    cell_origin.x + cell.margins.start,
                    cell_origin.y + cell.content_y_offset(cell_height),
                );
                // An `exact` row height clips content that overflows the cell.
                if *clip {
                    list.push(PaintItem::PushClip(cell_rect));
                    compose_blocks(list, &cell.blocks, content_origin);
                    list.push(PaintItem::PopClip);
                } else {
                    compose_blocks(list, &cell.blocks, content_origin);
                }
            }
        }
    }
}

/// Maximum number of on-runs expanded from one dashed edge. A pathological
/// sub-twip pattern falls back to one solid band instead of growing the display
/// list without bound.
const MAX_BORDER_PATTERN_RECTS: usize = 2_048;

#[derive(Clone, Copy)]
enum BorderAxis {
    Horizontal,
    Vertical,
}

/// Paints a cell's resolved (border-conflict-winning) edges. Horizontal sides
/// may be independently resolved at abutting grid boundaries.
fn compose_cell_borders(list: &mut DisplayList, rect: Rect, borders: &CellBorders) {
    compose_horizontal_border(list, rect, borders.top, &borders.top_segments, false);
    compose_horizontal_border(list, rect, borders.bottom, &borders.bottom_segments, true);
    if let Some(e) = borders.start {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(e.width, rect.size.height)),
            e,
            BorderAxis::Vertical,
        );
    }
    if let Some(e) = borders.end {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.right() - e.width, rect.origin.y),
                Size::new(e.width, rect.size.height),
            ),
            e,
            BorderAxis::Vertical,
        );
    }
}

fn compose_horizontal_border(
    list: &mut DisplayList,
    rect: Rect,
    fallback: Option<ResolvedEdge>,
    segments: &[ResolvedBorderSegment],
    bottom: bool,
) {
    if segments.is_empty() {
        let Some(edge) = fallback else {
            return;
        };
        let y = if bottom {
            rect.bottom() - edge.width
        } else {
            rect.origin.y
        };
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x, y),
                Size::new(rect.size.width, edge.width),
            ),
            edge,
            BorderAxis::Horizontal,
        );
        return;
    }
    for segment in segments {
        let y = if bottom {
            rect.bottom() - segment.edge.width
        } else {
            rect.origin.y
        };
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x + segment.offset, y),
                Size::new(segment.length, segment.edge.width),
            ),
            segment.edge,
            BorderAxis::Horizontal,
        );
    }
}

fn paint_border(list: &mut DisplayList, rect: Rect, edge: ResolvedEdge, axis: BorderAxis) {
    match edge.pattern {
        BorderPattern::Solid => push_border_rect(list, rect, edge.color),
        BorderPattern::Double => {
            let thickness = match axis {
                BorderAxis::Horizontal => rect.size.height,
                BorderAxis::Vertical => rect.size.width,
            };
            let band = Twip((thickness.raw() / 3).max(1));
            match axis {
                BorderAxis::Horizontal => {
                    push_border_rect(
                        list,
                        Rect::new(rect.origin, Size::new(rect.size.width, band)),
                        edge.color,
                    );
                    push_border_rect(
                        list,
                        Rect::new(
                            Point::new(rect.origin.x, rect.bottom() - band),
                            Size::new(rect.size.width, band),
                        ),
                        edge.color,
                    );
                }
                BorderAxis::Vertical => {
                    push_border_rect(
                        list,
                        Rect::new(rect.origin, Size::new(band, rect.size.height)),
                        edge.color,
                    );
                    push_border_rect(
                        list,
                        Rect::new(
                            Point::new(rect.right() - band, rect.origin.y),
                            Size::new(band, rect.size.height),
                        ),
                        edge.color,
                    );
                }
            }
        }
        pattern => {
            if let Some(rects) = patterned_rects(rect, edge.width, pattern, axis) {
                for dash in rects {
                    push_border_rect(list, dash, edge.color);
                }
            } else {
                push_border_rect(list, rect, edge.color);
            }
        }
    }
}

fn patterned_rects(
    rect: Rect,
    width: Twip,
    pattern: BorderPattern,
    axis: BorderAxis,
) -> Option<Vec<Rect>> {
    let runs: &[(i32, i32)] = match pattern {
        BorderPattern::Dotted => &[(1, 1)],
        BorderPattern::Dashed => &[(3, 2)],
        BorderPattern::DotDash => &[(1, 1), (3, 1)],
        BorderPattern::DotDotDash => &[(1, 1), (1, 1), (3, 1)],
        BorderPattern::Solid | BorderPattern::Double => return Some(vec![rect]),
    };
    let start = match axis {
        BorderAxis::Horizontal => rect.origin.x.raw(),
        BorderAxis::Vertical => rect.origin.y.raw(),
    };
    let total = match axis {
        BorderAxis::Horizontal => rect.size.width.raw(),
        BorderAxis::Vertical => rect.size.height.raw(),
    }
    .max(0);
    let end = start.saturating_add(total);
    let unit = width.raw().max(1);
    let cycle_units = runs
        .iter()
        .map(|(on, off)| on.saturating_add(*off))
        .sum::<i32>();
    let cycle = unit.saturating_mul(cycle_units).max(1);
    let mut out = Vec::new();
    let mut cursor = start - start.rem_euclid(cycle);
    let mut run_index = 0usize;
    while cursor < end {
        let (on_units, off_units) = runs[run_index % runs.len()];
        let on = unit.saturating_mul(on_units);
        let paint_start = cursor.max(start);
        let paint_end = cursor.saturating_add(on).min(end);
        if paint_start < paint_end {
            if out.len() == MAX_BORDER_PATTERN_RECTS {
                return None;
            }
            let dash = match axis {
                BorderAxis::Horizontal => Rect::new(
                    Point::new(Twip(paint_start), rect.origin.y),
                    Size::new(Twip(paint_end - paint_start), rect.size.height),
                ),
                BorderAxis::Vertical => Rect::new(
                    Point::new(rect.origin.x, Twip(paint_start)),
                    Size::new(rect.size.width, Twip(paint_end - paint_start)),
                ),
            };
            out.push(dash);
        }
        cursor = cursor.saturating_add(on);
        cursor = cursor.saturating_add(unit.saturating_mul(off_units));
        run_index += 1;
    }
    Some(out)
}

fn push_border_rect(list: &mut DisplayList, rect: Rect, color: [u8; 4]) {
    list.push(PaintItem::Rect {
        rect,
        fill: Some(Color {
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        }),
        stroke: None,
    });
}

/// Paints a paragraph's background shading (`w:shd`) as a fill covering `rect`
/// and its borders (`w:pBdr`) as filled edge rects, both behind the text.
fn compose_paragraph_decor(list: &mut DisplayList, rect: Rect, decor: &ParagraphDecor) {
    if let Some(fill) = decor.shading {
        list.push(PaintItem::Rect {
            rect,
            fill: Some(rgba(fill)),
            stroke: None,
        });
    }
    let b = &decor.borders;
    if let Some(e) = b.top {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(rect.size.width, e.width)),
            e,
            BorderAxis::Horizontal,
        );
    }
    if let Some(e) = b.bottom {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x, rect.bottom() - e.width),
                Size::new(rect.size.width, e.width),
            ),
            e,
            BorderAxis::Horizontal,
        );
    }
    if let Some(e) = b.start {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(e.width, rect.size.height)),
            e,
            BorderAxis::Vertical,
        );
    }
    if let Some(e) = b.end {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.right() - e.width, rect.origin.y),
                Size::new(e.width, rect.size.height),
            ),
            e,
            BorderAxis::Vertical,
        );
    }
}

/// Composes a vertical stack of block fragments (a table cell's content) starting
/// at `origin`, advancing by each fragment's height.
fn compose_blocks(list: &mut DisplayList, blocks: &[BlockFragment], origin: Point) {
    let mut y = origin.y;
    for block in blocks {
        compose_fragment(list, block, Point::new(origin.x, y));
        y = y + block.height();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelPos, ModelRange};
    use crate::shape::ParleyShaper;
    use crate::text::{Decoration, FontId, LineConstraints, LineShaper, StyledRun};
    use crate::units::Twip;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::DashStyle;

    #[test]
    fn compose_places_glyph_runs_at_the_paragraph_origin() {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(1, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Hi".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(11),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let origin = Point::new(Twip::from_points(72), Twip::from_points(72));
        let list = compose_paragraph(&layout, origin);
        assert!(
            !list.items.is_empty(),
            "the paragraph composes to paint items"
        );
        let PaintItem::Glyphs { run } = &list.items[0] else {
            panic!("expected a glyph run");
        };
        // The run is translated by the paragraph origin (x at least the left margin).
        assert!(run.origin.x.raw() >= origin.x.raw());
        assert!(run.origin.y.raw() >= origin.y.raw());
    }

    #[test]
    fn an_exact_height_line_clips_oversized_glyph_ink_to_its_line_box() {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(2, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Oversized".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(24),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                line_exact: Some(Twip(120)),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 9)),
        );
        assert_eq!(layout.lines[0].height, Twip(120));
        assert!(layout.lines[0].clip);

        let origin = Point::new(Twip(50), Twip(75));
        let list = compose_paragraph(&layout, origin);
        let PaintItem::PushClip(clip) = &list.items[0] else {
            panic!("an exact line starts a vertical clip");
        };
        assert_eq!(clip.origin.y, origin.y);
        assert_eq!(clip.size.height, Twip(120));
        assert!(matches!(list.items.last(), Some(PaintItem::PopClip)));
    }

    #[test]
    fn an_exact_row_clips_and_draws_its_resolved_borders() {
        use crate::block::{
            BlockFragment, CellBorders, CellContentMargins, CellFragment, CellVAlign,
            CellVerticalMerge, ResolvedEdge,
        };
        use crate::units::Size;
        let cell = CellFragment {
            id: NodeId::from_parts(1, 1).unwrap(),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            cell_spacing: Default::default(),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders {
                bottom: Some(ResolvedEdge {
                    color: [10, 20, 30, 255],
                    width: Twip(40),
                    pattern: BorderPattern::Solid,
                }),
                ..CellBorders::default()
            },
            table_borders: CellBorders::default(),
            shading: None,
        };
        let row = BlockFragment::TableRow {
            id: NodeId::from_parts(2, 1).unwrap(),
            table: NodeId::from_parts(3, 1).unwrap(),
            cells: vec![cell],
            height: Twip(300),
            can_split: false,
            header: false,
            merge_keep_next: false,
            clip: true,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &row, Point::new(Twip(100), Twip(200)));

        // The exact-height row wraps its content in a clip.
        assert!(
            list.items
                .iter()
                .any(|i| matches!(i, PaintItem::PushClip(_))),
            "an exact row pushes a clip"
        );
        assert!(
            list.items.iter().any(|i| matches!(i, PaintItem::PopClip)),
            "and pops it"
        );
        // The resolved bottom border is painted as a filled edge rect at the row
        // foot, 40 twips tall, in the resolved color.
        let border = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(color),
                stroke: None,
            } if rect.size == Size::new(Twip(3000), Twip(40)) => Some(*color),
            _ => None,
        });
        let color = border.expect("the resolved bottom border is drawn");
        assert_eq!(color.r, 10);
        assert_eq!(color.g, 20);
        assert_eq!(color.b, 30);
    }

    #[test]
    fn common_border_patterns_expand_to_deterministic_geometry() {
        let rect = Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(Twip(100), Twip(10)),
        );
        let cases = [
            (
                BorderPattern::Dotted,
                vec![(0, 10), (20, 10), (40, 10), (60, 10), (80, 10)],
            ),
            (BorderPattern::Dashed, vec![(0, 30), (50, 30)]),
            (
                BorderPattern::DotDash,
                vec![(0, 10), (20, 30), (60, 10), (80, 20)],
            ),
            (
                BorderPattern::DotDotDash,
                vec![(0, 10), (20, 10), (40, 30), (80, 10)],
            ),
        ];
        for (pattern, expected) in cases {
            let mut list = DisplayList::new();
            paint_border(
                &mut list,
                rect,
                ResolvedEdge {
                    color: [1, 2, 3, 255],
                    width: Twip(10),
                    pattern,
                },
                BorderAxis::Horizontal,
            );
            let actual: Vec<(i32, i32)> = list
                .items
                .iter()
                .filter_map(|item| match item {
                    PaintItem::Rect { rect, .. } => {
                        Some((rect.origin.x.raw(), rect.size.width.raw()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(actual, expected, "{pattern:?} keeps a stable phase");
        }
    }

    #[test]
    fn shared_dashes_keep_the_same_phase_across_different_segment_partitions() {
        let make_rect = |x, width| {
            Rect::new(
                Point::new(Twip(x), Twip::ZERO),
                Size::new(Twip(width), Twip(10)),
            )
        };
        let covered = |rects: Vec<Rect>| {
            rects
                .into_iter()
                .flat_map(|rect| rect.origin.x.raw()..rect.right().raw())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let whole = covered(
            patterned_rects(
                make_rect(13, 100),
                Twip(10),
                BorderPattern::DotDash,
                BorderAxis::Horizontal,
            )
            .unwrap(),
        );
        let partitioned = covered(
            [
                patterned_rects(
                    make_rect(13, 37),
                    Twip(10),
                    BorderPattern::DotDash,
                    BorderAxis::Horizontal,
                )
                .unwrap(),
                patterned_rects(
                    make_rect(50, 63),
                    Twip(10),
                    BorderPattern::DotDash,
                    BorderAxis::Horizontal,
                )
                .unwrap(),
            ]
            .concat(),
        );
        assert_eq!(
            whole, partitioned,
            "both cells sharing an edge paint the same on/off twips"
        );
    }

    #[test]
    fn a_double_border_paints_two_parallel_bands_inside_the_total_width() {
        let mut list = DisplayList::new();
        paint_border(
            &mut list,
            Rect::new(
                Point::new(Twip(100), Twip(200)),
                Size::new(Twip(500), Twip(60)),
            ),
            ResolvedEdge {
                color: [1, 2, 3, 255],
                width: Twip(60),
                pattern: BorderPattern::Double,
            },
            BorderAxis::Horizontal,
        );
        let bands: Vec<(i32, i32)> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect { rect, .. } => Some((rect.origin.y.raw(), rect.size.height.raw())),
                _ => None,
            })
            .collect();
        assert_eq!(bands, vec![(200, 20), (240, 20)]);
    }

    #[test]
    fn pathological_dash_expansion_falls_back_to_one_bounded_solid_band() {
        let rect = Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(Twip(10_000), Twip(1)),
        );
        let mut list = DisplayList::new();
        paint_border(
            &mut list,
            rect,
            ResolvedEdge {
                color: [1, 2, 3, 255],
                width: Twip(1),
                pattern: BorderPattern::Dotted,
            },
            BorderAxis::Horizontal,
        );
        assert_eq!(list.items.len(), 1);
        assert!(matches!(
            list.items[0],
            PaintItem::Rect {
                rect: painted,
                ..
            } if painted == rect
        ));
    }

    #[test]
    fn horizontal_border_segments_override_the_whole_side_fallback() {
        let mut list = DisplayList::new();
        let borders = CellBorders {
            bottom: Some(ResolvedEdge {
                color: [255, 0, 255, 255],
                width: Twip(5),
                pattern: BorderPattern::Solid,
            }),
            bottom_segments: vec![
                ResolvedBorderSegment {
                    offset: Twip(0),
                    length: Twip(50),
                    edge: ResolvedEdge {
                        color: [255, 0, 0, 255],
                        width: Twip(10),
                        pattern: BorderPattern::Solid,
                    },
                },
                ResolvedBorderSegment {
                    offset: Twip(50),
                    length: Twip(50),
                    edge: ResolvedEdge {
                        color: [0, 0, 255, 255],
                        width: Twip(10),
                        pattern: BorderPattern::Solid,
                    },
                },
            ],
            ..CellBorders::default()
        };
        compose_cell_borders(
            &mut list,
            Rect::new(
                Point::new(Twip(20), Twip(30)),
                Size::new(Twip(100), Twip(40)),
            ),
            &borders,
        );
        let painted: Vec<(i32, i32, Color)> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect {
                    rect,
                    fill: Some(color),
                    ..
                } => Some((rect.origin.x.raw(), rect.size.width.raw(), *color)),
                _ => None,
            })
            .collect();
        assert_eq!(
            painted,
            vec![
                (20, 50, Color::rgb(255, 0, 0)),
                (70, 50, Color::rgb(0, 0, 255)),
            ],
            "segment geometry and colors paint independently; magenta fallback is absent"
        );
    }

    use crate::block::{BlockBorders, BoxMetrics, ParagraphDecor, ResolvedEdge};
    use crate::text::{Glyph, GlyphRun, Line, LineBreak, LineLayout};

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    /// A one-line, one-run paragraph fragment whose single glyph run sits at the
    /// paragraph-relative `origin` with the given `advance` and optional highlight.
    fn one_run_line(advance: Twip, highlight: Option<[u8; 4]>) -> LineLayout {
        let run = GlyphRun {
            is_marker: false,
            is_leader: false,
            font: FontId(0),
            size: Twip(200),
            ascent: Twip(0),
            descent: Twip(0),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip(200)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight,
            shading: None,
            glyphs: vec![Glyph {
                id: 1,
                advance,
                cluster: 0,
                is_whitespace: false,
            }],
        };
        LineLayout {
            lines: vec![Line {
                runs: vec![run],
                ascent: Twip(200),
                descent: Twip(50),
                height: Twip(250),
                clip: false,
                range: ModelRange::new(ModelPos::new(node(1), 0), ModelPos::new(node(1), 0)),
                line_break: LineBreak::ParagraphEnd,
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                notes: Vec::new(),
                text_boxes: Vec::new(),
                rules: Vec::new(),
            }],
        }
    }

    #[test]
    fn a_bar_tab_stop_draws_a_vertical_rule() {
        // A line carrying a bar-stop x (1500) composes a thin vertical rule at that
        // x spanning the line height, at the paragraph origin.
        let mut layout = one_run_line(Twip(500), None);
        layout.lines[0].bars = vec![Twip(1500)];
        let list = compose_paragraph(&layout, Point::new(Twip(100), Twip(200)));
        let bar = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(fill),
                stroke: None,
            } if fill.r == 0 && rect.size.width == BAR_TAB_WIDTH => Some(rect.origin.x.raw()),
            _ => None,
        });
        assert_eq!(
            bar,
            Some(100 + 1500),
            "the bar rule is drawn at the paragraph origin plus the stop x"
        );
    }

    #[test]
    fn a_start_indent_shifts_the_composed_run_right() {
        // A paragraph with a 720-twip start indent composes its glyph runs at the
        // page origin plus the indent (the shaper already wrapped to the reduced
        // width; composition only applies the horizontal offset).
        let frag = BlockFragment::Paragraph {
            id: node(1),
            lines: one_run_line(Twip(500), None),
            box_metrics: BoxMetrics {
                indent_start: Twip(720),
                ..BoxMetrics::default()
            },
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor::default(),
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &frag, Point::new(Twip(1000), Twip(2000)));
        let glyph_x = list.items.iter().find_map(|i| match i {
            PaintItem::Glyphs { run } => Some(run.origin.x.raw()),
            _ => None,
        });
        assert_eq!(
            glyph_x,
            Some(1000 + 720),
            "the run starts at the page origin plus the start indent"
        );
    }

    #[test]
    fn a_highlighted_run_emits_a_fill_rect_behind_the_glyphs() {
        let layout = one_run_line(Twip(600), Some([255, 255, 0, 255]));
        let list = compose_paragraph(&layout, Point::new(Twip(100), Twip(200)));
        // The highlight fill precedes the glyph run.
        let fill_idx = list.items.iter().position(|i| {
            matches!(
                i,
                PaintItem::Rect {
                    fill: Some(c),
                    stroke: None,
                    ..
                } if *c == Color { r: 255, g: 255, b: 0, a: 255 }
            )
        });
        let glyph_idx = list
            .items
            .iter()
            .position(|i| matches!(i, PaintItem::Glyphs { .. }));
        let fill_idx = fill_idx.expect("a highlight fill rect is emitted");
        assert!(
            fill_idx < glyph_idx.expect("the glyphs are emitted"),
            "the highlight is painted behind (before) the glyphs"
        );
        // The fill spans the run's advance and the line's height.
        let PaintItem::Rect { rect, .. } = &list.items[fill_idx] else {
            unreachable!()
        };
        assert_eq!(rect.size.width, Twip(600));
        assert_eq!(rect.size.height, Twip(250));
    }

    #[test]
    fn a_shaded_paragraph_emits_a_background_rect_covering_its_box() {
        let frag = BlockFragment::Paragraph {
            id: node(1),
            lines: one_run_line(Twip(500), None),
            box_metrics: BoxMetrics::default(),
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor {
                shading: Some([220, 230, 240, 255]),
                borders: BlockBorders::default(),
                width: Twip(6000),
            },
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &frag, Point::new(Twip(0), Twip(0)));
        let shade = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(c),
                stroke: None,
            } if *c
                == (Color {
                    r: 220,
                    g: 230,
                    b: 240,
                    a: 255,
                }) =>
            {
                Some(*rect)
            }
            _ => None,
        });
        let rect = shade.expect("the shaded paragraph emits a background fill");
        assert_eq!(rect.size.width, Twip(6000), "the fill spans the box width");
        assert_eq!(rect.size.height, Twip(250), "and the lines' height");
    }

    #[test]
    fn a_bordered_paragraph_emits_border_edge_rects() {
        let edge = ResolvedEdge {
            color: [10, 20, 30, 255],
            width: Twip(40),
            pattern: BorderPattern::Solid,
        };
        let frag = BlockFragment::Paragraph {
            id: node(1),
            lines: one_run_line(Twip(500), None),
            box_metrics: BoxMetrics::default(),
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor {
                shading: None,
                borders: BlockBorders {
                    top: Some(edge),
                    bottom: Some(edge),
                    start: Some(edge),
                    end: Some(edge),
                },
                width: Twip(6000),
            },
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &frag, Point::new(Twip(0), Twip(0)));
        let border_rects = list
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    PaintItem::Rect {
                        fill: Some(c),
                        stroke: None,
                        ..
                    } if *c == (Color { r: 10, g: 20, b: 30, a: 255 })
                )
            })
            .count();
        assert_eq!(
            border_rects, 4,
            "all four paragraph border edges are stroked"
        );
    }

    #[test]
    fn separated_cell_paints_table_perimeter_and_cell_border_at_distinct_x_positions() {
        use crate::block::{
            BlockFragment, CellBorders, CellBoxSpacing, CellContentMargins, CellFragment,
            CellVAlign, CellVerticalMerge, ResolvedEdge,
        };
        let border = |color| ResolvedEdge {
            color,
            width: Twip(20),
            pattern: BorderPattern::Solid,
        };
        let cell = CellFragment {
            id: node(501),
            grid_span: 1,
            x: Twip(120),
            width: Twip(1760),
            cell_spacing: CellBoxSpacing {
                top: Twip(120),
                start: Twip(120),
                bottom: Twip(120),
                end: Twip(120),
            },
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::Top,
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders {
                start: Some(border([0, 0, 255, 255])),
                ..CellBorders::default()
            },
            table_borders: CellBorders {
                start: Some(border([255, 0, 0, 255])),
                ..CellBorders::default()
            },
            shading: None,
        };
        let row = BlockFragment::TableRow {
            id: node(502),
            table: node(503),
            cells: vec![cell],
            height: Twip(600),
            can_split: false,
            header: false,
            merge_keep_next: false,
            clip: false,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &row, Point::new(Twip(100), Twip(200)));

        let vertical_bands: Vec<_> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect {
                    rect,
                    fill: Some(fill),
                    stroke: None,
                } if rect.size.width == Twip(20) => Some((rect.origin.x, *fill)),
                _ => None,
            })
            .collect();
        assert!(vertical_bands.contains(&(Twip(100), rgba([255, 0, 0, 255]))));
        assert!(vertical_bands.contains(&(Twip(220), rgba([0, 0, 255, 255]))));
    }

    #[test]
    fn a_shaded_cell_emits_a_fill_behind_its_content() {
        use crate::block::{
            CellBorders, CellContentMargins, CellFragment, CellVAlign, CellVerticalMerge,
        };
        let cell = CellFragment {
            id: node(1),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            cell_spacing: Default::default(),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders::default(),
            table_borders: CellBorders::default(),
            shading: Some([200, 100, 50, 255]),
        };
        let row = BlockFragment::TableRow {
            id: node(2),
            table: node(3),
            cells: vec![cell],
            height: Twip(500),
            can_split: true,
            header: false,
            merge_keep_next: false,
            clip: false,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &row, Point::new(Twip(100), Twip(200)));
        // The very first paint op for the cell is its shading fill, behind the grid
        // line and content.
        let first_fill = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(c),
                stroke: None,
            } => Some((*rect, *c)),
            _ => None,
        });
        let (rect, color) = first_fill.expect("the shaded cell emits a fill");
        assert_eq!(
            color,
            Color {
                r: 200,
                g: 100,
                b: 50,
                a: 255
            }
        );
        assert_eq!(rect.origin, Point::new(Twip(100), Twip(200)));
        assert_eq!(rect.size, Size::new(Twip(3000), Twip(500)));
    }

    #[test]
    fn a_vertical_merge_paints_one_box_and_skips_its_continuation() {
        use crate::block::{
            CellBorders, CellContentMargins, CellFragment, CellVAlign, CellVerticalMerge,
        };
        let cell = |id, vertical_merge, shading| CellFragment {
            id: node(id),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            cell_spacing: Default::default(),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge,
            borders: CellBorders::default(),
            table_borders: CellBorders::default(),
            shading,
        };
        let restart = BlockFragment::TableRow {
            id: node(10),
            table: node(20),
            cells: vec![cell(
                11,
                CellVerticalMerge::Restart { height: Twip(1000) },
                Some([200, 100, 50, 255]),
            )],
            height: Twip(500),
            can_split: true,
            header: false,
            merge_keep_next: true,
            clip: false,
        };
        let continuation = BlockFragment::TableRow {
            id: node(12),
            table: node(20),
            cells: vec![cell(
                13,
                CellVerticalMerge::Continue,
                Some([10, 20, 30, 255]),
            )],
            height: Twip(500),
            can_split: true,
            header: false,
            merge_keep_next: false,
            clip: false,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &restart, Point::new(Twip(100), Twip(200)));
        compose_fragment(&mut list, &continuation, Point::new(Twip(100), Twip(700)));

        let fills: Vec<_> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect {
                    rect,
                    fill: Some(fill),
                    ..
                } => Some((*rect, *fill)),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 1, "the continuation emits no duplicate box");
        assert_eq!(fills[0].0.size.height, Twip(1000));
        assert_eq!(
            fills[0].1,
            Color {
                r: 200,
                g: 100,
                b: 50,
                a: 255
            }
        );
    }

    #[test]
    fn an_inline_text_box_paints_its_fill_border_and_content() {
        use crate::text::{InlineTextBox, TextBoxContentLayout, TextBoxStroke};

        // A text box carrying one inner paragraph fragment (a single glyph run),
        // with an explicit fill and border.
        let inner = BlockFragment::Paragraph {
            id: node(2),
            lines: one_run_line(Twip(400), None),
            box_metrics: BoxMetrics::default(),
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor::default(),
        };
        let text_box = InlineTextBox {
            origin: Point::new(Twip(500), Twip(600)),
            size: Size::new(Twip(3000), Twip(1000)),
            blocks: vec![inner],
            border: Some(TextBoxStroke {
                color: [10, 20, 30, 255],
                width: Twip(30),
            }),
            fill: Some([200, 210, 220, 255]),
            content_layout: TextBoxContentLayout {
                origin: Point::new(Twip(72), Twip(72)),
                clip_horizontal: false,
                clip_vertical: false,
            },
        };
        let mut layout = one_run_line(Twip(0), None);
        layout.lines[0].runs.clear();
        layout.lines[0].text_boxes = vec![text_box];

        let origin = Point::new(Twip(100), Twip(200));
        let list = compose_paragraph(&layout, origin);

        // The fill covers the box at its page-absolute origin.
        let fill = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(c),
                stroke: None,
            } if *c
                == (Color {
                    r: 200,
                    g: 210,
                    b: 220,
                    a: 255,
                }) =>
            {
                Some(*rect)
            }
            _ => None,
        });
        let fill_rect = fill.expect("the box fill paints");
        assert_eq!(fill_rect.origin, Point::new(Twip(600), Twip(800)));
        assert_eq!(fill_rect.size, Size::new(Twip(3000), Twip(1000)));

        // The border paints as a stroked rect over the same box.
        assert!(
            list.items.iter().any(|i| matches!(
                i,
                PaintItem::Rect {
                    stroke: Some(s),
                    fill: None,
                    ..
                } if s.color == (Color { r: 10, g: 20, b: 30, a: 255 })
                    && (s.width - 2.0).abs() < f32::EPSILON
            )),
            "the box border paints as a stroked rect"
        );

        // The inner glyph run composes offset into the box by the internal margin.
        let glyph_x = list.items.iter().find_map(|i| match i {
            PaintItem::Glyphs { run } => Some(run.origin.x.raw()),
            _ => None,
        });
        assert_eq!(
            glyph_x,
            Some(672),
            "the box content is inset from the box's left edge by the margin"
        );
    }

    #[test]
    fn a_text_box_clips_content_to_its_horizontal_bounds_as_a_backstop() {
        use crate::text::TextBoxContentLayout;

        // A shape/text-box with the schema-default overflow ("overflow"): its
        // content must still be clipped to the box horizontally, so a centered or
        // unbreakable label wider than the box cannot spill past its right edge
        // (the corpus "Paint"/"Image" sub-label overflow). Vertical clipping
        // follows the authored policy (here, unclipped).
        let rect = Rect::new(
            Point::new(Twip(1000), Twip(2000)),
            Size::new(Twip(800), Twip(400)),
        );
        let anchor = anchor_at(
            AnchorContent::TextBox {
                blocks: Vec::new(),
                fill: None,
                border: None,
                content_layout: TextBoxContentLayout {
                    origin: Point::new(Twip(72), Twip(72)),
                    clip_horizontal: false,
                    clip_vertical: false,
                },
            },
            rect,
        );
        let mut list = DisplayList::new();
        compose_anchor(&mut list, &anchor);

        let clip = list
            .items
            .iter()
            .find_map(|item| match item {
                PaintItem::PushClip(clip) => Some(*clip),
                _ => None,
            })
            .expect("the text box clips its content as a backstop");
        // The clip pins the horizontal extent to the box, so nothing paints past
        // the box's left/right edge.
        assert_eq!(clip.origin.x, rect.origin.x);
        assert_eq!(clip.size.width, rect.size.width);
        assert_eq!(
            clip.origin.x.raw() + clip.size.width.raw(),
            1800,
            "the clip ends exactly at the box's right edge"
        );
        // Vertical is unclipped (a broad span) because vertOverflow defaults to
        // "overflow", so autofit growth is never hidden.
        assert!(
            clip.size.height.raw() > rect.size.height.raw(),
            "vertical extent stays unclipped per the authored policy"
        );
        // The clip is balanced with a matching pop after the content.
        assert!(matches!(list.items.last(), Some(PaintItem::PopClip)));
    }

    fn anchor_at(content: AnchorContent, rect: Rect) -> PlacedAnchor {
        use crate::page::AnchorZ;
        PlacedAnchor {
            node: None,
            content,
            rect,
            behind_doc: false,
            z: AnchorZ {
                relative_height: 0,
                order: 0,
            },
            descr: None,
            transform: None,
        }
    }

    #[test]
    fn a_bordered_picture_composes_an_image_then_a_stroked_frame() {
        let rect = Rect::new(
            Point::new(Twip(100), Twip(200)),
            Size::new(Twip(500), Twip(400)),
        );
        let anchor = anchor_at(
            AnchorContent::Image {
                media: "word/media/image1.png".to_owned(),
                crop: None,
                border: Some(AnchorStroke {
                    color: [10, 20, 30, 255],
                    width: Twip(20),
                    dash: DashStyle::Solid,
                }),
            },
            rect,
        );
        let mut list = DisplayList::new();
        compose_anchor(&mut list, &anchor);

        assert!(
            matches!(&list.items[0], PaintItem::Image { rect: r, .. } if *r == rect),
            "the picture blits first"
        );
        assert!(
            matches!(
                &list.items[1],
                PaintItem::Shape {
                    geometry: ShapeGeometry::Rect { rect: r },
                    fill: None,
                    stroke: Some(s),
                    ..
                }
                    if *r == rect
                        && s.color == Color { r: 10, g: 20, b: 30, a: 255 }
                        && matches!(s.dash, DashStyle::Solid)
            ),
            "the frame paints as a stroked rect shape over the picture box"
        );
    }

    #[test]
    fn a_dashed_picture_frame_carries_its_dash_onto_the_stroked_shape() {
        // The border stroke's `a:prstDash` must ride onto the composed frame so a
        // dashed picture frame paints dashed (a solid control stays solid).
        let rect = Rect::new(
            Point::new(Twip(100), Twip(200)),
            Size::new(Twip(500), Twip(400)),
        );
        let frame = |dash| {
            let anchor = anchor_at(
                AnchorContent::Image {
                    media: "word/media/image1.png".to_owned(),
                    crop: None,
                    border: Some(AnchorStroke {
                        color: [10, 20, 30, 255],
                        width: Twip(20),
                        dash,
                    }),
                },
                rect,
            );
            let mut list = DisplayList::new();
            compose_anchor(&mut list, &anchor);
            let PaintItem::Shape {
                stroke: Some(outline),
                ..
            } = &list.items[1]
            else {
                panic!("the frame paints as a stroked shape");
            };
            outline.dash
        };

        assert!(
            matches!(frame(DashStyle::Dash), DashStyle::Dash),
            "a dashed frame carries its dash onto the stroke"
        );
        assert!(
            matches!(frame(DashStyle::Solid), DashStyle::Solid),
            "a solid frame stays solid"
        );
    }

    #[test]
    fn a_floats_transform_rides_onto_its_composed_shape_and_image() {
        use crate::display::ShapeTransform;
        let rect = Rect::new(
            Point::new(Twip(0), Twip(0)),
            Size::new(Twip(100), Twip(100)),
        );
        let xform = ShapeTransform {
            rotation: 90 * 60_000,
            flip_h: false,
            flip_v: true,
            center: Point::new(Twip(50), Twip(50)),
        };

        // An image float carries its rotation/flip onto the emitted blit.
        let mut anchor = anchor_at(
            AnchorContent::Image {
                media: "m".to_owned(),
                crop: None,
                border: None,
            },
            rect,
        );
        anchor.transform = Some(xform);
        let mut list = DisplayList::new();
        compose_anchor(&mut list, &anchor);
        assert!(
            matches!(&list.items[0], PaintItem::Image { transform: Some(t), .. } if *t == xform),
            "the image blit carries the float transform"
        );

        // A shape float likewise carries it onto the emitted shape.
        let mut anchor = anchor_at(
            AnchorContent::Rectangle {
                fill: None,
                stroke: None,
            },
            rect,
        );
        anchor.transform = Some(xform);
        let mut list = DisplayList::new();
        compose_anchor(&mut list, &anchor);
        assert!(
            matches!(&list.items[0], PaintItem::Shape { transform: Some(t), .. } if *t == xform),
            "the shape carries the float transform"
        );
    }

    #[test]
    fn a_gradient_dashed_shape_composes_to_a_shape_paint_item() {
        use casual_doc_model::v1::{
            Fill as ModelFill, GradientKind as ModelGradientKind,
            GradientStop as ModelGradientStop, Rgba,
        };

        let rect = Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(Twip(200), Twip(100)),
        );
        let anchor = anchor_at(
            AnchorContent::Rectangle {
                fill: Some(ModelFill::Gradient {
                    stops: vec![
                        ModelGradientStop {
                            position: 0,
                            color: Rgba {
                                r: 255,
                                g: 0,
                                b: 0,
                                a: 255,
                            },
                        },
                        ModelGradientStop {
                            position: 100_000,
                            color: Rgba {
                                r: 0,
                                g: 0,
                                b: 255,
                                a: 255,
                            },
                        },
                    ],
                    kind: ModelGradientKind::Linear { angle: 5_400_000 },
                }),
                stroke: Some(AnchorStroke {
                    color: [0, 0, 0, 255],
                    width: Twip(30),
                    dash: DashStyle::DashDot,
                }),
            },
            rect,
        );
        let mut list = DisplayList::new();
        compose_anchor(&mut list, &anchor);

        let PaintItem::Shape {
            geometry: ShapeGeometry::Rect { .. },
            fill: Some(Fill::Gradient(gradient)),
            stroke: Some(outline),
            ..
        } = &list.items[0]
        else {
            panic!("expected a gradient-filled shape, got {:?}", list.items[0]);
        };
        assert_eq!(gradient.stops.len(), 2);
        // 5_400_000 sixty-thousandths of a degree = 90°.
        assert!(matches!(
            gradient.kind,
            GradientKind::Linear { angle_deg } if (angle_deg - 90.0).abs() < 0.01
        ));
        assert!(matches!(outline.dash, DashStyle::DashDot));
    }

    // --- Emphasis marks and run borders (`docs/105` FID-L-14) ---------------
    //
    // These go through the real shaper on purpose. `w:em` and `w:bdr` are not
    // `parley` decorations, so they have to ride the run brush across shaping to
    // reach the glyph run at all; a test that hand-built a `GlyphRun` would paint
    // correctly while the production path still dropped both at the shaping
    // boundary. Shaping here means the guard fails if either the brush plumbing
    // or the paint arm is missing.

    /// Shapes `text` as one 11pt run carrying `decoration`, then composes it at a
    /// 1-inch origin. Returns the display list and that origin.
    fn compose_decorated(text: &str, decoration: Decoration) -> (DisplayList, Point) {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(7, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: text.into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(11),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration,
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let origin = Point::new(Twip::from_points(72), Twip::from_points(72));
        (compose_paragraph(&layout, origin), origin)
    }

    /// Every ellipse in a composed list, as `(rect, filled, stroked)`.
    fn ellipses(list: &DisplayList) -> Vec<(Rect, bool, bool)> {
        list.items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Ellipse { rect, fill, stroke } => {
                    Some((*rect, fill.is_some(), stroke.is_some()))
                }
                _ => None,
            })
            .collect()
    }

    /// The baseline y of the first composed glyph run.
    fn first_baseline(list: &DisplayList) -> Twip {
        list.items
            .iter()
            .find_map(|item| match item {
                PaintItem::Glyphs { run } => Some(run.origin.y),
                _ => None,
            })
            .expect("the text composed to at least one glyph run")
    }

    #[test]
    fn a_dot_emphasis_mark_paints_once_per_non_blank_cluster_above_the_text() {
        let (list, _) = compose_decorated(
            "ab c",
            Decoration {
                emphasis: Some(casual_doc_model::v1::EmphasisMark::Dot),
                ..Decoration::default()
            },
        );
        let marks = ellipses(&list);
        assert_eq!(
            marks.len(),
            3,
            "one mark over each of a, b and c — the space carries none (got {marks:?})"
        );
        let baseline = first_baseline(&list);
        for (rect, filled, stroked) in &marks {
            assert!(*filled && !*stroked, "a `dot` mark is a filled circle");
            assert!(
                rect.bottom() < baseline,
                "the mark sits entirely above the baseline ({:?} vs {baseline:?})",
                rect.bottom()
            );
            assert!(rect.size.width > Twip::ZERO && rect.size.height > Twip::ZERO);
        }
        // Marks follow the text left to right, one per cluster, non-overlapping.
        for pair in marks.windows(2) {
            assert!(
                pair[0].0.right() <= pair[1].0.origin.x,
                "marks advance with their clusters and do not overlap: {:?}",
                (pair[0].0, pair[1].0)
            );
        }
    }

    #[test]
    fn an_under_dot_emphasis_mark_paints_below_the_baseline_and_a_circle_is_hollow() {
        let (below, _) = compose_decorated(
            "ab",
            Decoration {
                emphasis: Some(casual_doc_model::v1::EmphasisMark::UnderDot),
                ..Decoration::default()
            },
        );
        let baseline = first_baseline(&below);
        let marks = ellipses(&below);
        assert_eq!(marks.len(), 2);
        for (rect, filled, _) in &marks {
            assert!(*filled);
            assert!(
                rect.origin.y > baseline,
                "`underDot` sits below the baseline ({:?} vs {baseline:?})",
                rect.origin.y
            );
        }

        let (hollow, _) = compose_decorated(
            "ab",
            Decoration {
                emphasis: Some(casual_doc_model::v1::EmphasisMark::Circle),
                ..Decoration::default()
            },
        );
        let marks = ellipses(&hollow);
        assert_eq!(marks.len(), 2);
        for (_, filled, stroked) in &marks {
            assert!(
                !*filled && *stroked,
                "a `circle` mark is an outline, not a filled dot"
            );
        }
    }

    #[test]
    fn an_explicit_emphasis_clear_and_an_unmarked_run_paint_no_marks() {
        // `w:em="none"` is resolved to `None` upstream (in `flow::run_decoration`),
        // and the paint arm refuses the variant too — belt and braces, because a
        // cleared mark that painted would be worse than one that never painted.
        for decoration in [
            Decoration::default(),
            Decoration {
                emphasis: Some(casual_doc_model::v1::EmphasisMark::None),
                ..Decoration::default()
            },
        ] {
            let (list, _) = compose_decorated("ab", decoration);
            assert!(
                ellipses(&list).is_empty(),
                "no emphasis mark is painted for {decoration:?}"
            );
        }
    }

    #[test]
    fn a_run_border_paints_a_four_sided_box_around_the_runs_glyph_box() {
        let edge = ResolvedEdge {
            color: [0x33, 0x55, 0xC4, 255],
            width: Twip(20),
            pattern: BorderPattern::Solid,
        };
        let (list, origin) = compose_decorated(
            "boxed",
            Decoration {
                border: Some(edge),
                ..Decoration::default()
            },
        );
        let rects: Vec<Rect> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect { rect, fill, .. } if *fill == Some(rgba(edge.color)) => {
                    Some(*rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            rects.len(),
            4,
            "a solid `w:bdr` paints one band per side (got {rects:?})"
        );
        // Two horizontal bands one edge-width tall, two vertical bands one
        // edge-width wide — and the box starts at the run's own left edge.
        let horizontal = rects.iter().filter(|r| r.size.height == edge.width).count();
        let vertical = rects.iter().filter(|r| r.size.width == edge.width).count();
        assert_eq!((horizontal, vertical), (2, 2), "{rects:?}");
        let left = rects.iter().map(|r| r.origin.x).min().unwrap();
        let right = rects.iter().map(Rect::right).max().unwrap();
        assert_eq!(left, origin.x, "the box hugs the run's leading edge");
        assert!(
            right > left,
            "the box spans the run's advance: {left:?}..{right:?}"
        );
        // The box brackets the baseline (ascent above, descent below).
        let baseline = first_baseline(&list);
        assert!(rects.iter().map(|r| r.origin.y).min().unwrap() < baseline);
        assert!(rects.iter().map(Rect::bottom).max().unwrap() > baseline);
    }

    #[test]
    fn a_nil_run_border_paints_nothing() {
        // `resolve_edge` suppresses a `nil`/`none` edge upstream, so the decoration
        // arrives as `None` and the run composes exactly as an unbordered one does.
        let (plain, _) = compose_decorated("boxed", Decoration::default());
        let plain_rects = plain
            .items
            .iter()
            .filter(|item| matches!(item, PaintItem::Rect { .. }))
            .count();
        assert_eq!(plain_rects, 0, "an unbordered run paints no border bands");
    }
}
