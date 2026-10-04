// SPDX-License-Identifier: Apache-2.0

//! DrawingML colour reading (`a:srgbClr`, `a:schemeClr`, `a:prstClr`, …) and the
//! fills and outlines built from it.
//!
//! # Two colour types, because the model has two, and the difference is real
//!
//! A shape's fill resolves to a concrete [`Rgba`] — that is what `v1::Fill`
//! holds, and what the display list paints. A *run's* colour is a
//! [`StyleColor`], which can be [`StyleColor::Placeholder`]: `a:phClr` is a
//! formal parameter standing for "whatever colour the referencing shape names",
//! and resolving it here would give every styled run in the deck the same colour.
//! The presentation model chose `StyleColor` for `TextCharacterProperties::fill`
//! for exactly that reason, and this module respects the distinction rather than
//! flattening one into the other.
//!
//! # What a scheme colour resolves to, and why that is reported
//!
//! `a:schemeClr val="accent1"` names a theme slot. The theme part
//! (`ppt/theme/theme1.xml`) is **not** read by this importer, so there is no
//! colour scheme to resolve against. Rather than invent a palette — which would
//! paint a branded deck in Office defaults and look deliberate — a scheme colour
//! is reported as degraded and the shape keeps no fill. `SKILL` §12: unsupported
//! data is preserved where safe or reported explicitly, never guessed.

use casual_doc_model::v1::{
    ColorTransform, DashStyle, Fill, LineEnd, LineEndKind, LineEndSize, Rgba, ShapeStroke,
    StyleColor,
};
use quick_xml::events::BytesStart;

use crate::ImportError;
use crate::loss::Reporter;
use crate::xml::{Cursor, attribute, children, integer_attribute, local_name};

/// The outcome of reading a colour-bearing element's child.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ColorRead {
    /// The concrete colour, when the element named one this build can resolve.
    pub(crate) rgba: Option<Rgba>,
    /// Whether the element named `a:phClr`, the placeholder.
    pub(crate) placeholder: bool,
}

impl ColorRead {
    /// This colour as a [`StyleColor`], preserving the placeholder.
    pub(crate) fn style_color(self) -> Option<StyleColor> {
        if self.placeholder {
            return Some(StyleColor::Placeholder(ColorTransform::default()));
        }
        self.rgba.map(StyleColor::Fixed)
    }
}

/// Reads the colour a `a:solidFill`-shaped element wraps.
///
/// Called having just entered the wrapper (`a:solidFill`, `a:fgClr`, `a:buClr`,
/// `a:ln/a:solidFill`, …), and consumes its subtree. A wrapper whose child names
/// no resolvable colour yields a default [`ColorRead`], which the caller
/// distinguishes from an absent wrapper by having entered one at all.
pub(crate) fn read_solid_fill(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
) -> Result<ColorRead, ImportError> {
    let mut read = ColorRead::default();
    let part = cursor.part().to_owned();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"srgbClr" => {
                read.rgba = parse_srgb(attribute(element, b"val", cursor.part())?.as_deref());
                if read.rgba.is_none() {
                    reporter.invalid(&part, b"srgbClr");
                }
                // `a:srgbClr` carries colour transforms as children
                // (`a:alpha`, `a:lumMod`, `a:tint`, …). The model's shape `Fill`
                // is a concrete `Rgba` with no room for an unapplied transform,
                // and applying only some of them would be worse than applying
                // none, so each is reported and the base colour stands.
                if !empty {
                    report_transforms(cursor, reporter, &part, b"srgbClr")?;
                    return Ok(true);
                }
            }
            b"schemeClr" => {
                read.placeholder = attribute(element, b"val", cursor.part())?
                    .is_some_and(|value| value == "phClr");
                if !read.placeholder {
                    // A theme slot with no theme read: reported, not guessed.
                    reporter.degraded(&part, b"schemeClr");
                }
                if !empty {
                    report_transforms(cursor, reporter, &part, b"schemeClr")?;
                    return Ok(true);
                }
            }
            b"sysClr" => {
                // `a:sysClr` carries the host system colour plus a `lastClr`
                // snapshot of what the producer resolved it to. Honouring
                // `lastClr` is the lossless reading: it is the colour the file
                // was authored to look like, and resolving the system slot on
                // this machine would make the deck change appearance per host.
                read.rgba = parse_srgb(attribute(element, b"lastClr", cursor.part())?.as_deref());
                if read.rgba.is_none() {
                    reporter.degraded(&part, b"sysClr");
                }
                if !empty {
                    report_transforms(cursor, reporter, &part, b"sysClr")?;
                    return Ok(true);
                }
            }
            b"prstClr" | b"hslClr" | b"scrgbClr" => {
                reporter.omitted(&part, local);
            }
            _ => {}
        }
        Ok(false)
    })?;
    Ok(read)
}

