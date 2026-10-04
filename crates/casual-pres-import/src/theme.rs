// SPDX-License-Identifier: Apache-2.0

//! `ppt/theme/theme1.xml` and the colour map, which together are what makes a
//! scheme colour resolvable at all.
//!
//! # Why this parser exists next to `casual-doc-import`'s, and what would remove it
//!
//! `a:theme` is the SAME DrawingML grammar in both document classes, and
//! `casual-doc-import/src/theme.rs` already reads it into the same
//! [`ColorScheme`], [`FontScheme`] and [`FormatScheme`] this module produces.
//! Reuse was the intent and it is blocked, by three things rather than one:
//!
//! 1. `casual_doc_import::theme` is a private module and `theme::parse` is
//!    `pub(crate)`; nothing is re-exported.
//! 2. Its signature takes `&mut casual_doc_import::report::Reporter`, which is
//!    also `pub(crate)` and whose findings carry no PART NAME. This importer
//!    charges every finding to the part it was read from, because a loss in the
//!    master and the same loss on slide 7 are different facts to act on.
//! 3. It takes `ImportConfig` for its bounds, where this crate bounds parts with
//!    [`ImportLimits`](crate::ImportLimits) through one reader that also refuses a
//!    `DOCTYPE`, and `casual-pres-import` does not depend on `casual-doc-import`
//!    at all — adding that dependency would pull the whole WordprocessingML
//!    importer onto the presentation open path.
//!
//! So the MODEL types, the colour arithmetic
//! ([`ColorTransform::apply`](casual_doc_model::v1::ColorTransform::apply)) and the
//! style-matrix index rules ([`FormatScheme::fill_style`] and its siblings) are
//! reused verbatim — every type this module produces is one `casual-doc-layout`
//! already resolves — and only the TRAVERSAL is written here, against this crate's
//! own bounded cursor. The fix that removes even that is named in the report
//! accompanying this change: lift the `a:theme` traversal into a crate both
//! importers depend on (`casual-doc-ooxml` is the natural home) parameterised over
//! a reporting trait, so one grammar has one reader.
//!
//! # Units
//!
//! `a:tint`/`a:shade`/`a:alpha`/`a:lumMod`/`a:lumOff` are `ST_Percentage` in
//! THOUSANDTHS of a percent: `val="40000"` is 40%. [`ColorTransform`] holds them
//! in exactly those units, unconverted, and `ColorTransform::apply` is the one
//! place the division by 100000 happens. `a:ln@w` is EMU.

use casual_doc_model::v1::{
    ColorScheme, ColorTransform, DashStyle, EffectStyle, FillStyle, FontCollection,
    FontCollectionIndex, FontReference, FontScheme, FormatScheme, GradientKind, GradientStyle,
    GradientStyleStop, LineStyle, PatternStyle, Rgba, SchemeColor, ScriptFont, ShapeStyleRef,
    StyleColor, SystemColor, ThemeFontAxis, ThemeFontEntry,
};
use casual_pres_model::{ColorMap, ColorRole, THEME_COLOR_SLOTS, ThemeColorSlot, ThemePalette};
use quick_xml::events::BytesStart;

use crate::ImportError;
use crate::color::{read_color_transform, read_style_color, read_transform_if_any};
use crate::limits::ImportLimits;
use crate::loss::Reporter;
use crate::xml::{Cursor, attribute, children, enter, integer_attribute, local_name};

/// The longest `@typeface`, `@panose` or `@name` this build keeps, matching the
/// bound `casual_doc_model::v1::Document::validate` enforces on the same fields so
/// a theme read here satisfies the invariants that validator checks.
const MAX_THEME_TEXT: usize = 255;

/// The longest `a:font@script` tag, the same bound again.
const MAX_SCRIPT_TAG: usize = 32;

/// What a deck's theme part declares, as far as this build models it.
#[derive(Clone, Debug, Default)]
pub(crate) struct ThemePart {
    /// `a:clrScheme` — the twelve slots.
    pub(crate) color_scheme: Option<ColorScheme>,
    /// `a:fontScheme` — the major and minor collections.
    pub(crate) font_scheme: Option<FontScheme>,
    /// `a:fmtScheme` — the modelled subset of the style matrix.
    pub(crate) format_scheme: Option<FormatScheme>,
}

/// What a part's colour-bearing elements resolve against: the theme palette under
/// that part's own effective colour map.
///
/// `Copy` and deliberately small — it is passed to every colour reader by value, so
/// no reader can mutate the theme while reading a shape. It carries the palette and
/// NOT the style matrix: a `p:style` reference is classified after the whole deck
/// is read, because that classification needs the side table of what was actually
/// referenced as well as the matrix itself.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Resolver {
    palette: Option<ThemePalette>,
}

/// How an `a:schemeClr` resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemeOutcome {
    /// The slot resolved to a concrete colour with its transforms folded in.
    Resolved(Rgba),
    /// The element named `a:phClr`: a formal parameter, not a colour.
    Placeholder,
    /// No theme colour scheme was read, or `@val` is outside
    /// `ST_SchemeColorVal`. Reported by the caller, never guessed.
    Unresolved,
}

