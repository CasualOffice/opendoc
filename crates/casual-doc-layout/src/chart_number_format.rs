// SPDX-License-Identifier: Apache-2.0

//! Excel number-format codes for chart labels (`c:numFmt@formatCode`,
//! `c:numCache/c:formatCode`) — the subset a chart's axis and data labels use
//! (`docs/155` §19).
//!
//! # What is honoured
//!
//! - `General` (alone, or as a section with literals around it);
//! - digit placeholders `0`, `#` and `?` (a `?` is treated as `#`: chart labels
//!   are not column-aligned, so the space it would pad with has no purpose),
//!   any number of decimals, `,` thousands grouping and trailing-`,` scaling by
//!   a thousand;
//! - `%` (multiplies by 100 per sign) and scientific `0.00E+00` / `0.0E-0`;
//! - literal text: `"quoted"`, a `\`-escaped character, `_x` (a space), the
//!   characters `$ - + ( ) : ! ^ & ' ~ { } < > =` and space, any non-ASCII
//!   character (`€`, `£`, `¥`), and a `[$€-407]` locale-currency tag (its
//!   symbol is printed, its locale ignored);
//! - up to four sections `positive;negative;zero;text`, where a negative
//!   section prints the magnitude with no automatic minus — so
//!   `#,##0;(#,##0)` brackets a loss;
//! - `@` (the text placeholder, which prints the number as `General`);
//! - colour tags (`[Red]`, `[Color12]`) are accepted and ignored: chart labels
//!   take their colour from the chart's text formatting.
//!
//! # What falls back
//!
//! Anything else — dates and times (`m/d/yyyy`, `[h]`), conditions (`[>100]`),
//! fractions (`# ?/?`), a literal between two digit placeholders (`000-0000`) —
//! is not half-implemented: the whole code falls back to [`general`], which is
//! what this module printed before format codes were read at all. A wrong
//! date would mislead; the plain number does not.
//!
//! # Rounding
//!
//! Half away from zero, after first rounding to 15 significant digits — Excel's
//! own precision — so `1.005` with `0.00` prints `1.01` (the binary double is
//! `1.00499999…`, which a naive `format!` rounds down).
//!
//! # Complexity and bounds
//!
//! O(code length + printed digits). A code longer than [`MAX_FORMAT_CODE_BYTES`]
//! falls back unparsed; decimals are capped at [`MAX_DECIMALS`]; nothing here
//! can panic on any input, and a non-finite value prints as the empty string.

/// The longest format code read; Excel's own limit is 255 characters.
pub const MAX_FORMAT_CODE_BYTES: usize = 255;

/// The most decimals a code may ask for; Excel's own cap is 30.
pub const MAX_DECIMALS: usize = 30;

/// Formats `value` through the Excel format `code`, or as [`general`] when the
/// code is absent, `General`, or outside the supported subset.
///
/// O(code length + printed digits); never panics.
#[must_use]
pub fn format_chart_number(value: f64, code: Option<&str>) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let Some(code) = code.filter(|code| !code.trim().is_empty()) else {
        return general(value);
    };
    if code.len() > MAX_FORMAT_CODE_BYTES {
        return general(value);
    }
    apply(value, code).unwrap_or_else(|| general(value))
}

/// Whether `code` means "no explicit format" — absent, empty, or `General`.
/// O(code length).
#[must_use]
pub fn is_general(code: Option<&str>) -> bool {
    code.is_none_or(|code| {
        let code = code.trim();
        code.is_empty() || code.eq_ignore_ascii_case("general")
    })
}

/// The chart's `General` rendering: the value plainly, at most two decimals,
/// trailing zeros (and a bare point) trimmed. Empty for a non-finite value.
///
/// This is what chart labels printed before format codes were read, and what
/// every unsupported code still falls back to. O(1).
#[must_use]
pub fn general(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if (value - value.round()).abs() < 1e-9 && value.abs() < 1e15 {
        return format!("{}", value.round() as i64);
    }
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// One parsed piece of a section, in print order.
#[derive(Debug)]
enum Piece {
    /// Literal text.
    Literal(String),
    /// The numeric field (digits, point, exponent).
    Number,
    /// `General`.
    General,
    /// `@`.
    Text,
}

/// Where a scan is relative to the section's numeric field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Before,
    In,
    After,
}

/// A scientific exponent's shape.
#[derive(Clone, Copy, Debug)]
struct Exponent {
    /// `E+` (always signed) rather than `E-` (signed only when negative).
    plus: bool,
    /// Minimum exponent digits.
    digits: usize,
}

