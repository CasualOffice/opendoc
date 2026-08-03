//! Attribute and property parsing shared by the body and styles parsers.
//!
//! The `apply_*` functions return `true` when the element was fully consumed
//! (mapped) and `false` when it is a property element the caller should report
//! (unknown, or present-but-out-of-domain/degraded).

use casual_doc_model::v1::{
    Alignment, BreakKind, Color, DropCapFrame, DropCapMode, EmphasisMark, FontName, FontRef,
    FrameHorizontalAlignment, FrameHorizontalAnchor, FrameVerticalAlignment, FrameVerticalAnchor,
    FrameWrap, HighlightColor, Indentation, Language, LineRule, MAX_SYMBOL_FONT_LEN,
    ParagraphProperties, RgbColor, RunFontHint, RunProperties, Spacing, StyleKind, ThemeFont,
    ThemeFontRef, VerticalAlignment, VerticalTextAlignment,
};
use quick_xml::events::BytesStart;

/// Applies a run-property element, returning whether it was fully mapped.
pub(crate) fn apply_run_property(
    properties: &mut RunProperties,
    local: &[u8],
    element: &BytesStart<'_>,
) -> bool {
    let value = attribute_value(element, b"val");
    match local {
        b"b" => properties.bold = Some(is_true(value.as_deref())),
        // Complex-script bold (`w:bCs`), same `CT_OnOff` parse as `w:b`.
        b"bCs" => properties.bold_complex = Some(is_true(value.as_deref())),
        b"i" => properties.italic = Some(is_true(value.as_deref())),
        // Complex-script italic (`w:iCs`), same `CT_OnOff` parse as `w:i`.
        b"iCs" => properties.italic_complex = Some(is_true(value.as_deref())),
        b"u" => properties.underline = Some(value.as_deref() != Some("none")),
        b"strike" => properties.strike = Some(is_true(value.as_deref())),
        b"sz" => {
            match value
                .as_deref()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|size| (1..=65_534).contains(size))
            {
                Some(size) => properties.size_half_points = Some(size),
                None => return false,
            }
        }
        // Complex-script font size (`w:szCs`), same range/parse as `w:sz`.
        b"szCs" => {
            match value
                .as_deref()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|size| (1..=65_534).contains(size))
            {
                Some(size) => properties.size_complex_half_points = Some(size),
                None => return false,
            }
        }
        b"color" => match value.as_deref().and_then(parse_rgb) {
            Some(rgb) => properties.color = Some(Color::Rgb(rgb)),
            None => return false,
        },
        // Toggle marks (`CT_OnOff`): present means on unless `val` is 0/false/off.
        b"caps" => properties.all_caps = Some(is_true(value.as_deref())),
        b"smallCaps" => properties.small_caps = Some(is_true(value.as_deref())),
        b"vanish" => properties.hidden = Some(is_true(value.as_deref())),
        b"webHidden" => properties.web_hidden = Some(is_true(value.as_deref())),
        b"dstrike" => properties.double_strike = Some(is_true(value.as_deref())),
        b"outline" => properties.outline = Some(is_true(value.as_deref())),
        b"shadow" => properties.shadow = Some(is_true(value.as_deref())),
        b"emboss" => properties.emboss = Some(is_true(value.as_deref())),
        b"imprint" => properties.imprint = Some(is_true(value.as_deref())),
        b"rtl" => properties.rtl = Some(is_true(value.as_deref())),
        b"snapToGrid" => properties.snap_to_grid = Some(is_true(value.as_deref())),
        b"specVanish" => properties.spec_vanish = Some(is_true(value.as_deref())),
        // Fonts: each slot prefers its theme attr, else its named attr; `@hint`
        // is modeled directly. Consumed when ANY slot or a recognized hint
        // resolves; an `rFonts` carrying only unmodeled detail (e.g. an unknown
        // hint) resolves nothing and is reported (no silent loss).
        b"rFonts" => {
            let ascii = font_slot(element, b"ascii", &[b"asciiTheme"]);
            let h_ansi = font_slot(element, b"hAnsi", &[b"hAnsiTheme"]);
            // `w:cstheme` is the standard spelling; `w:csTheme` is accepted as a
            // legacy fallback so documents written either way import identically.
            let cs = font_slot(element, b"cs", &[b"cstheme", b"csTheme"]);
            let east_asia = font_slot(element, b"eastAsia", &[b"eastAsiaTheme"]);
            let hint = attribute_value(element, b"hint")
                .as_deref()
                .and_then(run_font_hint);
            if ascii.is_none()
                && h_ansi.is_none()
                && cs.is_none()
                && east_asia.is_none()
                && hint.is_none()
            {
                return false;
            }
            if ascii.is_some() {
                properties.font_ref = ascii;
            }
            if h_ansi.is_some() {
                properties.font_ref_h_ansi = h_ansi;
            }
            if cs.is_some() {
                properties.font_ref_cs = cs;
            }
            if east_asia.is_some() {
                properties.font_ref_east_asia = east_asia;
            }
            if hint.is_some() {
                properties.font_hint = hint;
            }
        }
        // Named vocabularies: an unknown `val` is reported (not mapped), like `sz`.
        b"vertAlign" => match value.as_deref().and_then(vertical_alignment_from) {
            Some(alignment) => properties.vertical_alignment = Some(alignment),
            None => return false,
        },
        b"highlight" => match value.as_deref().and_then(highlight_from) {
            Some(highlight) => properties.highlight = Some(highlight),
            None => return false,
        },
        b"em" => match value.as_deref().and_then(emphasis_from) {
            Some(emphasis) => properties.emphasis = Some(emphasis),
            None => return false,
        },
        // Typographic metrics: an out-of-range value is reported, like `sz`.
        b"spacing" => match value.as_deref().and_then(|v| v.parse::<i32>().ok()) {
            Some(v) if (-31_680..=31_680).contains(&v) => {
                properties.character_spacing_twips = Some(v)
            }
            _ => return false,
        },
        b"w" => match value.as_deref().and_then(|v| v.parse::<u16>().ok()) {
            Some(v) if (1..=600).contains(&v) => properties.character_scale_percent = Some(v),
            _ => return false,
        },
        b"kern" => match value.as_deref().and_then(|v| v.parse::<u32>().ok()) {
            Some(v) if v <= 65_534 => properties.kerning_half_points = Some(v),
            _ => return false,
        },
        b"position" => match value.as_deref().and_then(|v| v.parse::<i32>().ok()) {
            Some(v) if (-31_680..=31_680).contains(&v) => properties.position_half_points = Some(v),
            _ => return false,
        },
        // Language tags (`w:lang`), retained opaque + bounded. Consumed if any
        // tag resolves; an empty/oversized-only element is reported.
        b"lang" => {
            let tag = |name: &[u8]| {
                attribute_value(element, name).filter(|v| !v.is_empty() && v.len() <= 85)
            };
            let language = Language {
                value: tag(b"val"),
                east_asia: tag(b"eastAsia"),
                bidi: tag(b"bidi"),
            };
            if language.is_empty() {
                return false;
            }
            properties.language = Some(language);
        }
        _ => return false,
    }
    true
}

