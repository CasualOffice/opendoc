//! Drawings as inline SVG: shapes, groups and charts, drawn from what the page
//! draws.
//!
//! The page evaluates a shape's preset or custom geometry into rectangles,
//! ellipses and paths (`casual_doc_layout::paint_values::shape_content`), and a
//! chart into rectangles, lines, paths, ellipses and labels
//! (`paint_values::chart_drawing`). Every one of those is an SVG element, so a
//! drawing is written as the same primitives the canvas paints — one geometry
//! evaluator, two outputs.
//!
//! Units are CSS pixels (15 twips to the pixel) with a `viewBox` of the same
//! size, so a text box's HTML inside a `<foreignObject>` is not rescaled by the
//! drawing it sits in.

use std::fmt::Write as _;

use casual_doc_layout::display::PathCommand;
use casual_doc_layout::page::{AnchorContent, AnchorPath, AnchorStroke};
use casual_doc_layout::paint_values::ChartDrawing;
use casual_doc_layout::text::{ChartPrimitive, ChartStroke};
use casual_doc_layout::units::{Point, Rect, Twip};
use casual_doc_model::v1::{DashStyle, Fill, GradientKind, PathFill, Rgba};

use super::{escape_attribute, escape_text};

/// Twips per CSS pixel.
const TWIPS_PER_PX: f64 = 15.0;

/// A length in twips as CSS pixels, rounded to hundredths.
pub(super) fn px(twips: i64) -> f64 {
    round2(twips as f64 / TWIPS_PER_PX)
}

/// EMUs as CSS pixels (9,525 EMU to the pixel).
pub(super) fn emu_px(emu: i64) -> f64 {
    round2(emu as f64 / 9525.0)
}

fn round2(value: f64) -> f64 {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

fn point(at: Point) -> (f64, f64) {
    (px(i64::from(at.x.raw())), px(i64::from(at.y.raw())))
}

/// Gradient definitions need document-unique ids; the writer owns the
/// counter, so two drawings in one file never share one.
pub(super) struct Ids<'a>(pub(super) &'a mut usize);

impl Ids<'_> {
    fn next(&mut self) -> String {
        *self.0 += 1;
        format!("g{}", self.0)
    }
}

/// One drawn shape — what [`AnchorContent`] the page paints for it — laid in
/// `rect` (twips), as SVG elements.
pub(super) fn content(content: &AnchorContent, rect: Rect, ids: &mut Ids<'_>, out: &mut String) {
    let (x, y) = point(rect.origin);
    let (w, h) = (
        px(i64::from(rect.size.width.raw())),
        px(i64::from(rect.size.height.raw())),
    );
    match content {
        AnchorContent::Rectangle { fill, stroke } => {
            let paint = paint(fill.as_ref(), stroke.as_ref(), ids, out);
            let _ = write!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{paint}/>"
            );
        }
        AnchorContent::RoundedRectangle {
            radius,
            fill,
            stroke,
        } => {
            let paint = paint(fill.as_ref(), stroke.as_ref(), ids, out);
            let r = px(i64::from(radius.raw()));
            let _ = write!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{r}\"{paint}/>"
            );
        }
        AnchorContent::Ellipse { fill, stroke } => {
            let paint = paint(fill.as_ref(), stroke.as_ref(), ids, out);
            let _ = write!(
                out,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{paint}/>",
                round2(x + w / 2.0),
                round2(y + h / 2.0),
                round2(w / 2.0),
                round2(h / 2.0)
            );
        }
        AnchorContent::Line {
            from, to, stroke, ..
        } => {
            let (x1, y1) = point(*from);
            let (x2, y2) = point(*to);
            let _ = write!(
                out,
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{}/>",
                stroke_attributes(Some(stroke))
            );
        }
        AnchorContent::Path {
            paths,
            fill,
            stroke,
            ..
        } => {
            let fill_paint = fill_attribute(fill.as_ref(), ids, out);
            for path in paths {
                path_element(path, &fill_paint, stroke.as_ref(), out);
            }
        }
        // Pictures and text boxes are not geometry; the writer draws them.
        _ => {}
    }
}

