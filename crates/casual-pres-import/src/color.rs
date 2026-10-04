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
//! # What a scheme colour resolves to, and against what
//!
//! `a:schemeClr val="accent1"` names a theme slot — but `val="tx1"` names a
//! presentation *role*, and only the master's `p:clrMap` says which of two theme
//! entries that role is bound to. Both halves are read now
//! ([`crate::theme`]), so a scheme colour resolves to a concrete colour through
//! [`Resolver::scheme_color`] with its `a:tint`/`a:shade`/`a:alpha`/`a:lumMod`/
//! `a:lumOff` folded in.
//!
//! What is still NOT resolved is a slot this build has no palette for: a deck with
//! no theme part, or a `@val` outside `ST_SchemeColorVal`. Inventing an Office
//! palette there would paint a branded deck in the wrong brand and look
//! deliberate, so it stays reported.
//!
//! # Units
//!
//! Every colour transform is `ST_Percentage` in THOUSANDTHS of a percent, and
//! [`ColorTransform`] holds them in exactly those units. `a:ln@w` is EMU. Nothing
//! here converts between the two.

use casual_doc_model::v1::{
    ColorTransform, DashStyle, Fill, LineEnd, LineEndKind, LineEndSize, Rgba, ShapeStroke,
    StyleColor,
};
use casual_pres_model::SlidePaint;
use quick_xml::events::BytesStart;

use crate::ImportError;
use crate::loss::Reporter;
use crate::theme::{Resolver, SchemeOutcome, parse_percentage};
use crate::xml::{Cursor, attribute, children, integer_attribute, local_name};

/// The outcome of reading a colour-bearing element's child.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ColorRead {
    /// The concrete colour, when the element named one this build can resolve.
    pub(crate) rgba: Option<Rgba>,
    /// Whether the element named `a:phClr`, the placeholder.
    pub(crate) placeholder: bool,
    /// The transforms the placeholder carries forward. Meaningful only when
    /// `placeholder` is set: a transform over a KNOWN base is folded into `rgba`
    /// immediately, because folding needs the base and here it exists.
    pub(crate) transform: ColorTransform,
}

impl ColorRead {
    /// This colour as a [`StyleColor`], preserving the placeholder and the
    /// transform that travels with it.
    pub(crate) fn style_color(self) -> Option<StyleColor> {
        if self.placeholder {
            return Some(StyleColor::Placeholder(self.transform));
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
    resolver: Resolver,
) -> Result<ColorRead, ImportError> {
    let mut read = ColorRead::default();
    let part = cursor.part().to_owned();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"srgbClr" => {
                let base = parse_srgb(attribute(element, b"val", cursor.part())?.as_deref());
                if base.is_none() {
                    reporter.invalid(&part, b"srgbClr");
                }
                // The base is known, so the transforms fold NOW. This is the
                // immediate half of the same arithmetic `ColorTransform::apply`
                // performs on a deferred placeholder, and it is the same function,
                // so the two cannot drift.
                let transform = read_transform_if_any(cursor, reporter, &part, empty)?;
                read.rgba = base.map(|base| transform.apply(base));
                Ok(!empty)
            }
            b"schemeClr" => {
                let value = attribute(element, b"val", cursor.part())?.unwrap_or_default();
                let transform = read_transform_if_any(cursor, reporter, &part, empty)?;
                match resolver.scheme_color(&value, transform) {
                    SchemeOutcome::Resolved(color) => read.rgba = Some(color),
                    SchemeOutcome::Placeholder => {
                        read.placeholder = true;
                        read.transform = transform;
                    }
                    // A theme slot with no palette to resolve it against, or a
                    // token outside the enumeration: reported, not guessed.
                    SchemeOutcome::Unresolved => reporter.degraded(&part, b"schemeClr"),
                }
                Ok(!empty)
            }
            b"sysClr" => {
                // `a:sysClr` carries the host system colour plus a `lastClr`
                // snapshot of what the producer resolved it to. Honouring
                // `lastClr` is the lossless reading: it is the colour the file
                // was authored to look like, and resolving the system slot on
                // this machine would make the deck change appearance per host.
                let base = parse_srgb(attribute(element, b"lastClr", cursor.part())?.as_deref());
                if base.is_none() {
                    reporter.degraded(&part, b"sysClr");
                }
                let transform = read_transform_if_any(cursor, reporter, &part, empty)?;
                read.rgba = base.map(|base| transform.apply(base));
                Ok(!empty)
            }
            b"prstClr" | b"hslClr" | b"scrgbClr" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            _ => Ok(false),
        }
    })?;
    Ok(read)
}