/// Resolves one `w:rFonts` slot: theme attr first (`major*`→Major, `minor*`→Minor),
/// else the named attr (bounded to 255 bytes). Returns `None` if neither resolves.
fn font_slot(element: &BytesStart<'_>, named: &[u8], themes: &[&[u8]]) -> Option<FontRef> {
    // A theme value IN the vocabulary wins; one OUTSIDE it (malformed/unknown)
    // falls through to the named attribute rather than swallowing the slot — so a
    // bogus theme next to a valid named family does not silently drop the family.
    // `themes` lists the accepted attribute spellings in priority order (the cs
    // slot has two: the standard `w:cstheme` and the legacy `w:csTheme`).
    for theme in themes {
        if let Some(value) = attribute_value(element, theme)
            && let Some(slot) = theme_font_ref(&value)
        {
            return Some(FontRef::Theme(ThemeFont { slot }));
        }
    }
    attribute_value(element, named)
        .filter(|name| !name.is_empty() && name.len() <= 255)
        .map(|name| FontRef::Named(FontName { name }))
}

fn theme_font_ref(value: &str) -> Option<ThemeFontRef> {
    Some(match value {
        "majorAscii" => ThemeFontRef::MajorAscii,
        "majorHAnsi" => ThemeFontRef::MajorHAnsi,
        "majorEastAsia" => ThemeFontRef::MajorEastAsia,
        "majorBidi" => ThemeFontRef::MajorBidi,
        "minorAscii" => ThemeFontRef::MinorAscii,
        "minorHAnsi" => ThemeFontRef::MinorHAnsi,
        "minorEastAsia" => ThemeFontRef::MinorEastAsia,
        "minorBidi" => ThemeFontRef::MinorBidi,
        _ => return None,
    })
}

