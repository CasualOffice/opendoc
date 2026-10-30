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
/// A key used by fewer paragraphs than this is not a heading level (a
/// letterhead, a subtitle, a one-off callout): a level is a typography rule
/// the document applies more than once. The title is the exception.
pub const HEADING_MIN_PARAGRAPHS: usize = 2;
/// The title: the largest heading key, at least this many times Normal's
/// size, used by at most [`TITLE_MAX_PARAGRAPHS`] paragraphs, when another
/// heading level exists below it.
pub const TITLE_SIZE_FACTOR: f64 = 1.6;
/// See [`TITLE_SIZE_FACTOR`].
pub const TITLE_MAX_PARAGRAPHS: usize = 2;

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

/// The inferred sheet: Normal, the Title and the heading levels (index 0 is
/// level 1).
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    /// Normal's key.
    pub body: Option<FontKey>,
    /// The Title's key, when the document has one.
    pub title: Option<FontKey>,
    /// Heading keys by level.
    pub headings: Vec<FontKey>,
}

impl StyleSheet {
    /// The heading level (1-based) of `key`, if it is one; 0 is the Title.
    #[must_use]
    pub fn level(&self, key: &FontKey) -> Option<u8> {
        let same = |k: &FontKey| k.half_points == key.half_points && k.bold == key.bold;
        if self.title.as_ref().is_some_and(same) {
            return Some(0);
        }
        self.headings
            .iter()
            .position(same)
            .and_then(|i| u8::try_from(i + 1).ok())
    }
}