/// Reports each colour transform on a colour element, then consumes the subtree.
fn report_transforms(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    part: &str,
    _owner: &[u8],
) -> Result<(), ImportError> {
    children(cursor, |_cursor, element, _empty| {
        reporter.omitted(part, local_name(element));
        Ok(false)
    })
}

/// Parses a six-hex-digit `val`, which is the only form `ST_HexColorRGB` admits.
///
/// Returns `None` for anything else — including the three-digit CSS shorthand,
/// which DrawingML does not permit and which a reader that accepted it would
/// silently reinterpret.
fn parse_srgb(value: Option<&str>) -> Option<Rgba> {
    let value = value?.trim();
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(value.get(range)?, 16).ok();
    Some(Rgba {
        r: channel(0..2)?,
        g: channel(2..4)?,
        b: channel(4..6)?,
        a: 255,
    })
}

/// How a shape's fill element resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FillRead {
    /// The modelled fill, when one was resolvable.
    pub(crate) fill: Option<Fill>,
    /// Whether the shape declared `a:noFill` — an explicit *nothing*, which is
    /// not the same as declaring no fill element at all.
    pub(crate) explicit_none: bool,
}

/// Reads a shape-property fill child, having just seen it.
///
/// Returns whether the element's subtree was consumed, matching
/// [`children`]'s contract.
///
/// # The `a:noFill` loss, stated rather than hidden
///
/// `v1::GroupShape::fill` is an `Option<Fill>`, and on a slide `None` has to mean
/// two different things: "this shape states no fill, so it inherits from its
/// placeholder slot and then from the theme", and "this shape states `a:noFill`,
/// so it is deliberately transparent and must *not* inherit". DOCX has no
/// placeholder cascade, so the document model never needed the distinction. A
/// slide does, and the reused type cannot carry it — so `a:noFill` is reported
/// `degraded` on every occurrence. That is a real fidelity gap with a real
/// visible consequence (a transparent shape over a filled placeholder reopens
/// filled), and the fix is a presentation-side field, not a guess here.
pub(crate) fn read_fill_child(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    read: &mut FillRead,
) -> Result<bool, ImportError> {
    let local = local_name(element);
    let part = cursor.part().to_owned();
    match local {
        b"noFill" => {
            read.explicit_none = true;
            reporter.degraded_attribute(&part, b"spPr", b"noFill");
            Ok(false)
        }
        b"solidFill" => {
            if empty {
                reporter.invalid(&part, b"solidFill");
                return Ok(false);
            }
            let color = read_solid_fill(cursor, reporter)?;
            read.fill = color.rgba.map(Fill::Solid);
            Ok(true)
        }
        b"gradFill" | b"blipFill" | b"pattFill" | b"grpFill" => {
            reporter.omitted(&part, local);
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// Reads an `a:ln`, having just entered it.
///
/// Returns `None` when the outline names no resolvable colour: `ShapeStroke`
/// requires a concrete [`Rgba`], and an outline painted in a substituted colour
/// looks like an authored choice. The width is still read and reported in that
/// case, so the loss is visible rather than silent.
pub(crate) fn read_line(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
) -> Result<Option<ShapeStroke>, ImportError> {
    let part = cursor.part().to_owned();
    // `a:ln@w` is in EMU, like every other DrawingML measure here. It is NOT in
    // points or eighths of a point, which is the conversion a reader coming from
    // VML gets wrong.
    let width_emu = integer_attribute(element, b"w", &part)?.unwrap_or(0);
    let mut color: Option<Rgba> = None;
    let mut dash: Option<DashStyle> = None;
    let mut head_end: Option<LineEnd> = None;
    let mut tail_end: Option<LineEnd> = None;
    let mut no_fill = false;

    children(cursor, |cursor, child, empty| {
        let local = local_name(child);
        match local {
            b"noFill" => {
                no_fill = true;
                Ok(false)
            }
            b"solidFill" => {
                if empty {
                    reporter.invalid(&part, b"solidFill");
                    return Ok(false);
                }
                color = read_solid_fill(cursor, reporter)?.rgba;
                Ok(true)
            }
            b"gradFill" | b"pattFill" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"prstDash" => {
                dash = attribute(child, b"val", cursor.part())?
                    .as_deref()
                    .and_then(dash_style);
                if dash.is_none() {
                    reporter.degraded_attribute(&part, b"prstDash", b"val");
                }
                Ok(false)
            }
            b"custDash" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"headEnd" => {
                head_end = read_line_end(child, cursor.part())?;
                Ok(false)
            }
            b"tailEnd" => {
                tail_end = read_line_end(child, cursor.part())?;
                Ok(false)
            }
            b"bevel" | b"round" | b"miter" => {
                // The join style has no field on `ShapeStroke`. Reported rather
                // than ignored: a mitred box corner and a rounded one differ
                // visibly at the widths a slide uses.
                reporter.omitted(&part, local);
                Ok(false)
            }
            _ => Ok(false),
        }
    })?;

    if no_fill {
        // `a:ln/a:noFill` is "explicitly unstroked". Same shape of loss as the
        // fill case and reported the same way.
        reporter.degraded_attribute(&part, b"ln", b"noFill");
        return Ok(None);
    }
    match color {
        Some(color) => Ok(Some(ShapeStroke {
            color,
            width_emu,
            dash,
            head_end,
            tail_end,
        })),
        None => {
            if width_emu != 0 || dash.is_some() {
                // The file states an outline but not a colour this build can
                // resolve — a theme line reference, normally. Dropping it
                // silently would make a bordered shape reopen unbordered.
                reporter.degraded(&part, b"ln");
            }
            Ok(None)
        }
    }
}

fn read_line_end(element: &BytesStart<'_>, part: &str) -> Result<Option<LineEnd>, ImportError> {
    let kind = match attribute(element, b"type", part)?.as_deref() {
        Some("triangle") => LineEndKind::Triangle,
        Some("stealth") => LineEndKind::Stealth,
        Some("diamond") => LineEndKind::Diamond,
        Some("oval") => LineEndKind::Oval,
        Some("arrow") => LineEndKind::Arrow,
        // `none` and any unrecognized token are both "no decoration", which is
        // `ST_LineEndType`'s own default.
        _ => LineEndKind::None,
    };
    if kind == LineEndKind::None {
        return Ok(None);
    }
    Ok(Some(LineEnd {
        kind,
        width: attribute(element, b"w", part)?
            .as_deref()
            .and_then(end_size),
        length: attribute(element, b"len", part)?
            .as_deref()
            .and_then(end_size),
    }))
}

fn end_size(token: &str) -> Option<LineEndSize> {
    match token {
        "sm" => Some(LineEndSize::Small),
        "med" => Some(LineEndSize::Medium),
        "lg" => Some(LineEndSize::Large),
        _ => None,
    }
}

fn dash_style(token: &str) -> Option<DashStyle> {
    Some(match token {
        "solid" => DashStyle::Solid,
        "dot" => DashStyle::Dot,
        "dash" => DashStyle::Dash,
        "lgDash" => DashStyle::LargeDash,
        "dashDot" => DashStyle::DashDot,
        "lgDashDot" => DashStyle::LargeDashDot,
        "lgDashDotDot" => DashStyle::LargeDashDotDot,
        "sysDash" => DashStyle::SystemDash,
        "sysDot" => DashStyle::SystemDot,
        "sysDashDot" => DashStyle::SystemDashDot,
        "sysDashDotDot" => DashStyle::SystemDashDotDot,
        _ => return None,
    })
}