fn path_element(path: &AnchorPath, fill: &str, stroke: Option<&AnchorStroke>, out: &mut String) {
    let filled = !matches!(path.fill, PathFill::None);
    let _ = write!(
        out,
        "<path d=\"{}\"{}{}/>",
        path_data(&path.commands),
        if filled {
            fill.to_owned()
        } else {
            " fill=\"none\"".to_owned()
        },
        if path.stroke {
            stroke_attributes(stroke)
        } else {
            " stroke=\"none\"".to_owned()
        }
    );
}

/// A path's commands as SVG path data.
pub(super) fn path_data(commands: &[PathCommand]) -> String {
    let mut d = String::new();
    for command in commands {
        match command {
            PathCommand::MoveTo { point: at } => {
                let (x, y) = point(*at);
                let _ = write!(d, "M{x} {y}");
            }
            PathCommand::LineTo { point: at } => {
                let (x, y) = point(*at);
                let _ = write!(d, "L{x} {y}");
            }
            PathCommand::CubicTo {
                control1,
                control2,
                point: at,
            } => {
                let (x1, y1) = point(*control1);
                let (x2, y2) = point(*control2);
                let (x, y) = point(*at);
                let _ = write!(d, "C{x1} {y1} {x2} {y2} {x} {y}");
            }
            PathCommand::QuadTo { control, point: at } => {
                let (x1, y1) = point(*control);
                let (x, y) = point(*at);
                let _ = write!(d, "Q{x1} {y1} {x} {y}");
            }
            PathCommand::Close => d.push('Z'),
        }
    }
    d
}

fn paint(
    fill: Option<&Fill>,
    stroke: Option<&AnchorStroke>,
    ids: &mut Ids<'_>,
    defs: &mut String,
) -> String {
    let mut attributes = fill_attribute(fill, ids, defs);
    attributes.push_str(&stroke_attributes(stroke));
    attributes
}

/// ` fill="…"` for a shape fill. A gradient is written as a definition into
/// `defs` (the output, ahead of the element that uses it) and referenced.
fn fill_attribute(fill: Option<&Fill>, ids: &mut Ids<'_>, defs: &mut String) -> String {
    match fill {
        None => " fill=\"none\"".to_owned(),
        Some(Fill::Solid(color)) => color_attributes("fill", *color),
        Some(Fill::Gradient { stops, kind }) if !stops.is_empty() => {
            let id = ids.next();
            let element = match kind {
                GradientKind::Linear { angle } => {
                    // `a:lin@ang` is clockwise from the x axis in 60,000ths of a
                    // degree; rotating the unit gradient about the box centre
                    // is the same sweep.
                    let degrees = f64::from(*angle) / 60_000.0;
                    let _ = write!(
                        defs,
                        "<defs><linearGradient id=\"{id}\" gradientTransform=\"rotate({} .5 .5)\">",
                        round2(degrees)
                    );
                    "linearGradient"
                }
                GradientKind::Radial => {
                    let _ = write!(defs, "<defs><radialGradient id=\"{id}\">");
                    "radialGradient"
                }
            };
            for stop in stops {
                let offset = round2(f64::from(stop.position.clamp(0, 100_000)) / 1000.0);
                let _ = write!(
                    defs,
                    "<stop offset=\"{offset}%\" stop-color=\"{}\"{}/>",
                    hex(stop.color),
                    if stop.color.a < 255 {
                        format!(" stop-opacity=\"{}\"", alpha(stop.color.a))
                    } else {
                        String::new()
                    }
                );
            }
            let _ = write!(defs, "</{element}></defs>");
            format!(" fill=\"url(#{id})\"")
        }
        Some(other) => color_attributes("fill", other.flat_color()),
    }
}