/// Reads a theme-style colour (`a:fmtScheme` entry, `p:style` argument) into
/// `slot`, matching [`children`]'s callback contract.
///
/// A fixed base folds its transform here, because the base is known; `a:phClr`
/// carries the transform forward, because its base arrives only when a shape's
/// `a:fillRef`/`a:lnRef` supplies one. That is the whole reason
/// [`StyleColor::Placeholder`] holds a [`ColorTransform`] and
/// [`StyleColor::Fixed`] does not: the default Office theme's gradient entries
/// are three `phClr` stops that differ ONLY in their transforms, so a build that
/// dropped them would resolve a three-stop gradient to three copies of one colour.
pub(crate) fn read_style_color(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    part: &str,
    slot: &mut Option<StyleColor>,
) -> Result<bool, ImportError> {
    let local = local_name(element);
    match local {
        b"schemeClr" => {
            let value = attribute(element, b"val", cursor.part())?.unwrap_or_default();
            let transform = read_transform_if_any(cursor, reporter, part, empty)?;
            if value == "phClr" {
                *slot = Some(StyleColor::Placeholder(transform));
            } else {
                // A theme-relative colour inside the theme's OWN style matrix. The
                // matrix is deck-wide while a colour map is per part, so there is
                // no one map to resolve it through here — the entry stays
                // unmodelled and its caller reports the whole entry.
                reporter.degraded(part, b"schemeClr");
            }
            Ok(!empty)
        }
        b"srgbClr" => {
            let base = parse_srgb(attribute(element, b"val", cursor.part())?.as_deref());
            let transform = read_transform_if_any(cursor, reporter, part, empty)?;
            match base {
                Some(base) => *slot = Some(StyleColor::Fixed(transform.apply(base))),
                None => reporter.invalid(part, b"srgbClr"),
            }
            Ok(!empty)
        }
        b"sysClr" => {
            let base = parse_srgb(attribute(element, b"lastClr", cursor.part())?.as_deref());
            let transform = read_transform_if_any(cursor, reporter, part, empty)?;
            match base {
                Some(base) => *slot = Some(StyleColor::Fixed(transform.apply(base))),
                None => reporter.degraded(part, b"sysClr"),
            }
            Ok(!empty)
        }
        other => {
            reporter.omitted(part, other);
            Ok(false)
        }
    }
}

/// Reads a colour element's transform children when it has any.
///
/// A self-closing colour element has no children to read AND no end tag, so
/// entering it would consume the following sibling's events — the bug
/// [`crate::xml::enter`] exists for. `<a:srgbClr val="4472C4"/>` and
/// `<a:tint val="40000"/>` are exactly that shape, which is why the attributes
/// above are read unconditionally and only the descent is guarded.
pub(crate) fn read_transform_if_any(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    part: &str,
    empty: bool,
) -> Result<ColorTransform, ImportError> {
    if empty {
        return Ok(ColorTransform::default());
    }
    read_color_transform(cursor, reporter, part)
}

