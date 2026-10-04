// SPDX-License-Identifier: Apache-2.0

//! `a:txBody` into [`TextBody`]: body properties, the nine-level list style, and
//! paragraphs with their runs, breaks and fields.
//!
//! # The unit trap this module exists to not fall into
//!
//! `w:sz` is in **half-points**; `a:rPr@sz` is in **hundredths of a point**. A
//! value carried from one to the other without conversion is wrong by a factor
//! of fifty and still looks like a plausible font size, so it passes every bounds
//! check and renders a 12pt deck at 600pt. There is no conversion in this file —
//! `a:rPr@sz` goes into `size_hundredths_point` verbatim, and the field is named
//! for its unit precisely so that a future edit cannot quietly reuse a
//! half-point value. The same discipline holds for the other four unit families
//! a slide mixes: offsets and extents are EMU, rotations are 1/60000 degree,
//! percentages are 1/1000 of a percent, and letter spacing is hundredths of a
//! point.
//!
//! # Levels are zero-based here and one-based in the element names
//!
//! `a:pPr@lvl` is `0..=8`. The list-style elements are `a:lvl1pPr` … `a:lvl9pPr`.
//! The two forms never share a variable in this file, which is the only reliable
//! defence against an off-by-one that shifts every indent in a deck by one level.

use casual_doc_model::v1::StyleColor;
use casual_pres_model::{
    AutoNumberScheme, ListStyle, TEXT_LEVELS, TextAlign, TextAnchor, TextAutoFit, TextBody,
    TextBodyProperties, TextBullet, TextCaps, TextCharacterProperties, TextField, TextLineBreak,
    TextParagraph, TextParagraphProperties, TextRun, TextRunText, TextSpacing, TextStrike,
    TextTabStop, TextUnderline, TextVertical, TextWrap, Typeface,
};
use quick_xml::events::BytesStart;

use crate::ImportError;
use crate::color::read_solid_fill;
use crate::ids::Ids;
use crate::loss::Reporter;
use crate::xml::{
    Cursor, attribute, boolean_attribute, children, enter, integer_attribute, local_name,
};

/// Reads an `a:txBody`, having just entered it.
///
/// # Complexity
///
/// O(text in the body), one pass, bounded by `ImportLimits`.
pub(crate) fn read_text_body(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
) -> Result<TextBody, ImportError> {
    let part = cursor.part().to_owned();
    let limits = cursor.limits();
    let mut body_properties = TextBodyProperties::default();
    let mut list_style = ListStyle::default();
    let mut paragraphs: Vec<TextParagraph> = Vec::new();

    children(cursor, |cursor, element, empty| match local_name(element) {
        b"bodyPr" => {
            body_properties = read_body_properties(cursor, reporter, element, empty)?;
            Ok(!empty)
        }
        b"lstStyle" => {
            if empty {
                return Ok(false);
            }
            list_style = read_list_style(cursor, reporter, ids)?;
            Ok(true)
        }
        b"p" => {
            if paragraphs.len() >= limits.max_paragraphs_per_body {
                reporter.invalid(&part, b"p");
                return Ok(false);
            }
            let paragraph = if empty {
                TextParagraph::empty(ids.next()?)
            } else {
                read_paragraph(cursor, reporter, ids)?
            };
            paragraphs.push(paragraph);
            Ok(!empty)
        }
        other => {
            reporter.omitted(&part, other);
            Ok(false)
        }
    })?;

    // `TextBody::validate` refuses zero paragraphs, and PowerPoint writes an
    // empty `a:p` rather than omitting it — so a body with none is malformed
    // source. One empty paragraph is substituted and the fact is reported, rather
    // than failing the whole deck for a construct whose repair is unambiguous.
    if paragraphs.is_empty() {
        reporter.invalid(&part, b"txBody");
        paragraphs.push(TextParagraph::empty(ids.next()?));
    }

    Ok(TextBody {
        body_properties,
        list_style,
        paragraphs,
    })
}