fn color_attributes(attribute: &str, color: Rgba) -> String {
    let mut out = format!(" {attribute}=\"{}\"", hex(color));
    if color.a < 255 {
        let _ = write!(out, " {attribute}-opacity=\"{}\"", alpha(color.a));
    }
    out
}

fn stroke_attributes(stroke: Option<&AnchorStroke>) -> String {
    let Some(stroke) = stroke else {
        return " stroke=\"none\"".to_owned();
    };
    // A zero width is a hairline on the page, not an invisible line.
    let width = px(i64::from(stroke.width.raw())).max(0.75);
    let mut out = format!(
        " stroke=\"{}\" stroke-width=\"{width}\"",
        hex_rgba(stroke.color)
    );
    if stroke.color[3] < 255 {
        let _ = write!(out, " stroke-opacity=\"{}\"", alpha(stroke.color[3]));
    }
    if let Some(pattern) = dash_pattern(stroke.dash) {
        let scaled: Vec<String> = pattern
            .iter()
            .map(|unit| format!("{}", round2(unit * width)))
            .collect();
        let _ = write!(out, " stroke-dasharray=\"{}\"", scaled.join(" "));
    }
    out
}

/// A dash style's pattern in multiples of the line width, as DrawingML
/// defines `a:prstDash`.
fn dash_pattern(dash: DashStyle) -> Option<&'static [f64]> {
    Some(match dash {
        DashStyle::Solid => return None,
        DashStyle::Dot | DashStyle::SystemDot => &[1.0, 1.0],
        DashStyle::Dash | DashStyle::SystemDash => &[4.0, 3.0],
        DashStyle::LargeDash => &[8.0, 3.0],
        DashStyle::DashDot | DashStyle::SystemDashDot => &[4.0, 3.0, 1.0, 3.0],
        DashStyle::LargeDashDot => &[8.0, 3.0, 1.0, 3.0],
        DashStyle::LargeDashDotDot | DashStyle::SystemDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
    })
}