/// Reads the colour transforms on a colour element, having just entered it.
///
/// Values stay in the per-100000 units the file states; the single division lives
/// in `ColorTransform::apply`.
///
/// `a:satMod` IS applied, as of the shared-model theme lane: `ColorTransform` now
/// carries a saturation field and `casual_doc_model::v1::fold_color_modifiers`
/// folds it as an exact HSL saturation scaling about the lightness, between the
/// luminance pair and the blends — so reading it here changes nothing about the
/// arithmetic and only stops throwing the value away. An earlier revision of this
/// comment called the omission deliberate on the ground that the model had no
/// field; that ground is gone.
///
/// The hue, gamma and channel-wise modifiers are still reported rather than
/// applied, and for the original reason: the shared fold applies none of them, and
/// a half-correct fold here would change every colour this build already resolves
/// on the document side too, since the arithmetic is shared.
pub(crate) fn read_color_transform(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    part: &str,
) -> Result<ColorTransform, ImportError> {
    let mut transform = ColorTransform::default();
    children(cursor, |cursor, element, _empty| {
        let local = local_name(element);
        let field = match local {
            b"lumMod" => &mut transform.lum_mod,
            b"lumOff" => &mut transform.lum_off,
            b"satMod" => &mut transform.sat_mod,
            b"tint" => &mut transform.tint,
            b"shade" => &mut transform.shade,
            b"alpha" => &mut transform.alpha,
            other => {
                reporter.omitted(part, other);
                return Ok(false);
            }
        };
        match attribute(element, b"val", cursor.part())?
            .as_deref()
            .and_then(parse_percentage)
        {
            Some(value) => *field = Some(value),
            // A modifier with no parsable `@val` is a stated transform this build
            // did not apply, which is a visible colour difference.
            None => reporter.degraded_attribute(part, local, b"val"),
        }
        Ok(false)
    })?;
    Ok(transform)
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

/// How a shape's fill element resolved: the value, and what the file stated.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FillRead {
    /// The modelled fill, when one was resolvable.
    pub(crate) fill: Option<Fill>,
    /// What the element stated, which is not derivable from `fill`: an
    /// unmodellable `a:gradFill` and an absent fill element both leave `fill`
    /// `None` and mean opposite things to a consumer that inherits.
    pub(crate) state: SlidePaint,
}