/// Whether a font key is a monospaced family. A monospaced line that stops
/// short of the column is a line of code (or a typed list), a paragraph of
/// its own however it ends; monospaced prose that fills its column joins as
/// any prose does.
#[must_use]
pub fn monospaced(key: &FontKey) -> bool {
    let family = key.family.to_ascii_lowercase();
    ["mono", "courier", "consol", "menlo", "code", "fixed"]
        .iter()
        .any(|m| family.contains(m))
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
                oblique: false,
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
            // A first-line indent starts a paragraph only after a line that
            // could end one (short, or ending a sentence): otherwise the
            // indented line is a hanging indent's second line.
            let could_end = short || terminal(&line_text(prev));
            let breaks = marker(&line.chars, false).is_some()
                || restyled(keys[i - 1].as_ref(), keys[i].as_ref(), short)
                || (short && keys[i - 1].as_ref().is_some_and(monospaced))
                || gap > PARAGRAPH_GAP * height
                || (short && terminal(&line_text(prev)))
                || (indented
                    && could_end
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
/// `last` was wrapped (it ran to within an em of the column's right edge, or
/// the first word of `next` would not have fitted after it, the test a line
/// breaker applies to ragged-right text) and does not end a sentence; `next`
/// starts lowercase (or `last` ends in a hyphen, or `next` sits directly
/// under `last` on its page), is not indented, carries no marker, and has the
/// same font. O(1) amortised.
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
    // The first word of `next`, and whether it would have fitted on `last`.
    let word: f64 = next
        .chars
        .iter()
        .skip_while(|c| c.text.trim().is_empty())
        .take_while(|c| !c.text.trim().is_empty())
        .map(|c| c.x1 - c.x0)
        .sum();
    // Directly under it: the next line of the same block of text, which a
    // block grouping with a tight line margin split (double spacing, a
    // generous leading, text in a shaded box narrower than the column).
    let under =
        next.page == last.page && next.top >= last.top && next.top - last.bottom <= 0.9 * em;
    // Under it, the block's own right edge is the wrap edge, provided the
    // line is long for the column (so two short lines one under the other,
    // a list of labels, are not run together).
    let right = if under && last.x1 - last.x0 >= 0.45 * (right - next_left) {
        right.min(last.x1.max(next.x1))
    } else {
        right
    };
    let code = dominant(last.chars.iter(), key).is_some_and(|k| monospaced(&k));
    // Monospaced text is set to the column or it is code: only a line that
    // reaches the edge runs on.
    let wrapped = !code && last.x1 + 0.25 * em + word > right - 0.5 * em;
    let full = last.x1 >= right - em || wrapped;
    let lower = next
        .chars
        .iter()
        .find(|c| !c.text.trim().is_empty())
        .and_then(|c| c.text.chars().next())
        .is_some_and(char::is_lowercase);
    full && !terminal(&text)
        && (lower || hyphen || under)
        && (next.x0 <= next_left + INDENT_EMS * em
            // Under it, the line may be inset like it (a shaded box's text)
            // or hang under its text (a list item's, a hanging indent's).
            || (under && next.x0 >= last.x0 - 1.0 && next.x0 <= last.x0 + (4.0 * em).max(40.0)))
        && marker(&next.chars, false).is_none()
        && !restyled(
            dominant(last.chars.iter(), key).as_ref(),
            dominant(next.chars.iter(), key).as_ref(),
            false,
        )
}

/// Whether the font change from a line whose characters mostly carry
/// `prev` to one mostly carrying `next` ends a paragraph. A change of size
/// does (a heading and its body, a caption, a note). A change of weight,
/// slant or family alone does only after a line that stopped short of the
/// column (`short`): a line that ran to the edge was wrapped, so the bold or
/// italic phrase, or the inline code, that fills most of the next line is
/// inside the paragraph (Word, LibreOffice and every report writer set
/// emphasis inline; breaking on it split the paragraph and made the
/// emphasised line a false heading). Half a point is the same size: sizes
/// are rounded to half points. O(1).
#[must_use]
pub fn restyled(prev: Option<&FontKey>, next: Option<&FontKey>, short: bool) -> bool {
    match (prev, next) {
        (Some(a), Some(b)) => a.half_points.abs_diff(b.half_points) > 1 || (short && a != b),
        (a, b) => a != b,
    }
}

/// Whether `next` continues `last`'s visual line: the same page, sharing
/// half the smaller line's height (so a raised citation mark is on the
/// line), starting at or after where `last` ends and within three ems of
/// it. O(1).
#[must_use]
pub fn same_line(last: &ILine, next: &ILine) -> bool {
    let em = last.size.max(next.size).max(1.0);
    let shared = last.bottom.min(next.bottom) - last.top.max(next.top);
    let smaller = (last.bottom - last.top)
        .min(next.bottom - next.top)
        .max(0.1);
    next.page == last.page
        && shared >= 0.5 * smaller
        && next.x0 >= last.x1 - 0.1 * em
        && next.x0 - last.x1 <= 3.0 * em
}

/// `chars` followed by `more`, the rest of the same line starting with
/// `next`: a space between them only where the line has a gap a space
/// would fill and neither side has one. O(characters).
#[must_use]
pub fn abut(mut chars: Vec<PChar>, more: Vec<PChar>, next: &ILine) -> Vec<PChar> {
    // From the first character kept (a leading space was trimmed by the
    // join, and the space it stood for is this gap).
    let start = more.first().map_or(next.x0, |c| c.x0);
    let gap = chars.last().map_or(0.0, |c| start - c.x1);
    let spaced = chars
        .last()
        .is_some_and(|c| c.text.ends_with(char::is_whitespace))
        || more
            .first()
            .is_some_and(|c| c.text.starts_with(char::is_whitespace));
    if !spaced
        && gap > 0.15 * next.size.max(1.0)
        && let Some(last) = chars.last()
    {
        let mut space = last.clone();
        space.text = " ".to_owned();
        space.sup = false;
        space.note = None;
        space.comments = Vec::new();
        chars.push(space);
    }
    chars.extend(more);
    chars
}

/// Per key: (characters, paragraphs, short paragraphs, brief paragraphs).
/// Short: brief and not ending like prose. Brief: at most
/// [`HEADING_MAX_LINES`] lines and [`HEADING_MAX_CHARS`] characters.
pub type KeyStats = BTreeMap<FontKey, (usize, usize, usize, usize)>;

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
        if lines <= HEADING_MAX_LINES && text_len <= HEADING_MAX_CHARS {
            entry.3 += 1;
            if !text_ends_like_prose(chars) {
                entry.2 += 1;
            }
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
    let mut candidates: Vec<(FontKey, usize)> = stats
        .iter()
        .filter(|(k, (_, paragraphs, short, brief))| {
            let larger =
                f64::from(k.half_points) >= HEADING_SIZE_FACTOR * f64::from(body.half_points);
            let bolder = k.bold && !body.bold && k.half_points >= body.half_points;
            // A larger size sets a heading apart on its own, so a heading
            // that is a sentence ("Lorem ipsum dolor sit amet, consectetur
            // adipiscing elit.") still counts; a heading only bolder than
            // the body must not read like prose, or every bold lead-in
            // paragraph would be one.
            let headed = if larger { *brief } else { *short };
            **k != body
                && (larger || bolder)
                && *paragraphs > 0
                && (headed as f64) >= HEADING_SHORT_SHARE * (*paragraphs as f64)
        })
        .map(|(k, (_, paragraphs, _, _))| (k.clone(), *paragraphs))
        .collect();
    candidates.sort_by(|a, b| {
        b.0.half_points
            .cmp(&a.0.half_points)
            .then(b.0.bold.cmp(&a.0.bold))
    });
    // Keys of one size and weight in several families are one level.
    let mut merged: Vec<(FontKey, usize)> = Vec::new();
    for (k, n) in candidates {
        match merged.last_mut() {
            Some((last, m)) if last.half_points == k.half_points && last.bold == k.bold => *m += n,
            _ => merged.push((k, n)),
        }
    }
    let title = merged.first().and_then(|(k, n)| {
        (f64::from(k.half_points) >= TITLE_SIZE_FACTOR * f64::from(body.half_points)
            && *n <= TITLE_MAX_PARAGRAPHS
            && merged[1..]
                .iter()
                .any(|(_, m)| *m >= HEADING_MIN_PARAGRAPHS))
        .then(|| k.clone())
    });
    let mut headings: Vec<FontKey> = merged
        .into_iter()
        .skip(usize::from(title.is_some()))
        .filter(|(_, n)| *n >= HEADING_MIN_PARAGRAPHS)
        .map(|(k, _)| k)
        .collect();
    headings.truncate(9);
    StyleSheet {
        body: Some(body),
        title,
        headings,
    }
}