impl Resolver {
    /// A resolver over one theme, under one colour map.
    pub(crate) fn new(theme: &ThemePart, map: ColorMap) -> Self {
        Self {
            palette: theme
                .color_scheme
                .as_ref()
                .map(|scheme| ThemePalette::new(scheme, map)),
        }
    }

    /// The same theme under a different colour map, for a part that states a
    /// `p:clrMapOvr`.
    pub(crate) fn with_map(self, map: ColorMap) -> Self {
        Self {
            palette: self.palette.map(|palette| palette.with_map(map)),
        }
    }

    /// Resolves one `a:schemeClr@val` with its colour transforms.
    ///
    /// # Complexity
    ///
    /// O(1).
    pub(crate) fn scheme_color(&self, value: &str, transform: ColorTransform) -> SchemeOutcome {
        if value == "phClr" {
            return SchemeOutcome::Placeholder;
        }
        match self
            .palette
            .and_then(|palette| palette.resolve(value, transform))
        {
            Some(color) => SchemeOutcome::Resolved(color),
            None => SchemeOutcome::Unresolved,
        }
    }
}

/// Reads `ppt/theme/theme1.xml`.
///
/// Every construct the model does not carry is reported on its OUTERMOST element
/// and its subtree skipped, so one dropped construct is one finding rather than
/// one per descendant — the convention `casual-doc-import`'s theme reader
/// established.
///
/// # Complexity
///
/// O(part), once, bounded by [`ImportLimits`].
pub(crate) fn read_theme_part(
    bytes: &[u8],
    part: &str,
    reporter: &mut Reporter,
    limits: ImportLimits,
) -> Result<ThemePart, ImportError> {
    let mut cursor = Cursor::new(bytes, part, limits);
    let root = cursor.root()?;
    // `a:theme@name` is the theme's display name. Nothing in the model carries it
    // and there is no presentation writer to re-emit the part, so a named theme
    // loses its name: reported as the attribute whose meaning was not carried.
    report_dropped_name(&root, b"theme", part, reporter)?;
    let mut theme = ThemePart::default();

    children(&mut cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            // A pure container; everything it holds is dispositioned below.
            b"themeElements" => enter(cursor, empty, |cursor, child, child_empty| {
                read_theme_element(cursor, reporter, child, child_empty, &mut theme)
            }),
            b"extLst" => Ok(false),
            // `a:objectDefaults`, `a:extraClrSchemeLst`, `a:custClrLst`, and any
            // foreign element. None is modelled and none is retained.
            other => {
                reporter.omitted(part, other);
                Ok(false)
            }
        }
    })?;
    Ok(theme)
}

/// Reads one child of `a:themeElements`.
fn read_theme_element(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    theme: &mut ThemePart,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    match local_name(element) {
        b"clrScheme" => {
            let mut scheme = ColorScheme {
                name: attribute(element, b"name", &part)?
                    .filter(|value| value.len() <= MAX_THEME_TEXT)
                    .unwrap_or_default(),
                ..ColorScheme::default()
            };
            let mut filled = 0_usize;
            let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                read_color_slot(
                    cursor,
                    reporter,
                    child,
                    child_empty,
                    &mut scheme,
                    &mut filled,
                )
            })?;
            // `ColorScheme`'s fields are values and default to black, so a scheme
            // that filled NO slot would resolve every role in the deck to black —
            // a confidently wrong answer dressed as a palette. No palette at all is
            // the honest outcome, and it is the one that keeps reporting the gap.
            if filled == 0 {
                reporter.invalid(&part, b"clrScheme");
                return Ok(consumed);
            }
            // A PARTIAL scheme is admitted, because the slots that did arrive are
            // real, but it is reported: the model cannot express "this slot was not
            // stated", so the missing ones silently read black.
            if filled < THEME_COLOR_SLOTS {
                reporter.degraded(&part, b"clrScheme");
            }
            theme.color_scheme = Some(scheme);
            Ok(consumed)
        }
        b"fontScheme" => {
            // `a:fontScheme@name` has no field either, same as the theme's own.
            report_dropped_name(element, b"fontScheme", &part, reporter)?;
            let mut scheme = FontScheme::default();
            let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                read_font_collection(cursor, reporter, child, child_empty, &mut scheme)
            })?;
            theme.font_scheme = Some(scheme);
            Ok(consumed)
        }
        b"fmtScheme" => {
            // `a:fmtScheme@name` has nowhere to live, and — unlike the document
            // side, which retains the whole subtree verbatim for its writer —
            // nothing here retains the part. So the name is reported, and so is
            // every entry the typed form cannot hold.
            report_dropped_name(element, b"fmtScheme", &part, reporter)?;
            let mut scheme = FormatScheme::default();
            let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                read_style_list(cursor, reporter, child, child_empty, &mut scheme)
            })?;
            theme.format_scheme = Some(scheme);
            Ok(consumed)
        }
        b"extLst" => Ok(false),
        other => {
            reporter.omitted(&part, other);
            Ok(false)
        }
    }
}