/// One parsed section.
#[derive(Debug, Default)]
struct Pattern {
    pieces: Vec<Piece>,
    /// `0`s before the point — the minimum integer digits.
    int_zeros: usize,
    /// All placeholders before the point.
    int_places: usize,
    /// A `,` between integer placeholders.
    grouping: bool,
    /// Trailing commas: divide by 1000 per comma.
    scale_thousands: u32,
    /// `0`s after the point — the minimum decimals.
    frac_zeros: usize,
    /// All placeholders after the point — the maximum decimals.
    frac_places: usize,
    /// Whether the field has a decimal point.
    point: bool,
    /// `%` signs.
    percent: u32,
    exponent: Option<Exponent>,
}

/// Formats through a whole code, or `None` when it is outside the subset.
fn apply(value: f64, code: &str) -> Option<String> {
    let sections = split_sections(code)?;
    let negative = value < 0.0;
    // A negative value takes the second section, when there is one, and prints
    // its magnitude with no minus of its own; a zero takes the third.
    let (section, signed) = if negative && sections.len() >= 2 {
        (sections[1], false)
    } else if value == 0.0 && sections.len() >= 3 {
        (sections[2], false)
    } else {
        (sections[0], negative)
    };
    let pattern = parse_section(section)?;
    let body = render(value.abs(), &pattern)?;
    if signed && body.bytes().any(|byte| (b'1'..=b'9').contains(&byte)) {
        Some(format!("-{body}"))
    } else {
        Some(body)
    }
}