fn run_font_hint(value: &str) -> Option<RunFontHint> {
    Some(match value {
        "default" => RunFontHint::Default,
        "eastAsia" => RunFontHint::EastAsia,
        "cs" => RunFontHint::Cs,
        _ => return None,
    })
}

fn vertical_alignment_from(value: &str) -> Option<VerticalAlignment> {
    match value {
        "baseline" => Some(VerticalAlignment::Baseline),
        "superscript" => Some(VerticalAlignment::Superscript),
        "subscript" => Some(VerticalAlignment::Subscript),
        _ => None,
    }
}

fn highlight_from(value: &str) -> Option<HighlightColor> {
    Some(match value {
        "none" => HighlightColor::None,
        "black" => HighlightColor::Black,
        "blue" => HighlightColor::Blue,
        "cyan" => HighlightColor::Cyan,
        "darkBlue" => HighlightColor::DarkBlue,
        "darkCyan" => HighlightColor::DarkCyan,
        "darkGray" => HighlightColor::DarkGray,
        "darkGreen" => HighlightColor::DarkGreen,
        "darkMagenta" => HighlightColor::DarkMagenta,
        "darkRed" => HighlightColor::DarkRed,
        "darkYellow" => HighlightColor::DarkYellow,
        "green" => HighlightColor::Green,
        "lightGray" => HighlightColor::LightGray,
        "magenta" => HighlightColor::Magenta,
        "red" => HighlightColor::Red,
        "white" => HighlightColor::White,
        "yellow" => HighlightColor::Yellow,
        _ => return None,
    })
}

fn emphasis_from(value: &str) -> Option<EmphasisMark> {
    match value {
        "none" => Some(EmphasisMark::None),
        "dot" => Some(EmphasisMark::Dot),
        "comma" => Some(EmphasisMark::Comma),
        "circle" => Some(EmphasisMark::Circle),
        "underDot" => Some(EmphasisMark::UnderDot),
        _ => None,
    }
}