/// Reads one `a:clrScheme` child (`a:dk1`, `a:accent1`, …) into its slot.
fn read_color_slot(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    scheme: &mut ColorScheme,
    filled: &mut usize,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    let local = local_name(element);
    if local == b"extLst" {
        return Ok(false);
    }
    let Some(slot) = ThemeColorSlot::from_token(&String::from_utf8_lossy(local)) else {
        reporter.omitted(&part, local);
        return Ok(false);
    };
    let mut value: Option<SchemeColor> = None;
    let consumed = enter(cursor, empty, |cursor, child, child_empty| {
        let child_local = local_name(child);
        match child_local {
            b"srgbClr" => {
                value = parse_rgb(attribute(child, b"val", cursor.part())?.as_deref())
                    .map(SchemeColor::Srgb);
                if value.is_none() {
                    reporter.invalid(&part, b"srgbClr");
                }
                // A slot's colour may carry transforms. `SchemeColor` holds the
                // base only, and a slot whose stated transform were dropped would
                // resolve every reference to it slightly wrong, so the transform is
                // folded into the stored base — which is exact, because the base is
                // known here.
                if !child_empty {
                    let transform = read_color_transform(cursor, reporter, &part)?;
                    if let Some(SchemeColor::Srgb(rgb)) = value {
                        let folded = transform.apply(Rgba {
                            r: rgb.r,
                            g: rgb.g,
                            b: rgb.b,
                            a: 255,
                        });
                        value = Some(SchemeColor::Srgb(casual_doc_model::v1::RgbColor {
                            r: folded.r,
                            g: folded.g,
                            b: folded.b,
                        }));
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            b"sysClr" => {
                value = Some(SchemeColor::System(SystemColor {
                    value: attribute(child, b"val", cursor.part())?
                        .filter(|value| !value.is_empty() && value.len() <= MAX_SCRIPT_TAG)
                        .unwrap_or_default(),
                    last_color: parse_rgb(attribute(child, b"lastClr", cursor.part())?.as_deref()),
                }));
                if !child_empty {
                    // The transform cannot be folded: the base is whichever colour
                    // the host resolves the system slot to.
                    let _ = read_color_transform(cursor, reporter, &part)?;
                    return Ok(true);
                }
                Ok(false)
            }
            // A colour choice `SchemeColor` does not carry. The slot keeps its
            // default, so the whole subtree is one loss.
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    if let Some(value) = value {
        *slot_mut(scheme, slot) = value;
        *filled = filled.saturating_add(1);
    }
    Ok(consumed)
}

/// The scheme field a slot names.
fn slot_mut(scheme: &mut ColorScheme, slot: ThemeColorSlot) -> &mut SchemeColor {
    match slot {
        ThemeColorSlot::Dark1 => &mut scheme.dark1,
        ThemeColorSlot::Light1 => &mut scheme.light1,
        ThemeColorSlot::Dark2 => &mut scheme.dark2,
        ThemeColorSlot::Light2 => &mut scheme.light2,
        ThemeColorSlot::Accent1 => &mut scheme.accent1,
        ThemeColorSlot::Accent2 => &mut scheme.accent2,
        ThemeColorSlot::Accent3 => &mut scheme.accent3,
        ThemeColorSlot::Accent4 => &mut scheme.accent4,
        ThemeColorSlot::Accent5 => &mut scheme.accent5,
        ThemeColorSlot::Accent6 => &mut scheme.accent6,
        ThemeColorSlot::Hyperlink => &mut scheme.hyperlink,
        ThemeColorSlot::FollowedHyperlink => &mut scheme.followed_hyperlink,
    }
}

/// Reads an `a:majorFont`/`a:minorFont` collection.
fn read_font_collection(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    scheme: &mut FontScheme,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    let collection: &mut FontCollection = match local_name(element) {
        b"majorFont" => &mut scheme.major,
        b"minorFont" => &mut scheme.minor,
        b"extLst" => return Ok(false),
        other => {
            reporter.omitted(&part, other);
            return Ok(false);
        }
    };
    enter(cursor, empty, |cursor, child, _child_empty| {
        let local = local_name(child);
        match local {
            b"latin" | b"ea" | b"cs" => {
                let entry = read_font_entry(child, cursor.part())?;
                match local {
                    b"latin" => collection.latin = entry,
                    b"ea" => collection.ea = entry,
                    _ => collection.cs = entry,
                }
                Ok(false)
            }
            b"font" => {
                match read_script_font(child, cursor.part())? {
                    Some(font) => collection.script_overrides.push(font),
                    // A script/typeface pair outside the modelled bounds, or one of
                    // the two missing: the override is dropped, not half-applied.
                    None => reporter.omitted(&part, local),
                }
                Ok(false)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })
}

/// Reads one `a:latin`/`a:ea`/`a:cs`.
fn read_font_entry(element: &BytesStart<'_>, part: &str) -> Result<ThemeFontEntry, ImportError> {
    Ok(ThemeFontEntry {
        typeface: attribute(element, b"typeface", part)?
            .filter(|value| value.len() <= MAX_THEME_TEXT)
            .unwrap_or_default(),
        panose: bounded(element, b"panose", part)?,
        pitch_family: bounded(element, b"pitchFamily", part)?,
        charset: bounded(element, b"charset", part)?,
    })
}

/// Reads one `<a:font script=".." typeface=".."/>` supplemental override.
fn read_script_font(
    element: &BytesStart<'_>,
    part: &str,
) -> Result<Option<ScriptFont>, ImportError> {
    let Some(script) = attribute(element, b"script", part)? else {
        return Ok(None);
    };
    let Some(typeface) = attribute(element, b"typeface", part)? else {
        return Ok(None);
    };
    Ok(
        (!script.is_empty() && script.len() <= MAX_SCRIPT_TAG && typeface.len() <= MAX_THEME_TEXT)
            .then_some(ScriptFont { script, typeface }),
    )
}

/// Reads one `a:fmtScheme` style list.
///
/// `a:bgFillStyleLst` contributes nothing on purpose: the only way to select it is
/// an `a:fillRef@idx >= 1000`, which [`FormatScheme::fill_style`] resolves to
/// nothing, so contributing entries here would only give index arithmetic
/// something wrong to find. It IS reported, because a `p:bgRef` names it and this
/// build paints no background style.
fn read_style_list(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    scheme: &mut FormatScheme,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    match local_name(element) {
        b"fillStyleLst" => enter(cursor, empty, |cursor, child, child_empty| {
            let entry = read_fill_entry(cursor, reporter, child, child_empty)?;
            scheme.fill_styles.push(entry.style);
            Ok(entry.consumed)
        }),
        b"lnStyleLst" => enter(cursor, empty, |cursor, child, child_empty| {
            let entry = read_line_entry(cursor, reporter, child, child_empty)?;
            scheme.line_styles.push(entry.style);
            Ok(entry.consumed)
        }),
        b"effectStyleLst" => enter(cursor, empty, |cursor, child, child_empty| {
            let local = local_name(child);
            if local != b"effectStyle" {
                reporter.omitted(&part, local);
                return Ok(false);
            }
            let mut carries_effects = false;
            let consumed = enter(cursor, child_empty, |cursor, inner, inner_empty| {
                read_effect_child(cursor, reporter, inner, inner_empty, &mut carries_effects)
            })?;
            scheme.effect_styles.push(EffectStyle { carries_effects });
            Ok(consumed)
        }),
        b"bgFillStyleLst" => {
            if !empty {
                reporter.omitted(&part, b"bgFillStyleLst");
            }
            Ok(false)
        }
        b"extLst" => Ok(false),
        other => {
            reporter.omitted(&part, other);
            Ok(false)
        }
    }
}

/// One style-list entry: the modelled style, or the `None` that keeps every later
/// entry's one-based index correct.
struct Entry<T> {
    style: Option<T>,
    consumed: bool,
}

/// Reads one `a:fillStyleLst` entry.
///
/// An entry this build cannot hold becomes `None` IN PLACE rather than being
/// dropped: dropping it would shift every later index, which is the one failure
/// mode that silently paints the wrong style.
fn read_fill_entry(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
) -> Result<Entry<FillStyle>, ImportError> {
    let part = cursor.part().to_owned();
    match local_name(element) {
        b"solidFill" => {
            let mut color: Option<StyleColor> = None;
            let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                read_style_color(cursor, reporter, child, child_empty, &part, &mut color)
            })?;
            if color.is_none() {
                reporter.omitted(&part, b"solidFill");
            }
            Ok(Entry {
                style: color.map(|color| FillStyle::Solid { color }),
                consumed,
            })
        }
        b"gradFill" => {
            let mut stops: Vec<GradientStyleStop> = Vec::new();
            let mut kind: Option<GradientKind> = None;
            let mut stop_lost = false;
            let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                read_gradient_child(
                    cursor,
                    reporter,
                    child,
                    child_empty,
                    &part,
                    &mut stops,
                    &mut kind,
                    &mut stop_lost,
                )
            })?;
            // A three-stop gradient delivered as two is a DIFFERENT gradient that
            // still paints, which is worse than no gradient: nothing downstream
            // could tell it had happened.
            if stops.is_empty() || stop_lost {
                reporter.omitted(&part, b"gradFill");
                return Ok(Entry {
                    style: None,
                    consumed,
                });
            }
            Ok(Entry {
                // ECMA-376 §20.1.8.33: neither `a:lin` nor `a:path` is a linear
                // sweep along the default axis.
                style: Some(FillStyle::Gradient(GradientStyle {
                    stops,
                    kind: kind.unwrap_or(GradientKind::Linear { angle: 0 }),
                })),
                consumed,
            })
        }
        b"pattFill" => {
            let preset = attribute(element, b"prst", &part)?
                .filter(|value| value.len() <= MAX_THEME_TEXT)
                .unwrap_or_default();
            let mut foreground: Option<StyleColor> = None;
            let mut background: Option<StyleColor> = None;
            let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                let slot = match local_name(child) {
                    b"fgClr" => &mut foreground,
                    b"bgClr" => &mut background,
                    other => {
                        reporter.omitted(&part, other);
                        return Ok(false);
                    }
                };
                enter(cursor, child_empty, |cursor, inner, inner_empty| {
                    read_style_color(cursor, reporter, inner, inner_empty, &part, slot)
                })
            })?;
            match (foreground, background) {
                (Some(foreground), Some(background)) => Ok(Entry {
                    style: Some(FillStyle::Pattern(PatternStyle {
                        preset,
                        foreground,
                        background,
                    })),
                    consumed,
                }),
                _ => {
                    reporter.omitted(&part, b"pattFill");
                    Ok(Entry {
                        style: None,
                        consumed,
                    })
                }
            }
        }
        // `a:noFill` is representable and it means "nothing". NOT a loss, which is
        // why it is distinct from the unmodelled kinds even though both push a
        // `None`: a theme that says "no fill" is honoured exactly, and reporting it
        // would be inventing a finding.
        b"noFill" => Ok(Entry {
            style: None,
            consumed: false,
        }),
        other => {
            reporter.omitted(&part, other);
            Ok(Entry {
                style: None,
                consumed: false,
            })
        }
    }
}

/// Reads one child of an `a:gradFill` fill-style entry.
#[expect(
    clippy::too_many_arguments,
    reason = "a gradient entry accumulates four independent facts (stops, geometry, \
              a lost stop, the part) and bundling them in a struct for one call site \
              would hide which of them this function writes"
)]
fn read_gradient_child(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    part: &str,
    stops: &mut Vec<GradientStyleStop>,
    kind: &mut Option<GradientKind>,
    stop_lost: &mut bool,
) -> Result<bool, ImportError> {
    match local_name(element) {
        b"gsLst" => enter(cursor, empty, |cursor, child, child_empty| {
            if local_name(child) != b"gs" {
                reporter.omitted(part, local_name(child));
                return Ok(false);
            }
            let position = attribute(child, b"pos", cursor.part())?
                .as_deref()
                .and_then(parse_percentage);
            let mut color: Option<StyleColor> = None;
            let consumed = enter(cursor, child_empty, |cursor, inner, inner_empty| {
                read_style_color(cursor, reporter, inner, inner_empty, part, &mut color)
            })?;
            match (position, color) {
                (Some(position), Some(color)) => stops.push(GradientStyleStop { position, color }),
                _ => *stop_lost = true,
            }
            Ok(consumed)
        }),
        b"lin" => {
            *kind = Some(GradientKind::Linear {
                angle: integer_attribute(element, b"ang", cursor.part())?
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or(0),
            });
            Ok(false)
        }
        b"path" => {
            *kind = Some(GradientKind::Radial);
            Ok(false)
        }
        b"tileRect" => Ok(false),
        other => {
            reporter.omitted(part, other);
            Ok(false)
        }
    }
}