/// Reads `a:bodyPr`'s attributes and its autofit child.
///
/// The insets default to DrawingML's own asymmetric values rather than zero,
/// because an absent attribute means *the default* and not "no inset" — 0.1 inch
/// left/right, 0.05 inch top/bottom. `TextBodyProperties::default` already
/// carries them, so an absent attribute is simply left alone.
fn read_body_properties(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
) -> Result<TextBodyProperties, ImportError> {
    let part = cursor.part().to_owned();
    let mut properties = TextBodyProperties::default();
    if let Some(value) = integer_attribute(element, b"lIns", &part)? {
        properties.inset_left_emu = value;
    }
    if let Some(value) = integer_attribute(element, b"tIns", &part)? {
        properties.inset_top_emu = value;
    }
    if let Some(value) = integer_attribute(element, b"rIns", &part)? {
        properties.inset_right_emu = value;
    }
    if let Some(value) = integer_attribute(element, b"bIns", &part)? {
        properties.inset_bottom_emu = value;
    }
    if let Some(token) = attribute(element, b"anchor", &part)? {
        properties.anchor = TextAnchor::from_token(&token);
    }
    if let Some(value) = boolean_attribute(element, b"anchorCtr", &part)? {
        properties.anchor_center = value;
    }
    if let Some(token) = attribute(element, b"wrap", &part)? {
        properties.wrap = TextWrap::from_token(&token);
    }
    if let Some(token) = attribute(element, b"vert", &part)? {
        properties.vertical = TextVertical::from_token(&token);
    }
    // `a:bodyPr@rot` is the TEXT's own rotation in 1/60000 degree, separate from
    // the shape's `a:xfrm@rot`. Both apply, so neither may be folded into the
    // other at import.
    properties.rotation =
        integer_attribute(element, b"rot", &part)?.and_then(|value| i32::try_from(value).ok());
    properties.column_count =
        integer_attribute(element, b"numCol", &part)?.and_then(|value| u16::try_from(value).ok());
    properties.column_space_emu = integer_attribute(element, b"spcCol", &part)?;

    if empty {
        return Ok(properties);
    }
    children(cursor, |cursor, child, child_empty| {
        let local = local_name(child);
        match local {
            b"noAutofit" => {
                properties.auto_fit = TextAutoFit::None;
                Ok(false)
            }
            b"spAutoFit" => {
                properties.auto_fit = TextAutoFit::Shape;
                Ok(false)
            }
            b"normAutofit" => {
                // The producer SOLVED the fit and recorded the answer. Honouring
                // it verbatim is what makes an untouched file render identically;
                // re-solving at open would replace PowerPoint's own numbers with
                // ours and change the appearance of a file nobody edited. The
                // units are thousandths of a percent, not a fraction.
                properties.auto_fit = TextAutoFit::Normal {
                    font_scale: integer_attribute(child, b"fontScale", cursor.part())?
                        .and_then(|value| u32::try_from(value).ok()),
                    line_space_reduction: integer_attribute(
                        child,
                        b"lnSpcReduction",
                        cursor.part(),
                    )?
                    .and_then(|value| u32::try_from(value).ok()),
                };
                Ok(false)
            }
            b"prstTxWarp" | b"scene3d" | b"sp3d" | b"flatTx" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"extLst" => Ok(false),
            _ => {
                let _ = child_empty;
                Ok(false)
            }
        }
    })?;
    Ok(properties)
}

/// Reads `a:lstStyle`'s `a:lvl1pPr` … `a:lvl9pPr` into the zero-based level
/// vector.
///
/// A level beyond nine is refused rather than stored: `ListStyle::validate`
/// bounds the vector at [`TEXT_LEVELS`], and a tenth level is markup no
/// conforming producer writes.
///
/// Shared with the master's `p:txStyles` tiers, which are `CT_TextListStyle` too —
/// the element name differs and the content model does not, so reading them through
/// a second function would be two readers of one grammar.
pub(crate) fn read_list_style(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
) -> Result<ListStyle, ImportError> {
    let part = cursor.part().to_owned();
    let mut levels: Vec<Option<TextParagraphProperties>> = vec![None; TEXT_LEVELS];
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        let Some(level) = list_style_level(local) else {
            reporter.omitted(&part, local);
            return Ok(false);
        };
        let properties = if empty {
            TextParagraphProperties::default()
        } else {
            read_paragraph_properties(cursor, reporter, ids, element)?
        };
        if let Some(slot) = levels.get_mut(usize::from(level)) {
            *slot = Some(properties);
        }
        Ok(!empty)
    })?;
    // Trailing `None`s carry no information and make `ListStyle::is_empty` false
    // for a style that declares nothing, so the vector is trimmed to its last
    // populated level.
    while levels.last().is_some_and(Option::is_none) {
        levels.pop();
    }
    Ok(ListStyle { levels })
}

