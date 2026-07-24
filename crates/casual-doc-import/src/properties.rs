//! Attribute and property parsing shared by the body and styles parsers.
//!
//! The `apply_*` functions return `true` when the element was fully consumed
//! (mapped) and `false` when it is a property element the caller should report
//! (unknown, or present-but-out-of-domain/degraded).

use casual_doc_model::v1::{
    Alignment, BreakKind, Color, Indentation, ParagraphProperties, RgbColor, RunProperties,
    Spacing, StyleKind,
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
        _ => return false,
    }
    true
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

fn is_true(value: Option<&str>) -> bool {
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

fn parse_rgb(value: &str) -> Option<RgbColor> {
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