/// Applies a paragraph-property element, returning whether it was fully mapped.
pub(crate) fn apply_paragraph_property(
    properties: &mut ParagraphProperties,
    local: &[u8],
    element: &BytesStart<'_>,
) -> bool {
    match local {
        b"jc" => match attribute_value(element, b"val")
            .as_deref()
            .and_then(alignment_from)
        {
            Some(alignment) => properties.alignment = Some(alignment),
            None => return false,
        },
        b"ind" => {
            let indentation = Indentation {
                start_twips: indent_attr(element, &[b"start", b"left"]),
                end_twips: indent_attr(element, &[b"end", b"right"]),
                first_line_twips: indent_attr(element, &[b"firstLine"]),
                hanging_twips: indent_attr(element, &[b"hanging"]),
            };
            if indentation == Indentation::default() {
                return false;
            }
            properties.indentation = Some(indentation);
        }
        b"spacing" => {
            let (line_percent, line_rule, line_twips) = spacing_line(element);
            let spacing = Spacing {
                before_twips: spacing_twips(element, b"before"),
                after_twips: spacing_twips(element, b"after"),
                line_percent,
                line_rule,
                line_twips,
                before_auto: spacing_auto(element, b"beforeAutospacing"),
                after_auto: spacing_auto(element, b"afterAutospacing"),
            };
            if spacing == Spacing::default() {
                return false;
            }
            properties.spacing = Some(spacing);
        }
        b"framePr" => match drop_cap_frame(element) {
            Some(frame) => properties.drop_cap_frame = Some(frame),
            None => return false,
        },
        // Toggle flags (`CT_OnOff`): present means on unless `val` is 0/false/off.
        b"keepNext" => properties.keep_next = is_true(attribute_value(element, b"val").as_deref()),
        b"keepLines" => {
            properties.keep_lines = is_true(attribute_value(element, b"val").as_deref())
        }
        b"pageBreakBefore" => {
            properties.page_break_before = is_true(attribute_value(element, b"val").as_deref())
        }
        b"widowControl" => {
            properties.widow_control = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"contextualSpacing" => {
            properties.contextual_spacing = is_true(attribute_value(element, b"val").as_deref())
        }
        b"suppressLineNumbers" => {
            properties.suppress_line_numbers = is_true(attribute_value(element, b"val").as_deref())
        }
        // Tri-state toggles: several default ON in OOXML, so an explicit off
        // (`w:val="0"`) is preserved as `Some(false)`.
        b"bidi" => properties.bidi = Some(is_true(attribute_value(element, b"val").as_deref())),
        b"wordWrap" => {
            properties.word_wrap = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"kinsoku" => {
            properties.kinsoku = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"snapToGrid" => {
            properties.snap_to_grid = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"mirrorIndents" => {
            properties.mirror_indents = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"adjustRightInd" => {
            properties.adjust_right_ind = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"suppressAutoHyphens" => {
            properties.suppress_auto_hyphens =
                Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"overflowPunct" => {
            properties.overflow_punct = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"topLinePunct" => {
            properties.top_line_punct = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"autoSpaceDE" => {
            properties.auto_space_de = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"autoSpaceDN" => {
            properties.auto_space_dn = Some(is_true(attribute_value(element, b"val").as_deref()))
        }
        b"textAlignment" => match attribute_value(element, b"val").as_deref() {
            Some("auto") => properties.text_alignment = Some(VerticalTextAlignment::Auto),
            Some("baseline") => properties.text_alignment = Some(VerticalTextAlignment::Baseline),
            Some("bottom") => properties.text_alignment = Some(VerticalTextAlignment::Bottom),
            Some("center") => properties.text_alignment = Some(VerticalTextAlignment::Center),
            Some("top") => properties.text_alignment = Some(VerticalTextAlignment::Top),
            _ => return false,
        },
        b"outlineLvl" => {
            match attribute_value(element, b"val")
                .and_then(|value| value.parse::<u8>().ok())
                .filter(|level| *level <= 9)
            {
                Some(level) => properties.outline_level = Some(level),
                None => return false,
            }
        }
        _ => return false,
    }
    true
}

fn drop_cap_frame(element: &BytesStart<'_>) -> Option<DropCapFrame> {
    let mode = match attribute_value(element, b"dropCap").as_deref()? {
        "drop" => DropCapMode::Drop,
        "margin" => DropCapMode::Margin,
        _ => return None,
    };
    let lines = match attribute_value(element, b"lines") {
        Some(value) => value.parse::<u8>().ok().filter(|lines| *lines > 0)?,
        None => 1,
    };
    let wrap = match attribute_value(element, b"wrap").as_deref() {
        Some("around") => Some(FrameWrap::Around),
        Some("notBeside") => Some(FrameWrap::NotBeside),
        Some("auto") => Some(FrameWrap::Auto),
        Some("none") => Some(FrameWrap::None),
        Some(_) => return None,
        None => None,
    };
    let horizontal_anchor = match attribute_value(element, b"hAnchor").as_deref() {
        Some("margin") => Some(FrameHorizontalAnchor::Margin),
        Some("page") => Some(FrameHorizontalAnchor::Page),
        Some("text") => Some(FrameHorizontalAnchor::Text),
        Some(_) => return None,
        None => None,
    };
    let vertical_anchor = match attribute_value(element, b"vAnchor").as_deref() {
        Some("margin") => Some(FrameVerticalAnchor::Margin),
        Some("page") => Some(FrameVerticalAnchor::Page),
        Some("text") => Some(FrameVerticalAnchor::Text),
        Some(_) => return None,
        None => None,
    };
    let horizontal_alignment = match attribute_value(element, b"xAlign").as_deref() {
        Some("center") => Some(FrameHorizontalAlignment::Center),
        Some("inside") => Some(FrameHorizontalAlignment::Inside),
        Some("left") => Some(FrameHorizontalAlignment::Left),
        Some("outside") => Some(FrameHorizontalAlignment::Outside),
        Some("right") => Some(FrameHorizontalAlignment::Right),
        Some(_) => return None,
        None => None,
    };
    let vertical_alignment = match attribute_value(element, b"yAlign").as_deref() {
        Some("bottom") => Some(FrameVerticalAlignment::Bottom),
        Some("center") => Some(FrameVerticalAlignment::Center),
        Some("inline") => Some(FrameVerticalAlignment::Inline),
        Some("inside") => Some(FrameVerticalAlignment::Inside),
        Some("outside") => Some(FrameVerticalAlignment::Outside),
        Some("top") => Some(FrameVerticalAlignment::Top),
        Some(_) => return None,
        None => None,
    };
    let signed_twips = |name: &[u8]| -> Option<Option<i32>> {
        match attribute_value(element, name) {
            Some(value) => value
                .parse::<i32>()
                .ok()
                .filter(|value| (-31_680..=31_680).contains(value))
                .map(Some),
            None => Some(None),
        }
    };
    let unsigned_twips = |name: &[u8]| -> Option<Option<u32>> {
        match attribute_value(element, name) {
            Some(value) => value
                .parse::<u32>()
                .ok()
                .filter(|value| *value <= 31_680)
                .map(Some),
            None => Some(None),
        }
    };
    Some(DropCapFrame {
        mode,
        lines,
        wrap,
        horizontal_anchor,
        vertical_anchor,
        horizontal_alignment,
        vertical_alignment,
        horizontal_position_twips: signed_twips(b"x")?,
        vertical_position_twips: signed_twips(b"y")?,
        horizontal_space_twips: unsigned_twips(b"hSpace")?,
        vertical_space_twips: unsigned_twips(b"vSpace")?,
    })
}

pub(crate) fn alignment_from(value: &str) -> Option<Alignment> {
    match value {
        "start" | "left" => Some(Alignment::Start),
        "end" | "right" => Some(Alignment::End),
        "center" => Some(Alignment::Center),
        "both" | "distribute" | "justify" => Some(Alignment::Justify),
        _ => None,
    }
}

pub(crate) fn style_kind_from(value: &str) -> Option<StyleKind> {
    match value {
        "paragraph" => Some(StyleKind::Paragraph),
        "character" => Some(StyleKind::Character),
        "table" => Some(StyleKind::Table),
        "numbering" => Some(StyleKind::Numbering),
        _ => None,
    }
}

pub(crate) fn break_kind(element: &BytesStart<'_>) -> BreakKind {
    match attribute_value(element, b"type").as_deref() {
        Some("page") => BreakKind::Page,
        Some("column") => BreakKind::Column,
        _ => BreakKind::Line,
    }
}

/// Resolves a `w:sym` symbol glyph from its `w:font` and `w:char` attributes.
///
/// Returns the (font, code point) pair only when a non-empty, length-bounded
/// font name and a parseable hexadecimal `w:char` are both present. A missing,
/// empty, over-long, or unparseable value yields `None` so the caller can report
/// the glyph rather than emit an invalid or misleading symbol node.
pub(crate) fn symbol_glyph(element: &BytesStart<'_>) -> Option<(String, u32)> {
    let font = attribute_value(element, b"font")?;
    if font.is_empty() || font.len() > MAX_SYMBOL_FONT_LEN {
        return None;
    }
    // `w:char` is a hexadecimal code point (`ST_ShortHexNumber`-style, e.g.
    // `F0FC`); tolerate an optional `0x` prefix a producer might emit.
    let raw = attribute_value(element, b"char")?;
    let char = u32::from_str_radix(raw.trim_start_matches("0x"), 16).ok()?;
    Some((font, char))
}

pub(crate) fn attribute_value(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attribute in element.attributes() {
        let attribute = attribute.ok()?;
        if attribute.key.local_name().as_ref() == name {
            // Unescape XML character references so an attribute value round-trips
            // symmetrically with a writer's escaping (e.g. a field instruction or
            // URL carrying `&`/`"`/`<`). Entity-free values are unchanged. This
            // mirrors the run-text path in `body.rs`.
            let raw = std::str::from_utf8(attribute.value.as_ref()).ok()?;
            return quick_xml::escape::unescape(raw)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

pub(crate) fn is_true(value: Option<&str>) -> bool {
    !matches!(value, Some("0") | Some("false") | Some("off"))
}

fn indent_attr(element: &BytesStart<'_>, names: &[&[u8]]) -> Option<i32> {
    for name in names {
        if let Some(value) = attribute_value(element, name).and_then(|raw| raw.parse::<i32>().ok())
        {
            return (-31_680..=31_680).contains(&value).then_some(value);
        }
    }
    None
}

fn spacing_twips(element: &BytesStart<'_>, name: &[u8]) -> Option<i32> {
    attribute_value(element, name)
        .and_then(|raw| raw.parse::<i32>().ok())
        .filter(|value| (0..=31_680).contains(value))
}

/// Parses the line-spacing attributes of a `w:spacing` element into the model's
/// `(line_percent, line_rule, line_twips)` triple. `auto` (or an absent rule) is a
/// multiple of single spacing, converted to a percentage; `atLeast`/`exact` carry
/// a twip value in `line_twips` and set `line_rule`.
fn spacing_line(element: &BytesStart<'_>) -> (Option<u16>, Option<LineRule>, Option<i32>) {
    let Some(line) = attribute_value(element, b"line").and_then(|raw| raw.parse::<i64>().ok())
    else {
        return (None, None, None);
    };
    match attribute_value(element, b"lineRule").as_deref() {
        None | Some("auto") => {
            let percent = line
                .checked_mul(100)
                .map(|value| value / 240)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| (1..=10_000).contains(value));
            (percent, None, None)
        }
        Some("atLeast") => {
            let twips = i32::try_from(line)
                .ok()
                .filter(|value| (0..=31_680).contains(value));
            (None, twips.map(|_| LineRule::AtLeast), twips)
        }
        Some("exact") => {
            let twips = i32::try_from(line)
                .ok()
                .filter(|value| (0..=31_680).contains(value));
            (None, twips.map(|_| LineRule::Exact), twips)
        }
        _ => (None, None, None),
    }
}

/// Parses a `w:beforeAutospacing`/`w:afterAutospacing` toggle: present means on
/// unless `val` is `0`/`false`/`off`. Absent yields `None` (do not autospace).
fn spacing_auto(element: &BytesStart<'_>, name: &[u8]) -> Option<bool> {
    attribute_value(element, name).map(|value| is_true(Some(value.as_str())))
}

pub(crate) fn parse_rgb(value: &str) -> Option<RgbColor> {
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&value[range], 16).ok();
    Some(RgbColor {
        r: channel(0..2)?,
        g: channel(2..4)?,
        b: channel(4..6)?,
    })
}
