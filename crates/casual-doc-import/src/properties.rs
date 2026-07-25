//! Attribute and property parsing shared by the body and styles parsers.
//!
//! The `apply_*` functions return `true` when the element was fully consumed
//! (mapped) and `false` when it is a property element the caller should report
//! (unknown, or present-but-out-of-domain/degraded).

use casual_doc_model::v1::{
    Alignment, BreakKind, Color, EmphasisMark, FontName, FontRef, HighlightColor, Indentation,
    ParagraphProperties, RgbColor, RunProperties, Spacing, StyleKind, ThemeFont, ThemeFontRef,
    VerticalAlignment,
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
        b"i" => properties.italic = Some(is_true(value.as_deref())),
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
        // Fonts: each slot prefers its theme attr, else its named attr. Consumed
        // when ANY slot resolves; an `rFonts` carrying only unmodeled detail (e.g.
        // just `@hint`) resolves nothing and is reported (no silent loss).
        b"rFonts" => {
            let ascii = font_slot(element, b"ascii", b"asciiTheme");
            let h_ansi = font_slot(element, b"hAnsi", b"hAnsiTheme");
            let cs = font_slot(element, b"cs", b"csTheme");
            let east_asia = font_slot(element, b"eastAsia", b"eastAsiaTheme");
            if ascii.is_none() && h_ansi.is_none() && cs.is_none() && east_asia.is_none() {
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
        _ => return false,
    }
    true
}

/// Resolves one `w:rFonts` slot: theme attr first (`major*`→Major, `minor*`→Minor),
/// else the named attr (bounded to 255 bytes). Returns `None` if neither resolves.
fn font_slot(element: &BytesStart<'_>, named: &[u8], theme: &[u8]) -> Option<FontRef> {
    // A theme value IN the vocabulary wins; one OUTSIDE it (malformed/unknown)
    // falls through to the named attribute rather than swallowing the slot — so a
    // bogus theme next to a valid named family does not silently drop the family.
    if let Some(value) = attribute_value(element, theme) {
        if let Some(slot) = theme_font_ref(&value) {
            return Some(FontRef::Theme(ThemeFont { slot }));
        }
    }
    attribute_value(element, named)
        .filter(|name| !name.is_empty() && name.len() <= 255)
        .map(|name| FontRef::Named(FontName { name }))
}

fn theme_font_ref(value: &str) -> Option<ThemeFontRef> {
    if value.starts_with("major") {
        Some(ThemeFontRef::Major)
    } else if value.starts_with("minor") {
        Some(ThemeFontRef::Minor)
    } else {
        None
    }
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
            let spacing = Spacing {
                before_twips: spacing_twips(element, b"before"),
                after_twips: spacing_twips(element, b"after"),
                line_percent: spacing_line_percent(element),
            };
            if spacing == Spacing::default() {
                return false;
            }
            properties.spacing = Some(spacing);
        }
        // Toggle flags (`CT_OnOff`): present means on unless `val` is 0/false/off.
        b"keepNext" => properties.keep_next = is_true(attribute_value(element, b"val").as_deref()),
        b"keepLines" => {
            properties.keep_lines = is_true(attribute_value(element, b"val").as_deref())
        }
        b"pageBreakBefore" => {
            properties.page_break_before = is_true(attribute_value(element, b"val").as_deref())
        }
        b"widowControl" => {
            properties.widow_control = is_true(attribute_value(element, b"val").as_deref())
        }
        b"contextualSpacing" => {
            properties.contextual_spacing = is_true(attribute_value(element, b"val").as_deref())
        }
        b"suppressLineNumbers" => {
            properties.suppress_line_numbers = is_true(attribute_value(element, b"val").as_deref())
        }
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

pub(crate) fn attribute_value(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attribute in element.attributes() {
        let attribute = attribute.ok()?;
        if attribute.key.local_name().as_ref() == name {
            return std::str::from_utf8(attribute.value.as_ref())
                .ok()
                .map(str::to_owned);
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

fn spacing_line_percent(element: &BytesStart<'_>) -> Option<u16> {
    let line = attribute_value(element, b"line").and_then(|raw| raw.parse::<i64>().ok())?;
    match attribute_value(element, b"lineRule").as_deref() {
        None | Some("auto") => {
            let percent = line.checked_mul(100)? / 240;
            u16::try_from(percent)
                .ok()
                .filter(|value| (1..=10_000).contains(value))
        }
        _ => None,
    }
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
