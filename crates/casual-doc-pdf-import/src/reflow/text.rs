//! Lines into real paragraphs, list markers, and the font clusters that
//! become the style sheet (`docs/182` §4.2; `docs/131` §10.4-10.5).
//!
//! **Paragraphs.** Fixed layout keeps a PDF's line ends as hard breaks;
//! reflow joins lines into paragraphs and breaks a paragraph where the
//! evidence says one ends: a list marker starts the next line, the line's
//! font changes, a short line ends in terminal punctuation, the next line is
//! indented as a first line, or the gap between lines opens past
//! [`PARAGRAPH_GAP`] of the line height. Joined lines are separated by one
//! space, except that a line ending in a hyphen after a letter, followed by a
//! line starting with a lowercase letter, is *de-hyphenated* (the hyphen is
//! the typesetter's, not the author's). A paragraph also continues across
//! elements (the next column, the next page) when it ran to the column's end
//! without terminal punctuation and the next element starts lowercase,
//! unindented, in the same font.
//!
//! **Styles.** Every character votes for its font key (family, size, bold,
//! italic); the key with the most characters is Normal. A key is a heading
//! level when its paragraphs are consistently short (the `docs/131` §10.5 bar:
//! "a consistent document-wide typography rule", not large type alone) and
//! it is larger than Normal, or as large and bold where Normal is not. Levels
//! are ranked by size, then weight. A tagged `H1`-`H6` role decides the level
//! outright.
//!
//! Complexity: O(lines) to segment, O(characters) to cluster; keys are
//! counted in a map, O(log k) each.

use std::collections::BTreeMap;

use casual_doc_model::v1::NumberFormat;

use super::{ILine, PChar};

/// A gap between lines larger than this fraction of the line height ends the
/// paragraph.
pub const PARAGRAPH_GAP: f64 = 0.6;
/// A line ending this many ems short of the column ends a paragraph when it
/// also ends a sentence.
pub const SHORT_LINE_EMS: f64 = 2.0;
/// A first line indented by more than this many ems starts a paragraph.
pub const INDENT_EMS: f64 = 0.8;
/// A key is a heading only if this share of its paragraphs is short.
pub const HEADING_SHORT_SHARE: f64 = 0.8;
/// "Short": at most this many lines.
pub const HEADING_MAX_LINES: usize = 3;
/// "Short": at most this many characters.
pub const HEADING_MAX_CHARS: usize = 200;
/// Larger than Normal by this factor.
pub const HEADING_SIZE_FACTOR: f64 = 1.15;

/// A font identity for clustering.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FontKey {
    /// Family the run is drawn with.
    pub family: String,
    /// Size in half points.
    pub half_points: u32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

/// The inferred sheet: Normal and the heading levels (index 0 is level 1).
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    /// Normal's key.
    pub body: Option<FontKey>,
    /// Heading keys by level.
    pub headings: Vec<FontKey>,
}

impl StyleSheet {
    /// The heading level (1-based) of `key`, if it is one.
    #[must_use]
    pub fn level(&self, key: &FontKey) -> Option<u8> {
        self.headings
            .iter()
            .position(|k| k.half_points == key.half_points && k.bold == key.bold)
            .and_then(|i| u8::try_from(i + 1).ok())
    }
}

/// Whether `text` ends a sentence.
fn terminal(text: &str) -> bool {
    text.trim_end()
        .ends_with(['.', '!', '?', ':', ';', '"', '\u{201d}', ')'])
}

fn line_text(line: &ILine) -> String {
    line.chars.iter().map(|c| c.text.as_str()).collect()
}

/// The key a line's characters mostly carry.
pub fn dominant<'a>(
    chars: impl Iterator<Item = &'a PChar>,
    key: &dyn Fn(&PChar) -> FontKey,
) -> Option<FontKey> {
    let mut counts: BTreeMap<FontKey, usize> = BTreeMap::new();
    for c in chars {
        if c.text.trim().is_empty() || c.sup {
            continue;
        }
        *counts.entry(key(c)).or_default() += c.text.chars().count();
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(k, _)| k)
}