/// Reads one `a:lnStyleLst` entry (`a:ln`).
///
/// Only a SOLID outline fill becomes an entry: [`LineStyle`] holds one colour, so
/// taking a gradient's first stop would draw a confidently wrong outline.
fn read_line_entry(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
) -> Result<Entry<LineStyle>, ImportError> {
    let part = cursor.part().to_owned();
    let local = local_name(element);
    if local != b"ln" {
        reporter.omitted(&part, local);
        return Ok(Entry {
            style: None,
            consumed: false,
        });
    }
    // EMU, like every other DrawingML measure here.
    let width_emu = integer_attribute(element, b"w", &part)?.unwrap_or(0);
    let mut color: Option<StyleColor> = None;
    let mut dash: Option<DashStyle> = None;
    let mut non_solid = false;
    let consumed = enter(cursor, empty, |cursor, child, child_empty| {
        match local_name(child) {
            b"solidFill" => enter(cursor, child_empty, |cursor, inner, inner_empty| {
                read_style_color(cursor, reporter, inner, inner_empty, &part, &mut color)
            }),
            b"gradFill" | b"pattFill" | b"blipFill" => {
                non_solid = true;
                reporter.omitted(&part, local_name(child));
                Ok(false)
            }
            b"prstDash" => {
                dash = attribute(child, b"val", cursor.part())?
                    .as_deref()
                    .and_then(crate::color::dash_style);
                Ok(false)
            }
            b"noFill" => Ok(false),
            // Cap, join, compound and the line ends have no field on `LineStyle`.
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok(Entry {
        style: (!non_solid)
            .then_some(color)
            .flatten()
            .map(|color| LineStyle {
                width_emu,
                color,
                dash,
            }),
        consumed,
    })
}

/// Reads one child of an `a:effectStyle`, recording only whether it carries an
/// effect at all.
///
/// Nothing in this build paints a DrawingML effect, so modelling an `a:outerShdw`'s
/// blur, distance, direction and colour would add a construct no user could reach
/// (`SKILL` §9.4). What the model carries is the one predicate the loss report
/// needs: an `a:effectStyleLst` routinely mixes an EMPTY `a:effectLst` with one
/// that holds a shadow, so reporting on the reference's index alone would raise a
/// finding for a shape that lost nothing.
fn read_effect_child(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    carries_effects: &mut bool,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    match local_name(element) {
        b"effectLst" => enter(cursor, empty, |_cursor, _child, _child_empty| {
            *carries_effects = true;
            Ok(false)
        }),
        b"effectDag" => {
            *carries_effects = true;
            Ok(false)
        }
        b"extLst" => Ok(false),
        // `a:scene3d` and `a:sp3d` sit BESIDE the effect list and are not effects.
        other => {
            reporter.omitted(&part, other);
            Ok(false)
        }
    }
}

/// Reads a `p:style` on a shape into a [`ShapeStyleRef`], having just entered it.
///
/// The `a:phClr` argument each reference supplies is resolved here, because it is
/// the SHAPE's own statement: `<a:fillRef idx="1"><a:schemeClr val="accent1"/></…>`
/// says "theme fill entry 1, in accent 1", and the accent is the shape's choice.
/// What is NOT resolved is the entry itself — the reference stays a reference, so a
/// theme change still follows and a round trip does not turn the theme's content
/// into the shape's authorship.
///
/// # Complexity
///
/// O(1) in the deck: four children, each with one colour.
pub(crate) fn read_shape_style(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    resolver: Resolver,
) -> Result<ShapeStyleRef, ImportError> {
    let part = cursor.part().to_owned();
    let mut style = ShapeStyleRef::default();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        let (index, color): (&mut Option<u32>, Option<&mut Option<Rgba>>) = match local {
            b"fillRef" => (&mut style.fill_idx, Some(&mut style.fill_color)),
            b"lnRef" => (&mut style.line_idx, Some(&mut style.line_color)),
            // `a:effectRef`'s colour has no field: the index is captured so the
            // reference RESOLVES (the loss report needs to know whether the entry
            // carries an effect), not so it renders.
            b"effectRef" => (&mut style.effect_idx, None),
            b"fontRef" => {
                // Handled on its own and NOT through the shared arm below, because
                // `a:fontRef@idx` is `ST_FontCollectionIndex` (`none`/`major`/
                // `minor`) rather than a numeric index into a style list — reading
                // it with the numeric grammar parses nothing and reports every
                // real reference as invalid.
                let consumed = read_font_reference(
                    cursor,
                    reporter,
                    element,
                    empty,
                    &part,
                    resolver,
                    &mut style.font_ref,
                )?;
                return Ok(consumed);
            }
            b"extLst" => return Ok(false),
            other => {
                reporter.omitted(&part, other);
                return Ok(false);
            }
        };
        *index = integer_attribute(element, b"idx", cursor.part())?
            .and_then(|value| u32::try_from(value).ok());
        if index.is_none() {
            reporter.invalid(&part, local);
        }
        let mut argument: Option<StyleColor> = None;
        let consumed = enter(cursor, empty, |cursor, child, child_empty| {
            read_scheme_argument(
                cursor,
                reporter,
                child,
                child_empty,
                &part,
                resolver,
                &mut argument,
            )
        })?;
        if let Some(color) = color {
            *color = argument.and_then(|argument| argument.resolve(None));
        }
        Ok(consumed)
    })?;
    Ok(style)
}

/// Reads an `a:fontRef`: the theme font collection a shape's text takes, and the
/// colour it takes with it.
///
/// # Why this is read at all, when nothing applies it
///
/// For the reason `ShapeStyleRef::effect_idx` is captured: so the reference
/// RESOLVES. Nothing in this build applies a shape-scoped text default — a slide
/// run still resolves its typeface through the placeholder cascade and the theme's
/// font scheme — so a themed typeface and a themed text colour both go unpainted.
/// But an uncaptured reference cannot say WHICH half was lost, and that is the
/// difference between a report naming a typeface and one naming an element.
///
/// Whether anything was LOST is decided later, by `report_unapplied_font_refs`,
/// for the reason `report_unpaintable_style_refs` is a post-pass too: the answer
/// needs the theme's font scheme as well as the reference, and a reader holding
/// only the reference would have to overstate — a `major` reference against a
/// theme with no font scheme asked for a typeface that does not exist, and
/// reporting it as unpainted would be a finding about nothing.
///
/// A token outside `ST_FontCollectionIndex` is reported as a degraded attribute
/// rather than defaulted to `minor`: guessing the body collection would give a
/// shape's text a typeface and look deliberate.
fn read_font_reference(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    part: &str,
    resolver: Resolver,
    slot: &mut Option<FontReference>,
) -> Result<bool, ImportError> {
    let index = match attribute(element, b"idx", cursor.part())?
        .as_deref()
        .map(FontCollectionIndex::from_token)
    {
        Some(Some(index)) => index,
        // Absent: `ST_FontCollectionIndex` has no default in the schema, and the
        // attribute is required on `a:fontRef`, so an absent one is as invalid as
        // an unrecognised one.
        Some(None) | None => {
            reporter.degraded_attribute(part, b"fontRef", b"idx");
            FontCollectionIndex::None
        }
    };
    let mut argument: Option<StyleColor> = None;
    let consumed = enter(cursor, empty, |cursor, child, child_empty| {
        read_scheme_argument(
            cursor,
            reporter,
            child,
            child_empty,
            part,
            resolver,
            &mut argument,
        )
    })?;
    let color = argument.and_then(|argument| argument.resolve(None));
    *slot = Some(FontReference { index, color });
    Ok(consumed)
}

/// Reports every `a:fontRef` that resolves and still goes unpainted.
///
/// Run after the parts are read, beside `report_unpaintable_style_refs`, and for
/// the same reason: the finding needs BOTH the reference and the theme's font
/// scheme, because "the shape asked for a typeface" and "the theme names one" are
/// different facts and only their conjunction is a loss.
///
/// # What is and is not a loss
///
/// `idx="none"` with no colour child is **not** a loss: the shape is saying its
/// text takes no theme typeface and no theme colour, and that is honoured exactly
/// — the same rule as an `@idx` of `0` on the other three references.
///
/// Everything else is, because nothing in this build applies a shape-scoped text
/// default: a slide run resolves its typeface through the placeholder cascade and
/// the theme's font scheme, never through the shape's own `p:style`. The three
/// reasons are the DOCX reader's own, so one DrawingML element has one vocabulary
/// across both document classes and a host need not learn two names for one loss:
///
/// * `typeface-and-colour-not-applied` — a resolvable collection AND a colour.
/// * `typeface-not-applied` — a resolvable collection, no colour.
/// * `colour-not-applied` — a colour, and a collection resolving to nothing.
///
/// # Complexity
///
/// O(styled shapes) — one O(1) resolution per reference, no scan of the deck.
pub(crate) fn report_unapplied_font_refs(
    font_scheme: Option<&FontScheme>,
    shape_styles: &casual_doc_model::v1::DefinitionMap<casual_doc_model::NodeId, ShapeStyleRef>,
    reporter: &mut Reporter,
) {
    for (_, reference) in shape_styles.iter() {
        let Some(font) = reference.font_ref else {
            continue;
        };
        // The Latin axis, because that is the axis a slide run resolves on unless
        // it states otherwise, and the entry falls back to Latin anyway.
        let typeface = font_scheme
            .and_then(|scheme| font.typeface(scheme, ThemeFontAxis::Latin))
            .is_some();
        let reason = match (typeface, font.color.is_some()) {
            (true, true) => "typeface-and-colour-not-applied",
            (true, false) => "typeface-not-applied",
            (false, true) => "colour-not-applied",
            // `idx="none"` with no colour: the shape asked for nothing.
            (false, false) => continue,
        };
        reporter.shape_appearance_unpainted("fontRef", reason);
    }
}

/// Reads the colour a `a:fillRef`/`a:lnRef`/`a:effectRef` supplies as its `a:phClr`
/// argument, resolved against the theme.
fn read_scheme_argument(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    part: &str,
    resolver: Resolver,
    argument: &mut Option<StyleColor>,
) -> Result<bool, ImportError> {
    let local = local_name(element);
    match local {
        // An `a:schemeClr` here is the shape's own argument, so it DOES resolve
        // through the part's colour map — unlike one inside the theme's own style
        // matrix, which has no single map to resolve through. That is why this is
        // not `read_style_color`.
        b"schemeClr" => {
            let value = attribute(element, b"val", cursor.part())?.unwrap_or_default();
            let transform = read_transform_if_any(cursor, reporter, part, empty)?;
            match resolver.scheme_color(&value, transform) {
                SchemeOutcome::Resolved(color) => *argument = Some(StyleColor::Fixed(color)),
                // A reference whose argument is itself `a:phClr` supplies no colour;
                // `StyleColor::resolve(None)` is what turns that into "no answer".
                SchemeOutcome::Placeholder => {
                    *argument = Some(StyleColor::Placeholder(transform));
                }
                SchemeOutcome::Unresolved => reporter.degraded(part, b"schemeClr"),
            }
            Ok(!empty)
        }
        // A fixed colour, a system colour, or something else: the shared reader
        // already distinguishes all three and folds the transform the same way.
        _ => read_style_color(cursor, reporter, element, empty, part, argument),
    }
}

/// Classifies every `p:style` reference against the style matrix it names, so a
/// reported loss and an unpainted shape are the same event seen twice.
///
/// Run after the parts are read, because it needs BOTH halves: the matrix (which
/// says what the entry is) and the side table (which says which entries are
/// actually asked for). Reporting from the theme alone would raise a finding for
/// every Office theme, whose third effect style carries an `a:outerShdw` no shape
/// in the deck need reference.
///
/// An `@idx` of `0` is NOT a loss: it means "no fill"/"no outline"/"no effect", and
/// that is honoured exactly.
///
/// # Complexity
///
/// O(styled shapes) — one index per reference, no scan of the deck.
pub(crate) fn report_unpaintable_style_refs(
    scheme: &FormatScheme,
    shape_styles: &casual_doc_model::v1::DefinitionMap<casual_doc_model::NodeId, ShapeStyleRef>,
    reporter: &mut Reporter,
) {
    for (_, reference) in shape_styles.iter() {
        if let Some(idx) = reference.fill_idx.filter(|idx| *idx != 0) {
            match scheme.fill_style(idx) {
                Some(FillStyle::Solid { .. } | FillStyle::Gradient(_)) => {}
                // Modelled, and painted by nothing: the report can say *pattern*
                // rather than *unknown*, which is a finding a caller can act on.
                Some(FillStyle::Pattern(_)) => {
                    reporter.theme_style_unpainted("fillStyleLst", "pattern");
                }
                None => reporter.theme_style_unpainted("fillStyleLst", "unmodeled"),
            }
        }
        if let Some(idx) = reference.line_idx.filter(|idx| *idx != 0)
            && scheme.line_style(idx).is_none()
        {
            reporter.theme_style_unpainted("lnStyleLst", "unmodeled");
        }
        if let Some(idx) = reference.effect_idx.filter(|idx| *idx != 0)
            && scheme
                .effect_style(idx)
                .is_some_and(|style| style.carries_effects)
        {
            reporter.theme_style_unpainted("effectStyleLst", "effect-not-rendered");
        }
    }
}

/// Reads the colour map a part states, having just read its root element.
///
/// One reader for `p:sldMaster/p:clrMap` and for
/// `p:sldLayout`/`p:sld`'s `p:clrMapOvr/a:overrideClrMapping`, because the mapping
/// element is the same `CT_ColorMapping` in all three and its position is what
/// distinguishes them — no per-part-kind branch.
///
/// # Why this is a pass of its own
///
/// `p:clrMap` is a SIBLING of `p:cSld` and the schema puts it AFTER it, so the map
/// is not known until the shape tree has already been read — and the shape tree is
/// exactly what needs it, because every `a:schemeClr` in it resolves through the
/// map. One targeted pass up front is the only ordering that works without
/// deferring every colour in the part, and `v1::Fill` holds a concrete `Rgba` with
/// nowhere to defer one to.
///
/// `None` means the part states no mapping of its own and inherits: a layout or
/// slide whose `p:clrMapOvr` is `<a:masterClrMapping/>`, which is the common case
/// and must stay distinguishable from an override that happens to equal the
/// master's.
///
/// # Complexity
///
/// O(part): one extra pass per part, on open only. Never on an interaction path.
pub(crate) fn read_color_map_of(
    bytes: &[u8],
    part: &str,
    reporter: &mut Reporter,
    limits: ImportLimits,
) -> Result<Option<ColorMap>, ImportError> {
    let mut cursor = Cursor::new(bytes, part, limits);
    cursor.root()?;
    let mut map: Option<ColorMap> = None;
    children(&mut cursor, |cursor, element, empty| {
        match local_name(element) {
            b"clrMap" => {
                map = Some(read_color_mapping(element, cursor.part(), reporter)?);
                Ok(false)
            }
            b"clrMapOvr" => enter(cursor, empty, |cursor, child, _child_empty| {
                if local_name(child) == b"overrideClrMapping" {
                    map = Some(read_color_mapping(child, cursor.part(), reporter)?);
                }
                Ok(false)
            }),
            _ => Ok(false),
        }
    })?;
    Ok(map)
}

/// Reads one `CT_ColorMapping`'s twelve attributes.
///
/// All twelve are required by the schema. A missing or unrecognised one falls back
/// to the identity binding for that role and is reported, so the map is never
/// half-applied — a partly-applied map is indistinguishable from a correct one
/// until the deck renders in the wrong colours.
fn read_color_mapping(
    element: &BytesStart<'_>,
    part: &str,
    reporter: &mut Reporter,
) -> Result<ColorMap, ImportError> {
    let mut map = ColorMap::IDENTITY;
    for role in ColorRole::ALL {
        let attribute_name = role.attribute();
        let token = attribute(element, attribute_name.as_bytes(), part)?;
        match token.as_deref().and_then(ThemeColorSlot::from_token) {
            Some(slot) => *map.slot_mut(role) = slot,
            None => {
                reporter.degraded_attribute(part, local_name(element), attribute_name.as_bytes())
            }
        }
    }
    Ok(map)
}

/// Reports `element`'s dropped `@name`, when it carries a non-empty one the model
/// has no field for.
///
/// Propagates the attribute error rather than treating an unreadable `@name` as an
/// absent one: this is the loss REPORT, and a finding silently skipped because its
/// own diagnostic failed is the shape of bug the report exists to prevent.
fn report_dropped_name(
    element: &BytesStart<'_>,
    local: &[u8],
    part: &str,
    reporter: &mut Reporter,
) -> Result<(), ImportError> {
    if attribute(element, b"name", part)?.is_some_and(|value| !value.is_empty()) {
        reporter.degraded_attribute(part, local, b"name");
    }
    Ok(())
}

/// An attribute kept only within the model's own bound.
fn bounded(
    element: &BytesStart<'_>,
    name: &[u8],
    part: &str,
) -> Result<Option<String>, ImportError> {
    Ok(attribute(element, name, part)?
        .filter(|value| !value.is_empty() && value.len() <= MAX_THEME_TEXT))
}

/// Parses a six-hex-digit `ST_HexColorRGB`.
fn parse_rgb(value: Option<&str>) -> Option<casual_doc_model::v1::RgbColor> {
    let value = value?.trim();
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(value.get(range)?, 16).ok();
    Some(casual_doc_model::v1::RgbColor {
        r: channel(0..2)?,
        g: channel(2..4)?,
        b: channel(4..6)?,
    })
}

/// Parses an `ST_Percentage`: thousandths of a percent (`40000` is 40%), or the
/// `"40%"` spelling `ST_Percentage`'s union also admits.
///
/// Returned in the per-100000 units the model holds, NOT as a fraction — the one
/// conversion is in `ColorTransform::apply`, and doing it twice is how a tint
/// becomes a thousandth of itself.
pub(crate) fn parse_percentage(value: &str) -> Option<i32> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        let percent: f64 = percent.trim().parse().ok()?;
        let scaled = (percent * 1000.0).round();
        if !scaled.is_finite() || scaled.abs() > f64::from(i32::MAX) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "bounded against i32::MAX on the line above"
        )]
        return Some(scaled as i32);
    }
    value.parse().ok()
}
