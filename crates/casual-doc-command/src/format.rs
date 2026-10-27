// SPDX-License-Identifier: Apache-2.0

//! Character formatting, lowered once (`docs/181` §4.2 step 2).
//!
//! Before this module the editor and the AI layer each turned "bold this" or
//! "highlight that yellow" into a `FormatDelta` their own way: the AI layer's
//! highlight table refused `gray`, which the editor's accepted, and the two
//! validated a font size differently (the AI rounded 10.3 points to 10.5; the
//! editor refused it). Both now call the functions here, so a value one front
//! door accepts the other accepts, and both build the same `FormatText`
//! operations for the same ranges.

use casual_doc_edit::{FormatDelta, Operation, Pos, Range};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{HighlightColor, RgbColor};

use crate::edit::TextFormat;

/// A colour written `#RRGGBB` (the `#` optional), or `None`. Only ASCII hex
/// digits are accepted, so the byte slices below are always on character
/// boundaries: a six-byte multibyte string used to pass a length check and
/// then panic when sliced, which traps a wasm module for good. O(1).
#[must_use]
pub fn parse_color(value: &str) -> Option<RgbColor> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    let bytes = hex.as_bytes();
    if bytes.len() != 6 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    Some(RgbColor {
        r: u8::from_str_radix(&hex[0..2], 16).ok()?,
        g: u8::from_str_radix(&hex[2..4], 16).ok()?,
        b: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
}

/// A highlight by Word's name (case-insensitive; `none` or empty clears it,
/// `gray`/`grey` are dark gray), or `None` for a name Word does not have.
/// Callers decide what an unknown name means: the editor's toolbar clears,
/// an agent is refused. O(1).
#[must_use]
pub fn parse_highlight(name: &str) -> Option<HighlightColor> {
    Some(match name.to_ascii_lowercase().as_str() {
        "none" | "" => HighlightColor::None,
        "black" => HighlightColor::Black,
        "blue" => HighlightColor::Blue,
        "cyan" => HighlightColor::Cyan,
        "green" => HighlightColor::Green,
        "magenta" => HighlightColor::Magenta,
        "red" => HighlightColor::Red,
        "white" => HighlightColor::White,
        "darkgray" | "gray" | "grey" => HighlightColor::DarkGray,
        "lightgray" => HighlightColor::LightGray,
        "darkblue" => HighlightColor::DarkBlue,
        "darkcyan" => HighlightColor::DarkCyan,
        "darkgreen" => HighlightColor::DarkGreen,
        "darkmagenta" => HighlightColor::DarkMagenta,
        "darkred" => HighlightColor::DarkRed,
        "darkyellow" => HighlightColor::DarkYellow,
        "yellow" => HighlightColor::Yellow,
        _ => return None,
    })
}

/// A font size in points as half-points: 1 to 1638 points in half-point
/// steps, as Word's size box allows. O(1).
///
/// # Errors
/// Outside that range, or not a half-point step.
pub fn half_points(points: f32) -> Result<u32, String> {
    if !points.is_finite() || !(1.0..=1638.0).contains(&points) || (points * 2.0).fract() != 0.0 {
        return Err("font size must be between 1 and 1638 points in 0.5 point steps".to_owned());
    }
    // In range and a whole number of half-points, so the cast is exact.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked above: positive, at most 3276, and integral"
    )]
    Ok((points * 2.0) as u32)
}

/// The `FormatDelta` an agent's `format` asks for. O(1).
///
/// # Errors
/// When a value is out of range or unknown, or `format` sets nothing.
pub fn delta(format: &TextFormat) -> Result<FormatDelta, String> {
    let delta = FormatDelta {
        bold: format.bold,
        italic: format.italic,
        underline: format.underline,
        strike: format.strike,
        size_half_points: format.size.map(half_points).transpose()?,
        color: format
            .color
            .as_deref()
            .map(|value| parse_color(value).ok_or_else(|| "A colour is `#RRGGBB`.".to_owned()))
            .transpose()?,
        highlight: format
            .highlight
            .as_deref()
            .map(|value| {
                parse_highlight(value)
                    .ok_or_else(|| "That highlight colour is not one Word has.".to_owned())
            })
            .transpose()?,
        font: format.font.clone(),
        ..FormatDelta::default()
    };
    if delta == FormatDelta::default() {
        return Err("`format` sets nothing.".to_owned());
    }
    Ok(delta)
}

/// One `FormatText` per `(paragraph, start, end)` range, in order: a selection
/// across paragraphs is formatted paragraph by paragraph as one step.
/// O(ranges).
#[must_use]
pub fn format_ranges(ranges: &[(NodeId, u32, u32)], delta: &FormatDelta) -> Vec<Operation> {
    ranges
        .iter()
        .map(|&(node, start, end)| Operation::FormatText {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
            delta: delta.clone(),
        })
        .collect()
}

/// One `ClearFormatting` per range: direct character formatting goes, the
/// paragraph's named style stays. O(ranges).
#[must_use]
pub fn clear_ranges(ranges: &[(NodeId, u32, u32)]) -> Vec<Operation> {
    ranges
        .iter()
        .map(|&(node, start, end)| Operation::ClearFormatting {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
        })
        .collect()
}