/// A list marker at the start of a line.
#[derive(Clone, Debug, PartialEq)]
pub enum Marker {
    /// A bullet glyph.
    Bullet(char),
    /// An enumerator.
    Number {
        /// Its value.
        value: u32,
        /// Decimal, letter or roman.
        format: NumberFormat,
        /// The level text (`%1.`, `%1)`, `(%1)`).
        pattern: &'static str,
    },
}

impl Marker {
    /// Whether two markers belong to the same list kind.
    #[must_use]
    pub fn same_kind(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bullet(_), Self::Bullet(_)) => true,
            (
                Self::Number {
                    format: a,
                    pattern: p,
                    ..
                },
                Self::Number {
                    format: b,
                    pattern: q,
                    ..
                },
            ) => a == b && p == q,
            _ => false,
        }
    }
}

const BULLETS: &[char] = &[
    '\u{2022}', '\u{25e6}', '\u{25aa}', '\u{25ab}', '\u{25a0}', '\u{25a1}', '\u{25cf}', '\u{25cb}',
    '\u{2023}', '\u{2043}', '\u{2013}', '\u{2014}', '-', '*', '\u{00b7}', '\u{27a2}', '\u{25ba}',
    '\u{2713}', '\u{2756}', '\u{f0b7}', '\u{f0a7}', '\u{f076}', '\u{f0d8}',
];

fn roman(s: &str) -> Option<u32> {
    let lower = s.to_ascii_lowercase();
    if lower.is_empty() || !lower.chars().all(|c| "ivxlc".contains(c)) {
        return None;
    }
    let value = |c| match c {
        'i' => 1,
        'v' => 5,
        'x' => 10,
        'l' => 50,
        _ => 100,
    };
    let digits: Vec<u32> = lower.chars().map(value).collect();
    let mut total = 0;
    for (i, d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|n| n > d) {
            total -= *d as i64;
        } else {
            total += *d as i64;
        }
    }
    u32::try_from(total).ok().filter(|v| (1..=39).contains(v))
}

/// The marker a line starts with and the number of characters (in the line's
/// `chars`) it occupies, including the space after it. A marker must be
/// followed by whitespace. `prefer_letter` reads `i.`/`v.`/`x.` as letters
/// (inside a lettered list).
#[must_use]
pub fn marker(chars: &[PChar], prefer_letter: bool) -> Option<(Marker, usize)> {
    let mut token = String::new();
    let mut taken = 0;
    for c in chars {
        if c.text.chars().all(char::is_whitespace) {
            break;
        }
        token.push_str(&c.text);
        taken += 1;
        if token.chars().count() > 6 {
            return None;
        }
    }
    let spaces = chars[taken..]
        .iter()
        .take_while(|c| c.text.chars().all(char::is_whitespace))
        .count();
    if spaces == 0 || taken == chars.len() {
        return None;
    }
    let mut it = token.chars();
    if let (Some(c), None) = (it.next(), it.next())
        && BULLETS.contains(&c)
    {
        let glyph = if ('\u{f000}'..='\u{f0ff}').contains(&c) {
            '\u{2022}'
        } else {
            c
        };
        return Some((Marker::Bullet(glyph), taken + spaces));
    }
    let (body, pattern) =
        if let Some(inner) = token.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
            (inner, "(%1)")
        } else if let Some(inner) = token.strip_suffix('.') {
            (inner, "%1.")
        } else if let Some(inner) = token.strip_suffix(')') {
            (inner, "%1)")
        } else {
            return None;
        };
    let number = if !body.is_empty() && body.len() <= 3 && body.bytes().all(|b| b.is_ascii_digit())
    {
        body.parse().ok().map(|v| (v, NumberFormat::Decimal))
    } else if let Some(v) = roman(body).filter(|_| !(prefer_letter && body.len() == 1)) {
        let upper = body.chars().all(|c| c.is_ascii_uppercase());
        Some((
            v,
            if upper {
                NumberFormat::UpperRoman
            } else {
                NumberFormat::LowerRoman
            },
        ))
    } else if body.len() == 1 && body.bytes().all(|b| b.is_ascii_alphabetic()) {
        let b = body.as_bytes()[0];
        if b.is_ascii_uppercase() {
            Some((u32::from(b - b'A') + 1, NumberFormat::UpperLetter))
        } else {
            Some((u32::from(b - b'a') + 1, NumberFormat::LowerLetter))
        }
    } else {
        None
    };
    let (value, format) = number?;
    Some((
        Marker::Number {
            value,
            format,
            pattern,
        },
        taken + spaces,
    ))
}