/// Maps `lvlNpPr` to its ZERO-based level. `a:lvl1pPr` is level `0`.
fn list_style_level(local: &[u8]) -> Option<u8> {
    let rest = local.strip_prefix(b"lvl")?;
    let digits = rest.strip_suffix(b"pPr")?;
    let [digit] = digits else { return None };
    let one_based = digit.checked_sub(b'0')?;
    if !(1..=9).contains(&one_based) {
        return None;
    }
    Some(one_based - 1)
}

/// Reads one `a:p`, having just entered it.
fn read_paragraph(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
) -> Result<TextParagraph, ImportError> {
    let part = cursor.part().to_owned();
    let limits = cursor.limits();
    let id = ids.next()?;
    let mut properties: Option<Box<TextParagraphProperties>> = None;
    let mut runs: Vec<TextRun> = Vec::new();
    let mut end_properties: Option<Box<TextCharacterProperties>> = None;

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"pPr" => {
                let read = if empty {
                    TextParagraphProperties::default()
                } else {
                    read_paragraph_properties(cursor, reporter, ids, element)?
                };
                if !read.is_empty() {
                    properties = Some(Box::new(read));
                }
                Ok(!empty)
            }
            b"r" | b"br" | b"fld" => {
                if runs.len() >= limits.max_runs_per_paragraph {
                    reporter.invalid(&part, local);
                    return Ok(false);
                }
                let consumed = read_run(cursor, reporter, ids, element, empty, local, &mut runs)?;
                Ok(consumed)
            }
            b"endParaRPr" => {
                let read = read_character_properties(cursor, reporter, element, empty)?;
                if !read.is_empty() {
                    end_properties = Some(Box::new(read));
                }
                Ok(!empty)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    Ok(TextParagraph {
        id,
        properties,
        runs,
        end_properties,
    })
}

/// Reads one paragraph child into `runs`, returning whether its subtree was
/// consumed.
fn read_run(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    element: &BytesStart<'_>,
    empty: bool,
    local: &[u8],
    runs: &mut Vec<TextRun>,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    let limits = cursor.limits();
    match local {
        b"br" => {
            // A break carries its own `a:rPr`, which sets the HEIGHT of the blank
            // line it introduces — so dropping it would collapse a deliberate
            // gap.
            let id = ids.next()?;
            let mut properties: Option<Box<TextCharacterProperties>> = None;
            if !empty {
                children(cursor, |cursor, child, child_empty| {
                    if local_name(child) == b"rPr" {
                        let read = read_character_properties(cursor, reporter, child, child_empty)?;
                        if !read.is_empty() {
                            properties = Some(Box::new(read));
                        }
                        return Ok(!child_empty);
                    }
                    Ok(false)
                })?;
            }
            runs.push(TextRun::LineBreak(TextLineBreak { id, properties }));
            Ok(!empty)
        }
        b"r" | b"fld" => {
            let id = ids.next()?;
            let field_id = attribute(element, b"id", &part)?;
            let kind = attribute(element, b"type", &part)?;
            let mut properties: Option<Box<TextCharacterProperties>> = None;
            let mut text = String::new();
            if !empty {
                children(cursor, |cursor, child, child_empty| {
                    match local_name(child) {
                        b"rPr" => {
                            let read =
                                read_character_properties(cursor, reporter, child, child_empty)?;
                            if !read.is_empty() {
                                properties = Some(Box::new(read));
                            }
                            Ok(!child_empty)
                        }
                        b"t" => {
                            if child_empty {
                                return Ok(false);
                            }
                            text = cursor.read_text(limits.max_run_bytes)?;
                            Ok(true)
                        }
                        b"pPr" => {
                            // `a:fld` may carry an `a:pPr`, which applies to the
                            // paragraph and is already read from the paragraph's
                            // own child. Reading it twice would be wrong and
                            // ignoring it silently would hide an override.
                            reporter.omitted(&part, b"fld/pPr");
                            Ok(false)
                        }
                        other => {
                            reporter.omitted(&part, other);
                            Ok(false)
                        }
                    }
                })?;
            }
            if local == b"fld" {
                // A field keeps BOTH its kind and its cached `a:t`. The cache is
                // what any reader that cannot evaluate the field displays, so
                // discarding it blanks every slide number in a deck opened by
                // anything but PowerPoint. The kind stays the authored token
                // rather than an enum: the datetime family alone has thirteen
                // locale-dependent members, and an unknown token must still
                // round-trip.
                runs.push(TextRun::Field(TextField {
                    id,
                    field_id: field_id.unwrap_or_default(),
                    kind: kind.unwrap_or_default(),
                    properties,
                    text,
                }));
            } else if text.is_empty() {
                // `TextParagraph::validate` refuses an empty `a:t`: a run with no
                // text carries no content and no position. Reported and dropped
                // rather than refusing the deck.
                reporter.invalid(&part, b"r");
            } else {
                runs.push(TextRun::Run(TextRunText {
                    id,
                    properties,
                    text,
                }));
            }
            Ok(!empty)
        }
        _ => Ok(false),
    }
}

/// Reads `a:pPr` (or one `a:lvlNpPr`) and its children.
fn read_paragraph_properties(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    element: &BytesStart<'_>,
) -> Result<TextParagraphProperties, ImportError> {
    let part = cursor.part().to_owned();
    let mut properties = TextParagraphProperties {
        // `a:pPr@lvl` is the single most load-bearing attribute on a slide
        // paragraph: it selects the `a:lstStyle` level AND the master's
        // `p:txStyles` tier, so getting it wrong misformats every bullet below
        // the first.
        level: integer_attribute(element, b"lvl", &part)?
            .and_then(|value| u8::try_from(value).ok()),
        alignment: attribute(element, b"algn", &part)?
            .as_deref()
            .map(TextAlign::from_token),
        margin_left_emu: integer_attribute(element, b"marL", &part)?,
        margin_right_emu: integer_attribute(element, b"marR", &part)?,
        // `a:pPr@indent` is the FIRST-LINE offset relative to `marL`, and it is
        // negative for the hanging indent every bulleted paragraph uses — so an
        // unsigned read would lose every bullet's hang.
        indent_emu: integer_attribute(element, b"indent", &part)?,
        default_tab_emu: integer_attribute(element, b"defTabSz", &part)?,
        right_to_left: boolean_attribute(element, b"rtl", &part)?,
        ..TextParagraphProperties::default()
    };

    children(cursor, |cursor, child, empty| {
        let local = local_name(child);
        match local {
            b"lnSpc" => {
                properties.line_spacing = read_spacing(cursor, reporter, empty)?;
                Ok(!empty)
            }
            b"spcBef" => {
                properties.space_before = read_spacing(cursor, reporter, empty)?;
                Ok(!empty)
            }
            b"spcAft" => {
                properties.space_after = read_spacing(cursor, reporter, empty)?;
                Ok(!empty)
            }
            // The three bullet forms are mutually exclusive in the schema. A
            // `a:buNone` is NOT the same as an absent bullet: absent inherits,
            // and `a:buNone` suppresses an inherited bullet. Collapsing them
            // would grow a bullet on a deliberately unbulleted paragraph inside a
            // bulleted body placeholder.
            b"buNone" => {
                properties.bullet = Some(TextBullet::None);
                Ok(false)
            }
            b"buChar" => {
                properties.bullet = Some(TextBullet::Character {
                    character: attribute(child, b"char", cursor.part())?.unwrap_or_default(),
                    // The font is carried by a sibling `a:buFont`, read below and
                    // folded in afterwards, because element order is not
                    // guaranteed.
                    font: None,
                });
                Ok(false)
            }
            b"buAutoNum" => {
                properties.bullet = Some(TextBullet::AutoNumber {
                    scheme: attribute(child, b"type", cursor.part())?
                        .as_deref()
                        .map_or(AutoNumberScheme::default(), AutoNumberScheme::from_token),
                    start_at: integer_attribute(child, b"startAt", cursor.part())?
                        .and_then(|value| u32::try_from(value).ok()),
                });
                Ok(false)
            }
            b"buFont" => {
                properties.bullet_font = read_typeface(child, cursor.part())?;
                Ok(false)
            }
            b"buClr" => {
                if empty {
                    return Ok(false);
                }
                properties.bullet_color = read_solid_fill(cursor, reporter)?.style_color();
                Ok(true)
            }
            b"buSzPct" => {
                properties.bullet_size_percent = integer_attribute(child, b"val", cursor.part())?
                    .and_then(|value| u32::try_from(value).ok());
                Ok(false)
            }
            b"buSzPts" => {
                // `TextParagraphProperties` carries only the percentage form, so
                // an absolute bullet size has nowhere to go.
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"buClrTx" | b"buSzTx" | b"buFontTx" => {
                // "Follow the text" is the DrawingML default and is what `None`
                // on each of the three fields already means, so these carry no
                // unrecovered meaning and raise nothing — a report that fires on
                // healthy markup is one callers learn to ignore.
                Ok(false)
            }
            b"tabLst" => {
                if empty {
                    return Ok(false);
                }
                properties.tab_stops = read_tab_stops(cursor, reporter)?;
                Ok(true)
            }
            b"defRPr" => {
                let read = read_character_properties(cursor, reporter, child, empty)?;
                if !read.is_empty() {
                    properties.default_character = Some(Box::new(read));
                }
                Ok(!empty)
            }
            b"lnSpcReduction" | b"extLst" => Ok(false),
            other => {
                let _ = ids;
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    // `a:buFont` is a sibling of `a:buChar`, so the font is attached after both
    // have been seen. Order-independent by construction rather than by relying on
    // a producer writing them in schema order.
    if let (Some(TextBullet::Character { character, font }), Some(bullet_font)) =
        (properties.bullet.clone(), properties.bullet_font.clone())
        && font.is_none()
    {
        properties.bullet = Some(TextBullet::Character {
            character,
            font: Some(bullet_font),
        });
    }
    Ok(properties)
}

/// Reads an `a:lnSpc`/`a:spcBef`/`a:spcAft` wrapper's single child.
///
/// The two forms are mutually exclusive in the schema, which is why the model is
/// an enum and not two optional fields: a type that can hold both can hold a
/// state no file can express.
fn read_spacing(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    empty: bool,
) -> Result<Option<TextSpacing>, ImportError> {
    if empty {
        return Ok(None);
    }
    let part = cursor.part().to_owned();
    let mut spacing = None;
    children(cursor, |cursor, child, _child_empty| {
        match local_name(child) {
            b"spcPct" => {
                spacing = integer_attribute(child, b"val", cursor.part())?
                    .and_then(|value| u32::try_from(value).ok())
                    .map(|thousandths| TextSpacing::Percent { thousandths });
            }
            b"spcPts" => {
                spacing = integer_attribute(child, b"val", cursor.part())?
                    .and_then(|value| u32::try_from(value).ok())
                    .map(|hundredths| TextSpacing::Points { hundredths });
            }
            other => reporter.omitted(&part, other),
        }
        Ok(false)
    })?;
    Ok(spacing)
}

fn read_tab_stops(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
) -> Result<Vec<TextTabStop>, ImportError> {
    let part = cursor.part().to_owned();
    let mut stops = Vec::new();
    children(cursor, |cursor, child, _empty| {
        if local_name(child) != b"tab" {
            reporter.omitted(&part, local_name(child));
            return Ok(false);
        }
        if let Some(position_emu) = integer_attribute(child, b"pos", cursor.part())? {
            stops.push(TextTabStop {
                position_emu,
                alignment: attribute(child, b"algn", cursor.part())?
                    .as_deref()
                    .map_or(TextAlign::Left, TextAlign::from_token),
            });
        }
        Ok(false)
    })?;
    Ok(stops)
}

/// Reads `a:rPr` / `a:defRPr` / `a:endParaRPr`: its attributes always, and its
/// children when it has any.
///
/// # Why `empty` is a parameter rather than the caller's business
///
/// It was the caller's business, and all four call sites got it wrong the same
/// way: each wrote `if empty { default() } else { read(..) }`, which discards the
/// **attributes** of a self-closing element. `<a:rPr lang="en-US" sz="2000" b="1"/>`
/// is the commonest run-properties form PowerPoint writes, and `<a:defRPr sz="2800"/>`
/// is what nearly every `p:txStyles` level carries — so every stated size, weight
/// and typeface on a childless element was being dropped, which is most of them.
/// The guard was there because this function used to call `children`
/// unconditionally, and doing that on a self-closing element consumes the following
/// sibling's events.
///
/// Taking `empty` here and entering through [`enter`] makes the guarded form the
/// only form, which is the same fix and the same reasoning `enter`'s own
/// documentation records for `a:avLst`.
///
/// Every field stays `Option`, and that is not tidiness: on a slide an unset
/// property **inherits** through the placeholder cascade (shape, then layout, then
/// master's `p:txStyles`, then `p:defaultTextStyle`), so collapsing "unset" into
/// "the default value" freezes inherited text at the wrong tier. It is the single
/// easiest way to make a whole deck render in the wrong font.
fn read_character_properties(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
) -> Result<TextCharacterProperties, ImportError> {
    let part = cursor.part().to_owned();
    let mut properties = TextCharacterProperties {
        // HUNDREDTHS of a point, verbatim. `w:sz`'s half-points are a different
        // unit on a different element and there is no conversion here.
        size_hundredths_point: integer_attribute(element, b"sz", &part)?
            .and_then(|value| u32::try_from(value).ok()),
        bold: boolean_attribute(element, b"b", &part)?,
        italic: boolean_attribute(element, b"i", &part)?,
        underline: attribute(element, b"u", &part)?
            .as_deref()
            .map(TextUnderline::from_token),
        strike: attribute(element, b"strike", &part)?
            .as_deref()
            .map(TextStrike::from_token),
        caps: attribute(element, b"cap", &part)?
            .as_deref()
            .map(TextCaps::from_token),
        spacing_hundredths_point: integer_attribute(element, b"spc", &part)?
            .and_then(|value| i32::try_from(value).ok()),
        // Thousandths of a percent of the font size, and a continuous value
        // rather than a super/sub boolean pair: Word's two scripts are two points
        // on this axis, and DrawingML expresses the whole axis.
        baseline_percent: integer_attribute(element, b"baseline", &part)?
            .and_then(|value| i32::try_from(value).ok()),
        language: attribute(element, b"lang", &part)?,
        latin: None,
        east_asian: None,
        complex_script: None,
        fill: None,
        dirty: boolean_attribute(element, b"dirty", &part)?.unwrap_or(false),
    };

    enter(cursor, empty, |cursor, child, empty| {
        let local = local_name(child);
        match local {
            b"latin" => {
                properties.latin = read_typeface(child, cursor.part())?;
                Ok(false)
            }
            b"ea" => {
                properties.east_asian = read_typeface(child, cursor.part())?;
                Ok(false)
            }
            b"cs" => {
                properties.complex_script = read_typeface(child, cursor.part())?;
                Ok(false)
            }
            b"sym" => {
                // A symbol font applies to the characters it covers and has no
                // field; reporting it is the difference between a Wingdings run
                // rendering as letters and a caller knowing why.
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"solidFill" => {
                if empty {
                    reporter.invalid(&part, b"solidFill");
                    return Ok(false);
                }
                // A run's colour is a `StyleColor`, not an `Rgba`: `a:phClr` is a
                // formal parameter, and resolving it here would give every styled
                // run the same colour.
                properties.fill = read_solid_fill(cursor, reporter)?.style_color();
                Ok(true)
            }
            b"noFill" | b"gradFill" | b"blipFill" | b"pattFill" | b"grpFill" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"ln" | b"effectLst" | b"effectDag" | b"highlight" | b"uFill" | b"uFillTx" | b"uLn"
            | b"uLnTx" | b"rtl" => {
                // `<a:effectLst/>` self-closed is "no effects" and carries no
                // lost meaning; populated, it is a lost shadow or glow. Only the
                // element can tell them apart.
                if !empty {
                    reporter.omitted(&part, local);
                }
                Ok(false)
            }
            b"hlinkClick" | b"hlinkMouseOver" => {
                // A hyperlink on a text run has no field on
                // `TextCharacterProperties`. On a *shape* it does
                // (`GroupShape::hyperlink`), so this is a genuine
                // presentation-side gap and not a universal one.
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    let _: Option<&StyleColor> = properties.fill.as_ref();
    Ok(properties)
}

/// Reads an `a:latin`/`a:ea`/`a:cs`/`a:buFont` typeface reference.
///
/// A `+mj-lt`/`+mn-lt` name is kept **verbatim**: it names the theme's
/// major/minor font, and resolving it at import would make the model state
/// something the file does not. Resolution belongs at layout, where the shape
/// style matrix is resolved.
fn read_typeface(element: &BytesStart<'_>, part: &str) -> Result<Option<Typeface>, ImportError> {
    let Some(name) = attribute(element, b"typeface", part)? else {
        return Ok(None);
    };
    if name.is_empty() {
        return Ok(None);
    }
    Ok(Some(Typeface {
        name,
        panose: attribute(element, b"panose", part)?,
    }))
}