/// Splits on `;` outside quotes, escapes and brackets. At most four sections.
fn split_sections(code: &str) -> Option<Vec<&str>> {
    let mut sections = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut bracket = false;
    let mut escaped = false;
    for (index, ch) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                sections.push(&code[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    sections.push(&code[start..]);
    (sections.len() <= 4).then_some(sections)
}

/// Parses one section, or `None` when it uses anything outside the subset.
fn parse_section(section: &str) -> Option<Pattern> {
    let chars: Vec<char> = section.chars().collect();
    let mut pattern = Pattern::default();
    let mut field = Field::Before;
    let mut after_point = false;
    let mut pending_commas = 0u32;
    let mut in_exponent = false;
    let mut i = 0;

    // Appends literal text, closing the numeric field if it was open.
    fn literal(pattern: &mut Pattern, field: &mut Field, text: String) {
        if *field == Field::In {
            *field = Field::After;
        }
        if let Some(Piece::Literal(last)) = pattern.pieces.last_mut() {
            last.push_str(&text);
        } else {
            pattern.pieces.push(Piece::Literal(text));
        }
    }

    while i < chars.len() {
        let ch = chars[i];
        // Trailing commas end at anything that is not an integer placeholder.
        if pending_commas > 0 && !matches!(ch, '0' | '#' | '?' | ',') {
            pattern.scale_thousands += pending_commas;
            pending_commas = 0;
        }
        match ch {
            '0' | '#' | '?' => {
                if field == Field::After {
                    return None;
                }
                if field == Field::Before {
                    pattern.pieces.push(Piece::Number);
                    field = Field::In;
                }
                if in_exponent {
                    if ch == '?' {
                        return None;
                    }
                    if let Some(exponent) = pattern.exponent.as_mut() {
                        exponent.digits += 1;
                    }
                } else if after_point {
                    pattern.frac_places += 1;
                    if ch == '0' {
                        pattern.frac_zeros += 1;
                    }
                } else {
                    if pending_commas > 0 {
                        pattern.grouping = true;
                        pending_commas = 0;
                    }
                    pattern.int_places += 1;
                    if ch == '0' {
                        pattern.int_zeros += 1;
                    }
                }
            }
            ',' if field == Field::In && !after_point && !in_exponent => pending_commas += 1,
            '.' if field != Field::After && !after_point && !in_exponent => {
                if field == Field::Before {
                    pattern.pieces.push(Piece::Number);
                    field = Field::In;
                }
                after_point = true;
                pattern.point = true;
            }
            'E' | 'e'
                if field == Field::In
                    && !in_exponent
                    && matches!(chars.get(i + 1), Some('+' | '-')) =>
            {
                pattern.exponent = Some(Exponent {
                    plus: chars[i + 1] == '+',
                    digits: 0,
                });
                in_exponent = true;
                i += 1;
            }
            '%' => {
                pattern.percent += 1;
                literal(&mut pattern, &mut field, "%".to_owned());
            }
            '"' => {
                let close = chars[i + 1..].iter().position(|c| *c == '"')?;
                let text: String = chars[i + 1..i + 1 + close].iter().collect();
                literal(&mut pattern, &mut field, text);
                i += close + 1;
            }
            '\\' => {
                let next = *chars.get(i + 1)?;
                literal(&mut pattern, &mut field, next.to_string());
                i += 1;
            }
            '_' => {
                // `_x` pads with the width of `x`; a space is the visible part.
                chars.get(i + 1)?;
                literal(&mut pattern, &mut field, " ".to_owned());
                i += 1;
            }
            '*' => {
                // `*x` repeats `x` to fill a cell; a label has no cell to fill.
                chars.get(i + 1)?;
                i += 1;
            }
            '[' => {
                let close = chars[i + 1..].iter().position(|c| *c == ']')?;
                let tag: String = chars[i + 1..i + 1 + close].iter().collect();
                if let Some(currency) = tag.strip_prefix('$') {
                    let symbol = currency.split('-').next().unwrap_or_default();
                    if !symbol.is_empty() {
                        literal(&mut pattern, &mut field, symbol.to_owned());
                    }
                } else if !is_colour_tag(&tag) {
                    // A condition (`[>100]`) or an elapsed time (`[h]`).
                    return None;
                }
                i += close + 1;
            }
            '@' => {
                if field != Field::Before {
                    return None;
                }
                pattern.pieces.push(Piece::Text);
                field = Field::After;
            }
            'G' | 'g' if starts_with_general(&chars[i..]) => {
                if field != Field::Before {
                    return None;
                }
                pattern.pieces.push(Piece::General);
                field = Field::After;
                i += "general".len() - 1;
            }
            '$' | '-' | '+' | '(' | ')' | ':' | '!' | '^' | '&' | '\'' | '~' | '{' | '}' | '<'
            | '>' | '=' | ' ' | ',' => literal(&mut pattern, &mut field, ch.to_string()),
            c if !c.is_ascii() => literal(&mut pattern, &mut field, c.to_string()),
            // Date/time letters, a fraction's `/`, and anything unrecognised.
            _ => return None,
        }
        // The exponent's digits end at the first thing that is not one.
        if in_exponent && !matches!(ch, '0' | '#' | 'E' | 'e') {
            in_exponent = false;
        }
        i += 1;
    }
    pattern.scale_thousands += pending_commas;
    // `E+` with no digits after it is not an exponent.
    if pattern
        .exponent
        .is_some_and(|exponent| exponent.digits == 0)
    {
        return None;
    }
    let numeric = pattern
        .pieces
        .iter()
        .filter(|piece| matches!(piece, Piece::Number | Piece::General | Piece::Text))
        .count();
    (numeric <= 1).then_some(pattern)
}

/// Whether `chars` begins with `General`, case-insensitively.
fn starts_with_general(chars: &[char]) -> bool {
    let word: String = chars.iter().take(7).collect();
    word.eq_ignore_ascii_case("general")
}

/// Whether a bracketed tag names a colour (`Red`, `Color12`).
fn is_colour_tag(tag: &str) -> bool {
    const COLOURS: [&str; 8] = [
        "black", "blue", "cyan", "green", "magenta", "red", "white", "yellow",
    ];
    let lower = tag.to_ascii_lowercase();
    COLOURS.contains(&lower.as_str())
        || lower
            .strip_prefix("color")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Prints `magnitude` (non-negative) through one parsed section.
fn render(magnitude: f64, pattern: &Pattern) -> Option<String> {
    let mut out = String::new();
    for piece in &pattern.pieces {
        match piece {
            Piece::Literal(text) => out.push_str(text),
            Piece::General | Piece::Text => out.push_str(&general(magnitude)),
            Piece::Number => {
                let mut scaled = magnitude;
                for _ in 0..pattern.percent.min(8) {
                    scaled *= 100.0;
                }
                for _ in 0..pattern.scale_thousands.min(8) {
                    scaled /= 1000.0;
                }
                if !scaled.is_finite() {
                    return None;
                }
                match pattern.exponent {
                    Some(exponent) => out.push_str(&scientific(scaled, pattern, exponent)),
                    None => out.push_str(&fixed(scaled, pattern)),
                }
            }
        }
    }
    Some(out)
}

/// The fixed-point rendering of a non-negative value.
fn fixed(value: f64, pattern: &Pattern) -> String {
    let decimals = pattern.frac_places.min(MAX_DECIMALS);
    let (int, frac) = round_decimal(value, decimals);
    let mut int = if int == "0" && pattern.int_zeros == 0 {
        String::new()
    } else {
        int
    };
    while int.len() < pattern.int_zeros {
        int.insert(0, '0');
    }
    if pattern.grouping {
        int = group_thousands(&int);
    }
    let mut frac = frac;
    while frac.len() > pattern.frac_zeros.min(MAX_DECIMALS) && frac.ends_with('0') {
        frac.pop();
    }
    if pattern.point {
        format!("{int}.{frac}")
    } else {
        int
    }
}

/// The scientific rendering of a non-negative value.
fn scientific(value: f64, pattern: &Pattern, exponent: Exponent) -> String {
    let int_digits = pattern.int_zeros.max(1);
    let decimals = pattern.frac_places.min(MAX_DECIMALS);
    let mut power = if value == 0.0 {
        0
    } else {
        value.log10().floor() as i32 - (int_digits as i32 - 1)
    };
    let mut mantissa = mantissa_of(value, power);
    // Rounding can carry the mantissa up a digit (9.995 -> 10.00): renormalise
    // once, which is always enough.
    if value != 0.0 && round_decimal(mantissa, decimals).0.len() > int_digits {
        power += 1;
        mantissa = mantissa_of(value, power);
    }
    let mantissa_pattern = Pattern {
        pieces: Vec::new(),
        grouping: false,
        scale_thousands: 0,
        percent: 0,
        exponent: None,
        int_zeros: pattern.int_zeros,
        int_places: pattern.int_places,
        frac_zeros: pattern.frac_zeros,
        frac_places: pattern.frac_places,
        point: pattern.point,
    };
    let body = fixed(mantissa, &mantissa_pattern);
    let sign = if power < 0 {
        "-"
    } else if exponent.plus {
        "+"
    } else {
        ""
    };
    let mut digits = power.unsigned_abs().to_string();
    while digits.len() < exponent.digits.min(8) {
        digits.insert(0, '0');
    }
    format!("{body}E{sign}{digits}")
}

/// `value / 10^power`, computed so a large or small power stays finite.
fn mantissa_of(value: f64, power: i32) -> f64 {
    let power = power.clamp(-330, 330);
    if power >= 0 {
        value / 10f64.powi(power)
    } else {
        value * 10f64.powi(-power)
    }
}

/// Rounds a non-negative finite value to `decimals` places, half away from zero
/// after Excel's 15-significant-digit precision, and returns the integer digits
/// (no leading zeros, `"0"` for none) and exactly `decimals` fraction digits.
///
/// Works on the decimal digit string, so it is exact for every magnitude a
/// double can hold and never overflows an integer type. O(digits).
fn round_decimal(value: f64, decimals: usize) -> (String, String) {
    let zeros = || "0".repeat(decimals);
    if value <= 0.0 || !value.is_finite() {
        return ("0".to_owned(), zeros());
    }
    let sci = format!("{value:.14e}");
    let Some((mantissa, exponent)) = sci.split_once('e') else {
        return ("0".to_owned(), zeros());
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return ("0".to_owned(), zeros());
    };
    let digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|byte| byte - b'0')
        .collect();
    // Integer digits before the point (may be zero or negative).
    let mut point = exponent + 1;
    let keep = point + decimals as i32;
    if keep < 0 {
        return ("0".to_owned(), zeros());
    }
    let keep = keep as usize;
    let mut kept: Vec<u8> = (0..keep)
        .map(|index| digits.get(index).copied().unwrap_or(0))
        .collect();
    if digits.get(keep).is_some_and(|digit| *digit >= 5) {
        let mut carry = true;
        for digit in kept.iter_mut().rev() {
            if *digit == 9 {
                *digit = 0;
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            kept.insert(0, 1);
            point += 1;
        }
    }
    let text: String = kept.iter().map(|digit| char::from(b'0' + digit)).collect();
    if point <= 0 {
        let mut frac = "0".repeat(point.unsigned_abs() as usize);
        frac.push_str(&text);
        frac.truncate(decimals);
        while frac.len() < decimals {
            frac.push('0');
        }
        return ("0".to_owned(), frac);
    }
    let split = (point as usize).min(text.len());
    let (int, frac) = text.split_at(split);
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    (int.to_owned(), frac.to_owned())
}

/// Inserts `,` every three digits from the right.
fn group_thousands(int: &str) -> String {
    let mut out = String::with_capacity(int.len() + int.len() / 3);
    for (index, ch) in int.chars().enumerate() {
        if index > 0 && (int.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table-driven: every row is (code, value, expected).
    #[test]
    fn format_codes_print_as_excel_prints_them() {
        let cases: &[(Option<&str>, f64, &str)] = &[
            (None, 1234.5, "1234.5"),
            (Some("General"), 1234.5, "1234.5"),
            (Some("General"), -3.0, "-3"),
            (Some("0"), 2.5, "3"),
            (Some("0"), -2.5, "-3"),
            (Some("0.00"), 1.005, "1.01"),
            (Some("0.00"), 2.0, "2.00"),
            (Some("000"), 7.0, "007"),
            (Some("#.00"), 0.5, ".50"),
            (Some("0.##"), 1.5, "1.5"),
            (Some("0.##"), 2.0, "2."),
            (Some("#,##0"), 1_234_567.0, "1,234,567"),
            (Some("#,##0"), 999.0, "999"),
            (Some("#,##0.00"), 1234.5, "1,234.50"),
            (Some("#,##0,"), 1_500_000.0, "1,500"),
            (Some("0%"), 0.256, "26%"),
            (Some("0.0%"), 0.2567, "25.7%"),
            (Some("0.00E+00"), 12345.0, "1.23E+04"),
            (Some("0.00E+00"), 0.00012, "1.20E-04"),
            (Some("0.00E+00"), 0.0, "0.00E+00"),
            (Some("0.00E+00"), 9.999, "1.00E+01"),
            (Some("0.0E-0"), 1500.0, "1.5E3"),
            (Some("$#,##0"), 1500.0, "$1,500"),
            (Some("$#,##0"), -1500.0, "-$1,500"),
            (Some("#,##0;(#,##0)"), -1500.0, "(1,500)"),
            (Some("#,##0;(#,##0)"), 1500.0, "1,500"),
            (Some("0;-0;\"zero\""), 0.0, "zero"),
            (Some("0;;"), -4.0, ""),
            (Some("0.0 \"kg\""), 3.06, "3.1 kg"),
            (Some("\\$0"), 5.0, "$5"),
            (Some("[$€-407] #,##0.00"), 1234.5, "€ 1,234.50"),
            (Some("#,##0.00 [$€-407]"), 1234.5, "1,234.50 €"),
            (Some("£#,##0"), 12.0, "£12"),
            (Some("[Red]0.0"), 1.25, "1.3"),
            (Some("0.0;[Red]-0.0"), -1.25, "-1.3"),
            (Some("@"), 12.0, "12"),
            (Some("General;(General)"), -3.0, "(3)"),
            (Some("0_);(0)"), 4.0, "4 "),
            // Outside the subset: the whole code falls back to General.
            (Some("m/d/yyyy"), 45_000.0, "45000"),
            (Some("[>100]0"), 7.5, "7.5"),
            (Some("# ?/?"), 1.5, "1.5"),
            (Some("000-0000"), 5_551_234.0, "5551234"),
            (Some("\"unterminated"), 1.0, "1"),
        ];
        let mut failures = Vec::new();
        for (code, value, expected) in cases {
            let got = format_chart_number(*value, *code);
            if got != *expected {
                failures.push(format!("{code:?} {value} -> {got:?}, want {expected:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn hostile_codes_and_values_never_panic_and_stay_bounded() {
        let long = "0".repeat(10_000);
        let codes = [
            "",
            ";;;;;",
            "\\",
            "_",
            "*",
            "[",
            "[$",
            "E+",
            "0E+",
            "0.0.0",
            "0.000000000000000000000000000000000000000000000000000",
            long.as_str(),
            "%%%%%%%%%%%%0",
            ",,,,,,,,,,,0",
            "0,,,,,,,,,,,,,",
        ];
        let values = [
            0.0,
            -0.0,
            1e308,
            -1e308,
            1e-308,
            f64::NAN,
            f64::INFINITY,
            123.456,
        ];
        for code in codes {
            for value in values {
                let text = format_chart_number(value, Some(code));
                assert!(text.len() < 2_000, "{code:?} {value} printed {text:?}");
            }
        }
        assert_eq!(format_chart_number(f64::NAN, Some("0.00")), "");
    }
}