/// Reads a shape-property fill child, having just seen it.
///
/// Returns whether the element's subtree was consumed, matching
/// [`children`]'s contract.
///
/// # `a:noFill` is CARRIED now, and the caller decides whether to report it
///
/// This function used to report every `a:noFill` as a degraded `spPr/@noFill`,
/// because `v1::GroupShape::fill` is an `Option<Fill>` in which `None` has to mean
/// two different things on a slide: "states no fill, so inherit from the
/// placeholder slot and then from the theme", and "states `a:noFill`, so is
/// deliberately transparent and must *not* inherit". That gap is closed by
/// `casual_pres_model::SlidePaint`, which is a presentation-side field exactly as
/// the previous revision of this comment said it had to be.
///
/// So nothing is reported here any more, and the reporting moved OUT rather than
/// away: whether the distinction survives depends on where the element was read,
/// and only the caller knows that.
///
/// | Caller | `p:spPr` of a top-level `p:sp`/`p:pic` | inside a `p:grpSp` | an `a:tcPr` | a `p:grpSpPr` |
/// | --- | --- | --- | --- | --- |
/// | Carries it? | yes, on the `SlideNode` | no — a group child is a bare `GroupChild` | no — `TableCellProperties` has no field | nothing to carry: a group has no fill |
/// | Reports? | no | yes, as `spPr/@noFill` | yes, as `tcPr/@noFill` | no |
///
/// Reporting it here regardless would have put 90 false findings in the one real
/// deck this was measured on — the loss is fixed for those, and a report that
/// still names it would be describing a loss that no longer happens (`SKILL` §9).
pub(crate) fn read_fill_child(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    read: &mut FillRead,
    resolver: Resolver,
) -> Result<bool, ImportError> {
    let local = local_name(element);
    let part = cursor.part().to_owned();
    match local {
        b"noFill" => {
            read.state = SlidePaint::Suppressed;
            Ok(false)
        }
        b"solidFill" => {
            if empty {
                reporter.invalid(&part, b"solidFill");
                return Ok(false);
            }
            // `Authored` even when the colour does not resolve: the FILE states a
            // fill either way, so a shape whose `a:schemeClr` found no palette must
            // still not inherit one from the theme matrix on top of the colour it
            // already failed to resolve.
            read.state = SlidePaint::Authored;
            // A fill whose colour did not resolve is reported by `read_solid_fill`
            // on the colour element itself, which is where the fact is; adding a
            // second finding here would double-count every one of them.
            read.fill = read_solid_fill(cursor, reporter, resolver)?
                .rgba
                .map(Fill::Solid);
            Ok(true)
        }
        b"gradFill" | b"blipFill" | b"pattFill" | b"grpFill" => {
            // Stated, and unmodellable. Both halves matter: the `omitted` finding
            // says the gradient was lost, and `Authored` says the shape is
            // nevertheless filled in the file — so a consumer must not quietly
            // substitute the theme's solid entry and make the reported loss
            // invisible on screen.
            read.state = SlidePaint::Authored;
            reporter.omitted(&part, local);
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// How an `a:ln` resolved: the stroke, and what the element stated.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LineRead {
    /// The modelled stroke, when the outline named a colour this build resolved.
    pub(crate) stroke: Option<ShapeStroke>,
    /// What the element stated. [`SlidePaint::Inherited`] when there was no `a:ln`
    /// at all — which is the one state this function never returns, since it is
    /// only called having seen one.
    pub(crate) state: SlidePaint,
}

/// Reads an `a:ln`, having just entered it.
///
/// `stroke` is `None` when the outline names no resolvable colour: `ShapeStroke`
/// requires a concrete [`Rgba`], and an outline painted in a substituted colour
/// looks like an authored choice. The width is still read and reported in that
/// case, so the loss is visible rather than silent.
///
/// `state` is the `a:ln/a:noFill` half of the same distinction `read_fill_child`
/// carries, and it is returned rather than reported here for the same reason —
/// `tcPr`'s four edges have nowhere to put it and a `p:spPr` does, so only the
/// caller can say whether anything was lost.
pub(crate) fn read_line(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    resolver: Resolver,
) -> Result<LineRead, ImportError> {
    let part = cursor.part().to_owned();
    // `a:ln@w` is in EMU, like every other DrawingML measure here. It is NOT in
    // points or eighths of a point, which is the conversion a reader coming from
    // VML gets wrong.
    //
    // Read BEFORE the `empty` check, and that ordering is the whole point:
    // `<a:ln w="12700"/>` and `<a:lnB w="12700"/>` are both legal and both state
    // their entire meaning in `@w`. The caller used to answer `None` for a
    // self-closing outline and the width was discarded — the same shape of bug
    // `xml::enter` exists for, in the one reader a table's four borders also use.
    let width_emu = integer_attribute(element, b"w", &part)?.unwrap_or(0);
    if empty {
        // No colour child, so there is nothing to stroke with. A stated width
        // with no resolvable colour is the `degraded` case the tail of this
        // function already reports, reached through the same path.
        if width_emu != 0 {
            reporter.degraded(&part, b"ln");
        }
        // `Authored`, not `Suppressed`: `<a:ln w="12700"/>` states an outline. It
        // is the width's colour that is missing, not the outline.
        return Ok(LineRead {
            stroke: None,
            state: SlidePaint::Authored,
        });
    }
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
                color = read_solid_fill(cursor, reporter, resolver)?.rgba;
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
        // `a:ln/a:noFill` is "explicitly unstroked" — CARRIED now, as
        // `SlidePaint::Suppressed`, and reported by the caller only where the
        // caller cannot carry it. A stroke is deliberately not returned beside it:
        // `ShapeStroke` has no "invisible" spelling, and a zero-width one paints a
        // hairline.
        return Ok(LineRead {
            stroke: None,
            state: SlidePaint::Suppressed,
        });
    }
    match color {
        Some(color) => Ok(LineRead {
            stroke: Some(ShapeStroke {
                color,
                width_emu,
                dash,
                head_end,
                tail_end,
            }),
            state: SlidePaint::Authored,
        }),
        None => {
            if width_emu != 0 || dash.is_some() {
                // The file states an outline but not a colour this build can
                // resolve — a theme line reference with no theme, normally.
                // Dropping it silently would make a bordered shape reopen
                // unbordered.
                reporter.degraded(&part, b"ln");
            }
            Ok(LineRead {
                stroke: None,
                // Still `Authored`: an `a:ln` that states only a `a:prstDash`, or
                // only a colour this build cannot resolve, is an outline the file
                // asked for. Calling it `Inherited` would let the theme's
                // `a:lnStyleLst` paint over an authored one.
                state: SlidePaint::Authored,
            })
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

/// Maps an `a:prstDash@val` token, shared with the theme's `a:lnStyleLst` reader so
/// a dash on a shape and a dash in the style matrix cannot be read differently.
pub(crate) fn dash_style(token: &str) -> Option<DashStyle> {
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