/// Whether `token` (a whole line's text) is nothing but a list marker.
#[must_use]
pub fn marker_token(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() || token.chars().count() > 6 {
        return false;
    }
    let mut chars: Vec<PChar> = token
        .chars()
        .chain(" x".chars())
        .map(|c| PChar {
            text: c.to_string(),
            x0: 0.0,
            x1: 0.0,
            baseline: 0.0,
            size: 0.0,
            style: crate::group::Style {
                font: 0,
                half_points: 0,
                fill: None,
                mode: casual_doc_pdf_read::RenderMode::Fill,
            },
            sup: false,
            link: None,
            note: None,
            comments: Vec::new(),
        })
        .collect();
    chars.shrink_to_fit();
    marker(&chars, false).is_some_and(|(_, n)| n == token.chars().count() + 1)
}

/// One paragraph's lines, as cut by [`segment`].
#[derive(Clone, Debug, Default)]
pub struct Segment {
    /// Indices into the element's lines.
    pub lines: Vec<usize>,
}

/// Cuts an element's lines into paragraphs. `left`/`right` are the column's
/// edges; `key` reads a character's font key. O(lines).
#[must_use]
pub fn segment(
    lines: &[ILine],
    left: f64,
    right: f64,
    key: &dyn Fn(&PChar) -> FontKey,
) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut current = Segment::default();
    let keys: Vec<Option<FontKey>> = lines
        .iter()
        .map(|l| dominant(l.chars.iter(), key))
        .collect();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            let prev = &lines[i - 1];
            let em = prev.size.max(line.size).max(1.0);
            let gap = line.top - prev.bottom;
            let height = (prev.bottom - prev.top)
                .max(line.bottom - line.top)
                .max(1.0);
            let short = prev.x1 < right - SHORT_LINE_EMS * em;
            let indented = line.x0 > left + INDENT_EMS * em
                && lines
                    .get(i + 1)
                    .is_none_or(|next| next.x0 < line.x0 - INDENT_EMS * em * 0.5);
            let breaks = marker(&line.chars, false).is_some()
                || keys[i] != keys[i - 1]
                || gap > PARAGRAPH_GAP * height
                || (short && terminal(&line_text(prev)))
                || (indented
                    && prev.x0 < line.x0 - INDENT_EMS * em * 0.5
                    && marker(&prev.chars, false).is_none());
            if breaks && !current.lines.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        }
        current.lines.push(i);
    }
    if !current.lines.is_empty() {
        out.push(current);
    }
    out
}

/// Joins lines into one character sequence: a space between lines, or a
/// de-hyphenation. Returns the characters and how many hyphens were removed.
/// O(characters).
#[must_use]
pub fn join(lines: &[&ILine]) -> (Vec<PChar>, u32) {
    let mut out: Vec<PChar> = Vec::new();
    let mut removed = 0;
    for (i, line) in lines.iter().enumerate() {
        let mut chars: Vec<PChar> = line.chars.clone();
        while chars.last().is_some_and(|c| c.text.trim().is_empty()) {
            chars.pop();
        }
        if i > 0 {
            let starts_lower = chars
                .iter()
                .find(|c| !c.text.trim().is_empty())
                .and_then(|c| c.text.chars().next())
                .is_some_and(char::is_lowercase);
            let hyphen = out
                .last()
                .is_some_and(|c| matches!(c.text.as_str(), "-" | "\u{2010}" | "\u{ad}"));
            let before_letter = out.len() >= 2
                && out[out.len() - 2]
                    .text
                    .chars()
                    .last()
                    .is_some_and(char::is_alphabetic);
            let soft = out.last().is_some_and(|c| c.text == "\u{ad}");
            if hyphen && (soft || (before_letter && starts_lower)) {
                out.pop();
                removed += 1;
            } else if !hyphen && let Some(last) = out.last() {
                let mut space = last.clone();
                space.text = " ".to_owned();
                space.sup = false;
                space.note = None;
                space.comments = Vec::new();
                out.push(space);
            }
        }
        while chars.first().is_some_and(|c| c.text.trim().is_empty()) {
            chars.remove(0);
        }
        out.extend(chars);
    }
    // A soft hyphen the PDF drew at the end of the paragraph is a hyphen the
    // reader saw; one inside the text is the discretionary hyphen it means.
    if let Some(last) = out.last_mut()
        && last.text == "\u{ad}"
    {
        last.text = "-".to_owned();
    }
    (out, removed)
}