fn hex(color: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn hex_rgba(color: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn alpha(value: u8) -> f64 {
    (f64::from(value) / 255.0 * 1000.0).round() / 1000.0
}

/// A chart, drawn as the page composes it, as a complete `<svg>` of
/// `width`×`height` twips. Labels are set as text in the document's font, each
/// at its own size, weight, slant and face (`docs/155` §19); a vertical axis
/// title is turned a quarter about its centre, as the page turns it. `name` is
/// what a screen reader announces: a drawing with `role="img"` hides its own
/// text from it.
pub(super) fn chart(drawing: &ChartDrawing, name: &str, width: Twip, height: Twip) -> String {
    let (w, h) = (px(i64::from(width.raw())), px(i64::from(height.raw())));
    let mut out = format!(
        "<svg class=\"chart\" role=\"img\" aria-label=\"{}\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" style=\"max-width:100%;height:auto;vertical-align:baseline\">",
        escape_attribute(name)
    );
    for primitive in &drawing.primitives {
        match primitive {
            ChartPrimitive::Rect { rect, fill, stroke } => {
                let (x, y) = point(rect.origin);
                let _ = write!(
                    out,
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{}\"{}{}/>",
                    px(i64::from(rect.size.width.raw())),
                    px(i64::from(rect.size.height.raw())),
                    chart_fill(*fill),
                    chart_stroke(stroke.as_ref())
                );
            }
            ChartPrimitive::Line { from, to, stroke } => {
                let (x1, y1) = point(*from);
                let (x2, y2) = point(*to);
                let _ = write!(
                    out,
                    "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{}/>",
                    chart_stroke(Some(stroke))
                );
            }
            ChartPrimitive::Path {
                commands,
                closed,
                fill,
                stroke,
            } => {
                let mut d = path_data(commands);
                if *closed && !d.ends_with('Z') {
                    d.push('Z');
                }
                let _ = write!(
                    out,
                    "<path d=\"{d}\"{}{}/>",
                    chart_fill(*fill),
                    chart_stroke(stroke.as_ref())
                );
            }
            ChartPrimitive::Ellipse { rect, fill, stroke } => {
                let (x, y) = point(rect.origin);
                let (rx, ry) = (
                    px(i64::from(rect.size.width.raw())) / 2.0,
                    px(i64::from(rect.size.height.raw())) / 2.0,
                );
                let _ = write!(
                    out,
                    "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{}{}/>",
                    round2(x + rx),
                    round2(y + ry),
                    round2(rx),
                    round2(ry),
                    chart_fill(*fill),
                    chart_stroke(stroke.as_ref())
                );
            }
            ChartPrimitive::Text { run } => chart_text(&mut out, drawing, run, ""),
            ChartPrimitive::RotatedText {
                runs,
                center,
                quarter_turns,
            } => {
                let (cx, cy) = point(*center);
                let transform = format!(
                    " transform=\"rotate({} {cx} {cy})\"",
                    i32::from(*quarter_turns) * 90
                );
                for run in runs {
                    chart_text(&mut out, drawing, run, &transform);
                }
            }
        }
    }
    out.push_str("</svg>");
    out
}

/// One chart label run as `<text>`, in its label's own style. The run's font is
/// the label's index (`ChartDrawing::labels`); a run naming no label is skipped.
fn chart_text(
    out: &mut String,
    drawing: &ChartDrawing,
    run: &casual_doc_layout::text::GlyphRun,
    transform: &str,
) {
    let Some(label) = usize::try_from(run.font.0)
        .ok()
        .and_then(|index| drawing.labels.get(index))
    else {
        return;
    };
    let (x, y) = point(run.origin);
    let mut attributes = format!(
        " font-size=\"{}\" fill=\"{}\"",
        px(i64::from(run.size.raw())),
        hex_rgba(run.color)
    );
    if label.style.bold {
        attributes.push_str(" font-weight=\"bold\"");
    }
    if label.style.italic {
        attributes.push_str(" font-style=\"italic\"");
    }
    // A named face only: a theme reference (`+mn-lt`) is the document's own
    // body face, which the page's stylesheet already sets.
    if let Some(face) = label
        .style
        .typeface
        .as_deref()
        .filter(|face| !face.is_empty() && !face.starts_with('+'))
    {
        let _ = write!(attributes, " font-family=\"{}\"", escape_attribute(face));
    }
    let _ = write!(
        out,
        "<text x=\"{x}\" y=\"{y}\"{attributes}{transform}>{}</text>",
        escape_text(&label.text)
    );
}

fn chart_fill(fill: Option<[u8; 4]>) -> String {
    match fill {
        Some(color) => {
            let mut out = format!(" fill=\"{}\"", hex_rgba(color));
            if color[3] < 255 {
                let _ = write!(out, " fill-opacity=\"{}\"", alpha(color[3]));
            }
            out
        }
        None => " fill=\"none\"".to_owned(),
    }
}

fn chart_stroke(stroke: Option<&ChartStroke>) -> String {
    match stroke {
        Some(stroke) => {
            let width = px(i64::from(stroke.width.raw())).max(0.75);
            let mut out = format!(
                " stroke=\"{}\" stroke-width=\"{width}\"",
                hex_rgba(stroke.color)
            );
            // A series, trendline or error-bar dash, in the same multiples of
            // the width the page's own outlines use.
            if let Some(pattern) = dash_pattern(stroke.dash) {
                let scaled: Vec<String> = pattern
                    .iter()
                    .map(|unit| format!("{}", round2(unit * width)))
                    .collect();
                let _ = write!(out, " stroke-dasharray=\"{}\"", scaled.join(" "));
            }
            out
        }
        None => " stroke=\"none\"".to_owned(),
    }
}