/// Whether a paragraph ending `last` continues into one starting `next`:
/// `last` ran to within an em of the column's right edge and does not end a
/// sentence; `next` starts lowercase (or `last` ends in a hyphen), is not
/// indented, carries no marker, and has the same font. O(1) amortised.
#[must_use]
pub fn continues(
    last: &ILine,
    right: f64,
    next: &ILine,
    next_left: f64,
    key: &dyn Fn(&PChar) -> FontKey,
) -> bool {
    let em = last.size.max(1.0);
    let text = line_text(last);
    let hyphen = text.trim_end().ends_with('-');
    let full = last.x1 >= right - em;
    let lower = next
        .chars
        .iter()
        .find(|c| !c.text.trim().is_empty())
        .and_then(|c| c.text.chars().next())
        .is_some_and(char::is_lowercase);
    full && !terminal(&text)
        && (lower || hyphen)
        && next.x0 <= next_left + INDENT_EMS * em
        && marker(&next.chars, false).is_none()
        && dominant(last.chars.iter(), key) == dominant(next.chars.iter(), key)
}

/// Per key: (characters, paragraphs, short paragraphs).
pub type KeyStats = BTreeMap<FontKey, (usize, usize, usize)>;

/// Adds one paragraph to the statistics.
pub fn count(stats: &mut KeyStats, chars: &[PChar], lines: usize, key: &dyn Fn(&PChar) -> FontKey) {
    for c in chars {
        if c.text.trim().is_empty() || c.sup {
            continue;
        }
        stats.entry(key(c)).or_default().0 += c.text.chars().count();
    }
    if let Some(k) = dominant(chars.iter(), key) {
        let text_len = chars.iter().map(|c| c.text.chars().count()).sum::<usize>();
        let entry = stats.entry(k).or_default();
        entry.1 += 1;
        if lines <= HEADING_MAX_LINES
            && text_len <= HEADING_MAX_CHARS
            && !text_ends_like_prose(chars)
        {
            entry.2 += 1;
        }
    }
}

fn text_ends_like_prose(chars: &[PChar]) -> bool {
    let text: String = chars.iter().map(|c| c.text.as_str()).collect();
    let t = text.trim_end();
    t.ends_with('.') && t.split_whitespace().count() > 8
}

/// The style sheet from the statistics. O(keys log keys).
#[must_use]
pub fn sheet(stats: &KeyStats) -> StyleSheet {
    let Some((body, _)) = stats
        .iter()
        .max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.0.cmp(a.0)))
    else {
        return StyleSheet::default();
    };
    let body = body.clone();
    let mut headings: Vec<FontKey> = stats
        .iter()
        .filter(|(k, (_, paragraphs, short))| {
            let larger =
                f64::from(k.half_points) >= HEADING_SIZE_FACTOR * f64::from(body.half_points);
            let bolder = k.bold && !body.bold && k.half_points >= body.half_points;
            **k != body
                && (larger || bolder)
                && *paragraphs > 0
                && (*short as f64) >= HEADING_SHORT_SHARE * (*paragraphs as f64)
        })
        .map(|(k, _)| k.clone())
        .collect();
    headings.sort_by(|a, b| b.half_points.cmp(&a.half_points).then(b.bold.cmp(&a.bold)));
    headings.dedup_by(|a, b| a.half_points == b.half_points && a.bold == b.bold);
    headings.truncate(9);
    StyleSheet {
        body: Some(body),
        headings,
    }
}
